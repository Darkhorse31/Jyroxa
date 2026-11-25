//! Parse our yt-dlp protocol, never its human-readable progress messages.
use std::collections::BTreeMap;

use jyroxa_core::events::VideoProgress;
use serde_json::Value;

pub const PLAN_TEMPLATE: &str = "before_dl:JX_PLAN:{\"formats\":%(requested_formats)j,\"id\":%(format_id)j,\"total\":%(filesize|0)s,\"estimate\":%(filesize_approx|0)s}";
pub const DOWNLOAD_TEMPLATE: &str = "download:JX_PROGRESS:{\"id\":%(info.format_id)j,\"downloaded\":%(progress.downloaded_bytes|0)s,\"total\":%(progress.total_bytes|0)s,\"estimate\":%(progress.total_bytes_estimate|0)s,\"speed\":%(progress.speed|0)s,\"eta\":%(progress.eta|null)s,\"status\":%(progress.status)j}";
pub const POST_TEMPLATE: &str = "postprocess:JX_POST:%(progress.postprocessor)s";

#[derive(Default)]
struct Transfer {
    downloaded: u64,
    total: Option<u64>,
    estimated: bool,
}

#[derive(Default)]
pub struct Tracker {
    streams: BTreeMap<String, Transfer>,
    processing_phase: Option<&'static str>,
}

fn positive(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n > 0).or_else(|| {
        value
            .as_f64()
            .filter(|n| n.is_finite() && *n > 0.0 && *n < u64::MAX as f64)
            .map(|n| n.ceil() as u64)
    })
}

fn size(value: &Value, total: &str, estimate: &str) -> (Option<u64>, bool) {
    positive(&value[total]).map_or_else(
        || (positive(&value[estimate]), true),
        |total| (Some(total), false),
    )
}

impl Tracker {
    pub fn fetching() -> VideoProgress {
        VideoProgress {
            phase: "fetching".into(),
            downloaded: 0,
            total: None,
            total_is_estimate: false,
            speed_bps: 0.0,
            eta_secs: None,
        }
    }

    pub fn line(&mut self, line: &str) -> Option<VideoProgress> {
        if let Some(json) = line.strip_prefix("JX_PLAN:") {
            let value: Value = serde_json::from_str(json).ok()?;
            self.streams.clear();
            self.processing_phase = None;
            if let Some(formats) = value["formats"].as_array().filter(|f| !f.is_empty()) {
                for format in formats {
                    let id = format["format_id"].as_str()?.to_owned();
                    let (total, estimated) = size(format, "filesize", "filesize_approx");
                    self.streams.insert(
                        id,
                        Transfer {
                            total,
                            estimated,
                            ..Default::default()
                        },
                    );
                }
            } else {
                let id = value["id"].as_str()?.to_owned();
                let (total, estimated) = size(&value, "total", "estimate");
                self.streams.insert(
                    id,
                    Transfer {
                        total,
                        estimated,
                        ..Default::default()
                    },
                );
            }
        } else if let Some(json) = line.strip_prefix("JX_PROGRESS:") {
            let value: Value = serde_json::from_str(json).ok()?;
            let id = value["id"].as_str()?.to_owned();
            let downloaded = value["downloaded"].as_u64()?;
            let stream = self.streams.entry(id).or_default();
            // Replace, rather than sum, counters: retries and repeated finished
            // notifications describe the same on-disk stream.
            stream.downloaded = downloaded;
            let (total, estimated) = size(&value, "total", "estimate");
            if value["status"] == "finished" {
                stream.total = Some(downloaded);
                stream.estimated = false;
            } else if total.is_some() && (!estimated || stream.total.is_none() || stream.estimated)
            {
                stream.total = total;
                stream.estimated = estimated;
            }
            let speed = value["speed"]
                .as_f64()
                .filter(|n| n.is_finite() && *n >= 0.0)
                .unwrap_or(0.0);
            // Postprocessor records arrive on stderr, download records on
            // stdout. Draining independently can deliver a late transfer line.
            let mut progress = self.snapshot(
                self.processing_phase.unwrap_or("downloading"),
                if self.processing_phase.is_some() {
                    0.0
                } else {
                    speed
                },
            );
            // Per-stream ETA excludes the as-yet undownloaded audio/video track.
            progress.eta_secs = progress
                .total
                .filter(|total| *total > progress.downloaded)
                .filter(|_| speed > 0.0 && self.processing_phase.is_none())
                .map(|total| (total - progress.downloaded) as f64 / speed);
            return Some(progress);
        } else {
            let processor = line.strip_prefix("JX_POST:")?;
            let phase = if processor.contains("Merger") {
                "merging"
            } else {
                "processing"
            };
            self.processing_phase = Some(phase);
            let progress = self.snapshot(phase, 0.0);
            return Some(progress);
        }
        let progress = self.snapshot("downloading", 0.0);
        Some(progress)
    }

    fn snapshot(&self, phase: &str, speed_bps: f64) -> VideoProgress {
        let total: Option<u64> = self
            .streams
            .values()
            .map(|stream| stream.total)
            .try_fold(0u64, |sum, size| sum.checked_add(size?));
        VideoProgress {
            phase: phase.into(),
            downloaded: self
                .streams
                .values()
                .fold(0u64, |sum, stream| sum.saturating_add(stream.downloaded)),
            total: total.filter(|total| *total > 0),
            total_is_estimate: self.streams.values().any(|s| s.estimated),
            speed_bps,
            eta_secs: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_video_and_audio_without_double_counting() {
        let mut tracker = Tracker::default();
        tracker.line(r#"JX_PLAN:{"formats":[{"format_id":"v","filesize":1000},{"format_id":"a","filesize":200}]}"#).unwrap();
        let line = r#"JX_PROGRESS:{"id":"v","downloaded":500,"total":1000,"speed":100,"status":"downloading"}"#;
        let first = tracker.line(line).unwrap();
        assert_eq!(
            (first.downloaded, first.total, first.eta_secs),
            (500, Some(1200), Some(7.0))
        );
        assert_eq!(tracker.line(line).unwrap(), first);
        tracker
            .line(r#"JX_PROGRESS:{"id":"v","downloaded":1000,"status":"finished"}"#)
            .unwrap();
        let audio = tracker
            .line(r#"JX_PROGRESS:{"id":"a","downloaded":50,"total":200,"speed":50}"#)
            .unwrap();
        assert_eq!(
            (audio.downloaded, audio.total, audio.eta_secs),
            (1050, Some(1200), Some(3.0))
        );
        let merging = tracker.line("JX_POST:Merger").unwrap();
        assert_eq!(merging.phase, "merging");
        assert_eq!(merging.speed_bps, 0.0);
        assert_eq!(merging.eta_secs, None);
        let late = tracker.line(line).unwrap();
        assert_eq!(
            late.phase, "merging",
            "late stdout must not undo processing status"
        );
        assert_eq!(late.speed_bps, 0.0);
        assert_eq!(late.eta_secs, None);
    }

    #[test]
    fn handles_unknown_estimated_and_restarted_transfers() {
        let mut tracker = Tracker::default();
        tracker
            .line(r#"JX_PLAN:{"id":"18","formats":null}"#)
            .unwrap();
        let unknown = tracker
            .line(r#"JX_PROGRESS:{"id":"18","downloaded":10}"#)
            .unwrap();
        assert_eq!((unknown.total, unknown.eta_secs), (None, None));
        let estimate = tracker
            .line(r#"JX_PROGRESS:{"id":"18","downloaded":20,"estimate":100,"speed":10}"#)
            .unwrap();
        assert_eq!(estimate.total, Some(100));
        assert!(estimate.total_is_estimate);
        let exact = tracker
            .line(r#"JX_PROGRESS:{"id":"18","downloaded":30,"total":90}"#)
            .unwrap();
        assert!(!exact.total_is_estimate);
        let later_estimate = tracker
            .line(r#"JX_PROGRESS:{"id":"18","downloaded":30,"estimate":120}"#)
            .unwrap();
        assert_eq!(
            later_estimate.total,
            Some(90),
            "an estimate cannot replace a known size"
        );
        assert!(!later_estimate.total_is_estimate);
        let mut estimate_tracker = Tracker::default();
        let fractional = estimate_tracker
            .line(r#"JX_PROGRESS:{"id":"18","downloaded":30,"estimate":99.5}"#)
            .unwrap();
        assert_eq!(fractional.total, Some(100));
        assert!(fractional.total_is_estimate);
        tracker
            .line(r#"JX_PLAN:{"id":"18","formats":null,"total":90}"#)
            .unwrap();
        assert_eq!(
            tracker
                .line(r#"JX_PROGRESS:{"id":"18","downloaded":5}"#)
                .unwrap()
                .downloaded,
            5
        );
        for line in [
            "unrelated warning",
            "JX_PROGRESS:broken",
            r#"JX_PROGRESS:{"id":"18","downloaded":-1}"#,
        ] {
            assert!(tracker.line(line).is_none());
        }
    }
}
