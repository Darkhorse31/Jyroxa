//! Filename suggestion, sanitization, deduplication and category heuristics.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

/// High-level file category for organizing downloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum Category {
    Video,
    Music,
    Documents,
    Compressed,
    Programs,
    Other,
}

const MAX_FILENAME_LEN: usize = 200;

/// Suggest a safe local filename from response metadata and the final URL.
///
/// Priority: `Content-Disposition` `filename*` (RFC 5987) > `filename=` > last
/// URL path segment > `"download"`. Adds an extension from `Content-Type` when
/// the name has none.
pub fn suggest_filename(
    final_url: &str,
    content_disposition: Option<&str>,
    content_type: Option<&str>,
) -> String {
    let mut name = content_disposition
        .and_then(filename_from_disposition)
        .or_else(|| {
            Url::parse(final_url)
                .ok()
                .and_then(|u| filename_from_url_path(&u))
        })
        .unwrap_or_else(|| "download".to_string());

    if !has_extension(&name) {
        if let Some(ext) = content_type.and_then(extension_for_mime) {
            name = format!("{name}.{ext}");
        }
    }

    sanitize_filename(&name)
}

/// Parse `Content-Disposition` for a filename (`filename*` preferred).
pub fn filename_from_disposition(value: &str) -> Option<String> {
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename*=") {
            let rest = rest.trim();
            if let Some((charset, enc)) = parse_filename_star(rest) {
                let decoded = percent_decode_bytes(&enc, charset.as_deref()).ok()?;
                let base = strip_path_components(&decoded);
                if !base.is_empty() {
                    return Some(base);
                }
            }
        }
    }
    for part in value.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix("filename=") {
            let v = rest.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(strip_path_components(v));
            }
        }
    }
    None
}

fn parse_filename_star(rest: &str) -> Option<(Option<String>, String)> {
    // e.g. UTF-8''na%C3%AFve.pdf
    let idx = rest.rfind("''")?;
    let charset = rest[..idx].trim();
    let charset = if charset.is_empty() {
        None
    } else {
        Some(charset.to_ascii_uppercase())
    };
    Some((charset, rest[idx + 2..].to_string()))
}

/// Derive a filename from the last non-empty URL path segment (decoded).
pub fn filename_from_url_path(u: &Url) -> Option<String> {
    let path = u.path();
    let seg = path.rsplit('/').find(|s| !s.is_empty())?;
    let decoded = percent_decode_lossy(seg);
    let base = strip_path_components(&decoded);
    if base.is_empty() {
        None
    } else {
        Some(base)
    }
}

/// Sanitize a filename for Windows: invalid chars, reserved names, length.
pub fn sanitize_filename(name: &str) -> String {
    let base = strip_path_components(name);
    let mut cleaned: String = base
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    while cleaned.ends_with('.') || cleaned.ends_with(' ') {
        cleaned.pop();
    }
    if cleaned.is_empty() {
        cleaned = "download".into();
    }
    if is_reserved_windows_name(&cleaned) {
        cleaned = format!("_{cleaned}");
    }
    if cleaned.len() > MAX_FILENAME_LEN {
        if let Some((stem, ext)) = cleaned.rsplit_once('.') {
            let max_stem = MAX_FILENAME_LEN.saturating_sub(ext.len() + 1);
            cleaned = format!("{}.{}", &stem[..stem.len().min(max_stem)], ext);
        } else {
            cleaned.truncate(MAX_FILENAME_LEN);
        }
    }
    cleaned
}

/// If `dir/name` exists, return `dir/name (1).ext`, `name (2).ext`, etc.
pub fn dedupe_path(dir: impl AsRef<Path>, name: &str) -> PathBuf {
    let dir = dir.as_ref();
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = split_name_ext(name);
    for n in 1..=9999 {
        let tagged = if ext.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}){ext}")
        };
        let path = dir.join(&tagged);
        if !path.exists() {
            return path;
        }
    }
    dir.join(format!("{stem}_dup{ext}"))
}

pub fn category_for(name: &str, mime: Option<&str>) -> Category {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    let mime = mime.unwrap_or("").to_ascii_lowercase();

    let video_ext = [
        "mp4", "mkv", "webm", "avi", "mov", "m4v", "flv", "wmv", "ts", "m2ts",
    ];
    let music_ext = ["mp3", "m4a", "flac", "wav", "aac", "ogg", "opus", "wma"];
    let doc_ext = [
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "csv", "rtf",
        "epub",
    ];
    let arch_ext = [
        "zip", "rar", "7z", "gz", "tgz", "bz2", "xz", "tar", "iso",
    ];
    let prog_ext = ["exe", "msi", "apk", "dmg", "deb", "rpm", "appimage"];

    if mime.starts_with("video/") || video_ext.contains(&ext.as_str()) {
        Category::Video
    } else if mime.starts_with("audio/") || music_ext.contains(&ext.as_str()) {
        Category::Music
    } else if mime.starts_with("application/pdf")
        || mime.contains("document")
        || mime.contains("spreadsheet")
        || mime.contains("presentation")
        || doc_ext.contains(&ext.as_str())
    {
        Category::Documents
    } else if mime.contains("zip")
        || mime.contains("compressed")
        || mime.contains("x-rar")
        || arch_ext.contains(&ext.as_str())
    {
        Category::Compressed
    } else if mime.contains("octet-stream") && (ext == "exe" || ext == "msi" || ext == "apk") {
        Category::Programs
    } else if prog_ext.contains(&ext.as_str()) {
        Category::Programs
    } else {
        Category::Other
    }
}

fn extension_for_mime(ct: &str) -> Option<&'static str> {
    let mime = ct.split(';').next()?.trim().to_ascii_lowercase();
    match mime.as_str() {
        "video/mp4" => Some("mp4"),
        "video/x-matroska" => Some("mkv"),
        "video/webm" => Some("webm"),
        "audio/mpeg" => Some("mp3"),
        "audio/mp4" => Some("m4a"),
        "audio/flac" => Some("flac"),
        "audio/wav" | "audio/x-wav" => Some("wav"),
        "application/zip" => Some("zip"),
        "application/x-rar-compressed" => Some("rar"),
        "application/x-7z-compressed" => Some("7z"),
        "application/gzip" => Some("gz"),
        "application/x-tar" => Some("tar"),
        "application/pdf" => Some("pdf"),
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => Some("docx"),
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => Some("xlsx"),
        "application/vnd.openxmlformats-officedocument.presentationml.presentation" => Some("pptx"),
        "text/plain" => Some("txt"),
        "application/vnd.android.package-archive" => Some("apk"),
        "application/x-msdownload" => Some("exe"),
        "application/x-msi" => Some("msi"),
        "application/x-iso9660-image" => Some("iso"),
        "image/jpeg" => Some("jpg"),
        "image/png" => Some("png"),
        "image/gif" => Some("gif"),
        "image/webp" => Some("webp"),
        "application/octet-stream" => None,
        _ if mime.starts_with("video/") => Some("mp4"),
        _ if mime.starts_with("audio/") => Some("mp3"),
        _ if mime.starts_with("image/") => Some("png"),
        _ => None,
    }
}

fn has_extension(name: &str) -> bool {
    let base = strip_path_components(name);
    base.rsplit_once('.').is_some_and(|(stem, ext)| !stem.is_empty() && !ext.is_empty())
}

fn split_name_ext(name: &str) -> (String, String) {
    let base = strip_path_components(name);
    match base.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => {
            (stem.to_string(), format!(".{ext}"))
        }
        _ => (base, String::new()),
    }
}

fn strip_path_components(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .trim()
        .trim_matches('.')
        .to_string()
}

fn percent_decode_lossy(s: &str) -> String {
    percent_decode_bytes(s, None).unwrap_or_else(|_| s.to_string())
}

fn percent_decode_bytes(s: &str, _charset: Option<&str>) -> Result<String, ()> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16).ok_or(())?;
                let lo = (bytes[i + 2] as char).to_digit(16).ok_or(())?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

fn is_reserved_windows_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let stem = upper.rsplit_once('.').map(|(s, _)| s).unwrap_or(&upper);
    matches!(
        stem,
        "CON" | "PRN" | "AUX" | "NUL"
            | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6" | "COM7" | "COM8" | "COM9"
            | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6" | "LPT7" | "LPT8" | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc5987_beats_plain_filename() {
        let cd = r#"attachment; filename="fallback.bin"; filename*=UTF-8''na%C3%AFve.pdf"#;
        let name = suggest_filename("https://h.test/d", Some(cd), None);
        assert_eq!(name, "naïve.pdf");
    }

    #[test]
    fn disposition_quoted_and_unquoted() {
        assert_eq!(
            filename_from_disposition(r#"attachment; filename="my file.zip""#),
            Some("my file.zip".to_string())
        );
        assert_eq!(
            filename_from_disposition("attachment; filename=plain.zip"),
            Some("plain.zip".to_string())
        );
    }

    #[test]
    fn url_path_decoded_no_query() {
        let u = Url::parse("https://host.test/dir/hello%20world.bin?x=1#frag").unwrap();
        assert_eq!(filename_from_url_path(&u).as_deref(), Some("hello world.bin"));
    }

    #[test]
    fn mime_adds_extension_when_missing() {
        let name = suggest_filename("https://h.test/file", None, Some("video/mp4"));
        assert_eq!(name, "file.mp4");
    }

    #[test]
    fn octet_stream_does_not_add_extension() {
        let name = suggest_filename("https://h.test/data", None, Some("application/octet-stream"));
        assert_eq!(name, "data");
    }

    #[test]
    fn sanitizes_invalid_chars_and_reserved() {
        let name = format!("foo<>:bar{}.txt", '?');
        assert_eq!(sanitize_filename(&name), "foo___bar_.txt");
        assert_eq!(sanitize_filename("CON.txt"), "_CON.txt");
        assert_eq!(sanitize_filename("name..."), "name");
    }

    #[test]
    fn dedupe_path_increments() {
        let dir = tempfile::tempdir().unwrap();
        let p0 = dir.path().join("a.zip");
        std::fs::write(&p0, b"x").unwrap();
        let p1 = dedupe_path(dir.path(), "a.zip");
        assert_eq!(p1.file_name().unwrap().to_str().unwrap(), "a (1).zip");
    }

    #[test]
    fn categories() {
        assert_eq!(category_for("clip.mkv", None), Category::Video);
        assert_eq!(category_for("song.mp3", None), Category::Music);
        assert_eq!(category_for("doc.pdf", None), Category::Documents);
        assert_eq!(category_for("arc.7z", None), Category::Compressed);
        assert_eq!(category_for("setup.exe", None), Category::Programs);
    }
}
