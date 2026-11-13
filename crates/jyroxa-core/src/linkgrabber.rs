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

use crate::error::Result;
use crate::naming::{suggest_filename, filename_from_url_path};
use crate::probe::probe_url;

/// Common downloadable file extensions used by the bare-URL and filter logic.
pub const COMMON_EXTENSIONS: &[&str] = &[
    "zip", "rar", "7z", "tar", "gz", "tgz", "bz2", "xz", "iso", "exe", "msi", "dmg", "pkg", "deb",
    "rpm", "apk", "appimage", "bin", "img", "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v",
    "mp3", "flac", "wav", "aac", "ogg", "opus", "m4a", "pdf", "epub", "mobi", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "csv", "jpg", "jpeg", "png", "gif", "webp", "svg", "torrent", "dat",
    "zst", "lz", "lzma", "lz4", "cab", "jar", "whl", "nupkg", "vsix", "xpi", "crx", "ipa", "aab",
    "msix", "appx", "srt", "vtt", "m3u8", "mpd", "3gp", "wma", "aiff", "heic", "tif", "tiff",
];

/// Page assets that a LinkGrabber page crawl should never offer as downloads.
const PAGE_ASSET_EXTENSIONS: &[&str] = &[
    "css", "js", "mjs", "map", "woff", "woff2", "ttf", "otf", "eot", "ico", "json", "xml",
    "webmanifest", "rss", "atom",
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
            .or_else(|| {
                Url::parse(&self.url)
                    .ok()
                    .and_then(|u| filename_from_url_path(&u))
            })?;
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
        // Template placeholders from page scripts (`https://{host}/x`,
        // `${base}/file.zip`) are not real links.
        if is_template_placeholder(raw) {
            return;
        }
        let resolved = match Url::parse(raw) {
            Ok(u) => Some(u),
            Err(url::ParseError::RelativeUrlWithoutBase) => base.and_then(|b| b.join(raw).ok()),
            Err(_) => None,
        };
        if let Some(u) = resolved {
            if (u.scheme() == "http" || u.scheme() == "https") && has_real_host(&u) {
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

fn tag_attr_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?is)<([a-z][a-z0-9-]*)\b[^>]*?\s(?:href|src|data-src|data-href)\s*=\s*["']([^"'<>]+)["']"#,
        )
        .unwrap()
    })
}

fn og_media_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r#"(?i)<meta[^>]*?(?:property|name)\s*=\s*["']og:(?:video|audio)(?::url|:secure_url)?["'][^>]*?content\s*=\s*["']([^"']+)["']|<meta[^>]*?content\s*=\s*["']([^"']+)["'][^>]*?(?:property|name)\s*=\s*["']og:(?:video|audio)(?::url|:secure_url)?["']"#,
        )
        .unwrap()
    })
}

/// Tags whose link is something the page offers the visitor (a download, a
/// video/audio source), as opposed to the page's own furniture (`<img>`,
/// `<link rel=stylesheet|icon>`, `<script>`, `<iframe>`).
const OFFERED_TAGS: &[&str] = &["a", "area", "source", "video", "audio", "embed", "track", "object"];

/// Links a crawled HTML page *offers*: anchors and media sources, plus
/// `og:video`/`og:audio`. Page decoration (images, icons, styles, scripts and
/// `og:image`) is left out. Also returns `<include-fragment>` URLs, which pages
/// such as GitHub releases use to load their file lists.
pub fn extract_page_links(base: Option<&Url>, html: &str) -> (Vec<String>, Vec<String>) {
    let mut offered = String::new();
    let mut fragments = String::new();
    for cap in tag_attr_re().captures_iter(html) {
        let tag = cap[1].to_ascii_lowercase();
        let url = &cap[2];
        if OFFERED_TAGS.contains(&tag.as_str()) {
            offered.push_str(&format!("<a href=\"{url}\">\n"));
        } else if tag == "include-fragment" {
            fragments.push_str(&format!("<a href=\"{url}\">\n"));
        }
    }
    for cap in og_media_re().captures_iter(html) {
        if let Some(m) = cap.get(1).or_else(|| cap.get(2)) {
            offered.push_str(&format!("<a href=\"{}\">\n", m.as_str()));
        }
    }
    let links = extract_links(base, &offered);
    let frags = extract_links(base, &fragments)
        .into_iter()
        .filter(|f| {
            // Only same-host fragments; never wander off-site automatically.
            let fh = Url::parse(f).ok().and_then(|u| u.host_str().map(str::to_string));
            let bh = base.and_then(|b| b.host_str().map(str::to_string));
            fh.is_some() && fh == bh
        })
        .collect();
    (links, frags)
}

/// `{host}`, `${x}`, `{{x}}` and their percent-encoded forms.
/// File URLs written as plain text in an HTML page, ignoring anything inside
/// tags, comments, `<script>`, `<style>`, `<noscript>` and `<template>`.
fn visible_text_urls(html: &str) -> Vec<String> {
    static BLOCKS: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    let blocks = BLOCKS.get_or_init(|| {
        Regex::new(r"(?is)<!--.*?-->|<script\b.*?</script\s*>|<style\b.*?</style\s*>|<noscript\b.*?</noscript\s*>|<template\b.*?</template\s*>").unwrap()
    });
    let tags = TAGS.get_or_init(|| Regex::new(r"(?s)<[^>]*>").unwrap());
    let without_blocks = blocks.replace_all(html, " ");
    let text = tags.replace_all(&without_blocks, " ");
    let mut out: Vec<String> = Vec::new();
    for m in bare_re().find_iter(&text) {
        let l = m.as_str().replace("&amp;", "&");
        if !is_template_placeholder(&l) && !out.contains(&l) {
            out.push(l);
        }
    }
    out
}

fn is_template_placeholder(raw: &str) -> bool {
    raw.contains('{')
        || raw.contains('}')
        || raw.contains("${")
        || raw.to_ascii_lowercase().contains("%7b")
        || raw.to_ascii_lowercase().contains("%7d")
}

/// A host that can actually be resolved: an IP, or a DNS name made of
/// letters, digits, `-`, `_` and dots.
fn has_real_host(u: &Url) -> bool {
    match u.host() {
        Some(url::Host::Ipv4(_)) | Some(url::Host::Ipv6(_)) => true,
        Some(url::Host::Domain(d)) => {
            !d.is_empty()
                && d.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
                && !d.starts_with('.')
                && !d.contains("..")
        }
        None => false,
    }
}

/// Whether a URL very likely points at a downloadable file (by extension).
pub fn looks_like_file(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| filename_from_url_path(&u))
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
        #[cfg(feature = "telegram")]
        if is_single_url(input) && crate::telegram::is_telegram_post_url(input) {
            return self.grab_telegram_post(input).await;
        }
        let (base, text, is_page) = if is_single_url(input) {
            let base = Url::parse(input).ok();
            match self.fetch_text(input).await {
                Ok(body) => (base, body, true),
                // If the page cannot be fetched, treat the URL itself as a link.
                Err(_) => (None, input.to_string(), false),
            }
        } else {
            (None, input.to_string(), false)
        };

        let mut links = if is_page {
            let (mut links, fragments) = extract_page_links(base.as_ref(), &text);
            // Bare file URLs in the page's visible text (e.g. listed in a
            // <pre> block). URLs inside tags, scripts and styles are page
            // decoration (icons, og:image, inline JSON), not offered files.
            for l in visible_text_urls(&text) {
                if !links.contains(&l) {
                    links.push(l);
                }
            }
            for frag in fragments.into_iter().take(4) {
                if let Ok(body) = self.fetch_text(&frag).await {
                    let b = Url::parse(&frag).ok();
                    for l in extract_page_links(b.as_ref(), &body).0 {
                        if !links.contains(&l) {
                            links.push(l);
                        }
                    }
                }
            }
            links
        } else {
            extract_links(base.as_ref(), &text)
        };
        if !is_page {
            // Pasted text: every whitespace-separated http(s) URL is a candidate,
            // whether or not its path carries a known file extension.
            let mut direct = plain_text_urls(&text);
            direct.retain(|u| !links.contains(u));
            direct.extend(links);
            links = direct;
        }

        // If the input was a direct file URL (not an HTML page), include it.
        if is_page && looks_like_file(input) && !links.contains(&input.to_string()) {
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
                    extra.extend(extract_page_links(b.as_ref(), &body).0);
                }
            }
            for l in extra {
                if !links.contains(&l) {
                    links.push(l);
                }
            }
        }

        let mut extensionless_links: HashSet<String> = HashSet::new();
        if is_page {
            // A page crawl lists files, not the page's own styles, scripts, fonts
            // or links to other pages (`deep` has already followed those).
            // Extensionless links may still be downloads (served with
            // `Content-Disposition: attachment`), so a bounded number of them is
            // probed and kept only if they turn out not to be HTML.
            let mut extensionless = 0usize;
            let probed_extensionless = &mut extensionless_links;
            links.retain(|l| {
                if l == input {
                    return true;
                }
                if has_asset_extension(l) || has_page_extension(l) {
                    return false;
                }
                if has_file_extension(l) {
                    return true;
                }
                if is_same_page(l, input) {
                    return false;
                }
                extensionless += 1;
                let keep = extensionless <= MAX_EXTENSIONLESS_PROBES;
                if keep {
                    probed_extensionless.insert(l.clone());
                }
                keep
            });
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

        // Size filters (post-probe). Links that turned out to be HTML pages are
        // navigation, not downloads, when they came from crawling a page.
        let infos: Vec<LinkInfo> = infos
            .into_iter()
            .filter(|i| self.passes_size_filters(i, opts))
            .filter(|i| !(is_page && is_html_mime(i.mime.as_deref())))
            // An extensionless page link is only a download if the probe
            // confirmed it; unreachable ones (HEAD refused, 404) are dropped.
            .filter(|i| !(extensionless_links.contains(&i.url) && i.mime.is_none() && i.size.is_none()))
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
                .and_then(|u| filename_from_url_path(&u))
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
        #[cfg(feature = "telegram")]
        if crate::telegram::is_telegram_post_url(url) {
            return match crate::telegram::probe_post(&self.client, url).await {
                Ok(item) => item.to_link_info(),
                Err(_) => {
                    let mut info = LinkInfo::bare(url.to_string());
                    info.filename = Some(format!("telegram_post"));
                    info
                }
            };
        }
        match probe_url(&self.client, url, self.timeout).await {
            Ok(p) => LinkInfo {
                url: p.final_url,
                filename: Some(p.suggested_name),
                size: p.size,
                mime: p.mime,
                resumable: p.resumable,
            },
            Err(_) => {
                let mut info = LinkInfo::bare(url.to_string());
                info.filename = Some(suggest_filename(url, None, None));
                info
            }
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    #[cfg(feature = "telegram")]
    async fn grab_telegram_post(&self, url: &str) -> Result<Vec<Package>> {
        let items = crate::telegram::resolve_post(&self.client, url)
            .await
            .map_err(|e| crate::error::Error::other(e.to_string()))?;
        let infos: Vec<LinkInfo> = items.into_iter().map(|i| i.to_link_info()).collect();
        Ok(group_packages(infos))
    }
}

/// Whether the whole input is exactly one http(s) URL (no whitespace, parses).
fn is_single_url(s: &str) -> bool {
    !s.chars().any(char::is_whitespace) && parse_http_url(s).is_some()
}

fn parse_http_url(s: &str) -> Option<Url> {
    Url::parse(s)
        .ok()
        .filter(|u| matches!(u.scheme(), "http" | "https") && u.host_str().is_some())
}

/// Every http(s) URL in free text, one per whitespace-separated token, with
/// surrounding quotes/brackets and trailing punctuation removed. Order-preserving.
pub fn plain_text_urls(text: &str) -> Vec<String> {
    const OPEN: &[char] = &['"', '\'', '<', '(', '[', '{'];
    const CLOSE: &[char] = &['"', '\'', '>', ')', ']', '}', ',', ';', '.', '!'];
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for token in text.split_whitespace() {
        let token = token.trim_start_matches(OPEN).trim_end_matches(CLOSE);
        let Some(start) = token.find("http://").or_else(|| token.find("https://")) else {
            continue;
        };
        if is_template_placeholder(&token[start..]) {
            continue;
        }
        if let Some(u) = parse_http_url(&token[start..]).filter(has_real_host) {
            let s = u.to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

/// Upper bound on extensionless page links probed for attachments.
const MAX_EXTENSIONLESS_PROBES: usize = 40;

fn url_extension(url: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    let name = filename_from_url_path(&u)?;
    name.rsplit_once('.').map(|(_, e)| e.to_lowercase())
}

fn has_asset_extension(url: &str) -> bool {
    url_extension(url)
        .map(|e| PAGE_ASSET_EXTENSIONS.contains(&e.as_str()))
        .unwrap_or(false)
}

fn has_page_extension(url: &str) -> bool {
    url_extension(url)
        .map(|e| matches!(e.as_str(), "html" | "htm" | "shtml" | "xhtml" | "php" | "asp" | "aspx" | "jsp"))
        .unwrap_or(false)
}

fn has_file_extension(url: &str) -> bool {
    url_extension(url)
        .map(|e| COMMON_EXTENSIONS.contains(&e.as_str()) || e.len() <= 5 && e.chars().all(|c| c.is_ascii_alphanumeric()) && !e.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

/// The same page with a different fragment/query (e.g. `?lang=fr`).
fn is_same_page(link: &str, page: &str) -> bool {
    match (Url::parse(link), Url::parse(page)) {
        (Ok(a), Ok(b)) => a.host_str() == b.host_str() && a.path().trim_end_matches('/') == b.path().trim_end_matches('/'),
        _ => false,
    }
}

#[cfg(test)]
fn is_page_asset(url: &str) -> bool {
    let Some(u) = Url::parse(url).ok() else {
        return true;
    };
    let ext = filename_from_url_path(&u).and_then(|n| n.rsplit_once('.').map(|(_, e)| e.to_lowercase()));
    match ext {
        Some(e) if PAGE_ASSET_EXTENSIONS.contains(&e.as_str()) => true,
        Some(e) if COMMON_EXTENSIONS.contains(&e.as_str()) => false,
        _ => is_html_page(url),
    }
}

fn is_html_mime(mime: Option<&str>) -> bool {
    mime.map(|m| {
        let m = m.split(';').next().unwrap_or("").trim();
        m.eq_ignore_ascii_case("text/html") || m.eq_ignore_ascii_case("application/xhtml+xml")
    })
    .unwrap_or(false)
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
    fn single_url_detection_rejects_multiline_text() {
        assert!(is_single_url("https://example.com/a.zip"));
        assert!(!is_single_url("https://example.com/a.zip\nhttps://example.com/b.zip"));
        assert!(!is_single_url("see https://example.com/a.zip"));
        assert!(!is_single_url("ftp://example.com/a.zip"));
    }

    #[test]
    fn plain_text_urls_accepts_any_http_url_per_token() {
        let text = "Links:\nhttps://proof.ovh.net/files/10Mb.dat\n(https://httpbin.org/bytes/512),\n<https://example.com/a.zip>. https://example.com/a.zip not-a-url";
        assert_eq!(
            plain_text_urls(text),
            vec![
                "https://proof.ovh.net/files/10Mb.dat".to_string(),
                "https://httpbin.org/bytes/512".to_string(),
                "https://example.com/a.zip".to_string(),
            ]
        );
    }

    #[test]
    fn page_assets_and_pages_are_not_downloads() {
        assert!(is_page_asset("https://proof.ovh.net/style.css"));
        assert!(is_page_asset("https://proof.ovh.net/fonts/x.woff2?v=1"));
        assert!(is_page_asset("https://proof.ovh.net/about"));
        assert!(is_page_asset("https://proof.ovh.net/index.html"));
        assert!(!is_page_asset("https://proof.ovh.net/files/10Mb.dat"));
        assert!(!is_page_asset("https://example.com/setup.exe"));
        assert!(is_html_mime(Some("text/html; charset=utf-8")));
        assert!(!is_html_mime(Some("application/octet-stream")));
        assert!(!is_html_mime(None));
    }

    #[test]
    fn page_extraction_keeps_offered_files_only() {
        let base = Url::parse("https://proof.ovh.net/").unwrap();
        let html = r#"
            <link rel="icon" href="/icon.svg"><link rel="apple-touch-icon" href="/apple-touch-icon.png">
            <link rel="stylesheet" href="/style.css"><script src="/app.js"></script>
            <img src="/logo_com.png" alt="">
            <meta property="og:image" content="https://cdn.ovh.net/og-image.png">
            <a href="/files/10Mb.dat">10 MB</a>
            <a href="/files/1Gb.dat">1 GB</a>
            <video><source src="/media/clip.mp4"></video>
            <a href="https://{host}/files/1Mb.dat">template</a>
            <script>const u = `https://${host}/files/100Mb.dat`;</script>
        "#;
        let (links, frags) = extract_page_links(Some(&base), html);
        assert_eq!(
            links,
            vec![
                "https://proof.ovh.net/files/10Mb.dat".to_string(),
                "https://proof.ovh.net/files/1Gb.dat".to_string(),
                "https://proof.ovh.net/media/clip.mp4".to_string(),
            ]
        );
        assert!(frags.is_empty());
    }

    #[test]
    fn bare_urls_only_from_visible_text() {
        let html = r#"
            <link rel="icon" href="https://assets.test/favicon.png">
            <meta property="og:image" content="https://cdn.test/og-image.png">
            <picture><img srcset="https://assets.test/hero.png 1x"></picture>
            <script type="application/json">{"u":"https://assets.test/data.zip"}</script>
            <style>body{background:url(https://assets.test/bg.png)}</style>
            <!-- https://old.test/gone.zip -->
            <pre>https://mirror.test/pub/tool-1.0.tar.gz
https://mirror.test/pub/tool-1.0.zip</pre>
        "#;
        assert_eq!(
            visible_text_urls(html),
            vec![
                "https://mirror.test/pub/tool-1.0.tar.gz".to_string(),
                "https://mirror.test/pub/tool-1.0.zip".to_string(),
            ]
        );
    }

    #[test]
    fn template_and_bogus_hosts_are_rejected() {
        let links = extract_links(None, r#"<a href="https://{host}/a.zip"> <a href="https://%7Bhost%7D/b.zip"> https://ok.test/c.zip"#);
        assert_eq!(links, vec!["https://ok.test/c.zip".to_string()]);
        assert!(plain_text_urls("https://{host}/a.zip https://ok.test/d.bin").iter().all(|u| u.starts_with("https://ok.test/")));
    }

    #[test]
    fn include_fragments_same_host_only() {
        let base = Url::parse("https://github.com/o/r/releases/tag/v1").unwrap();
        let html = r#"<include-fragment loading="lazy" src="https://github.com/o/r/releases/expanded_assets/v1"></include-fragment>
                      <include-fragment src="https://evil.test/x"></include-fragment>"#;
        let (_, frags) = extract_page_links(Some(&base), html);
        assert_eq!(frags, vec!["https://github.com/o/r/releases/expanded_assets/v1".to_string()]);
    }

    #[test]
    fn page_link_classification() {
        assert!(has_asset_extension("https://h.test/a.css?v=2"));
        assert!(has_page_extension("https://h.test/index.html"));
        assert!(has_file_extension("https://h.test/wget-1.25.0.tar.gz"));
        assert!(has_file_extension("https://h.test/files/10Mb.dat"));
        assert!(!has_file_extension("https://h.test/software/wget/"));
        assert!(is_same_page("https://h.test/a/?lang=fr", "https://h.test/a"));
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
