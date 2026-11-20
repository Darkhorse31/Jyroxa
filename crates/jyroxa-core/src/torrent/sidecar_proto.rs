//! Machine-readable progress lines from `jyroxa torrent get` (stderr).

pub const METADATA_TIMEOUT_SECS: u64 = 300;
pub const STALL_TIMEOUT_SECS: u64 = 120;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TorrentProgressPhase {
    Metadata,
    Download,
}

impl TorrentProgressPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Download => "download",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "metadata" => Some(Self::Metadata),
            "download" => Some(Self::Download),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TorrentProgressLine {
    pub phase: TorrentProgressPhase,
    pub state: String,
    pub downloaded: u64,
    pub total: u64,
    pub peers: u32,
    pub seeds: u32,
    pub speed_bps: f64,
}

impl TorrentProgressLine {
    pub fn format(&self) -> String {
        format!(
            "jyroxa-torrent-progress phase={} state={} downloaded={} total={} peers={} seeds={} speed_bps={:.0}",
            self.phase.as_str(),
            sanitize_state_token(&self.state),
            self.downloaded,
            self.total,
            self.peers,
            self.seeds,
            self.speed_bps
        )
    }
}

fn sanitize_state_token(state: &str) -> String {
    state
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .take(64)
        .collect()
}

/// Legacy lines without `phase=` remain supported for older bench scripts.
pub fn parse_progress_line(line: &str) -> Option<TorrentProgressLine> {
    let line = line.trim();
    if !line.starts_with("jyroxa-torrent-progress ") {
        return None;
    }
    let rest = &line["jyroxa-torrent-progress ".len()..];
    let mut phase = TorrentProgressPhase::Download;
    let mut state = String::new();
    let mut downloaded = 0u64;
    let mut total = 0u64;
    let mut peers = 0u32;
    let mut seeds = 0u32;
    let mut speed_bps = 0.0f64;

    for part in rest.split_whitespace() {
        let Some((key, val)) = part.split_once('=') else {
            continue;
        };
        match key {
            "phase" => {
                if let Some(p) = TorrentProgressPhase::parse(val) {
                    phase = p;
                }
            }
            "state" => state = val.to_string(),
            "downloaded" => downloaded = val.parse().unwrap_or(0),
            "total" => total = val.parse().unwrap_or(0),
            "peers" => peers = val.parse().unwrap_or(0),
            "seeds" => seeds = val.parse().unwrap_or(0),
            "speed_bps" => speed_bps = val.parse().unwrap_or(0.0),
            _ => {}
        }
    }
    Some(TorrentProgressLine {
        phase,
        state,
        downloaded,
        total,
        peers,
        seeds,
        speed_bps,
    })
}

pub fn parse_error_line(line: &str) -> Option<String> {
    let line = line.trim();
    if let Some(msg) = line.strip_prefix("jyroxa-torrent-error ") {
        return Some(msg.to_string());
    }
    if let Some(msg) = line.strip_prefix("error: ") {
        return Some(msg.to_string());
    }
    None
}

pub fn stall_user_message(idle_secs: u64, peers: u32, phase: TorrentProgressPhase) -> String {
    match phase {
        TorrentProgressPhase::Metadata if peers == 0 => format!(
            "No peers found for metadata after {idle_secs}s (DHT/trackers). Check firewall or try again later."
        ),
        TorrentProgressPhase::Metadata => format!(
            "Metadata fetch timed out after {idle_secs}s ({peers} peers seen)."
        ),
        TorrentProgressPhase::Download if peers == 0 => format!(
            "No peers connected for {idle_secs}s. Check firewall, VPN, or port forwarding."
        ),
        TorrentProgressPhase::Download => format!(
            "Download stalled for {idle_secs}s (no progress)."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_roundtrip() {
        let p = TorrentProgressLine {
            phase: TorrentProgressPhase::Metadata,
            state: "live".into(),
            downloaded: 0,
            total: 0,
            peers: 12,
            seeds: 3,
            speed_bps: 0.0,
        };
        let s = p.format();
        let parsed = parse_progress_line(&s).expect("parse");
        assert_eq!(parsed, p);
    }

    #[test]
    fn legacy_progress_without_phase() {
        let line = "jyroxa-torrent-progress downloaded=100 total=1000 peers=2 seeds=1 speed_bps=50000";
        let p = parse_progress_line(line).unwrap();
        assert_eq!(p.phase, TorrentProgressPhase::Download);
        assert_eq!(p.downloaded, 100);
    }

    #[test]
    fn error_lines() {
        assert_eq!(
            parse_error_line("error: boom"),
            Some("boom".into())
        );
        assert_eq!(
            parse_error_line("jyroxa-torrent-error meta timeout"),
            Some("meta timeout".into())
        );
    }
}