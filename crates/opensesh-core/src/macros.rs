//! Macros shared by the settings modules.

/// Declares a string-backed settings enum with its TOML/QML identifiers.
macro_rules! choice {
    (
        $(#[$meta:meta])*
        $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal),+ $(,)? }
        default $default:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($(#[$vmeta])* $variant),+
        }

        impl $name {
            /// All values, in UI order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// Stable identifier used in the settings files and QML.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text),+
                }
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::theme::UnknownValue;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                Self::ALL
                    .iter()
                    .copied()
                    .find(|value| value.as_str() == text)
                    .ok_or_else(|| $crate::theme::UnknownValue(text.to_owned()))
            }
        }
    };
}
