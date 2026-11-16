//! HTTP HEAD/GET probing for link metadata.

use std::time::Duration;

use reqwest::header as h;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::naming::{category_for, suggest_filename, Category};

/// Metadata from probing a remote URL (after redirects).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbeInfo {
    pub final_url: String,
    pub suggested_name: String,
    pub size: Option<u64>,
    pub mime: Option<String>,
    pub resumable: bool,
    pub category: Category,
    pub server: String,
    pub protocol: String,
    /// Public embed hid the file; user must sign in to resolve via MTProto.
    #[serde(default)]
    pub telegram_login_required: bool,
}

fn mime_from_headers(headers: &h::HeaderMap) -> Option<String> {
    headers
        .get(h::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string())
}

fn protocol_label(version: reqwest::Version) -> String {
    match version {
        reqwest::Version::HTTP_09 => "HTTP/0.9".into(),
        reqwest::Version::HTTP_10 => "HTTP/1.0".into(),
        reqwest::Version::HTTP_11 => "HTTP/1.1".into(),
        reqwest::Version::HTTP_2 => "HTTP/2".into(),
        reqwest::Version::HTTP_3 => "HTTP/3".into(),
        _ => "HTTP".into(),
    }
}

fn apply_extra_headers(
    builder: reqwest::RequestBuilder,
    headers: &std::collections::HashMap<String, String>,
) -> reqwest::RequestBuilder {
    let mut b = builder;
    for (k, v) in headers {
        if !k.is_empty() && !v.is_empty() {
            b = b.header(k.as_str(), v.as_str());
        }
    }
    b
}

/// Probe `url` with HEAD, falling back to a ranged GET (`bytes=0-0`).
pub async fn probe_url(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<ProbeInfo> {
    probe_url_with_headers(client, url, &std::collections::HashMap::new(), timeout).await
}

/// Probe with optional HTTP headers (e.g. Cookie / Referer from a browser handoff).
pub async fn probe_url_with_headers(
    client: &reqwest::Client,
    url: &str,
    headers: &std::collections::HashMap<String, String>,
    timeout: Duration,
) -> Result<ProbeInfo> {
    let mut final_url = url.to_string();
    let mut size = None;
    let mut mime = None;
    let mut resumable = false;
    let mut content_disposition = None;
    let mut protocol = "HTTP/1.1".to_string();

    let head = apply_extra_headers(client.head(url), headers);
    if let Ok(resp) = head
        .timeout(timeout.min(crate::download::HEAD_TIMEOUT))
        .send()
        .await
    {
        if resp.status().is_success() || resp.status() == reqwest::StatusCode::METHOD_NOT_ALLOWED {
            final_url = resp.url().to_string();
            protocol = protocol_label(resp.version());
            let headers = resp.headers();
            mime = mime_from_headers(headers);
            size = resp.content_length().filter(|&n| n > 0);
            resumable = headers
                .get(h::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_ascii_lowercase().contains("bytes"))
                .unwrap_or(false);
            content_disposition = headers
                .get(h::CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
        }
    }

    let needs_ranged = size.is_none() || !resumable;
    if needs_ranged {
        let get = apply_extra_headers(client.get(&final_url), headers);
        let resp = get
            .header(h::RANGE, "bytes=0-0")
            .timeout(timeout)
            .send()
            .await?;
        final_url = resp.url().to_string();
        protocol = protocol_label(resp.version());
        let status = resp.status();
        let headers = resp.headers().clone();
        if mime.is_none() {
            mime = mime_from_headers(&headers);
        }
        if content_disposition.is_none() {
            content_disposition = headers
                .get(h::CONTENT_DISPOSITION)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
        }
        if status == reqwest::StatusCode::PARTIAL_CONTENT {
            resumable = true;
            if let Some(cr) = headers.get(h::CONTENT_RANGE).and_then(|v| v.to_str().ok()) {
                size = cr.rsplit('/').next().and_then(|s| s.trim().parse().ok());
            }
        } else if status.is_success() {
            if let Some(len) = resp.content_length() {
                size = Some(len);
            }
            resumable = headers
                .get(h::ACCEPT_RANGES)
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_ascii_lowercase().contains("bytes"))
                .unwrap_or(false);
        } else if status != reqwest::StatusCode::METHOD_NOT_ALLOWED {
            return Err(Error::Status(status.as_u16()));
        }
    }

    let suggested_name =
        suggest_filename(&final_url, content_disposition.as_deref(), mime.as_deref());
    let server = Url::parse(&final_url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string());
    let category = category_for(&suggested_name, mime.as_deref());

    Ok(ProbeInfo {
        final_url,
        suggested_name,
        size,
        mime,
        resumable,
        category,
        server,
        protocol,
        telegram_login_required: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{HeaderMap, HeaderValue, StatusCode};
    use axum::response::Response;
    use axum::routing::get;
    use axum::Router;
    use std::collections::HashMap;
    use tokio::net::TcpListener;

    fn attachment_response(status: StatusCode, headers: &HeaderMap) -> Response<Body> {
        let mut resp = Response::builder()
            .status(status)
            .header(
                "Content-Disposition",
                HeaderValue::from_static("attachment; filename=\"pixeldrain.bin\""),
            )
            .header("Content-Length", "4242")
            .header("Accept-Ranges", "bytes")
            .body(Body::empty())
            .unwrap();
        if headers.contains_key("range") {
            *resp.status_mut() = StatusCode::PARTIAL_CONTENT;
            resp.headers_mut().insert(
                "Content-Range",
                HeaderValue::from_static("bytes 0-0/4242"),
            );
        }
        resp
    }

    async fn attachment_get(headers: HeaderMap) -> Response<Body> {
        attachment_response(StatusCode::OK, &headers)
    }

    async fn attachment_head(headers: HeaderMap) -> Response<Body> {
        attachment_response(StatusCode::OK, &headers)
    }

    #[tokio::test]
    async fn probe_uses_content_disposition_with_headers() {
        let app = Router::new()
            .route("/file", get(attachment_get).head(attachment_head));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = crate::build_client().unwrap();
        let url = format!("http://{addr}/file?id=abc");
        let mut headers = HashMap::new();
        headers.insert("Cookie".into(), "sess=1".into());
        let info = probe_url_with_headers(
            &client,
            &url,
            &headers,
            std::time::Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert_eq!(info.suggested_name, "pixeldrain.bin");
        assert_eq!(info.size, Some(4242));
    }
}
