//! Application identity shared by every OpenSesh binary.

/// Human-readable application name.
pub const APP_NAME: &str = "OpenSesh";

/// Reverse-DNS application id. It is the Wayland `app_id`, the `.desktop` file name (without the
/// extension) and the icon name.
pub const APP_ID: &str = "cc.caixa.OpenSesh";

/// Domain of the project owner, used as the Qt organization domain.
pub const ORGANIZATION_DOMAIN: &str = "caixa.cc";

/// Version of the OpenSesh crates, taken from the workspace `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_id_is_reverse_dns_of_the_organization_domain() {
        let reversed: Vec<&str> = ORGANIZATION_DOMAIN.split('.').rev().collect();
        let expected = format!("{}.{APP_NAME}", reversed.join("."));
        assert_eq!(APP_ID, expected);
    }

    #[test]
    fn app_id_is_a_valid_desktop_file_id() {
        // Desktop entry spec: segments of [A-Za-z0-9_], no segment starting with a digit.
        for segment in APP_ID.split('.') {
            assert!(!segment.is_empty());
            assert!(!segment.starts_with(|c: char| c.is_ascii_digit()));
            assert!(
                segment
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_')
            );
        }
        assert!(APP_ID.split('.').count() >= 3);
    }
}
