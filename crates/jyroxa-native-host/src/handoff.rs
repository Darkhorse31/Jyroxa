//! Browser → native host → desktop handoff messages.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExtensionMessage {
    Ping,
    Handoff(BrowserHandoff),
    GetSettings,
    SetSilentAdd { silent_add: bool },
    GrabLinks { html: String, page_url: String },
    DownloadUrl(BrowserHandoff),
    ProbeVideoFormats { page_url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BrowserHandoff {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub referrer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookies: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    /// When true, `url` is a watch/page URL and the desktop app should resolve formats (yt-dlp).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_page: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipeEnvelope {
    pub token: String,
    #[serde(flatten)]
    pub message: PipeMessage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PipeMessage {
    Handoff(BrowserHandoff),
    Ping,
    GetSettings,
    SetSilentAdd { silent_add: bool },
    GrabLinks { html: String, page_url: String },
    ProbeVideoFormats { page_url: String },
}

#[derive(Debug, Error)]
pub enum HandoffError {
    #[error("missing url")]
    MissingUrl,
    #[error("{0}")]
    Json(#[from] serde_json::Error),
}

impl BrowserHandoff {
    pub fn validate(&self) -> Result<(), HandoffError> {
        let u = self.url.trim();
        let valid = url::Url::parse(u)
            .ok()
            .is_some_and(|parsed| match parsed.scheme() {
                "http" | "https" => parsed.host_str().is_some(),
                "magnet" => parsed.query_pairs().any(|(key, value)| {
                    key == "xt"
                        && value.strip_prefix("urn:btih:").is_some_and(|hash| {
                            hash.len() == 40 && hash.bytes().all(|b| b.is_ascii_hexdigit())
                        })
                }),
                "file" => parsed.to_file_path().ok().is_some_and(|path| {
                    path.is_file()
                        && path
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("torrent"))
                }),
                _ => false,
            });
        if !valid {
            return Err(HandoffError::MissingUrl);
        }
        Ok(())
    }
}

pub fn parse_extension_message(raw: &serde_json::Value) -> Result<ExtensionMessage, HandoffError> {
    let msg: ExtensionMessage = serde_json::from_value(raw.clone())?;
    if let ExtensionMessage::Handoff(ref h) | ExtensionMessage::DownloadUrl(ref h) = msg {
        h.validate()?;
    }
    Ok(msg)
}

pub fn handoff_to_headers(h: &BrowserHandoff) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(ua) = h.user_agent.as_ref().filter(|s| !s.is_empty()) {
        out.push(("User-Agent".into(), ua.clone()));
    }
    if let Some(r) = h.referrer.as_ref().filter(|s| !s.is_empty()) {
        out.push(("Referer".into(), r.clone()));
    }
    if let Some(c) = h.cookies.as_ref().filter(|s| !s.is_empty()) {
        out.push(("Cookie".into(), c.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_handoff_json() {
        let raw = serde_json::json!({
            "type": "handoff",
            "url": "https://cdn.example/file.zip",
            "referrer": "https://example/",
            "cookies": "a=1",
            "user_agent": "Mozilla/5.0",
            "filename": "file.zip"
        });
        let msg = parse_extension_message(&raw).unwrap();
        match msg {
            ExtensionMessage::Handoff(h) => {
                assert_eq!(h.url, "https://cdn.example/file.zip");
                assert_eq!(h.filename.as_deref(), Some("file.zip"));
            }
            _ => panic!("expected handoff"),
        }
    }

    #[test]
    fn rejects_bad_url() {
        let raw = serde_json::json!({ "type": "handoff", "url": "ftp://x" });
        assert!(parse_extension_message(&raw).is_err());
    }

    #[test]
    fn accepts_magnet_handoff_and_rejects_invalid_hash() {
        let magnet = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567";
        assert!(
            parse_extension_message(&serde_json::json!({ "type": "handoff", "url": magnet }))
                .is_ok()
        );
        assert!(parse_extension_message(
            &serde_json::json!({ "type": "handoff", "url": "magnet:?xt=urn:btih:invalid" })
        )
        .is_err());
    }

    #[test]
    fn accepts_existing_torrent_file_handoff() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample + test.torrent");
        std::fs::write(&path, b"fixture").unwrap();
        let file = url::Url::from_file_path(&path).unwrap();
        assert!(parse_extension_message(
            &serde_json::json!({ "type": "handoff", "url": file.as_str() })
        )
        .is_ok());
        std::fs::remove_file(path).unwrap();
        assert!(parse_extension_message(
            &serde_json::json!({ "type": "handoff", "url": file.as_str() })
        )
        .is_err());
    }

    #[test]
    fn pipe_envelope_round_trip() {
        let env = PipeEnvelope {
            token: "secret".into(),
            message: PipeMessage::Handoff(BrowserHandoff {
                url: "https://x/y".into(),
                referrer: None,
                cookies: None,
                user_agent: None,
                filename: Some("y".into()),
                page_url: None,
                video_page: None,
                format_id: None,
            }),
        };
        let text = serde_json::to_string(&env).unwrap();
        let back: PipeEnvelope = serde_json::from_str(&text).unwrap();
        assert_eq!(back, env);
    }
}
