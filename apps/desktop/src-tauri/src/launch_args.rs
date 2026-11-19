//! Parse magnet / torrent / HTTP URLs from a second desktop instance's argv.

use jyroxa_core::torrent::is_torrent_source;
use std::path::Path;

/// First non-flag argument that should open the Add dialog.
pub fn launch_target_from_args<I>(args: I) -> Option<String>
where
    I: IntoIterator<Item = String>,
{
    args.into_iter()
        .skip(1)
        .find_map(|a| {
            let t = strip_launch_arg(&a);
            is_launch_target(t).then(|| t.to_string())
        })
}

fn strip_launch_arg(s: &str) -> &str {
    s.trim().trim_matches('"')
}

fn is_launch_target(s: &str) -> bool {
    if s.is_empty() || s.starts_with('-') {
        return false;
    }
    if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("magnet:") {
        return true;
    }
    if is_torrent_source(s) {
        return true;
    }
    let path = Path::new(s);
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("torrent"))
        .unwrap_or(false)
        && path.is_file()
}

/// Whether a single-instance pipe payload should become `external-url`.
pub fn should_emit_external_url(msg: &str) -> bool {
    let t = strip_launch_arg(msg);
    !t.is_empty()
        && (t.starts_with("http://")
            || t.starts_with("https://")
            || is_torrent_source(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_magnet_in_argv() {
        let args = vec![
            "jyroxa-desktop.exe".into(),
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567".into(),
        ];
        let t = launch_target_from_args(args).expect("magnet");
        assert!(t.starts_with("magnet:"));
    }

    #[test]
    fn skips_flags_before_url() {
        let args = vec![
            "jyroxa-desktop.exe".into(),
            "--minimized".into(),
            "https://example.com/file.bin".into(),
        ];
        assert_eq!(
            launch_target_from_args(args).as_deref(),
            Some("https://example.com/file.bin")
        );
    }

    #[test]
    fn quoted_torrent_path() {
        let path = r#"C:\Users\me\game.torrent"#;
        let args = vec!["jyroxa-desktop.exe".into(), format!("\"{path}\"")];
        if Path::new(path).is_file() {
            assert_eq!(launch_target_from_args(args).as_deref(), Some(path));
        }
    }

    #[test]
    fn emit_includes_magnet_and_torrent_url() {
        assert!(should_emit_external_url(
            "magnet:?xt=urn:btih:0123456789abcdef0123456789abcdef01234567"
        ));
        assert!(should_emit_external_url(
            "https://releases.example/ubuntu.torrent"
        ));
        assert!(!should_emit_external_url(""));
        assert!(!should_emit_external_url("not-a-url"));
    }
}