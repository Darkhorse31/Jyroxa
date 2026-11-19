//! Minimal HLS (M3U8) playlist parser.

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HlsVariant {
    pub uri: String,
    pub bandwidth: Option<u64>,
    pub resolution: Option<String>,
    pub codecs: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HlsSegment {
    pub uri: String,
    pub duration: f64,
    pub byte_range: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HlsPlaylist {
    Master { variants: Vec<HlsVariant> },
    Media {
        segments: Vec<HlsSegment>,
        init_map: Option<String>,
        target_duration: Option<u64>,
    },
}

fn parse_attrs(line: &str) -> Vec<(String, String)> {
    let inner = line.trim().trim_start_matches('#').split_once(':');
    let Some((_, rest)) = inner else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut key = String::new();
    let mut val = String::new();
    let mut in_val = false;
    let mut quote = None;
    for ch in rest.chars() {
        if quote.is_some() {
            if ch == quote.unwrap() {
                quote = None;
                in_val = false;
                out.push((key.trim().to_string(), val.trim().to_string()));
                key.clear();
                val.clear();
            } else {
                val.push(ch);
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            in_val = true;
            continue;
        }
        if ch == '=' && !in_val {
            in_val = true;
            continue;
        }
        if ch == ',' && !in_val {
            if !key.is_empty() {
                out.push((key.trim().to_string(), val.trim().to_string()));
            }
            key.clear();
            val.clear();
            continue;
        }
        if in_val {
            val.push(ch);
        } else {
            key.push(ch);
        }
    }
    if !key.is_empty() {
        out.push((key.trim().to_string(), val.trim().to_string()));
    }
    out
}

fn attr_u64(attrs: &[(String, String)], name: &str) -> Option<u64> {
    attrs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.parse().ok())
}

fn attr_str<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

fn split_stream_attrs(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    for ch in s.chars() {
        if ch == '"' {
            in_quote = !in_quote;
            cur.push(ch);
            continue;
        }
        if ch == ',' && !in_quote {
            if !cur.trim().is_empty() {
                parts.push(cur.trim().to_string());
            }
            cur.clear();
            continue;
        }
        cur.push(ch);
    }
    if !cur.trim().is_empty() {
        parts.push(cur.trim().to_string());
    }
    parts
}

fn parse_stream_inf(line: &str) -> Vec<(String, String)> {
    let rest = line
        .trim()
        .strip_prefix("#EXT-X-STREAM-INF:")
        .or_else(|| line.split_once(':').map(|(_, r)| r))
        .unwrap_or("");
    let mut attrs = Vec::new();
    for part in split_stream_attrs(rest) {
        let Some((k, v)) = part.split_once('=') else {
            continue;
        };
        let mut val = v.trim().to_string();
        if val.starts_with('"') && val.ends_with('"') && val.len() >= 2 {
            val = val[1..val.len() - 1].to_string();
        }
        attrs.push((k.trim().to_string(), val));
    }
    attrs
}

pub fn resolve_uri(base: &str, uri: &str) -> String {
    if uri.starts_with("http://") || uri.starts_with("https://") {
        return uri.to_string();
    }
    Url::parse(base)
        .ok()
        .and_then(|u| u.join(uri).ok())
        .map(|u| u.to_string())
        .unwrap_or_else(|| uri.to_string())
}

/// Parse M3U8 text. `base_url` resolves relative segment and variant URIs.
pub fn parse_m3u8(text: &str, base_url: &str) -> Result<HlsPlaylist, String> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    if lines.first().map(|l| *l != "#EXTM3U").unwrap_or(true) {
        return Err("not an m3u8 playlist".into());
    }

    let mut variants = Vec::new();
    let mut segments = Vec::new();
    let mut pending_variant_attrs: Option<Vec<(String, String)>> = None;
    let mut pending_duration: Option<f64> = None;
    let mut init_map: Option<String> = None;
    let mut target_duration: Option<u64> = None;
    let mut is_master = false;

    for line in &lines {
        if line.starts_with("#EXT-X-STREAM-INF:") {
            is_master = true;
            pending_variant_attrs = Some(parse_stream_inf(line));
        } else if line.starts_with("#EXTINF:") {
            let dur = line
                .trim_start_matches("#EXTINF:")
                .split(',')
                .next()
                .and_then(|s| s.trim().parse().ok())
                .unwrap_or(0.0);
            pending_duration = Some(dur);
        } else if line.starts_with("#EXT-X-TARGETDURATION:") {
            target_duration = line
                .trim_start_matches("#EXT-X-TARGETDURATION:")
                .trim()
                .parse()
                .ok();
        } else if line.starts_with("#EXT-X-MAP:") {
            let attrs = parse_attrs(line);
            if let Some(u) = attr_str(&attrs, "URI") {
                init_map = Some(resolve_uri(base_url, u));
            }
        } else if !line.starts_with('#') {
            if let Some(attrs) = pending_variant_attrs.take() {
                variants.push(HlsVariant {
                    uri: resolve_uri(base_url, line),
                    bandwidth: attr_u64(&attrs, "BANDWIDTH"),
                    resolution: attr_str(&attrs, "RESOLUTION").map(str::to_string),
                    codecs: attr_str(&attrs, "CODECS").map(str::to_string),
                });
            } else {
                let dur = pending_duration.take().unwrap_or(0.0);
                segments.push(HlsSegment {
                    uri: resolve_uri(base_url, line),
                    duration: dur,
                    byte_range: None,
                });
            }
        }
    }

    if is_master && !variants.is_empty() {
        return Ok(HlsPlaylist::Master { variants });
    }
    if segments.is_empty() {
        return Err("no segments in media playlist".into());
    }
    Ok(HlsPlaylist::Media {
        segments,
        init_map,
        target_duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_master_playlist() {
        let text = r#"#EXTM3U
#EXT-X-STREAM-INF:BANDWIDTH=1280000,RESOLUTION=854x480,CODECS="avc1.4d001f,mp4a.40.2"
480p.m3u8
#EXT-X-STREAM-INF:BANDWIDTH=2560000,RESOLUTION=1280x720
720p.m3u8
"#;
        let pl = parse_m3u8(text, "https://cdn.example/vid/master.m3u8").unwrap();
        match pl {
            HlsPlaylist::Master { variants } => {
                assert_eq!(variants.len(), 2);
                assert_eq!(variants[0].bandwidth, Some(1_280_000));
                assert_eq!(variants[0].resolution.as_deref(), Some("854x480"));
                assert!(variants[0].uri.ends_with("480p.m3u8"));
            }
            _ => panic!("expected master"),
        }
    }

    #[test]
    fn parses_media_playlist() {
        let text = r#"#EXTM3U
#EXT-X-TARGETDURATION:5
#EXTINF:4.0,
seg0.ts
#EXTINF:4.0,
seg1.ts
"#;
        let pl = parse_m3u8(text, "https://cdn.example/vid/playlist.m3u8").unwrap();
        match pl {
            HlsPlaylist::Media { segments, .. } => {
                assert_eq!(segments.len(), 2);
                assert!(segments[0].uri.contains("seg0.ts"));
            }
            _ => panic!("expected media"),
        }
    }
}