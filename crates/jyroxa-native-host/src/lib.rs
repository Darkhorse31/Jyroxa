//! Native messaging framing and browser handoff payloads (shared with desktop).

pub mod framing;
pub mod handoff;
pub mod pipe_client;

pub const NATIVE_HOST_NAME: &str = "com.jyroxa.browser";
pub const HANDOFF_PIPE_NAME: &str = r"\\.\pipe\JyroxaBrowserHandoff";
