//! Parse Telegram post URLs (public and private).

use regex::Regex;
use std::sync::OnceLock;
use url::Url;

use super::error::{Result, TelegramError};
use super::TelegramPostRef;

fn tme_host(host: &str) -> bool {
    let h = host.to_ascii_lowercase();
    h == "t.me" || h == "telegram.me" || h == "www.t.me" || h == "www.telegram.me"
}

fn message_id_from_path(seg: &str) -> Option<u32> {
    seg.parse().ok().filter(|&id: &u32| id > 0)
}

/// Parse a user-pasted Telegram link into a [`TelegramPostRef`].
pub fn parse_telegram_link(input: &str) -> Result<TelegramPostRef> {
    let trimmed = input.trim();
    let url = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        Url::parse(trimmed)?
    } else if trimmed.starts_with("t.me/") || trimmed.starts_with("telegram.me/") {
        Url::parse(&format!("https://{trimmed}"))?
    } else {
        return Err(TelegramError::InvalidLink(trimmed.to_string()));
    };

    let host = url.host_str().ok_or_else(|| TelegramError::InvalidLink(trimmed.to_string()))?;
    if !tme_host(host) {
        return Err(TelegramError::InvalidLink(trimmed.to_string()));
    }

    let segments: Vec<&str> = url
        .path()
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let canonical = canonical_public_url(&url, &segments);

    match segments.as_slice() {
        ["c", chat, msg] => {
            let chat_id = parse_private_chat_id(chat)?;
            let message_id = message_id_from_path(msg)
                .ok_or_else(|| TelegramError::InvalidLink(trimmed.to_string()))?;
            Ok(TelegramPostRef::Private {
                chat_id,
                message_id,
                raw_url: trimmed.to_string(),
            })
        }
        ["s", channel, msg] => {
            let message_id = message_id_from_path(msg)
                .ok_or_else(|| TelegramError::InvalidLink(trimmed.to_string()))?;
            Ok(TelegramPostRef::Public {
                channel: channel.to_string(),
                message_id,
                raw_url: trimmed.to_string(),
                embed_url: format!("https://t.me/{channel}/{message_id}?embed=1&mode=tme"),
            })
        }
        [channel, msg] if *channel != "+" && !channel.starts_with('+') => {
            let message_id = message_id_from_path(msg)
                .ok_or_else(|| TelegramError::InvalidLink(trimmed.to_string()))?;
            Ok(TelegramPostRef::Public {
                channel: channel.to_string(),
                message_id,
                raw_url: canonical.unwrap_or_else(|| trimmed.to_string()),
                embed_url: format!("https://t.me/{channel}/{message_id}?embed=1&mode=tme"),
            })
        }
        _ => Err(TelegramError::InvalidLink(trimmed.to_string())),
    }
}

fn parse_private_chat_id(chat: &str) -> Result<i64> {
    let id: i64 = chat
        .parse()
        .map_err(|_| TelegramError::InvalidLink(format!("bad private chat id: {chat}")))?;
    // t.me/c/ links use the channel id without the -100 prefix.
    Ok(-(1_000_000_000_000 + id))
}

fn canonical_public_url(url: &Url, segments: &[&str]) -> Option<String> {
    match segments {
        ["s", channel, msg] | [channel, msg] if message_id_from_path(msg).is_some() => {
            Some(format!(
                "https://t.me/{}/{}",
                channel.trim_start_matches('@'),
                msg
            ))
        }
        _ => Some(url.to_string()),
    }
}

static TME_URL_RE: OnceLock<Regex> = OnceLock::new();

/// Return every Telegram post URL found in free text.
pub fn extract_telegram_urls(text: &str) -> Vec<String> {
    let re = TME_URL_RE.get_or_init(|| {
        Regex::new(r#"(?i)https?://(?:t\.me|telegram\.me)/[^\s<>"']+"#).unwrap()
    });
    let mut out = Vec::new();
    for cap in re.find_iter(text) {
        let mut s = cap.as_str().trim_end_matches(|c: char| {
            matches!(c, '.' | ',' | ';' | ')' | ']' | '}')
        });
        out.push(s.to_string());
    }
    let bare = Regex::new(r#"(?i)(?:^|[\s(])t\.me/[^\s<>"']+"#).unwrap();
    for cap in bare.find_iter(text) {
        let slice = cap.as_str().trim_start_matches(|c: char| c.is_whitespace() || c == '(');
        let mut s = slice.trim_end_matches(|c: char| {
            matches!(c, '.' | ',' | ';' | ')' | ']' | '}')
        });
        let url = format!("https://{s}");
        if !out.contains(&url) {
            out.push(url);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// True when `url` looks like a Telegram post link (not a CDN media URL).
pub fn is_telegram_post_url(url: &str) -> bool {
    parse_telegram_link(url).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_channel_link() {
        let p = parse_telegram_link("https://t.me/telegram/193").unwrap();
        match p {
            TelegramPostRef::Public {
                channel,
                message_id,
                embed_url,
                ..
            } => {
                assert_eq!(channel, "telegram");
                assert_eq!(message_id, 193);
                assert!(embed_url.contains("embed=1"));
            }
            _ => panic!("expected public"),
        }
    }

    #[test]
    fn public_s_prefix() {
        let p = parse_telegram_link("https://t.me/s/durov/100").unwrap();
        assert!(matches!(p, TelegramPostRef::Public { .. }));
    }

    #[test]
    fn telegram_me_host() {
        let p = parse_telegram_link("https://telegram.me/telegram/1").unwrap();
        assert!(matches!(p, TelegramPostRef::Public { .. }));
    }

    #[test]
    fn private_c_link() {
        let p = parse_telegram_link("https://t.me/c/1234567890/42").unwrap();
        match p {
            TelegramPostRef::Private { chat_id, message_id, .. } => {
                assert_eq!(chat_id, -1_001_234_567_890);
                assert_eq!(message_id, 42);
            }
            _ => panic!("expected private"),
        }
    }

    #[test]
    fn extract_from_text() {
        let urls = extract_telegram_urls("see https://t.me/telegram/193 and also t.me/durov/5 here");
        assert_eq!(urls.len(), 2);
    }
}
