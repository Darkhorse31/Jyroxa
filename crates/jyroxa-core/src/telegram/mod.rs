//! Telegram post resolver and optional MTProto downloader (`telegram` feature).

mod embed;
mod error;
mod link;

#[cfg(feature = "telegram")]
mod mtproto;
#[cfg(feature = "telegram")]
mod session;

pub use embed::{fetch_embed_media, parse_embed_html};
pub use error::{Result as TelegramResult, TelegramError};
pub use link::{extract_telegram_urls, is_telegram_post_url, parse_telegram_link};

#[cfg(feature = "telegram")]
pub use mtproto::{
    TelegramAccountView, TelegramAuthState, TelegramClientPool, TelegramLoginStep,
    download_private_media, resolve_private_media,
};
#[cfg(feature = "telegram")]
pub use session::SessionStore;

use serde::{Deserialize, Serialize};

/// Reference to a Telegram channel post or private message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TelegramPostRef {
    Public {
        channel: String,
        message_id: u32,
        raw_url: String,
        embed_url: String,
    },
    Private {
        chat_id: i64,
        message_id: u32,
        raw_url: String,
    },
}

/// One file resolved from a post (public CDN URL or private MTProto target).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelegramMediaItem {
    /// Direct HTTP URL for public posts; empty when [`Self::needs_login`].
    pub download_url: String,
    pub filename: String,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub source_post_url: String,
    pub resumable: bool,
    #[serde(default)]
    pub needs_login: bool,
    #[serde(default)]
    pub private_chat_id: Option<i64>,
    #[serde(default)]
    pub private_message_id: Option<u32>,
}

impl TelegramMediaItem {
    pub fn to_link_info(&self) -> crate::linkgrabber::LinkInfo {
        crate::linkgrabber::LinkInfo {
            url: if self.needs_login {
                self.source_post_url.clone()
            } else {
                self.download_url.clone()
            },
            filename: Some(self.filename.clone()),
            size: self.size,
            mime: self.mime.clone(),
            resumable: self.resumable,
        }
    }
}

/// Resolve a Telegram post URL into zero or more downloadable media items.
pub async fn resolve_post(
    client: &reqwest::Client,
    input_url: &str,
) -> TelegramResult<Vec<TelegramMediaItem>> {
    let post = parse_telegram_link(input_url)?;
    match post {
        TelegramPostRef::Public {
            embed_url,
            raw_url,
            ..
        } => fetch_embed_media(client, &embed_url, &raw_url).await,
        TelegramPostRef::Private {
            chat_id,
            message_id,
            raw_url,
        } => {
            #[cfg(feature = "telegram")]
            {
                let stub = TelegramMediaItem {
                    download_url: String::new(),
                    filename: format!("telegram_{message_id}"),
                    size: None,
                    mime: None,
                    source_post_url: raw_url,
                    resumable: true,
                    needs_login: true,
                    private_chat_id: Some(chat_id),
                    private_message_id: Some(message_id),
                };
                Ok(vec![stub])
            }
            #[cfg(not(feature = "telegram"))]
            {
                let _ = (chat_id, message_id, raw_url);
                Err(TelegramError::LoginRequired)
            }
        }
    }
}

/// Probe size/filename for a Telegram post without starting a download.
pub async fn probe_post(client: &reqwest::Client, url: &str) -> TelegramResult<TelegramMediaItem> {
    let items = resolve_post(client, url).await?;
    items
        .into_iter()
        .next()
        .ok_or(TelegramError::NoMedia)
}
