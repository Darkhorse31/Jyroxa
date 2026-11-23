//! Curated public tracker list merged into every torrent for faster peer discovery.

/// Well-known public trackers (HTTP/UDP). Kept compact; duplicates removed at merge time.
pub const BUILTIN_TRACKERS: &[&str] = &[
    "udp://tracker.opentrackr.org:1337/announce",
    "udp://open.tracker.cl:1337/announce",
    "udp://tracker.torrent.eu.org:451/announce",
    "udp://explodie.org:6969/announce",
    "udp://tracker1.bt.moack.co.kr:80/announce",
    "udp://tracker.theoks.net:6969/announce",
    "udp://opentracker.i2p.rocks:6969/announce",
    "udp://tracker-udp.gbitt.info:80/announce",
    "udp://run.publictracker.xyz:6969/announce",
    "udp://public.tracker.vraphim.com:6969/announce",
    "udp://tracker.tiny-vps.com:6969/announce",
    "udp://open.demonii.com:1337/announce",
    "udp://tracker.openbittorrent.com:6969/announce",
    "udp://exodus.desync.com:6969/announce",
    "udp://open.stealth.si:80/announce",
    "udp://tracker.moeking.me:6969/announce",
    "udp://ipv4.tracker.harry.lu:80/announce",
    "udp://retracker01-msk-virt.corbina.net:80/announce",
    "udp://tracker.denise.hlawrence.com:6969/announce",
    "https://tracker.tamersunion.org:443/announce",
    "http://tracker.openbittorrent.com:80/announce",
    "http://tracker.opentrackr.org:1337/announce",
    "http://open.acgnxtracker.com:80/announce",
    "http://www.peckrocks.com:6969/announce",
];

/// Return built-in tracker announce URLs (owned strings).
pub fn builtin_trackers() -> Vec<String> {
    BUILTIN_TRACKERS.iter().map(|s| (*s).to_string()).collect()
}

/// Merge `extra` trackers into `existing`, preserving order and deduplicating case-insensitively.
pub fn merge_tracker_lists(existing: &[String], extra: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for t in existing.iter().chain(extra.iter()) {
        let key = t.trim().to_ascii_lowercase();
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        out.push(t.trim().to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_dedupes_and_keeps_existing_first() {
        let a = vec![
            "udp://tracker.opentrackr.org:1337/announce".into(),
            "udp://foo:1/announce".into(),
        ];
        let b = builtin_trackers();
        let merged = merge_tracker_lists(&a, &b);
        assert_eq!(merged[0], a[0]);
        assert_eq!(merged[1], a[1]);
        assert!(merged.len() > 2);
        let keys: std::collections::HashSet<_> =
            merged.iter().map(|s| s.to_ascii_lowercase()).collect();
        assert_eq!(keys.len(), merged.len());
    }

    #[test]
    fn builtin_list_nonempty() {
        assert!(BUILTIN_TRACKERS.len() >= 10);
        for t in BUILTIN_TRACKERS {
            assert!(
                t.starts_with("http://") || t.starts_with("https://") || t.starts_with("udp://")
            );
        }
    }
}
