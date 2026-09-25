//! Regex search in the terminal buffer (Ctrl+Shift+F), on top of `alacritty_terminal`'s lazy DFA
//! search.
//!
//! Patterns use Rust `regex` syntax and are case-insensitive unless they contain an uppercase
//! letter ("smart case", as in Alacritty). Searches run under the `Term` lock on the caller's
//! thread, so every scan is bounded: a search step covers at most
//! [`crate::session::SessionConfig::search_max_lines`] lines, and highlighting only scans the
//! viewport.

use std::borrow::Cow;

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

/// Lines above the viewport scanned for highlights, so a match that starts just above the view
/// and ends inside it is still highlighted.
const HIGHLIGHT_MARGIN_LINES: i32 = 8;

/// Most matches highlighted at once (a pattern like `.` would otherwise match every cell).
const MAX_VISIBLE_MATCHES: usize = 4096;

/// Errors of [`crate::session::Session::search`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SearchError {
    /// The pattern is not a valid regular expression, or it compiles to an automaton that is too
    /// large. The text is the regex engine's message.
    #[error("invalid search pattern: {0}")]
    InvalidPattern(String),
}

/// An active search: the compiled pattern and the current ("focused") match.
#[derive(Debug, Clone)]
pub(crate) struct Search {
    pattern: String,
    /// Compiled once and never used directly: every scan works on a copy with fresh DFA
    /// caches. `alacritty_terminal` unwraps the lazy DFA's start state, which can only fail once
    /// a cache has "given up" after repeated clears (pathological patterns), so a fresh cache per
    /// scan keeps a bad pattern from panicking the caller's (GUI) thread.
    regex: RegexSearch,
    focused: Option<Match>,
}

impl Search {
    /// Compiles `pattern`.
    pub(crate) fn new(pattern: &str) -> Result<Self, SearchError> {
        let regex = RegexSearch::new(&ascii_word_boundaries(pattern))
            .map_err(|error| SearchError::InvalidPattern(error.to_string()))?;
        Ok(Self {
            pattern: pattern.to_owned(),
            regex,
            focused: None,
        })
    }

    /// The pattern this search was compiled from.
    pub(crate) fn pattern(&self) -> &str {
        &self.pattern
    }

    /// The current match, in grid coordinates.
    pub(crate) fn focused(&self) -> Option<&Match> {
        self.focused.as_ref()
    }

    /// Finds the next match and makes it the current one.
    ///
    /// `forward` searches toward the bottom (newer output), backward toward the top (older
    /// output). The first search starts at the top-left of the viewport (forward) or its
    /// bottom-right (backward); later ones continue after (or before) the current match and wrap
    /// around the buffer. At most `max_lines` lines are scanned.
    pub(crate) fn find<T>(
        &mut self,
        term: &Term<T>,
        forward: bool,
        max_lines: usize,
    ) -> Option<Match> {
        let display_offset = i32::try_from(term.grid().display_offset()).unwrap_or(i32::MAX);
        let (origin, side, direction) = match (&self.focused, forward) {
            (Some(current), true) => (
                current.end().add(term, Boundary::None, 1),
                Side::Left,
                Direction::Right,
            ),
            (Some(current), false) => (
                current.start().sub(term, Boundary::None, 1),
                Side::Right,
                Direction::Left,
            ),
            (None, true) => (
                Point::new(Line(-display_offset), Column(0)),
                Side::Left,
                Direction::Right,
            ),
            (None, false) => (
                Point::new(term.bottommost_line() - display_offset, term.last_column()),
                Side::Right,
                Direction::Left,
            ),
        };
        let origin = origin.grid_clamp(term, Boundary::Grid);
        let mut regex = self.regex.clone();
        let found = term.search_next(&mut regex, origin, direction, side, Some(max_lines));
        self.focused.clone_from(&found);
        found
    }

    /// Collects the matches visible in the viewport, in reading order, into `out`.
    pub(crate) fn visible_matches<T>(&self, term: &Term<T>, out: &mut Vec<Match>) {
        out.clear();
        let display_offset = i32::try_from(term.grid().display_offset()).unwrap_or(i32::MAX);
        let top = Line(-display_offset);
        let bottom = term.bottommost_line() - display_offset;
        let start_line =
            Line(top.0.saturating_sub(HIGHLIGHT_MARGIN_LINES)).max(term.topmost_line());
        let start = Point::new(start_line, Column(0));
        let end = Point::new(bottom, term.last_column());
        let viewport_start = Point::new(top, Column(0));
        let mut regex = self.regex.clone();
        for found in RegexIter::new(start, end, Direction::Right, term, &mut regex) {
            if *found.end() < viewport_start {
                continue;
            }
            out.push(found);
            if out.len() >= MAX_VISIBLE_MATCHES {
                break;
            }
        }
    }
}

/// `alacritty_terminal`'s lazy DFAs reject Unicode word boundaries, the default meaning of `\b`
/// and `\B`: rewrite them (outside character classes) to their ASCII forms, which are supported.
fn ascii_word_boundaries(pattern: &str) -> Cow<'_, str> {
    if !pattern.contains("\\b") && !pattern.contains("\\B") {
        return Cow::Borrowed(pattern);
    }
    let mut out = String::with_capacity(pattern.len() + 16);
    let mut chars = pattern.chars().peekable();
    let mut in_class = false;
    while let Some(ch) = chars.next() {
        match ch {
            '\\' => match chars.next() {
                // `\b{start}` and friends are already explicit: leave them alone.
                Some(escaped @ ('b' | 'B')) if !in_class && chars.peek() != Some(&'{') => {
                    out.push_str(if escaped == 'b' {
                        r"(?-u:\b)"
                    } else {
                        r"(?-u:\B)"
                    });
                }
                Some(escaped) => {
                    out.push('\\');
                    out.push(escaped);
                }
                None => out.push('\\'),
            },
            '[' => {
                in_class = true;
                out.push(ch);
            }
            ']' => {
                in_class = false;
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::term::test::mock_term;

    #[test]
    fn word_boundaries_become_ascii() {
        assert_eq!(ascii_word_boundaries(r"plain"), "plain");
        assert_eq!(ascii_word_boundaries(r"\bword\B"), r"(?-u:\b)word(?-u:\B)");
        assert_eq!(ascii_word_boundaries(r"\\b"), r"\\b");
        assert_eq!(ascii_word_boundaries(r"[\b]\b"), r"[\b](?-u:\b)");
        assert_eq!(ascii_word_boundaries(r"\b{start}x"), r"\b{start}x");
        let term = mock_term("cat concat\r\ncat");
        let search = Search::new(r"\bcat\b").expect("valid pattern");
        let mut out = Vec::new();
        search.visible_matches(&term, &mut out);
        assert_eq!(out.len(), 2, "`concat` does not match");
    }

    fn text(term: &Term<alacritty_terminal::event::VoidListener>, found: &Match) -> String {
        term.bounds_to_string(*found.start(), *found.end())
    }

    #[test]
    fn invalid_patterns_are_errors() {
        assert!(matches!(
            Search::new("(unclosed"),
            Err(SearchError::InvalidPattern(_))
        ));
        assert!(Search::new(r"line \d+").is_ok());
    }

    #[test]
    fn forward_and_backward_cycle_through_matches() {
        let term = mock_term("alpha one\r\nbeta two\r\nalpha three\r\ngamma\r\nalpha four");
        let mut search = Search::new("alpha \\w+").expect("valid pattern");
        // Backward from the bottom: the newest match first.
        let first = search.find(&term, false, 1000).expect("a match");
        assert_eq!(text(&term, &first), "alpha four");
        let second = search.find(&term, false, 1000).expect("a match");
        assert_eq!(text(&term, &second), "alpha three");
        let third = search.find(&term, false, 1000).expect("a match");
        assert_eq!(text(&term, &third), "alpha one");
        // Wraps around.
        let wrapped = search.find(&term, false, 1000).expect("a match");
        assert_eq!(text(&term, &wrapped), "alpha four");
        // And forward again.
        let forward = search.find(&term, true, 1000).expect("a match");
        assert_eq!(text(&term, &forward), "alpha one");
        assert_eq!(search.focused(), Some(&forward));
    }

    #[test]
    fn smart_case_and_no_match() {
        let term = mock_term("Hello World\r\nhello world");
        let mut insensitive = Search::new("hello").expect("valid pattern");
        let found = insensitive.find(&term, true, 100).expect("a match");
        assert_eq!(found.start().line, Line(0));
        let mut sensitive = Search::new("World").expect("valid pattern");
        let found = sensitive.find(&term, false, 100).expect("a match");
        assert_eq!(found.start().line, Line(0));
        let mut missing = Search::new("absent").expect("valid pattern");
        assert_eq!(missing.find(&term, true, 100), None);
        assert_eq!(missing.focused(), None);
    }

    #[test]
    fn visible_matches_are_in_reading_order() {
        let term = mock_term("ab ab\r\nxx ab\r\nab");
        let search = Search::new("ab").expect("valid pattern");
        let mut out = Vec::new();
        search.visible_matches(&term, &mut out);
        let starts: Vec<_> = out
            .iter()
            .map(|found| (found.start().line.0, found.start().column.0))
            .collect();
        assert_eq!(starts, [(0, 0), (0, 3), (1, 3), (2, 0)]);
    }
}
