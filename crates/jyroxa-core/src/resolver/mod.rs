//! Bounded, generic resolution of public download pages.
use crate::{Error, Result};
use async_trait::async_trait;
use reqwest::header as h;
use scraper::{Html, Selector};
use std::{
    collections::{HashSet, VecDeque},
    time::Duration,
};
use url::Url;

const MAX_REQUESTS: usize = 32;
const MAX_PAGE: usize = 2 * 1024 * 1024;

pub struct FetchedPage {
    pub html: String,
}
#[derive(Debug, Clone)]
pub struct ResolvedLink {
    pub url: Url,
    pub filename: Option<String>,
    pub size: Option<u64>,
    pub referer: Option<Url>,
    pub mime: Option<String>,
    pub resumable: bool,
    /// Embedded HTML fragments are pages even when their MIME type is missing.
    pub is_page: bool,
}
pub struct ResolveCtx {
    pub client: reqwest::Client,
    pub timeout: Duration,
}
#[async_trait]
pub trait LinkResolver: Send + Sync {
    fn name(&self) -> &str;
    fn matches(&self, url: &Url) -> bool;
    async fn resolve(
        &self,
        ctx: &ResolveCtx,
        url: &Url,
        page: &FetchedPage,
    ) -> Result<Option<Vec<ResolvedLink>>>;
}
pub struct GenericPageResolver;
#[async_trait]
impl LinkResolver for GenericPageResolver {
    fn name(&self) -> &str {
        "generic"
    }
    fn matches(&self, _: &Url) -> bool {
        true
    }
    async fn resolve(
        &self,
        _: &ResolveCtx,
        url: &Url,
        page: &FetchedPage,
    ) -> Result<Option<Vec<ResolvedLink>>> {
        Ok(Some(candidates(url, &page.html)))
    }
}

pub(crate) fn candidates(url: &Url, text: &str) -> Vec<ResolvedLink> {
    let doc = Html::parse_document(text);
    let selector = Selector::parse("a, button, meta, link, source, video, audio, include-fragment")
        .expect("static selector");
    let mut ranked = Vec::new();
    let mut push = |raw: &str, rank: u8, name: Option<&str>, is_page: bool| {
        if let Ok(mut target) = url.join(raw.trim()) {
            target.set_fragment(None);
            if matches!(target.scheme(), "http" | "https") && target != *url {
                ranked.push((
                    rank,
                    ResolvedLink {
                        url: target,
                        filename: name
                            .filter(|n| !n.is_empty())
                            .map(crate::naming::sanitize_filename),
                        size: None,
                        referer: Some(url.clone()),
                        mime: None,
                        resumable: false,
                        is_page,
                    },
                ));
            }
        }
    };
    for node in doc.select(&selector) {
        let e = node.value();
        if e.name() == "meta" {
            let kind = e
                .attr("property")
                .or_else(|| e.attr("name"))
                .unwrap_or("")
                .to_ascii_lowercase();
            if kind.starts_with("og:video") || kind.starts_with("og:audio") {
                if let Some(v) = e.attr("content") {
                    push(v, 2, None, false);
                }
            }
            if e.attr("http-equiv")
                .is_some_and(|v| v.eq_ignore_ascii_case("refresh"))
            {
                if let Some((_, v)) = e.attr("content").and_then(|v| v.split_once(';')) {
                    if let Some((key, value)) = v.split_once('=') {
                        if key.trim().eq_ignore_ascii_case("url") {
                            push(value.trim().trim_matches(['\'', '"']), 2, None, false);
                        }
                    }
                }
            }
            continue;
        }
        if e.name() == "link" {
            let rel = e.attr("rel").unwrap_or("").to_ascii_lowercase();
            if !rel.split_whitespace().any(|token| token == "alternate")
                || rel.split_whitespace().any(|token| token.contains("icon"))
            {
                continue;
            }
        }
        if let Some(raw) = e
            .attr("href")
            .or_else(|| e.attr("data-href"))
            .or_else(|| e.attr("src"))
        {
            let target = url.join(raw).ok();
            let file = target
                .as_ref()
                .is_some_and(|u| crate::linkgrabber::looks_like_file(u.as_str()));
            let label = node.text().collect::<String>().to_ascii_lowercase();
            let download = e.attr("download");
            let alternate = e.name() == "link"
                && e.attr("rel").is_some_and(|v| {
                    v.split_whitespace()
                        .any(|r| r.eq_ignore_ascii_case("alternate"))
                })
                && e.attr("type")
                    .is_some_and(|t| !t.contains("html") && !t.contains("xml"));
            if download.is_some()
                || file
                || alternate
                || label.contains("download")
                || matches!(e.name(), "source" | "video" | "audio" | "include-fragment")
            {
                let rank = if download.is_some() {
                    0
                } else if file && target.as_ref().and_then(Url::host_str) == url.host_str() {
                    1
                } else {
                    2
                };
                push(raw, rank, download, e.name() == "include-fragment");
            }
        }
    }
    static JS: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = JS.get_or_init(|| regex::Regex::new(r#"(?i)(?:(?:window\.)?location(?:\.href)?\s*=\s*|(?:window\.)?location\.replace\(\s*)['"]([^'"<>]+)['"]"#).expect("static regex"));
    for cap in re.captures_iter(text) {
        push(&cap[1], 2, None, false);
    }
    ranked.sort_by_key(|(rank, _)| *rank);
    let mut seen = HashSet::new();
    ranked
        .into_iter()
        .map(|(_, link)| link)
        .filter(|l| seen.insert(l.url.clone()))
        .collect()
}

pub fn is_html(mime: Option<&str>, prefix: &[u8]) -> bool {
    let mime = mime.unwrap_or("").split(';').next().unwrap_or("").trim();
    let text = String::from_utf8_lossy(&prefix[..prefix.len().min(512)]);
    let text = text
        .trim_start_matches('\u{feff}')
        .trim_start()
        .to_ascii_lowercase();
    mime.eq_ignore_ascii_case("text/fragment+html")
        || mime.eq_ignore_ascii_case("text/html")
        || mime.eq_ignore_ascii_case("application/xhtml+xml")
        || text.starts_with("<!doctype html")
        || text.starts_with("<html")
}
fn explicit_html(url: &Url) -> bool {
    let p = url.path().to_ascii_lowercase();
    p.ends_with(".html") || p.ends_with(".htm")
}

pub struct ResolverRegistry {
    resolvers: Vec<Box<dyn LinkResolver>>,
}
impl Default for ResolverRegistry {
    fn default() -> Self {
        Self {
            resolvers: vec![Box::new(GenericPageResolver)],
        }
    }
}
impl ResolverRegistry {
    pub fn register(&mut self, resolver: Box<dyn LinkResolver>) {
        self.resolvers.insert(0, resolver);
    }
    /// Resolve at most two page transitions, with bounded requests and page memory.
    pub async fn resolve(&self, ctx: &ResolveCtx, input: &str) -> Result<Vec<ResolvedLink>> {
        self.resolve_with_limit(ctx, input, usize::MAX).await
    }

    async fn resolve_with_limit(
        &self,
        ctx: &ResolveCtx,
        input: &str,
        limit: usize,
    ) -> Result<Vec<ResolvedLink>> {
        let original = Url::parse(input)?;
        let mut queue = VecDeque::from([(
            ResolvedLink {
                url: original.clone(),
                filename: None,
                size: None,
                referer: None,
                mime: None,
                resumable: false,
                is_page: false,
            },
            0,
        )]);
        let mut seen = HashSet::new();
        let mut files = Vec::new();
        let mut requests = 0;
        while let Some((mut link, depth)) = queue.pop_front() {
            if requests + 3 > MAX_REQUESTS {
                break;
            }
            if !seen.insert(link.url.clone()) {
                continue;
            }
            let mut head = ctx.client.head(link.url.clone()).timeout(ctx.timeout);
            if let Some(r) = &link.referer {
                head = head.header(h::REFERER, r.as_str());
            }
            requests += 1;
            let mut metadata = head.send().await.ok();
            if metadata.as_ref().map_or(true, |r| !r.status().is_success()) {
                let mut fallback = ctx
                    .client
                    .get(link.url.clone())
                    .header(h::RANGE, "bytes=0-0")
                    .timeout(ctx.timeout);
                if let Some(r) = &link.referer {
                    fallback = fallback.header(h::REFERER, r.as_str());
                }
                requests += 1;
                metadata = fallback.send().await.ok();
            }
            let mut get = ctx.client.get(link.url.clone()).timeout(ctx.timeout);
            if let Some(r) = &link.referer {
                get = get.header(h::REFERER, r.as_str());
            }
            // A small range validates non-HTML HEAD results without buffering a file.
            if metadata.as_ref().is_some_and(|r| {
                r.status().is_success()
                    && !is_html(
                        r.headers()
                            .get(h::CONTENT_TYPE)
                            .and_then(|v| v.to_str().ok()),
                        &[],
                    )
            }) {
                get = get.header(h::RANGE, "bytes=0-511");
            }
            requests += 1;
            let mut fetched = get.send().await;
            if fetched
                .as_ref()
                .is_ok_and(|r| matches!(r.status().as_u16(), 400 | 405 | 416))
                && requests < MAX_REQUESTS
            {
                let mut plain = ctx.client.get(link.url.clone()).timeout(ctx.timeout);
                if let Some(r) = &link.referer {
                    plain = plain.header(h::REFERER, r.as_str());
                }
                requests += 1;
                fetched = plain.send().await;
            }
            let mut response = match fetched {
                Ok(r) if r.status().is_success() => r,
                Ok(_) | Err(_) if depth > 0 => continue,
                Ok(r) => return Err(Error::Status(r.status().as_u16())),
                Err(e) => return Err(e.into()),
            };
            if response.url() != &link.url && link.referer.is_none() {
                link.referer = Some(link.url.clone());
            }
            link.url = response.url().clone();
            let headers = response.headers().clone();
            let mime = headers.get(h::CONTENT_TYPE).and_then(|v| v.to_str().ok());
            let mut bytes = Vec::new();
            while bytes.len() < 512 {
                match response.chunk().await? {
                    Some(chunk) => {
                        bytes.extend_from_slice(&chunk[..chunk.len().min(MAX_PAGE - bytes.len())])
                    }
                    None => break,
                }
            }
            if !link.is_page && (explicit_html(&original) || !is_html(mime, &bytes)) {
                link.filename = headers
                    .get(h::CONTENT_DISPOSITION)
                    .or_else(|| {
                        metadata
                            .as_ref()
                            .and_then(|r| r.headers().get(h::CONTENT_DISPOSITION))
                    })
                    .and_then(|v| v.to_str().ok())
                    .and_then(crate::naming::filename_from_disposition)
                    .map(|name| crate::naming::sanitize_filename(&name))
                    .or(link.filename)
                    .or_else(|| {
                        Some(crate::naming::suggest_filename(
                            link.url.as_str(),
                            None,
                            mime,
                        ))
                    });
                link.size = headers
                    .get(h::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.rsplit('/').next()?.parse().ok())
                    .or_else(|| {
                        metadata
                            .as_ref()
                            .and_then(|r| r.headers().get(h::CONTENT_RANGE))
                            .and_then(|v| v.to_str().ok())
                            .and_then(|s| s.rsplit('/').next()?.parse().ok())
                    })
                    .or_else(|| {
                        if response.status() == reqwest::StatusCode::OK {
                            response.content_length()
                        } else {
                            None
                        }
                    })
                    .or_else(|| {
                        metadata
                            .as_ref()
                            .filter(|r| r.status() == reqwest::StatusCode::OK)
                            .and_then(|r| r.content_length())
                    });
                link.mime = mime.map(str::to_owned);
                link.resumable = response.status() == reqwest::StatusCode::PARTIAL_CONTENT
                    || headers.get(h::ACCEPT_RANGES).is_some_and(|v| v == "bytes");
                files.push(link);
                if files.len() >= limit {
                    break;
                }
                continue;
            }
            if depth >= 2 {
                continue;
            }
            // A mislabeled ranged response needs the complete page.
            if response.status() == reqwest::StatusCode::PARTIAL_CONTENT {
                if requests >= MAX_REQUESTS {
                    break;
                }
                let mut request = ctx.client.get(link.url.clone()).timeout(ctx.timeout);
                if let Some(r) = &link.referer {
                    request = request.header(h::REFERER, r.as_str());
                }
                requests += 1;
                response = request.send().await?.error_for_status()?;
                bytes.clear();
            }
            while bytes.len() < MAX_PAGE {
                match response.chunk().await? {
                    Some(chunk) => {
                        bytes.extend_from_slice(&chunk[..chunk.len().min(MAX_PAGE - bytes.len())])
                    }
                    None => break,
                }
            }
            let page = FetchedPage {
                html: String::from_utf8_lossy(&bytes).into_owned(),
            };
            for resolver in &self.resolvers {
                if resolver.matches(&link.url) {
                    if let Some(links) = resolver.resolve(ctx, &link.url, &page).await? {
                        queue.extend(links.into_iter().take(MAX_REQUESTS).map(|l| (l, depth + 1)));
                    }
                }
            }
        }
        if files.is_empty() {
            Err(Error::PageNotFile { url: input.into() })
        } else {
            Ok(files)
        }
    }
}
pub async fn resolve_links(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Vec<ResolvedLink>> {
    ResolverRegistry::default()
        .resolve(
            &ResolveCtx {
                client: client.clone(),
                timeout,
            },
            url,
        )
        .await
}

/// Resolve the first ranked, validated file without probing remaining candidates.
pub async fn resolve_link(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<ResolvedLink> {
    let links = ResolverRegistry::default()
        .resolve_with_limit(
            &ResolveCtx {
                client: client.clone(),
                timeout,
            },
            url,
            1,
        )
        .await?;
    links.into_iter().next().ok_or_else(|| Error::PageNotFile {
        url: url.to_owned(),
    })
}

pub(crate) async fn prepare_download_controlled(
    client: &reqwest::Client,
    request: &mut crate::DownloadRequest,
    timeout: Duration,
    control: &crate::Control,
) -> Result<Option<Url>> {
    tokio::select! {
        biased;
        _ = control.cancelled() => Err(Error::Cancelled),
        result = prepare_download(client, request, timeout) => result,
    }
}

/// Validate mirrors before creating any output or resume state.
pub async fn prepare_download(
    client: &reqwest::Client,
    request: &mut crate::DownloadRequest,
    timeout: Duration,
) -> Result<Option<Url>> {
    let mut referer = None;
    let mut targets = Vec::new();
    let mut failure = None;
    for url in &request.urls {
        match resolve_link(client, url, timeout).await {
            Ok(link) => {
                if referer.is_none() {
                    referer = link.referer;
                }
                targets.push(link.url.to_string());
            }
            Err(e) => failure = Some(e),
        }
    }
    if targets.is_empty() {
        return Err(failure.unwrap_or(Error::NoUrl));
    }
    request.urls = targets;
    Ok(referer)
}

#[cfg(test)]
mod tests;

pub(crate) fn session_clients(
    timeout: Duration,
    jar: std::sync::Arc<reqwest::cookie::Jar>,
    referer: Option<&Url>,
) -> Result<(reqwest::Client, reqwest::Client)> {
    let mut headers = h::HeaderMap::new();
    if let Some(url) = referer {
        if let Ok(value) = h::HeaderValue::from_str(url.as_str()) {
            headers.insert(h::REFERER, value);
        }
    }
    let build = |http1| {
        let mut builder = reqwest::Client::builder()
            .user_agent(crate::USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(5))
            .cookie_provider(jar.clone())
            .default_headers(headers.clone())
            .pool_max_idle_per_host(crate::MAX_CONNECTIONS)
            .tcp_nodelay(true)
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(timeout);
        if http1 {
            builder = builder.http1_only();
        }
        builder.build().map_err(Error::from)
    };
    Ok((build(false)?, build(true)?))
}
