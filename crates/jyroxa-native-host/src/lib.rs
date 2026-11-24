//! Native messaging framing and browser handoff payloads (shared with desktop).

pub mod ext_id;
pub mod framing;
pub mod handoff;
pub mod integration;
pub mod pipe_client;
pub mod torrent_associations;

pub const NATIVE_HOST_NAME: &str = "com.jyroxa.browser";
pub const HANDOFF_PIPE_NAME: &str = r"\\.\pipe\JyroxaBrowserHandoff";

/// An isolated endpoint for headless verification, leaving the running app alone.
pub fn handoff_pipe_name() -> String {
    std::env::var("JYROXA_HANDOFF_PIPE")
        .ok()
        .filter(|name| name.starts_with(r"\\.\pipe\jyroxa-test-") && name.len() < 240)
        .unwrap_or_else(|| HANDOFF_PIPE_NAME.to_string())
}
