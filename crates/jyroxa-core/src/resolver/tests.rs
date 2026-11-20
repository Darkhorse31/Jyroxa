use super::*;
#[test]
fn extraction_and_ranking() {
    let url = Url::parse("https://example.test/page").unwrap();
    let html = include_str!("../../tests/fixtures/resolver-candidates.html");
    let links = candidates(&url, html);
    assert_eq!(links[0].url.path(), "/opaque");
    assert_eq!(links[0].filename.as_deref(), Some("nice.zip"));
    assert_eq!(links[1].url.path(), "/same.zip");
    for path in [
        "/refresh",
        "/movie",
        "/sound",
        "/alternate",
        "/js",
        "/replace",
        "/plain",
    ] {
        assert!(links.iter().any(|l| l.url.path() == path), "{path}");
    }
    assert_eq!(links.len(), 11);
    assert!(!links
        .iter()
        .any(|link| link.url.path().contains("icon.png")));
}
#[test]
fn html_signatures() {
    for mime in [
        "text/html; charset=utf-8",
        "application/xhtml+xml",
        "TEXT/HTML",
    ] {
        assert!(is_html(Some(mime), b""));
    }
    for bytes in [b" \n<!DOCTYPE HTML>".as_slice(), b"<HTML lang=en>"] {
        assert!(is_html(None, bytes));
    }
    assert!(!is_html(Some("application/zip"), b"PK"));
}
async fn server() -> (String, tokio::task::JoinHandle<()>) {
    use axum::{
        http::{HeaderMap, StatusCode},
        response::IntoResponse,
        routing::get,
        Router,
    };
    let router = Router::new()
        .route(
            "/fragment-root",
            get(|| async {
                (
                    [("content-type", "text/html")],
                    "<include-fragment src='/fragment-html'></include-fragment>",
                )
            }),
        )
        .route(
            "/fragment-html",
            get(|| async {
                (
                    [("content-type", "text/fragment+html")],
                    "<a href='/fallback' download>file</a>",
                )
            }),
        )
        .route(
            "/fragment-page",
            get(|| async {
                (
                    [("content-type", "text/html")],
                    "<include-fragment src='/json'></include-fragment>",
                )
            }),
        )
        .route(
            "/json",
            get(|| async { ([("content-type", "application/json")], "{}") }),
        )
        .route(
            "/redirect",
            get(|| async { axum::response::Redirect::temporary("/page") }),
        )
        .route(
            "/xhtml",
            get(|| async {
                (
                    [("content-type", "application/xhtml+xml")],
                    "<html><a download href='/fallback'>file</a></html>",
                )
            }),
        )
        .route(
            "/page",
            get(|| async {
                (
                    [
                        ("content-type", "text/html"),
                        ("set-cookie", "access=yes; Path=/"),
                    ],
                    "<a href='/file.zip' download>download</a>",
                )
            }),
        )
        .route(
            "/file.zip",
            get(|headers: HeaderMap| async move {
                if !headers
                    .get("cookie")
                    .is_some_and(|v| v.to_str().unwrap_or("").contains("access=yes"))
                {
                    return StatusCode::FORBIDDEN.into_response();
                }
                if !headers
                    .get("referer")
                    .is_some_and(|v| v.to_str().unwrap_or("").ends_with("/page"))
                {
                    return StatusCode::FORBIDDEN.into_response();
                }
                (
                    [
                        ("content-type", "application/zip"),
                        ("content-disposition", "attachment; filename=real.zip"),
                    ],
                    "PKcontents",
                )
                    .into_response()
            }),
        )
        .route(
            "/sniff",
            get(|| async {
                (
                    [("content-type", "application/octet-stream")],
                    "<!doctype html><html><p>No file</p></html>",
                )
            }),
        )
        .route(
            "/explicit.html",
            get(|| async { ([("content-type", "text/html")], "<html>saved</html>") }),
        )
        .route(
            "/loop",
            get(|| async {
                (
                    [("content-type", "text/html")],
                    "<a href='/loop2' download>next</a>",
                )
            }),
        )
        .route(
            "/loop2",
            get(|| async {
                (
                    [("content-type", "text/html")],
                    "<a href='/loop' download>next</a>",
                )
            }),
        )
        .route(
            "/fallback",
            get(|| async { ([("content-type", "application/zip")], "PKfallback") })
                .head(|| async { StatusCode::METHOD_NOT_ALLOWED }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{address}"), task)
}
#[tokio::test]
async fn local_resolution_and_download_guard() {
    let (base, task) = server().await;
    let client = crate::build_client().unwrap();
    let links = resolve_links(&client, &format!("{base}/page"), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(links[0].filename.as_deref(), Some("real.zip"));
    for path in ["sniff", "loop", "fragment-page"] {
        assert!(matches!(
            resolve_links(&client, &format!("{base}/{path}"), Duration::from_secs(5)).await,
            Err(Error::PageNotFile { .. })
        ));
    }
    for path in [
        "explicit.html",
        "fallback",
        "redirect",
        "xhtml",
        "fragment-root",
    ] {
        assert_eq!(
            resolve_links(&client, &format!("{base}/{path}"), Duration::from_secs(5))
                .await
                .unwrap()
                .len(),
            1
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("bad.bin");
    let engine = crate::Downloader::new(crate::DownloadConfig::default()).unwrap();
    let result = engine
        .download(
            crate::DownloadRequest::new(format!("{base}/sniff"), &output),
            None,
            crate::Control::default(),
            None,
        )
        .await;
    assert!(matches!(result, Err(Error::PageNotFile { .. })));
    assert!(!output.exists());
    let good = dir.path().join("real.zip");
    engine
        .download(
            crate::DownloadRequest::new(format!("{base}/page"), &good),
            None,
            crate::Control::default(),
            None,
        )
        .await
        .unwrap();
    assert_eq!(std::fs::read(&good).unwrap(), b"PKcontents");
    let packages = crate::LinkGrabber::new(client)
        .grab(&format!("{base}/page"), &crate::GrabOptions::default())
        .await
        .unwrap();
    assert_eq!(packages[0].links[0].filename.as_deref(), Some("real.zip"));
    task.abort();
}
#[tokio::test]
#[ignore = "public network smoke test"]
async fn legitimate_public_pages() {
    let client = crate::build_client().unwrap();
    let mut failures = Vec::new();
    for url in [
        "https://github.com/BurntSushi/ripgrep/releases/latest",
        "https://sourceforge.net/projects/sevenzip/files/latest/download",
        "https://downloads.apache.org/httpd/",
        "https://cdn.kernel.org/pub/linux/kernel/v6.x/",
        "https://httpbin.org/redirect-to?url=https://proof.ovh.net/files/1Mb.dat",
    ] {
        match resolve_links(&client, url, Duration::from_secs(30)).await {
            Ok(links) => {
                println!("{url}: {} validated files", links.len());
            }
            Err(error) => {
                failures.push(format!("{url}: {error}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn depth_request_and_page_limits() {
    use axum::{extract::Path, routing::get, Router};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let count = Arc::new(AtomicUsize::new(0));
    let observed = count.clone();
    let app = Router::new().route(
        "/:depth",
        get(move |Path(depth): Path<u32>| {
            let observed = observed.clone();
            async move {
                observed.fetch_add(1, Ordering::SeqCst);
                let links = (0..100)
                    .map(|n| format!("<a download href='/{}'>next</a>", depth * 100 + n + 1))
                    .collect::<String>();
                ([("content-type", "text/html")], links)
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let client = crate::build_client().unwrap();
    assert!(matches!(
        resolve_links(&client, &format!("{base}/0"), Duration::from_secs(5)).await,
        Err(Error::PageNotFile { .. })
    ));
    assert!(count.load(Ordering::SeqCst) <= MAX_REQUESTS);
    task.abort();

    let app = Router::new().route(
        "/",
        get(|| async {
            let text = format!(
                "<html>{}<a download href='/file.zip'>late</a>",
                " ".repeat(MAX_PAGE)
            );
            ([("content-type", "text/html")], text)
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    assert!(matches!(
        resolve_links(&client, &url, Duration::from_secs(5)).await,
        Err(Error::PageNotFile { .. })
    ));
    task.abort();
}

#[tokio::test]
async fn registry_accepts_custom_resolvers() {
    struct Custom;
    #[async_trait]
    impl LinkResolver for Custom {
        fn name(&self) -> &str {
            "custom"
        }
        fn matches(&self, url: &Url) -> bool {
            url.path() == "/loop"
        }
        async fn resolve(
            &self,
            _: &ResolveCtx,
            url: &Url,
            _: &FetchedPage,
        ) -> Result<Option<Vec<ResolvedLink>>> {
            Ok(Some(vec![ResolvedLink {
                url: url.join("/fallback")?,
                filename: Some("custom.zip".into()),
                size: None,
                referer: Some(url.clone()),
                mime: None,
                resumable: false,
                is_page: false,
            }]))
        }
    }
    let (base, task) = server().await;
    let mut registry = ResolverRegistry::default();
    registry.register(Box::new(Custom));
    let links = registry
        .resolve(
            &ResolveCtx {
                client: crate::build_client().unwrap(),
                timeout: Duration::from_secs(5),
            },
            &format!("{base}/loop"),
        )
        .await
        .unwrap();
    assert_eq!(links[0].filename.as_deref(), Some("custom.zip"));
    task.abort();
}

#[tokio::test]
async fn third_page_transition_is_rejected() {
    use axum::{extract::Path, routing::get, Router};
    let app = Router::new().route(
        "/:depth",
        get(|Path(depth): Path<u32>| async move {
            if depth == 3 {
                return ([("content-type", "application/zip")], "PKfile".to_owned());
            }
            (
                [("content-type", "text/html")],
                format!("<a download href='/{}'>next</a>", depth + 1),
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/0", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    assert!(matches!(
        resolve_links(
            &crate::build_client().unwrap(),
            &url,
            Duration::from_secs(5)
        )
        .await,
        Err(Error::PageNotFile { .. })
    ));
    task.abort();
}

#[tokio::test]
async fn cancellation_interrupts_resolution_without_output() {
    use axum::{routing::get, Router};
    let app = Router::new().route(
        "/",
        get(|| async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            "<html>waiting</html>"
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("cancel.bin");
    let control = crate::Control::new();
    let engine = crate::Downloader::new(crate::DownloadConfig::default()).unwrap();
    let download = engine.download(
        crate::DownloadRequest::new(&url, &output),
        None,
        control.clone(),
        None,
    );
    tokio::pin!(download);
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_millis(50)) => {}
        result = &mut download => panic!("resolution completed early: {result:?}"),
    }
    control.cancel();
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(2), &mut download)
            .await
            .unwrap(),
        Err(Error::Cancelled)
    ));
    assert!(!output.exists());
    let early = crate::Control::new();
    early.cancel();
    assert!(matches!(
        engine
            .download(crate::DownloadRequest::new(url, &output), None, early, None)
            .await,
        Err(Error::Cancelled)
    ));
    task.abort();
}
