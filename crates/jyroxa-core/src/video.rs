//! Recognize video pages without mistaking arbitrary HTML or CDN URLs for media.

pub fn is_youtube_page(input: &str) -> bool {
    let Ok(url) = url::Url::parse(input) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    match url.host_str().unwrap_or_default() {
        "youtu.be" => url
            .path_segments()
            .is_some_and(|mut s| s.next().is_some_and(|id| !id.is_empty())),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            (url.path() == "/watch"
                && url
                    .query_pairs()
                    .any(|(key, value)| key == "v" && !value.is_empty()))
                || ["/shorts/", "/live/", "/embed/"].iter().any(|prefix| {
                    url.path()
                        .strip_prefix(prefix)
                        .is_some_and(|id| !id.is_empty())
                })
        }
        _ => false,
    }
}

/// Use the same watch-page extractor for watch, Shorts and short-link inputs.
/// Redirect and embed players may otherwise expose a different challenge script.
pub fn youtube_page_url(input: &str) -> Option<String> {
    if !is_youtube_page(input) {
        return None;
    }
    let source = url::Url::parse(input).ok()?;
    let id = if source.path() == "/watch" {
        source
            .query_pairs()
            .find(|(key, _)| key == "v")?
            .1
            .into_owned()
    } else {
        let mut parts = source.path_segments()?.filter(|part| !part.is_empty());
        if source.host_str() != Some("youtu.be") {
            parts.next()?;
        }
        parts.next()?.to_string()
    };
    let mut watch = url::Url::parse("https://www.youtube.com/watch").ok()?;
    watch.query_pairs_mut().append_pair("v", &id);
    Some(watch.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recognizes_video_pages_only() {
        for url in [
            "https://youtube.com/watch?v=jNQXAC9IVRw",
            "https://youtu.be/jNQXAC9IVRw?t=1",
            "https://www.youtube.com/shorts/abc",
            "https://m.youtube.com/live/abc",
            "https://music.youtube.com/watch?v=abc",
        ] {
            assert!(is_youtube_page(url), "{url}");
        }
        for url in [
            "https://youtube.com/",
            "https://youtube.com/watch?list=abc",
            "https://youtube.com/shorts/",
            "https://youtube.com.evil/watch?v=abc",
            "https://example.com/?url=https://youtu.be/abc",
            "file://youtube.com/watch?v=abc",
        ] {
            assert!(!is_youtube_page(url), "{url}");
        }
    }

    #[test]
    fn youtube_inputs_share_one_extraction_url() {
        for url in [
            "https://youtube.com/watch?v=jNQXAC9IVRw&list=test",
            "https://youtu.be/jNQXAC9IVRw?t=2",
            "https://www.youtube.com/shorts/jNQXAC9IVRw",
            "https://youtube.com/embed/jNQXAC9IVRw",
        ] {
            assert_eq!(
                youtube_page_url(url).as_deref(),
                Some("https://www.youtube.com/watch?v=jNQXAC9IVRw")
            );
        }
        assert!(youtube_page_url("https://youtu.be.evil/jNQXAC9IVRw").is_none());
    }
}
