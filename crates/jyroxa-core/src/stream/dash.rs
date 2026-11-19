//! Minimal DASH MPD parser (SegmentTemplate / BaseURL).

use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashVariant {
    pub id: String,
    pub bandwidth: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mime_type: Option<String>,
    pub codecs: Option<String>,
    pub base_url: String,
    pub init_url: Option<String>,
    pub media_template: Option<String>,
    pub start_number: u64,
    pub timescale: u64,
    pub segment_duration: u64,
    pub segment_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpdInfo {
    pub variants: Vec<DashVariant>,
}

fn text_between<'a>(xml: &'a str, tag: &str) -> Vec<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find(&open) {
        let after = &rest[start..];
        let gt = after.find('>').unwrap_or(0);
        let inner_start = start + gt + 1;
        let slice = &rest[inner_start..];
        if let Some(end) = slice.find(&close) {
            out.push(slice[..end].trim());
            rest = &slice[end + close.len()..];
        } else {
            break;
        }
    }
    out
}

fn attr_value(open_tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = open_tag.find(&needle)? + needle.len();
    let rest = &open_tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn find_open_tags(xml: &str, tag: &str) -> Vec<String> {
    let open = format!("<{tag}");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let slice = &rest[i..];
        let end = slice.find('>').unwrap_or(slice.len());
        out.push(slice[..=end].to_string());
        rest = &slice[end + 1..];
    }
    out
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

/// Parse a DASH MPD document and enumerate downloadable representations.
pub fn parse_mpd(xml: &str, base_url: &str) -> Result<MpdInfo, String> {
    let period_xml = text_between(xml, "Period").first().copied().unwrap_or(xml);
    let mut variants = Vec::new();

    for adapt_open in find_open_tags(period_xml, "AdaptationSet") {
        let mime = attr_value(&adapt_open, "mimeType").or_else(|| attr_value(&adapt_open, "contentType"));
        let adapt_start = period_xml.find(&adapt_open).unwrap_or(0);
        let adapt_xml = &period_xml[adapt_start + adapt_open.len()..];
        let adapt_end = adapt_xml.find("</AdaptationSet>").unwrap_or(adapt_xml.len());
        let adapt_xml = &adapt_xml[..adapt_end];

        for rep_open in find_open_tags(adapt_xml, "Representation") {
            let id = attr_value(&rep_open, "id").unwrap_or_else(|| "0".into());
            let bandwidth = attr_value(&rep_open, "bandwidth").and_then(|s| s.parse().ok());
            let width = attr_value(&rep_open, "width").and_then(|s| s.parse().ok());
            let height = attr_value(&rep_open, "height").and_then(|s| s.parse().ok());
            let codecs = attr_value(&rep_open, "codecs");

            let rep_start = adapt_xml.find(&rep_open).unwrap_or(0);
            let rep_xml = &adapt_xml[rep_start + rep_open.len()..];
            let rep_end = rep_xml.find("</Representation>").unwrap_or(rep_xml.len());
            let rep_xml = &rep_xml[..rep_end];

            let base = text_between(rep_xml, "BaseURL")
                .first()
                .map(|u| resolve_uri(base_url, u))
                .unwrap_or_else(|| base_url.to_string());

            let seg_open = find_open_tags(rep_xml, "SegmentTemplate")
                .first()
                .cloned()
                .or_else(|| find_open_tags(adapt_xml, "SegmentTemplate").first().cloned());

            let (init_url, media_template, start_number, timescale, segment_duration, segment_count) =
                if let Some(ref so) = seg_open {
                    let init = attr_value(so, "initialization").map(|t| {
                        let media = attr_value(so, "media").unwrap_or("$Number$.m4s".into());
                        media_template_url(&base, &media, &t, start_number_from(so))
                    });
                    let media = attr_value(so, "media");
                    let sn = start_number_from(so);
                    let timescale = attr_value(so, "timescale")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1);
                    let dur = attr_value(so, "duration")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);
                    let count = segment_count_from_period(period_xml, timescale, dur);
                    (init, media, sn, timescale, dur, count)
                } else {
                    (None, None, 1, 1, 0, 0)
                };

            if media_template.is_none() && init_url.is_none() {
                continue;
            }

            variants.push(DashVariant {
                id,
                bandwidth,
                width,
                height,
                mime_type: mime.clone(),
                codecs,
                base_url: base,
                init_url,
                media_template,
                start_number,
                timescale,
                segment_duration,
                segment_count: segment_count.max(1),
            });
        }
    }

    if variants.is_empty() {
        return Err("no representations in MPD".into());
    }
    Ok(MpdInfo { variants })
}

fn start_number_from(seg_open: &str) -> u64 {
    attr_value(seg_open, "startNumber")
        .and_then(|s| s.parse().ok())
        .unwrap_or(1)
}

fn segment_count_from_period(period_xml: &str, timescale: u64, seg_dur: u64) -> usize {
    if seg_dur == 0 || timescale == 0 {
        return 0;
    }
    for open in find_open_tags(period_xml, "Period") {
        if let Some(d) = attr_value(&open, "duration") {
            if let Ok(ms) = d.parse::<u64>() {
                let ticks = ms * timescale / 1000;
                if ticks > 0 {
                    return ((ticks + seg_dur - 1) / seg_dur) as usize;
                }
            }
        }
    }
    0
}

fn media_template_url(base: &str, media: &str, init: &str, start_number: u64) -> String {
    let path = init
        .replace("$RepresentationID$", "0")
        .replace("$Number$", &start_number.to_string())
        .replace("$Bandwidth$", "0");
    let media_path = media
        .replace("$RepresentationID$", "0")
        .replace("$Number$", &start_number.to_string());
    let combined = if init.contains('/') || media.contains('/') {
        path
    } else {
        format!("{}/{}", path.trim_end_matches('/'), media_path.trim_start_matches('/'))
    };
    resolve_uri(base, &combined)
}

pub fn dash_segment_urls(v: &DashVariant) -> Vec<String> {
    let Some(ref tmpl) = v.media_template else {
        return Vec::new();
    };
    (0..v.segment_count)
        .map(|i| {
            let num = v.start_number + i as u64;
            let path = tmpl
                .replace("$Number$", &num.to_string())
                .replace("$RepresentationID$", &v.id)
                .replace("$Bandwidth$", &v.bandwidth.unwrap_or(0).to_string());
            resolve_uri(&v.base_url, &path)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple_mpd() {
        let xml = r#"<?xml version="1.0"?>
<MPD>
  <Period duration="PT8S">
    <AdaptationSet mimeType="video/mp4">
      <Representation id="v1" bandwidth="1200000" width="1280" height="720" codecs="avc1">
        <BaseURL>https://cdn.example/dash/</BaseURL>
        <SegmentTemplate media="seg-$Number$.m4s" initialization="init.m4s" startNumber="1" timescale="1000" duration="2000"/>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#;
        let info = parse_mpd(xml, "https://cdn.example/manifest.mpd").unwrap();
        assert_eq!(info.variants.len(), 1);
        assert_eq!(info.variants[0].bandwidth, Some(1_200_000));
        let urls = dash_segment_urls(&info.variants[0]);
        assert!(!urls.is_empty());
        assert!(urls[0].contains("seg-1.m4s"));
    }
}