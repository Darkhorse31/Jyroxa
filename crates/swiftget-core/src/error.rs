//! Error types for SwiftGet core.

use std::fmt;

/// The result type used throughout the crate.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors that the engine and LinkGrabber can produce.
#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("no usable URL was provided")]
    NoUrl,

    #[error("server returned HTTP status {0}")]
    Status(u16),

    #[error("server does not support range requests and the size is unknown")]
    NoRangeSupport,

    #[error("checksum mismatch ({algo}): expected {expected}, computed {actual}")]
    Checksum {
        algo: &'static str,
        expected: String,
        actual: String,
    },

    #[error("all mirrors failed")]
    AllMirrorsFailed,

    #[error("download was cancelled")]
    Cancelled,

    #[error("link expired ({status}) and no refresher produced a fresh URL")]
    LinkExpired { status: u16 },

    #[error("remote file changed since the saved state (validator mismatch)")]
    ValidatorChanged,

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl fmt::Display) -> Self {
        Error::Other(msg.to_string())
    }
}
