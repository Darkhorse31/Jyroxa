//! Parse Telegram public embed widget HTML.

use regex::Regex;
use std::sync::OnceLock;
use url::Url;

use super::error::{Result, TelegramError};
use super::TelegramMediaItem;

fn video_src_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<video[^>]+src="([^"]+)""#).unwrap()
    })
}

fn document_block_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?is)class="tgme_widget_message_document[^"]*"[^>]*>.*?href="([^"]+)"[^>]*>.*?class="[^"]*document_title[^"]*"[^>]*>([^<]+)"#,
        )
        .unwrap()
    })
}

fn document_size_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"(?i)class="[^"]*document_size[^"]*"[^>]*>([^<]+)"#).unwrap())
}

fn photo_link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?is)class="tgme_widget_message_photo_wrap[^"]*"[^>]*style="[^"]*background-image:\s*url\('([^']+)'\)"#,
        )
        .unwrap()
    })
}

fn grouped_wrap_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?is)<div class="tgme_widget_message_grouped_wrap[^"]*">(.*?)</div>\s*<div class="tgme_widget_message_footer"#)
            .unwrap()
    })
}

/// Extract downloadable media from a Telegram embed page body.
pub fn parse_embed_html(html: &str, source_post_url: &str) -> Result<Vec<TelegramMediaItem>> {
    if html.contains("tgme_widget_message_error") || html.contains("This channel is private") {
        return Err(TelegramError::Private);
    }
    if html.contains("Post not found") {
        return Err(TelegramError::NotFound);
    }

    let mut items = Vec::new();

    if let Some(cap) = grouped_wrap_re().captures(html) {
        let group_html = cap.get(1).map(|m| m.as_str()).unwrap_or("");
        items.extend(parse_media_chunks(group_html, source_post_url)?);
    } else {
        items.extend(parse_media_chunks(html, source_post_url)?);
    }

    if items.is_empty() {
        return Err(TelegramError::NoMedia);
    }
    Ok(items)
}

fn parse_media_chunks(html: &str, source_post_url: &str) -> Result<Vec<TelegramMediaItem>> {
    let mut items = Vec::new();

    for cap in video_src_re().captures_iter(html) {
        let src = normalize_embed_url(cap.get(1).map(|m| m.as_str()).unwrap_or(""));
        let filename = filename_from_cdn_url(&src).unwrap_or_else(|| "video.mp4".into());
        items.push(TelegramMediaItem {
            download_url: src,
            filename,
            size: None,
            mime: Some("video/mp4".into()),
            source_post_url: source_post_url.to_string(),
            resumable: true,
            needs_login: false,
            private_chat_id: None,
            private_message_id: None,
        });
    }

    for cap in document_block_re().captures_iter(html) {
        let href = normalize_embed_url(cap.get(1).map(|m| m.as_str()).unwrap_or(""));
        let title = cap
            .get(2)
            .map(|m| decode_html_entities(m.as_str().trim()))
            .unwrap_or_else(|| "document".into());
        let size = document_size_re()
            .captures(cap.get(0).map(|m| m.as_str()).unwrap_or(""))
            .and_then(|c| c.get(1))
            .and_then(|m| parse_human_size(m.as_str()));
        items.push(TelegramMediaItem {
            download_url: href,
            filename: sanitize_filename(&title),
            size,
            mime: None,
            source_post_url: source_post_url.to_string(),
            resumable: true,
            needs_login: false,
            private_chat_id: None,
            private_message_id: None,
        });
    }

    for cap in photo_link_re().captures_iter(html) {
        let src = normalize_embed_url(cap.get(1).map(|m| m.as_str()).unwrap_or(""));
        let filename = filename_from_cdn_url(&src).unwrap_or_else(|| "photo.jpg".into());
        items.push(TelegramMediaItem {
            download_url: src,
            filename,
            size: None,
            mime: Some("image/jpeg".into()),
            source_post_url: source_post_url.to_string(),
            resumable: true,
            needs_login: false,
            private_chat_id: None,
            private_message_id: None,
        });
    }

    Ok(items)
}

pub async fn fetch_embed_media(
    client: &reqwest::Client,
    embed_url: &str,
    source_post_url: &str,
) -> Result<Vec<TelegramMediaItem>> {
    let resp = client
        .get(embed_url)
        .header(
            reqwest::header::USER_AGENT,
            "Mozilla/5.0 (compatible; Jyroxa/1.0)",
        )
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(TelegramError::NotFound);
    }
    if !resp.status().is_success() {
        return Err(TelegramError::other(format!(
            "embed page HTTP {}",
            resp.status()
        )));
    }
    let body = resp.text().await?;
    let mut items = parse_embed_html(&body, source_post_url)?;
    enrich_sizes(client, &mut items).await;
    Ok(items)
}

async fn enrich_sizes(client: &reqwest::Client, items: &mut [TelegramMediaItem]) {
    for item in items.iter_mut() {
        if item.size.is_some() {
            continue;
        }
        if let Ok(resp) = client
            .head(&item.download_url)
            .send()
            .await
        {
            if let Some(len) = resp.content_length() {
                item.size = Some(len);
            }
        }
    }
}

fn normalize_embed_url(raw: &str) -> String {
    if raw.starts_with("//") {
        format!("https:{raw}")
    } else if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else {
        format!("https://t.me{raw}")
    }
}

fn filename_from_cdn_url(url: &str) -> Option<String> {
    let parsed = Url::parse(url).ok()?;
    let path = parsed.path();
    path.rsplit('/').next().map(|s| s.to_string())
}

fn sanitize_filename(name: &str) -> String {
    let bad = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let mut out: String = name
        .chars()
        .map(|c| if bad.contains(&c) { '_' } else { c })
        .collect();
    if out.is_empty() {
        out = "document".into();
    }
    out
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn parse_human_size(text: &str) -> Option<u64> {
    let t = text.trim().to_uppercase();
    let parts: Vec<&str> = t.split_whitespace().collect();
    if parts.len() != 2 {
        return None;
    }
    let val: f64 = parts[0].replace(',', ".").parse().ok()?;
    let mult = match parts[1] {
        "B" => 1.0,
        "KB" => 1024.0,
        "MB" => 1024.0 * 1024.0,
        "GB" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((val * mult) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parse_fixture_video() {
        let html = fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/telegram_embed_video.html"),
        )
        .expect("fixture");
        let items = parse_embed_html(&html, "https://t.me/telegram/193").unwrap();
        assert!(!items.is_empty());
        assert!(items[0].download_url.contains("telesco.pe") || items[0].download_url.contains(".mp4"));
        assert!(items[0].filename.ends_with(".mp4") || items[0].filename.contains("mp4"));
    }
}
