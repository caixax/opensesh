//! Minimal HTTPS download helper. Only `cargo xtask icons`, `fonts`, `conpty` and `vttest` use
//! the network, and only when the developer runs them explicitly.

use std::sync::LazyLock;
use std::time::Duration;

use anyhow::{Context, Result};

/// Shared agent, built once per process so every download reuses the same configuration.
static AGENT: LazyLock<ureq::Agent> = LazyLock::new(agent);

/// HTTPS only (redirects included), few redirects and bounded connect / total time, so a
/// misbehaving server can't downgrade the transport or hang the task.
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(3)
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into()
}

/// Downloads `url` into memory, refusing bodies larger than `max_bytes`.
///
/// # Errors
///
/// Fails on network or HTTP errors, non-HTTPS URLs or redirects, timeouts, or if the body exceeds
/// `max_bytes`.
pub fn get(url: &str, max_bytes: u64) -> Result<Vec<u8>> {
    let mut response = AGENT
        .get(url)
        .header(
            "User-Agent",
            concat!("opensesh-xtask/", env!("CARGO_PKG_VERSION")),
        )
        .call()
        .with_context(|| format!("GET {url}"))?;
    response
        .body_mut()
        .with_config()
        .limit(max_bytes)
        .read_to_vec()
        .with_context(|| format!("reading the body of {url}"))
}
