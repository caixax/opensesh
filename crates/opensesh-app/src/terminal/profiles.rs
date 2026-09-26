//! The terminal customization in memory (PLAN §6.2 to §6.5): profiles, themes and keyword
//! highlighting rules, shared by the `TerminalProfiles` QML singleton (which loads, edits and
//! saves them) and every `TerminalItem` (which reads them).
//!
//! The [`Library`] is immutable once published: a change builds a new one and swaps it in, with
//! a higher `revision`, so a reader never sees half an update and never blocks the writer for
//! more than the swap. Terminals re-resolve their settings when the revision changes.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, PoisonError, RwLock};

use opensesh_core::hosts::TerminalLevel;
use opensesh_core::terminal::highlight::HighlightLibrary;
use opensesh_core::terminal::profile::{Level, ProfileSet};
use opensesh_core::terminal::settings::{
    BackspaceKey, CursorStyle, DeleteKey, Osc52Access, TerminalSettings,
};
use opensesh_core::terminal::theme::{TerminalTheme, ThemeSet};
use opensesh_term::highlight::Highlighter;
use opensesh_term::input::keys::KeyOptions;
use opensesh_term::palette::Palette;
use opensesh_term::session::SessionOptions;
use opensesh_term::snapshot::CursorShape;

/// Font used when a profile leaves the family empty (bundled with the app).
pub const DEFAULT_FONT: &str = "JetBrains Mono";

/// Profiles, themes and rules, as last loaded or edited.
#[derive(Debug, Default)]
pub struct Library {
    /// Every profile.
    pub profiles: ProfileSet,
    /// Every theme.
    pub themes: ThemeSet,
    /// Every highlight rule set.
    pub highlights: HighlightLibrary,
    /// Grows with every change.
    pub revision: u64,
    /// Compiled rule sets, by the list of set ids (compiling costs milliseconds).
    highlighters: Mutex<HashMap<Vec<String>, Arc<Highlighter>>>,
}

static LIBRARY: LazyLock<RwLock<Arc<Library>>> =
    LazyLock::new(|| RwLock::new(Arc::new(Library::default())));

/// The current library.
#[must_use]
pub fn current() -> Arc<Library> {
    Arc::clone(&LIBRARY.read().unwrap_or_else(PoisonError::into_inner))
}

/// Replaces the parts `change` modifies and publishes the result with a new revision. Returns
/// the new library.
pub fn update(
    change: impl FnOnce(&mut ProfileSet, &mut ThemeSet, &mut HighlightLibrary),
) -> Arc<Library> {
    let mut slot = LIBRARY.write().unwrap_or_else(PoisonError::into_inner);
    let mut profiles = slot.profiles.clone();
    let mut themes = slot.themes.clone();
    let mut highlights = slot.highlights.clone();
    change(&mut profiles, &mut themes, &mut highlights);
    let next = Arc::new(Library {
        profiles,
        themes,
        highlights,
        revision: slot.revision + 1,
        highlighters: Mutex::new(HashMap::new()),
    });
    *slot = Arc::clone(&next);
    next
}

/// Everything a terminal needs from its profile.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// The profile's options.
    pub settings: TerminalSettings,
    /// The theme in use (dark or light).
    pub theme: TerminalTheme,
    /// Colors with the profile's color options.
    pub palette: Palette,
    /// The engine's options.
    pub options: SessionOptions,
    /// Key encoding options.
    pub keys: KeyOptions,
    /// Keyword highlighting, when the profile turns sets on.
    pub highlighter: Option<Arc<Highlighter>>,
}

impl Library {
    /// The settings of a terminal that uses profile `profile` (the global profile applies
    /// first; an unknown profile means the global one alone).
    #[must_use]
    pub fn settings(&self, profile: &str) -> TerminalSettings {
        self.profiles.resolve(&[Level {
            profile: Some(profile),
            overrides: None,
        }])
    }

    /// The theme `settings` picks for the app's dark or light scheme.
    #[must_use]
    pub fn theme(&self, settings: &TerminalSettings, dark: bool) -> TerminalTheme {
        let id = if dark {
            &settings.theme_dark
        } else {
            &settings.theme_light
        };
        self.themes.get_or_default(id, dark)
    }

    /// The compiled rules of `ids` (unknown ids are skipped), or `None` when there are none.
    #[must_use]
    pub fn highlighter(&self, ids: &[String]) -> Option<Arc<Highlighter>> {
        if ids.is_empty() {
            return None;
        }
        let mut cache = self
            .highlighters
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(found) = cache.get(ids) {
            return Some(Arc::clone(found)).filter(|h| !h.is_empty());
        }
        let sets = ids.iter().filter_map(|id| self.highlights.get(id));
        let compiled = Arc::new(Highlighter::new(sets));
        cache.insert(ids.to_vec(), Arc::clone(&compiled));
        Some(compiled).filter(|h| !h.is_empty())
    }

    /// Everything a terminal needs when its host's `levels` (groups, then the host) come before
    /// its own `profile` (empty for none): the profile chain of ADR 0016.
    #[must_use]
    pub fn resolve_with(&self, levels: &[TerminalLevel], profile: &str, dark: bool) -> Resolved {
        let mut chain: Vec<Level<'_>> = levels
            .iter()
            .map(|level| Level {
                profile: level.profile.as_deref(),
                overrides: Some(&level.overrides),
            })
            .collect();
        chain.push(Level {
            profile: (!profile.is_empty()).then_some(profile),
            overrides: None,
        });
        let settings = self.profiles.resolve(&chain);
        let theme = self.theme(&settings, dark);
        let palette = Palette::for_settings(&theme.colors, &settings);
        Resolved {
            options: session_options(&settings),
            keys: key_options(&settings),
            highlighter: self.highlighter(&settings.highlight_sets),
            palette,
            theme,
            settings,
        }
    }
}

/// The engine options of a profile.
#[must_use]
pub fn session_options(settings: &TerminalSettings) -> SessionOptions {
    SessionOptions {
        scrollback_lines: usize::try_from(settings.scrollback_lines).unwrap_or(usize::MAX),
        cursor_shape: match settings.cursor_shape {
            CursorStyle::Block => CursorShape::Block,
            CursorStyle::Beam => CursorShape::Beam,
            CursorStyle::Underline => CursorShape::Underline,
        },
        cursor_blinking: settings.cursor_blinking,
        cursor_hollow_unfocused: settings.cursor_hollow_unfocused,
        word_separators: settings.word_separators.clone(),
        osc52_copy: settings.osc52 == Osc52Access::Copy,
        encoding: settings.encoding.clone(),
        answerback: settings.answerback.clone(),
    }
}

/// The key encoding options of a profile.
#[must_use]
pub fn key_options(settings: &TerminalSettings) -> KeyOptions {
    KeyOptions {
        backspace_sends_ctrl_h: settings.backspace == BackspaceKey::CtrlH,
        alt_sends_escape: settings.alt_as_meta,
        delete_sends_del: settings.delete == DeleteKey::Del,
        ..KeyOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opensesh_core::terminal::profile::Profile;
    use opensesh_core::terminal::settings::TerminalOverrides;

    #[test]
    fn a_profile_resolves_to_engine_options() {
        let library = Library::default();
        let resolved = library.resolve_with(&[], "default", true);
        assert_eq!(resolved.palette, Palette::OPENSESH_DARK);
        assert_eq!(resolved.options, SessionOptions::default());
        assert_eq!(resolved.keys, KeyOptions::default());
        assert!(resolved.highlighter.is_none());
        assert_eq!(
            library.resolve_with(&[], "default", false).palette,
            Palette::OPENSESH_LIGHT
        );
    }

    #[test]
    fn profiles_themes_and_rules_reach_the_terminal() {
        let mut profiles = ProfileSet::default();
        let mut ops = Profile::new("ops", "Ops");
        ops.terminal = TerminalOverrides {
            theme_dark: Some("dracula".into()),
            backspace: Some(BackspaceKey::CtrlH),
            osc52: Some(Osc52Access::Copy),
            highlight_sets: Some(vec!["logs".into(), "missing".into()]),
            scrollback_lines: Some(500),
            ..TerminalOverrides::default()
        };
        profiles.insert(ops);
        let library = Library {
            profiles,
            ..Library::default()
        };
        let resolved = library.resolve_with(&[], "ops", true);
        assert_eq!(resolved.theme.id, "dracula");
        assert!(resolved.keys.backspace_sends_ctrl_h);
        assert!(resolved.options.osc52_copy);
        assert_eq!(resolved.options.scrollback_lines, 500);
        let highlighter = resolved.highlighter.unwrap();
        assert!(!highlighter.is_empty());
        // The compiled rules are cached.
        let again = library
            .highlighter(&["logs".into(), "missing".into()])
            .unwrap();
        assert!(Arc::ptr_eq(&highlighter, &again));
        // An unknown profile is the global one.
        assert_eq!(
            library.resolve_with(&[], "nope", true).theme.id,
            "opensesh-dark"
        );
    }
}
