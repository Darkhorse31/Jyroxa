//! Magnet URI parsing and classification helpers.

use crate::error::{Error, Result};
use url::Url;

/// True when `s` is a magnet link with a BTIH info hash.
pub fn is_magnet_uri(s: &str) -> bool {
    let Ok(url) = Url::parse(s.trim()) else {
        return false;
    };
    if url.scheme() != "magnet" {
        return false;
    }
    parse_info_hash_hex(s).is_ok()
}

/// True for magnets, local `.torrent` paths, or HTTP(S) URLs ending in `.torrent`.
pub fn is_torrent_source(s: &str) -> bool {
    let t = s.trim();
    if is_magnet_uri(t) {
        return true;
    }
    if t.len() == 40 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return true;
    }
    if t.starts_with("http://") || t.starts_with("https://") {
        return t.to_ascii_lowercase().ends_with(".torrent");
    }
    let path = std::path::Path::new(t);
    if path.extension().and_then(|e| e.to_str()) == Some("torrent") {
        return path.is_file();
    }
    false
}

/// Extract trackers (`tr=`) from a magnet URI.
pub fn magnet_trackers(magnet: &str) -> Vec<String> {
    let Ok(url) = Url::parse(magnet.trim()) else {
        return Vec::new();
    };
    url.query_pairs()
        .filter(|(k, _)| k == "tr" || k == "tr")
        .map(|(_, v)| v.into_owned())
        .collect()
}

/// Display name from `dn=` when present.
pub fn magnet_display_name(magnet: &str) -> Option<String> {
    let url = Url::parse(magnet.trim()).ok()?;
    url.query_pairs()
        .find(|(k, _)| k == "dn")
        .map(|(_, v)| v.into_owned())
}

/// Return the 40-character lowercase hex BTIH from a magnet or bare hash.
pub fn parse_info_hash_hex(input: &str) -> Result<String> {
    let t = input.trim();
    if t.len() == 40 && t.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(t.to_ascii_lowercase());
    }
    let url = Url::parse(t).map_err(|e| Error::other(format!("invalid magnet: {e}")))?;
    if url.scheme() != "magnet" {
        return Err(Error::other("expected magnet URI or 40-char info hash"));
    }
    for (key, value) in url.query_pairs() {
        if key != "xt" {
            continue;
        }
        let v = value.as_ref();
        let prefix = "urn:btih:";
        let hash_part = if let Some(rest) = v.strip_prefix(prefix) {
            rest
        } else if let Some(rest) = v.strip_prefix("urn:btih:") {
            rest
        } else {
            continue;
        };
        let hex = normalize_btih(hash_part)?;
        return Ok(hex);
    }
    Err(Error::other("magnet missing xt=urn:btih"))
}

fn normalize_btih(raw: &str) -> Result<String> {
    let h = raw.trim();
    if h.len() == 40 && h.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Ok(h.to_ascii_lowercase());
    }
    if h.len() == 32 {
        // base32 without padding — librqbit_core can decode; expose hex via decode if needed.
        return Err(Error::other(
            "base32 btih: use librqbit AddTorrent::from_url instead",
        ));
    }
    Err(Error::other("unrecognized btih encoding"))
}

/// Rebuild a magnet with an expanded tracker list (preserves other params).
pub fn magnet_with_trackers(magnet: &str, trackers: &[String]) -> Result<String> {
    let mut url = Url::parse(magnet.trim()).map_err(|e| Error::other(format!("{e}")))?;
    if url.scheme() != "magnet" {
        return Err(Error::other("not a magnet URI"));
    }
    let existing: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| k.as_ref() != "tr")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut pairs: Vec<(String, String)> = existing;
    for tr in trackers {
        pairs.push(("tr".into(), tr.clone()));
    }
    let qs: String = pairs
        .iter()
        .map(|(k, v)| format!("{}={}", urlencoding_pair(k), urlencoding_pair(v)))
        .collect::<Vec<_>>()
        .join("&");
    url.set_query(Some(&qs));
    Ok(url.into())
}

fn urlencoding_pair(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&dn=ubuntu";

    #[test]
    fn parse_hex_btih() {
        let h = parse_info_hash_hex(SAMPLE).unwrap();
        assert_eq!(h, "0123456789abcdef0123456789abcdef01234567");
    }

    #[test]
    fn bare_hash_ok() {
        let h = "0123456789abcdef0123456789abcdef01234567";
        assert!(parse_info_hash_hex(h).is_ok());
    }

    #[test]
    fn is_torrent_source_cases() {
        assert!(is_magnet_uri(SAMPLE));
        assert!(is_torrent_source(SAMPLE));
        assert!(is_torrent_source(
            "https://releases.ubuntu.com/22.04/ubuntu-22.04.3-desktop-amd64.iso.torrent"
        ));
        assert!(!is_torrent_source("https://example.com/file.zip"));
    }

    #[test]
    fn magnet_trackers_roundtrip() {
        let m = "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567&tr=udp%3A%2F%2Ftracker.example%3A6969";
        let tr = magnet_trackers(m);
        assert_eq!(tr.len(), 1);
        assert!(tr[0].contains("tracker.example"));
    }
}
