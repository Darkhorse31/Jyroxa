//! JDownloader-style LinkGrabber.
//!
//! Given raw text, clipboard contents or a page URL, extract candidate links
//! (anchors, media sources, Open Graph tags and bare file URLs), resolve
//! relative URLs, dedupe, optionally crawl one level deep, probe each link
//! concurrently for filename/size/mime/resumability, group into packages and
//! apply filters before handing results to the queue.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use futures::stream::{self, StreamExt};
use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::download::{filename_from_disposition, filename_from_url};
use crate::error::Result;

/// Common downloadable file extensions used by the bare-URL and filter logic.
pub const COMMON_EXTENSIONS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso", "exe", "msi", "dmg", "pkg", "deb",
    "rpm", "apk", "appimage", "bin", "img", "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v",
    "mp3", "flac", "wav", "aac", "ogg", "opus", "m4a", "pdf", "epub", "mobi", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "csv", "jpg", "jpeg", "png", "gif", "webp", "svg", "torrent",
];

/// Metadata discovered for a single link.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LinkInfo {
    pub url: String,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub resumable: bool,
}

impl LinkInfo {
    fn bare(url: String) -> Self {
        LinkInfo {
            url,
            filename: None,
            size: None,
            mime: None,
            resumable: false,
        }
    }

    /// The file extension (lowercase) implied by the filename or URL.
    pub fn extension(&self) -> Option<String> {
        let name = self
            .filename
            .clone()
            .or_else(|| Url::parse(&self.url).ok().and_then(|u| filename_from_url(&u)))?;
        name.rsplit_once('.').map(|(_, ext)| ext.to_lowercase())
    }
}

/// A group of related links (by host, page or base name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub links: Vec<LinkInfo>,
}

/// Options controlling extraction, filtering and probing.
#[derive(Debug, Clone, Default)]
pub struct GrabOptions {
    /// Crawl discovered HTML pages one level deep.
    pub deep: bool,
    /// Keep only these extensions (lowercase, no dot) if set.
    pub include_ext: Option<Vec<String>>,
    /// Minimum size in bytes (requires probing).
    pub min_size: Option<u64>,
    /// Maximum size in bytes (requires probing).
    pub max_size: Option<u64>,
    /// Keep only links whose host contains this substring.
    pub host_filter: Option<String>,
    /// Probe concurrency.
    pub concurrency: usize,
    /// Skip HEAD/GET probing (extraction only).
    pub no_probe: bool,
}

impl GrabOptions {
    pub fn concurrency_or_default(&self) -> usize {
        if self.concurrency == 0 {
            16
        } else {
            self.concurrency
        }
    }
}

fn attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?i)(?:href|src|data-src|data-href)\s*=\s*["']([^"'<>]+)["']"#).unwrap()
    })
}

fn og_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Match <meta ... property="og:video|image|audio" ... content="URL">
        // in either attribute order.
        Regex::new(
            r#"(?i)<meta[^>]*?(?:property|name)\s*=\s*["']og:(?:video|audio|image)(?::url)?["'][^>]*?content\s*=\s*["']([^"']+)["']|<meta[^>]*?content\s*=\s*["']([^"']+)["'][^>]*?(?:property|name)\s*=\s*["']og:(?:video|audio|image)(?::url)?["']"#,
        )
        .unwrap()
    })
}

fn bare_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        let exts = COMMON_EXTENSIONS.join("|");
        Regex::new(&format!(
            r#"(?i)https?://[^\s"'<>\)\]]+\.(?:{exts})(?:\?[^\s"'<>\)\]]*)?"#
        ))
        .unwrap()
    })
}

/// Extract absolute links from text (optionally HTML), resolving relative URLs
/// against `base`. Deduplicated, order-preserving.
pub fn extract_links(base: Option<&Url>, text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    let push = |raw: &str, out: &mut Vec<String>, seen: &mut HashSet<String>| {
        let raw = raw.trim();
        if raw.is_empty()
            || raw.starts_with('#')
            || raw.starts_with("javascript:")
            || raw.starts_with("data:")
            || raw.starts_with("mailto:")
        {
            return;
        }
        let resolved = match Url::parse(raw) {
            Ok(u) => Some(u),
            Err(url::ParseError::RelativeUrlWithoutBase) => base.and_then(|b| b.join(raw).ok()),
            Err(_) => None,
        };
        if let Some(u) = resolved {
            if u.scheme() == "http" || u.scheme() == "https" {
                let s = u.to_string();
                if seen.insert(s.clone()) {
                    out.push(s);
                }
            }
        }
    };

    for cap in attr_re().captures_iter(text) {
        push(&cap[1], &mut out, &mut seen);
    }
    for cap in og_re().captures_iter(text) {
        if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
            push(m.as_str(), &mut out, &mut seen);
        }
    }
    for m in bare_re().find_iter(text) {
        push(m.as_str(), &mut out, &mut seen);
    }

    out
}

/// Whether a URL very likely points at a downloadable file (by extension).
pub fn looks_like_file(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| filename_from_url(&u))
        .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_lowercase()))
        .map(|ext| COMMON_EXTENSIONS.contains(&ext.as_str()))
        .unwrap_or(false)
}

/// The LinkGrabber engine.
pub struct LinkGrabber {
    client: reqwest::Client,
    timeout: Duration,
}

impl LinkGrabber {
    pub fn new(client: reqwest::Client) -> Self {
        LinkGrabber {
            client,
            timeout: Duration::from_secs(20),
        }
    }

    /// Grab from an input that is either a URL (fetched and parsed) or raw text.
    pub async fn grab(&self, input: &str, opts: &GrabOptions) -> Result<Vec<Package>> {
        let input = input.trim();
        let (base, text, is_page) = if is_url(input) {
            let base = Url::parse(input).ok();
            match self.fetch_text(input).await {
                Ok(body) => (base, body, true),
                // If the page cannot be fetched, treat the URL itself as a link.
                Err(_) => (None, input.to_string(), false),
            }
        } else {
            (None, input.to_string(), false)
        };

        let mut links = extract_links(base.as_ref(), &text);

        // If the input was a direct file URL (not an HTML page), include it.
        if is_page && looks_like_file(input) && !links.contains(&input.to_string()) {
            links.insert(0, input.to_string());
        }
        if !is_page && is_url(input) && !links.contains(&input.to_string()) {
            links.insert(0, input.to_string());
        }

        // Optional one-level deep crawl of discovered HTML pages.
        if opts.deep && is_page {
            let pages: Vec<String> = links
                .iter()
                .filter(|l| !looks_like_file(l) && is_html_page(l))
                .take(20)
                .cloned()
                .collect();
            let mut extra = Vec::new();
            for page in pages {
                if let Ok(body) = self.fetch_text(&page).await {
                    let b = Url::parse(&page).ok();
                    extra.extend(extract_links(b.as_ref(), &body));
                }
            }
            for l in extra {
                if !links.contains(&l) {
                    links.push(l);
                }
            }
        }

        // Apply host/extension filters pre-probe (cheap).
        links.retain(|l| self.passes_cheap_filters(l, opts));

        // Probe concurrently.
        let infos: Vec<LinkInfo> = if opts.no_probe {
            links.into_iter().map(LinkInfo::bare).collect()
        } else {
            let conc = opts.concurrency_or_default();
            stream::iter(links.into_iter())
                .map(|l| async move { self.probe_link(&l).await })
                .buffer_unordered(conc)
                .collect::<Vec<_>>()
                .await
        };

        // Size filters (post-probe).
        let infos: Vec<LinkInfo> = infos
            .into_iter()
            .filter(|i| self.passes_size_filters(i, opts))
            .collect();

        Ok(group_packages(infos))
    }

    fn passes_cheap_filters(&self, url: &str, opts: &GrabOptions) -> bool {
        if let Some(hostf) = &opts.host_filter {
            let host = Url::parse(url)
                .ok()
                .and_then(|u| u.host_str().map(|s| s.to_string()))
                .unwrap_or_default();
            if !host.contains(hostf) {
                return false;
            }
        }
        if let Some(exts) = &opts.include_ext {
            let ext = Url::parse(url)
                .ok()
                .and_then(|u| filename_from_url(&u))
                .and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_lowercase()));
            match ext {
                Some(e) => exts.iter().any(|x| x.eq_ignore_ascii_case(&e)),
                None => false,
            }
        } else {
            true
        }
    }

    fn passes_size_filters(&self, info: &LinkInfo, opts: &GrabOptions) -> bool {
        if let (Some(min), Some(sz)) = (opts.min_size, info.size) {
            if sz < min {
                return false;
            }
        }
        if let (Some(max), Some(sz)) = (opts.max_size, info.size) {
            if sz > max {
                return false;
            }
        }
        true
    }

    async fn fetch_text(&self, url: &str) -> Result<String> {
        let resp = self.client.get(url).timeout(self.timeout).send().await?;
        Ok(resp.text().await?)
    }

    /// Probe a single link: HEAD first, then a ranged GET fallback, for
    /// filename / size / mime / resumability.
    pub async fn probe_link(&self, url: &str) -> LinkInfo {
        use reqwest::header as h;
        let mut info = LinkInfo::bare(url.to_string());
        info.filename = Url::parse(url).ok().and_then(|u| filename_from_url(&u));

        if let Ok(resp) = self.client.head(url).timeout(self.timeout).send().await {
            if resp.status().is_success() {
                let hdr = resp.headers();
                info.size = resp.content_length();
                info.mime = hdr
                    .get(h::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
                info.resumable = hdr
                    .get(h::ACCEPT_RANGES)
                    .and_then(|v| v.to_str().ok())
                    .map(|v| v.to_ascii_lowercase().contains("bytes"))
                    .unwrap_or(false);
                if let Some(cd) = hdr.get(h::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
                    if let Some(name) = filename_from_disposition(cd) {
                        info.filename = Some(name);
                    }
                }
                if info.size.is_some() {
                    return info;
                }
            }
        }

        // Fallback: ranged GET bytes=0-0.
        if let Ok(resp) = self
            .client
            .get(url)
            .header(h::RANGE, "bytes=0-0")
            .timeout(self.timeout)
            .send()
            .await
        {
            let status = resp.status();
            let hdr = resp.headers().clone();
            if info.mime.is_none() {
                info.mime = hdr
                    .get(h::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
            }
            if let Some(cd) = hdr.get(h::CONTENT_DISPOSITION).and_then(|v| v.to_str().ok()) {
                if let Some(name) = filename_from_disposition(cd) {
                    info.filename = Some(name);
                }
            }
            if status == reqwest::StatusCode::PARTIAL_CONTENT {
                info.resumable = true;
                if let Some(cr) = hdr.get(h::CONTENT_RANGE).and_then(|v| v.to_str().ok()) {
                    info.size = cr.rsplit('/').next().and_then(|s| s.trim().parse().ok());
                }
            } else if status.is_success() {
                info.size = resp.content_length();
            }
        }

        info
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

fn is_html_page(url: &str) -> bool {
    match Url::parse(url) {
        Ok(u) => {
            let path = u.path();
            let ext = path.rsplit_once('.').map(|(_, e)| e.to_lowercase());
            match ext {
                Some(e) => matches!(e.as_str(), "html" | "htm" | "php" | "asp" | "aspx" | "jsp"),
                None => true, // extensionless paths are usually pages
            }
        }
        Err(_) => false,
    }
}

/// Group links into packages by host.
pub fn group_packages(infos: Vec<LinkInfo>) -> Vec<Package> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<String, Vec<LinkInfo>> = BTreeMap::new();
    for info in infos {
        let host = Url::parse(&info.url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "unknown".to_string());
        groups.entry(host).or_default().push(info);
    }
    groups
        .into_iter()
        .map(|(name, links)| Package { name, links })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_anchors_and_resolve_relative() {
        let base = Url::parse("https://example.com/dir/page.html").unwrap();
        let html = r##"
            <a href="/files/a.zip">a</a>
            <a href="sub/b.mp4">b</a>
            <a href="https://cdn.example.com/c.pdf">c</a>
            <a href="#frag">frag</a>
            <a href="mailto:x@y.z">mail</a>
        "##;
        let links = extract_links(Some(&base), html);
        assert!(links.contains(&"https://example.com/files/a.zip".to_string()));
        assert!(links.contains(&"https://example.com/dir/sub/b.mp4".to_string()));
        assert!(links.contains(&"https://cdn.example.com/c.pdf".to_string()));
        assert_eq!(links.len(), 3); // frag and mailto excluded
    }

    #[test]
    fn extract_media_and_og() {
        let base = Url::parse("https://v.example.com/").unwrap();
        let html = r#"
            <video><source src="/media/clip.webm" type="video/webm"></video>
            <meta property="og:video" content="https://v.example.com/og.mp4">
            <meta content="https://v.example.com/og2.mp4" name="og:video:url">
        "#;
        let links = extract_links(Some(&base), html);
        assert!(links.contains(&"https://v.example.com/media/clip.webm".to_string()));
        assert!(links.contains(&"https://v.example.com/og.mp4".to_string()));
        assert!(links.contains(&"https://v.example.com/og2.mp4".to_string()));
    }

    #[test]
    fn extract_bare_urls_from_plain_text() {
        let text = "grab this https://host.test/big.iso?token=1 and https://host.test/song.mp3 plus not-a-file https://host.test/page";
        let links = extract_links(None, text);
        assert!(links.contains(&"https://host.test/big.iso?token=1".to_string()));
        assert!(links.contains(&"https://host.test/song.mp3".to_string()));
        assert!(!links.iter().any(|l| l.ends_with("/page")));
    }

    #[test]
    fn dedupe_preserves_first() {
        let text = "https://h.test/a.zip https://h.test/a.zip https://h.test/b.zip";
        let links = extract_links(None, text);
        assert_eq!(links, vec!["https://h.test/a.zip", "https://h.test/b.zip"]);
    }

    #[test]
    fn packages_grouped_by_host() {
        let infos = vec![
            LinkInfo::bare("https://a.test/1.zip".into()),
            LinkInfo::bare("https://a.test/2.zip".into()),
            LinkInfo::bare("https://b.test/3.zip".into()),
        ];
        let pkgs = group_packages(infos);
        assert_eq!(pkgs.len(), 2);
        let a = pkgs.iter().find(|p| p.name == "a.test").unwrap();
        assert_eq!(a.links.len(), 2);
    }

    #[test]
    fn looks_like_file_detects_extensions() {
        assert!(looks_like_file("https://h.test/x.zip"));
        assert!(looks_like_file("https://h.test/x.mp4?x=1"));
        assert!(!looks_like_file("https://h.test/page"));
    }

    #[test]
    fn link_extension_helper() {
        let mut li = LinkInfo::bare("https://h.test/movie.MKV".into());
        assert_eq!(li.extension().as_deref(), Some("mkv"));
        li.filename = Some("renamed.mp4".into());
        assert_eq!(li.extension().as_deref(), Some("mp4"));
    }
}
