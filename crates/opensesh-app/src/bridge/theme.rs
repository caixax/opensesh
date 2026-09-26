//! `Theme` QML singleton: the design tokens (PLAN §5.2) resolved in `opensesh-core::theme`.
//!
//! QML binds the inputs (`requestedMode`, `requestedAccent`, `requestedDensity`, `uiScale`,
//! `reduceMotion`, `uiFontFamily`, `systemDark`) once in `Main.qml`; every other property is a
//! read-only token that changes together with `themeChanged`. See `docs/design/components.md`.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        /// Qt string type from cxx-qt-lib.
        type QString = cxx_qt_lib::QString;

        include!("cxx-qt-lib/qcolor.h");
        /// Qt color type from cxx-qt-lib.
        type QColor = cxx_qt_lib::QColor;

        include!("cxx-qt-lib/qstringlist.h");
        /// Qt string list type from cxx-qt-lib.
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        /// Design tokens for QML.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // Inputs.
        #[qproperty(QString, requested_mode, cxx_name = "requestedMode", READ, WRITE = set_requested_mode, NOTIFY = inputs_changed)]
        #[qproperty(QString, requested_accent, cxx_name = "requestedAccent", READ, WRITE = set_requested_accent, NOTIFY = inputs_changed)]
        #[qproperty(QString, requested_density, cxx_name = "requestedDensity", READ, WRITE = set_requested_density, NOTIFY = inputs_changed)]
        #[qproperty(f64, ui_scale, cxx_name = "uiScale", READ, WRITE = set_ui_scale, NOTIFY = inputs_changed)]
        #[qproperty(bool, reduce_motion, cxx_name = "reduceMotion", READ, WRITE = set_reduce_motion, NOTIFY = inputs_changed)]
        #[qproperty(bool, system_dark, cxx_name = "systemDark", READ, WRITE = set_system_dark, NOTIFY = inputs_changed)]
        #[qproperty(QString, ui_font_family, cxx_name = "uiFontFamily", READ, WRITE = set_ui_font_family, NOTIFY = inputs_changed)]
        // Flags.
        #[qproperty(bool, dark, READ, NOTIFY = theme_changed)]
        #[qproperty(bool, compact, READ, NOTIFY = theme_changed)]
        #[qproperty(bool, accent_low_contrast, cxx_name = "accentLowContrast", READ, NOTIFY = theme_changed)]
        // Colors.
        #[qproperty(QColor, bg, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, surface, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, surface2, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, border, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, border_strong, cxx_name = "borderStrong", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, text, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, text_muted, cxx_name = "textMuted", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, text_disabled, cxx_name = "textDisabled", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, accent, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, default_accent, cxx_name = "defaultAccent", READ, NOTIFY = theme_changed)]
        #[qproperty(
            QStringList,
            accent_presets,
            cxx_name = "accentPresets",
            READ,
            CONSTANT
        )]
        #[qproperty(
            QStringList,
            tab_color_names,
            cxx_name = "tabColorNames",
            READ,
            CONSTANT
        )]
        #[qproperty(QStringList, tab_colors, cxx_name = "tabColors", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, accent_text, cxx_name = "accentText", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, accent_fg, cxx_name = "accentFg", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, success, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, warning, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, danger, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, info, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, focus_ring, cxx_name = "focusRing", READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, hover, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, pressed, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, selection, READ, NOTIFY = theme_changed)]
        #[qproperty(QColor, scrim, READ, NOTIFY = theme_changed)]
        // Spacing.
        #[qproperty(f64, spacing_xs, cxx_name = "spacingXs", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, spacing_sm, cxx_name = "spacingSm", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, spacing_md, cxx_name = "spacingMd", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, spacing_lg, cxx_name = "spacingLg", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, spacing_xl, cxx_name = "spacingXl", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, spacing_xxl, cxx_name = "spacingXxl", READ, NOTIFY = theme_changed)]
        // Controls and shell.
        #[qproperty(f64, control_height, cxx_name = "controlHeight", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, control_height_small, cxx_name = "controlHeightSmall", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, control_padding, cxx_name = "controlPadding", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, row_height, cxx_name = "rowHeight", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, icon_size, cxx_name = "iconSize", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, icon_size_small, cxx_name = "iconSizeSmall", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, title_bar_height, cxx_name = "titleBarHeight", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, rail_width, cxx_name = "railWidth", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, rail_width_labels, cxx_name = "railWidthLabels", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, status_bar_height, cxx_name = "statusBarHeight", READ, NOTIFY = theme_changed)]
        // Shape.
        #[qproperty(f64, radius_card, cxx_name = "radiusCard", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, radius_control, cxx_name = "radiusControl", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, radius_small, cxx_name = "radiusSmall", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, border_width, cxx_name = "borderWidth", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, focus_ring_width, cxx_name = "focusRingWidth", READ, NOTIFY = theme_changed)]
        // Type.
        #[qproperty(f64, font_size_small, cxx_name = "fontSizeSmall", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, font_size, cxx_name = "fontSize", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, font_size_large, cxx_name = "fontSizeLarge", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, font_size_title, cxx_name = "fontSizeTitle", READ, NOTIFY = theme_changed)]
        #[qproperty(QString, font_family, cxx_name = "fontFamily", READ, NOTIFY = theme_changed)]
        #[qproperty(QString, mono_font_family, cxx_name = "monoFontFamily", READ, NOTIFY = theme_changed)]
        // Motion.
        #[qproperty(f64, duration_fast, cxx_name = "durationFast", READ, NOTIFY = theme_changed)]
        #[qproperty(f64, duration_normal, cxx_name = "durationNormal", READ, NOTIFY = theme_changed)]
        type Theme = super::ThemeRust;

        /// Emitted when an input changes.
        #[qsignal]
        #[cxx_name = "inputsChanged"]
        fn inputs_changed(self: Pin<&mut Self>);

        /// Emitted when the resolved tokens change.
        #[qsignal]
        #[cxx_name = "themeChanged"]
        fn theme_changed(self: Pin<&mut Self>);

        /// `system`, `dark` or `light`.
        fn set_requested_mode(self: Pin<&mut Self>, value: QString);
        /// `default` or `#RRGGBB`.
        fn set_requested_accent(self: Pin<&mut Self>, value: QString);
        /// `comfortable` or `compact`.
        fn set_requested_density(self: Pin<&mut Self>, value: QString);
        /// UI zoom factor.
        fn set_ui_scale(self: Pin<&mut Self>, value: f64);
        /// Disable animations.
        fn set_reduce_motion(self: Pin<&mut Self>, value: bool);
        /// Whether the OS currently uses a dark color scheme.
        fn set_system_dark(self: Pin<&mut Self>, value: bool);
        /// UI font family; empty uses the bundled default.
        fn set_ui_font_family(self: Pin<&mut Self>, value: QString);

        /// Readable text color for content drawn on `fill`.
        #[qinvokable]
        #[cxx_name = "textOn"]
        fn text_on(self: &Self, fill: &QColor) -> QColor;
    }

    impl cxx_qt::Initialize for Theme {}
}

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::{QColor, QString, QStringList};
use opensesh_core::theme::{self, ColorScheme, Density, Rgba, ThemeInputs, ThemeMode};

/// Bundled UI font (registered at startup).
pub const DEFAULT_UI_FONT: &str = "Inter";

/// Bundled monospace font.
pub const DEFAULT_MONO_FONT: &str = "JetBrains Mono";

/// Rust state behind `Theme`.
#[derive(Debug)]
pub struct ThemeRust {
    requested_mode: QString,
    requested_accent: QString,
    requested_density: QString,
    ui_scale: f64,
    reduce_motion: bool,
    system_dark: bool,
    ui_font_family: QString,

    dark: bool,
    compact: bool,
    accent_low_contrast: bool,

    bg: QColor,
    surface: QColor,
    surface2: QColor,
    border: QColor,
    border_strong: QColor,
    text: QColor,
    text_muted: QColor,
    text_disabled: QColor,
    accent: QColor,
    /// The default ("Sesame") accent of the current scheme, whatever accent is chosen.
    default_accent: QColor,
    /// `#RRGGBB` codes of [`theme::ACCENT_PRESETS`].
    accent_presets: QStringList,
    /// Names of the tab colors ([`theme::TAB_COLOR_NAMES`]).
    tab_color_names: QStringList,
    /// `#RRGGBB` codes of the tab colors for the current scheme, in the order of the names.
    tab_colors: QStringList,
    accent_text: QColor,
    accent_fg: QColor,
    success: QColor,
    warning: QColor,
    danger: QColor,
    info: QColor,
    focus_ring: QColor,
    hover: QColor,
    pressed: QColor,
    selection: QColor,
    scrim: QColor,

    spacing_xs: f64,
    spacing_sm: f64,
    spacing_md: f64,
    spacing_lg: f64,
    spacing_xl: f64,
    spacing_xxl: f64,
    control_height: f64,
    control_height_small: f64,
    control_padding: f64,
    row_height: f64,
    icon_size: f64,
    icon_size_small: f64,
    title_bar_height: f64,
    rail_width: f64,
    rail_width_labels: f64,
    status_bar_height: f64,
    radius_card: f64,
    radius_control: f64,
    radius_small: f64,
    border_width: f64,
    focus_ring_width: f64,
    font_size_small: f64,
    font_size: f64,
    font_size_large: f64,
    font_size_title: f64,
    font_family: QString,
    mono_font_family: QString,
    duration_fast: f64,
    duration_normal: f64,
}

impl Default for ThemeRust {
    fn default() -> Self {
        let mut state = Self {
            requested_mode: QString::from(ThemeMode::System.as_str()),
            requested_accent: QString::from("default"),
            requested_density: QString::from(Density::Comfortable.as_str()),
            ui_scale: 1.0,
            reduce_motion: false,
            system_dark: true,
            ui_font_family: QString::default(),
            dark: true,
            compact: false,
            accent_low_contrast: false,
            bg: QColor::default(),
            surface: QColor::default(),
            surface2: QColor::default(),
            border: QColor::default(),
            border_strong: QColor::default(),
            text: QColor::default(),
            text_muted: QColor::default(),
            text_disabled: QColor::default(),
            accent: QColor::default(),
            default_accent: QColor::default(),
            accent_presets: theme::ACCENT_PRESETS
                .iter()
                .map(|preset| QString::from(&preset.to_hex()))
                .collect(),
            tab_color_names: theme::TAB_COLOR_NAMES
                .iter()
                .map(|name| QString::from(*name))
                .collect(),
            tab_colors: QStringList::default(),
            accent_text: QColor::default(),
            accent_fg: QColor::default(),
            success: QColor::default(),
            warning: QColor::default(),
            danger: QColor::default(),
            info: QColor::default(),
            focus_ring: QColor::default(),
            hover: QColor::default(),
            pressed: QColor::default(),
            selection: QColor::default(),
            scrim: QColor::default(),
            spacing_xs: 0.0,
            spacing_sm: 0.0,
            spacing_md: 0.0,
            spacing_lg: 0.0,
            spacing_xl: 0.0,
            spacing_xxl: 0.0,
            control_height: 0.0,
            control_height_small: 0.0,
            control_padding: 0.0,
            row_height: 0.0,
            icon_size: 0.0,
            icon_size_small: 0.0,
            title_bar_height: 0.0,
            rail_width: 0.0,
            rail_width_labels: 0.0,
            status_bar_height: 0.0,
            radius_card: 0.0,
            radius_control: 0.0,
            radius_small: 0.0,
            border_width: 1.0,
            focus_ring_width: 2.0,
            font_size_small: 0.0,
            font_size: 0.0,
            font_size_large: 0.0,
            font_size_title: 0.0,
            font_family: QString::from(DEFAULT_UI_FONT),
            mono_font_family: QString::from(DEFAULT_MONO_FONT),
            duration_fast: 0.0,
            duration_normal: 0.0,
        };
        state.recompute();
        state
    }
}

/// Converts a core color to a `QColor`.
fn qcolor(color: Rgba) -> QColor {
    QColor::from_rgba(
        i32::from(color.r),
        i32::from(color.g),
        i32::from(color.b),
        i32::from(color.a),
    )
}

/// Converts a `QColor` to an opaque core color (channels are 0..=255 by construction).
fn rgba(color: &QColor) -> Rgba {
    let channel = |value: i32| u8::try_from(value.clamp(0, 255)).unwrap_or(u8::MAX);
    Rgba::rgb(
        channel(color.red()),
        channel(color.green()),
        channel(color.blue()),
    )
}

impl ThemeRust {
    /// Current inputs, with unknown values replaced by defaults.
    fn inputs(&self) -> ThemeInputs {
        let text = |value: &QString| value.to_string();
        ThemeInputs {
            mode: text(&self.requested_mode).parse().unwrap_or_default(),
            system_scheme: if self.system_dark {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            },
            accent: text(&self.requested_accent).parse::<Rgba>().ok(),
            density: text(&self.requested_density).parse().unwrap_or_default(),
            ui_scale: self.ui_scale,
            reduce_motion: self.reduce_motion,
        }
    }

    /// Recomputes every output from the inputs.
    fn recompute(&mut self) {
        let inputs = self.inputs();
        let resolved = theme::resolve(&inputs);
        let p = resolved.palette;
        let m = resolved.metrics;

        self.dark = resolved.scheme == ColorScheme::Dark;
        self.compact = inputs.density == Density::Compact;
        self.accent_low_contrast = resolved.accent_low_contrast;

        self.bg = qcolor(p.bg);
        self.surface = qcolor(p.surface);
        self.surface2 = qcolor(p.surface2);
        self.border = qcolor(p.border);
        self.border_strong = qcolor(p.border_strong);
        self.text = qcolor(p.text);
        self.text_muted = qcolor(p.text_muted);
        self.text_disabled = qcolor(p.text_disabled);
        self.accent = qcolor(p.accent);
        self.default_accent = qcolor(if self.dark {
            theme::DEFAULT_ACCENT_DARK
        } else {
            theme::DEFAULT_ACCENT_LIGHT
        });
        self.tab_colors = theme::tab_colors(resolved.scheme)
            .iter()
            .map(|color| QString::from(&color.to_hex()))
            .collect();
        self.accent_text = qcolor(p.accent_text);
        self.accent_fg = qcolor(p.accent_fg);
        self.success = qcolor(p.success);
        self.warning = qcolor(p.warning);
        self.danger = qcolor(p.danger);
        self.info = qcolor(p.info);
        self.focus_ring = qcolor(p.focus_ring);
        self.hover = qcolor(p.hover);
        self.pressed = qcolor(p.pressed);
        self.selection = qcolor(p.selection);
        self.scrim = qcolor(p.scrim);

        let unit = m.spacing;
        self.spacing_xs = unit;
        self.spacing_sm = unit * 2.0;
        self.spacing_md = unit * 3.0;
        self.spacing_lg = unit * 4.0;
        self.spacing_xl = unit * 6.0;
        self.spacing_xxl = unit * 8.0;
        self.control_height = m.control_height;
        self.control_height_small = m.control_height_small;
        self.control_padding = m.control_padding;
        self.row_height = m.row_height;
        self.icon_size = m.icon_size;
        self.icon_size_small = m.icon_size_small;
        self.title_bar_height = m.title_bar_height;
        self.rail_width = m.rail_width;
        self.rail_width_labels = m.rail_width_labels;
        self.status_bar_height = m.status_bar_height;
        self.radius_card = m.radius_card;
        self.radius_control = m.radius_control;
        self.radius_small = m.radius_small;
        self.font_size_small = m.font_size_small;
        self.font_size = m.font_size;
        self.font_size_large = m.font_size_large;
        self.font_size_title = m.font_size_title;
        self.duration_fast = m.duration_fast;
        self.duration_normal = m.duration_normal;

        let family = self.ui_font_family.to_string();
        self.font_family = if family.trim().is_empty() {
            QString::from(DEFAULT_UI_FONT)
        } else {
            QString::from(family.trim())
        };
    }
}

impl qobject::Theme {
    /// Applies an input change: stores it, recomputes the tokens and notifies QML.
    fn update(mut self: Pin<&mut Self>, apply: impl FnOnce(&mut ThemeRust) -> bool) {
        let changed = {
            let mut state = self.as_mut().rust_mut();
            apply(&mut state)
        };
        if !changed {
            return;
        }
        self.as_mut().rust_mut().recompute();
        self.as_mut().inputs_changed();
        self.as_mut().theme_changed();
    }

    /// See the bridge declaration.
    pub fn set_requested_mode(self: Pin<&mut Self>, value: QString) {
        self.update(|state| replace(&mut state.requested_mode, value));
    }

    /// See the bridge declaration.
    pub fn set_requested_accent(self: Pin<&mut Self>, value: QString) {
        self.update(|state| replace(&mut state.requested_accent, value));
    }

    /// See the bridge declaration.
    pub fn set_requested_density(self: Pin<&mut Self>, value: QString) {
        self.update(|state| replace(&mut state.requested_density, value));
    }

    /// See the bridge declaration.
    pub fn set_ui_scale(self: Pin<&mut Self>, value: f64) {
        self.update(|state| {
            let changed = (state.ui_scale - value).abs() > f64::EPSILON;
            state.ui_scale = value;
            changed
        });
    }

    /// See the bridge declaration.
    pub fn set_reduce_motion(self: Pin<&mut Self>, value: bool) {
        self.update(|state| replace(&mut state.reduce_motion, value));
    }

    /// See the bridge declaration.
    pub fn set_system_dark(self: Pin<&mut Self>, value: bool) {
        self.update(|state| replace(&mut state.system_dark, value));
    }

    /// See the bridge declaration.
    pub fn set_ui_font_family(self: Pin<&mut Self>, value: QString) {
        self.update(|state| replace(&mut state.ui_font_family, value));
    }

    /// See the bridge declaration.
    pub fn text_on(&self, fill: &QColor) -> QColor {
        qcolor(theme::best_text_on(rgba(fill)))
    }
}

/// Stores `value` in `slot`; returns whether it changed (setters must be idempotent to avoid
/// binding loops).
fn replace<T: PartialEq>(slot: &mut T, value: T) -> bool {
    if *slot == value {
        false
    } else {
        *slot = value;
        true
    }
}

impl cxx_qt::Initialize for qobject::Theme {
    fn initialize(self: Pin<&mut Self>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_reports_changes_only() {
        let mut value = 1;
        assert!(!replace(&mut value, 1));
        assert!(replace(&mut value, 2));
        assert_eq!(value, 2);
    }

    #[test]
    fn unknown_inputs_fall_back_to_defaults() {
        let state = ThemeRust {
            requested_mode: QString::from("sepia"),
            requested_density: QString::from("huge"),
            requested_accent: QString::from("amber"),
            ..ThemeRust::default()
        };
        let inputs = state.inputs();
        assert_eq!(inputs.mode, ThemeMode::System);
        assert_eq!(inputs.density, Density::Comfortable);
        assert_eq!(inputs.accent, None);
    }
}
