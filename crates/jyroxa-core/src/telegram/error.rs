//! Telegram resolver and MTProto error types.

use std::time::Duration;

/// Result type for Telegram operations.
pub type Result<T> = std::result::Result<T, TelegramError>;

/// Errors produced when resolving or downloading Telegram content.
#[derive(Debug, thiserror::Error)]
pub enum TelegramError {
    #[error("invalid Telegram link: {0}")]
    InvalidLink(String),

    #[error("post or media not found")]
    NotFound,

    #[error("this post is private; connect your Telegram account in Settings")]
    LoginRequired,

    #[error("this content is only available to members you cannot access")]
    Private,

    #[error("no downloadable media in this post")]
    NoMedia,

    #[error("Telegram rate limit: retry after {retry_after:?}")]
    FloodWait { retry_after: Duration },

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("URL parse error: {0}")]
    Url(#[from] url::ParseError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[cfg(feature = "telegram")]
    #[error("MTProto error: {0}")]
    MtProto(String),

    #[error("{0}")]
    Other(String),
}

impl TelegramError {
    pub fn other(msg: impl std::fmt::Display) -> Self {
        TelegramError::Other(msg.to_string())
    }

    #[cfg(feature = "telegram")]
    pub fn from_invocation(err: grammers_client::InvocationError) -> Self {
        let text = err.to_string();
        if let Some(secs) = parse_flood_wait_secs(&text) {
            return TelegramError::FloodWait {
                retry_after: Duration::from_secs(secs),
            };
        }
        TelegramError::MtProto(text)
    }
}

fn parse_flood_wait_secs(text: &str) -> Option<u64> {
    let needle = "FLOOD_WAIT_";
    let pos = text.find(needle)?;
    let rest = &text[pos + needle.len()..];
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}
