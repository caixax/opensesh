//! Keyword highlighting at render time (PLAN §6.5).
//!
//! A [`Highlighter`] holds the compiled rules of the rule sets a profile turned on. When the
//! snapshot builds a row, it rebuilds that row's text and asks [`Highlighter::row`] which rule
//! styles each column; the painter then applies the rule's colors, bold and underline before
//! selection and search highlights (which win). Only rows that are redrawn are scanned, and a
//! match can't span a soft-wrapped line.

use opensesh_core::terminal::highlight::{
    HighlightColor, HighlightSet, HighlightStyle, REGEX_SIZE_LIMIT,
};
use regex::{Regex, RegexBuilder};

/// Compiled highlighting rules.
#[derive(Debug, Clone, Default)]
pub struct Highlighter {
    rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
struct Rule {
    regex: Regex,
    /// Whether the pattern has a capture group (then only the first group is styled).
    grouped: bool,
    style: HighlightStyle,
}

impl Highlighter {
    /// Compiles the rules of `sets`, in order. A rule that doesn't compile is skipped and logged
    /// (the rules file is checked when it is read, so this is rare).
    #[must_use]
    pub fn new<'a>(sets: impl IntoIterator<Item = &'a HighlightSet>) -> Self {
        let mut rules = Vec::new();
        for set in sets {
            for rule in &set.rules {
                match RegexBuilder::new(&rule.pattern)
                    .case_insensitive(rule.ignore_case)
                    .size_limit(REGEX_SIZE_LIMIT)
                    .build()
                {
                    Ok(regex) => rules.push(Rule {
                        grouped: regex.captures_len() > 1,
                        regex,
                        style: rule.style,
                    }),
                    Err(error) => {
                        tracing::warn!(set = %set.id, "highlight rule skipped: {error}");
                    }
                }
            }
        }
        Self { rules }
    }

    /// Whether there is no rule.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// The style of rule `index` (as stored by [`Highlighter::row`], minus one).
    pub(crate) fn style(&self, index: usize) -> Option<&HighlightStyle> {
        self.rules.get(index).map(|rule| &rule.style)
    }

    /// Styles one row. `row.text` is the row's text and `row.columns[i]` the cells (start, end)
    /// that byte `i` of it covers. Fills `out` (one entry per column) with `0` for no rule or
    /// `1 +` the index of the last rule that matched there.
    pub(crate) fn row(&self, row: &RowText, columns: usize, out: &mut Vec<u16>) {
        out.clear();
        out.resize(columns, 0);
        for (index, rule) in self.rules.iter().enumerate() {
            let mark = u16::try_from(index + 1).unwrap_or(u16::MAX);
            let mut apply = |start: usize, end: usize| {
                if start >= end {
                    return;
                }
                let (Some(first), Some(last)) = (row.columns.get(start), row.columns.get(end - 1))
                else {
                    return;
                };
                let (from, to) = (usize::from(first.0), usize::from(last.1).min(columns));
                for slot in out.iter_mut().take(to).skip(from) {
                    *slot = mark;
                }
            };
            if rule.grouped {
                for captures in rule.regex.captures_iter(&row.text) {
                    if let Some(found) = captures.get(1).or_else(|| captures.get(0)) {
                        apply(found.start(), found.end());
                    }
                }
            } else {
                for found in rule.regex.find_iter(&row.text) {
                    apply(found.start(), found.end());
                }
            }
        }
    }
}

/// A row's text as the highlighter sees it, reused between rows.
#[derive(Debug, Default)]
pub(crate) struct RowText {
    /// The characters of the row (spacers of wide characters left out).
    pub(crate) text: String,
    /// For each byte of `text`, the columns (start, end) of the cell it belongs to.
    pub(crate) columns: Vec<(u16, u16)>,
}

impl RowText {
    pub(crate) fn clear(&mut self) {
        self.text.clear();
        self.columns.clear();
    }

    /// Appends a cell's characters, which cover `width` columns from `column`.
    pub(crate) fn push(&mut self, ch: char, marks: &[char], column: usize, width: usize) {
        let start = u16::try_from(column).unwrap_or(u16::MAX);
        let end = u16::try_from(column + width).unwrap_or(u16::MAX);
        for c in std::iter::once(ch).chain(marks.iter().copied()) {
            let before = self.text.len();
            self.text.push(c);
            self.columns
                .extend(std::iter::repeat_n((start, end), self.text.len() - before));
        }
    }
}

/// A rule color as an index into the terminal's color table, or a fixed color.
pub(crate) enum RuleColor {
    /// ANSI color 0-15.
    Indexed(usize),
    /// `0xRRGGBB` channels.
    Rgb(u8, u8, u8),
}

impl From<HighlightColor> for RuleColor {
    fn from(color: HighlightColor) -> Self {
        match color {
            HighlightColor::Ansi(index) => Self::Indexed(usize::from(index.min(15))),
            HighlightColor::Rgb(color) => Self::Rgb(color.r, color.g, color.b),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opensesh_core::terminal::highlight::{HighlightRule, builtin_sets};

    fn row_of(text: &str) -> RowText {
        let mut row = RowText::default();
        for (column, ch) in text.chars().enumerate() {
            row.push(ch, &[], column, 1);
        }
        row
    }

    #[test]
    fn later_rules_win_and_columns_are_marked() {
        let set = HighlightSet {
            id: "t".into(),
            name: "t".into(),
            rules: vec![
                HighlightRule {
                    pattern: "ERROR.*".into(),
                    ignore_case: false,
                    style: HighlightStyle::default(),
                },
                HighlightRule {
                    pattern: "disk".into(),
                    ignore_case: true,
                    style: HighlightStyle::default(),
                },
            ],
            builtin: false,
        };
        let highlighter = Highlighter::new([&set]);
        let mut out = Vec::new();
        highlighter.row(&row_of("ok ERROR Disk full"), 20, &mut out);
        assert_eq!(&out[..3], [0, 0, 0]);
        assert_eq!(&out[3..9], [1; 6]);
        assert_eq!(&out[9..13], [2; 4], "the later rule wins");
        assert_eq!(&out[13..18], [1; 5]);
        assert_eq!(&out[18..], [0, 0], "past the text");
    }

    #[test]
    fn wide_characters_and_capture_groups() {
        let set = HighlightSet {
            id: "t".into(),
            name: "t".into(),
            rules: vec![HighlightRule {
                pattern: r"see (\S+)".into(),
                ignore_case: false,
                style: HighlightStyle::default(),
            }],
            builtin: false,
        };
        let highlighter = Highlighter::new([&set]);
        let mut row = RowText::default();
        // "see 日本" with each CJK character two columns wide.
        for (column, ch) in "see ".chars().enumerate() {
            row.push(ch, &[], column, 1);
        }
        row.push('日', &[], 4, 2);
        row.push('本', &[], 6, 2);
        let mut out = Vec::new();
        highlighter.row(&row, 10, &mut out);
        assert_eq!(
            out,
            [0, 0, 0, 0, 1, 1, 1, 1, 0, 0],
            "only the group, both halves"
        );
    }

    #[test]
    fn presets_compile_and_find_things() {
        let sets = builtin_sets();
        let highlighter = Highlighter::new(&sets);
        assert!(!highlighter.is_empty());
        let mut out = Vec::new();
        highlighter.row(&row_of("ERROR from 10.0.0.1"), 19, &mut out);
        assert!(out[..5].iter().all(|&m| m > 0));
        assert!(out[11..].iter().all(|&m| m > 0));
        assert!(highlighter.style(0).is_some());
    }
}
