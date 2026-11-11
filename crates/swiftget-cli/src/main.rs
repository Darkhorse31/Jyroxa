//! SwiftGet command-line interface.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};

use swiftget_core::download::{filename_from_url, Control};
use swiftget_core::events::{self, ProgressEvent};
use swiftget_core::{
    DownloadConfig, DownloadRequest, DownloadState, Downloader, GrabOptions, HostStats, LinkGrabber,
};

#[derive(Parser)]
#[command(
    name = "swiftget",
    version,
    about = "SwiftGet — a fast multi-connection download manager with a LinkGrabber"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download a URL with multiple connections (and optional mirrors).
    Get {
        /// The URL to download.
        url: String,
        /// Max connections.
        #[arg(short = 'c', long, default_value_t = 8)]
        connections: usize,
        /// Additional mirror URLs for the same file (repeatable).
        #[arg(long = "mirror")]
        mirrors: Vec<String>,
        /// Output path (file or directory). Defaults to the URL's filename.
        #[arg(short = 'o', long)]
        output: Option<PathBuf>,
        /// Expected SHA-256 hex digest to verify.
        #[arg(long)]
        sha256: Option<String>,
        /// Expected MD5 hex digest to verify.
        #[arg(long)]
        md5: Option<String>,
        /// Speed limit, e.g. 5M, 500K (per download).
        #[arg(long)]
        limit: Option<String>,
        /// Disable per-host connection auto-tuning.
        #[arg(long)]
        no_autotune: bool,
    },
    /// Extract downloadable links from a URL, file or text (LinkGrabber).
    Grab {
        /// A URL, a local file, or raw text containing links.
        input: String,
        /// Emit JSON instead of a table.
        #[arg(long)]
        json: bool,
        /// Crawl discovered HTML pages one level deep.
        #[arg(long)]
        deep: bool,
        /// Keep only these extensions (comma-separated, e.g. zip,mp4).
        #[arg(long)]
        ext: Option<String>,
        /// Only keep links whose host contains this substring.
        #[arg(long)]
        host: Option<String>,
        /// Minimum size, e.g. 1M.
        #[arg(long)]
        min_size: Option<String>,
        /// Maximum size, e.g. 2G.
        #[arg(long)]
        max_size: Option<String>,
        /// Skip probing (extraction only, much faster).
        #[arg(long)]
        no_probe: bool,
    },
    /// Resume a download from its `.sgstate` file.
    Resume {
        /// Path to the `<output>.sgstate` file.
        statefile: PathBuf,
        /// Max connections to use on resume.
        #[arg(short = 'c', long, default_value_t = 8)]
        connections: usize,
    },
    /// Benchmark single- vs multi-connection throughput for a URL.
    Bench {
        /// The URL to benchmark.
        url: String,
        /// Connection count for the multi-connection run.
        #[arg(short = 'c', long, default_value_t = 16)]
        connections: usize,
        /// Directory for temporary benchmark files (kept inside the project).
        #[arg(long, default_value = ".swiftget-bench")]
        dir: PathBuf,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = real_main().await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn real_main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Get {
            url,
            connections,
            mirrors,
            output,
            sha256,
            md5,
            limit,
            no_autotune,
        } => {
            let limit = limit.as_deref().map(parse_size).transpose()?;
            run_get(url, connections, mirrors, output, sha256, md5, limit, !no_autotune).await
        }
        Command::Grab {
            input,
            json,
            deep,
            ext,
            host,
            min_size,
            max_size,
            no_probe,
        } => {
            let opts = GrabOptions {
                deep,
                include_ext: ext.map(|e| {
                    e.split(',')
                        .map(|s| s.trim().trim_start_matches('.').to_lowercase())
                        .filter(|s| !s.is_empty())
                        .collect()
                }),
                min_size: min_size.as_deref().map(parse_size).transpose()?,
                max_size: max_size.as_deref().map(parse_size).transpose()?,
                host_filter: host,
                concurrency: 16,
                no_probe,
            };
            run_grab(input, json, opts).await
        }
        Command::Resume {
            statefile,
            connections,
        } => run_resume(statefile, connections).await,
        Command::Bench {
            url,
            connections,
            dir,
        } => run_bench(url, connections, dir).await,
    }
}

fn make_engine(limit: Option<u64>, autotune: bool, fresh_host_stats: bool) -> Result<Arc<Downloader>> {
    let mut config = DownloadConfig::default();
    config.autotune = autotune;
    let host_stats = if fresh_host_stats {
        Arc::new(HostStats::in_memory())
    } else {
        Arc::new(HostStats::with_file(HostStats::default_path()))
    };
    let engine = Downloader::new(config)?
        .with_global_limit(limit)
        .with_host_stats(host_stats);
    Ok(Arc::new(engine))
}

#[allow(clippy::too_many_arguments)]
async fn run_get(
    url: String,
    connections: usize,
    mirrors: Vec<String>,
    output: Option<PathBuf>,
    sha256: Option<String>,
    md5: Option<String>,
    limit: Option<u64>,
    autotune: bool,
) -> Result<()> {
    let engine = make_engine(None, autotune, false)?;
    let out = resolve_output(&engine, &url, output.as_deref()).await?;

    let mut req = DownloadRequest::new(url, out.clone()).connections(connections);
    for m in mirrors {
        req = req.mirror(m);
    }
    req.sha256 = sha256;
    req.md5 = md5;
    req.speed_limit = limit;
    req.autotune = autotune;

    println!("→ downloading to {}", out.display());

    let (tx, mut rx) = events::channel();
    let control = Control::new();
    let dl = engine.clone();
    let handle = tokio::spawn(async move { dl.download(req, Some(tx), control, None).await });

    render_events(&mut rx).await;

    let outcome = handle.await.context("download task panicked")??;
    println!(
        "\n✓ done: {} in {:.1}s  (avg {}/s, {} connections)",
        human_bytes(outcome.total_size),
        outcome.elapsed.as_secs_f64(),
        human_bytes(outcome.average_bps as u64),
        outcome.connections_used
    );
    Ok(())
}

async fn run_resume(statefile: PathBuf, connections: usize) -> Result<()> {
    let state = DownloadState::load(&statefile)
        .context("reading state file")?
        .ok_or_else(|| anyhow!("state file not found: {}", statefile.display()))?;
    if state.urls.is_empty() {
        return Err(anyhow!("state file has no URLs"));
    }
    println!(
        "→ resuming {} ({} already) with up to {} connections",
        state.output.display(),
        human_bytes(state.bytes_downloaded()),
        connections
    );

    let engine = make_engine(None, true, false)?;
    let mut req = DownloadRequest::new(state.urls[0].clone(), state.output.clone())
        .connections(connections);
    for m in state.urls.iter().skip(1) {
        req = req.mirror(m.clone());
    }

    let (tx, mut rx) = events::channel();
    let control = Control::new();
    let dl = engine.clone();
    let handle = tokio::spawn(async move { dl.download(req, Some(tx), control, None).await });
    render_events(&mut rx).await;
    let outcome = handle.await.context("download task panicked")??;
    println!(
        "\n✓ done: {} in {:.1}s (avg {}/s)",
        human_bytes(outcome.total_size),
        outcome.elapsed.as_secs_f64(),
        human_bytes(outcome.average_bps as u64)
    );
    Ok(())
}

async fn run_grab(input: String, json: bool, opts: GrabOptions) -> Result<()> {
    // If `input` is a local file, read its contents as the grab text.
    let effective = if !input.starts_with("http://")
        && !input.starts_with("https://")
        && std::path::Path::new(&input).is_file()
    {
        std::fs::read_to_string(&input).context("reading input file")?
    } else {
        input.clone()
    };

    let client = swiftget_core::build_client()?;
    let grabber = LinkGrabber::new(client);
    let packages = grabber.grab(&effective, &opts).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&packages)?);
        return Ok(());
    }

    let total: usize = packages.iter().map(|p| p.links.len()).sum();
    if total == 0 {
        println!("No links found.");
        return Ok(());
    }
    for pkg in &packages {
        println!("\n📦 {} ({} links)", pkg.name, pkg.links.len());
        println!("  {:<40}  {:>10}  {:<22}  {:>5}", "FILENAME", "SIZE", "TYPE", "RESUME");
        println!("  {}", "-".repeat(84));
        for link in &pkg.links {
            let name = link
                .filename
                .clone()
                .or_else(|| url::Url::parse(&link.url).ok().and_then(|u| filename_from_url(&u)))
                .unwrap_or_else(|| link.url.clone());
            let size = link.size.map(human_bytes).unwrap_or_else(|| "?".into());
            let mime = link.mime.clone().unwrap_or_else(|| "-".into());
            println!(
                "  {:<40}  {:>10}  {:<22}  {:>5}",
                truncate(&name, 40),
                size,
                truncate(&mime, 22),
                if link.resumable { "yes" } else { "no" }
            );
        }
    }
    println!("\n{total} link(s) in {} package(s).", packages.len());
    Ok(())
}

async fn run_bench(url: String, connections: usize, dir: PathBuf) -> Result<()> {
    std::fs::create_dir_all(&dir).context("creating bench dir")?;

    println!("SwiftGet benchmark for {url}\n");

    // Single-connection run.
    let out1 = dir.join("bench_single.part");
    let (mbps1, _) = bench_run(&url, 1, false, &out1).await?;
    cleanup(&out1);
    println!("  1 connection    : {mbps1:>8.2} MB/s");

    // Multi-connection run.
    let outn = dir.join("bench_multi.part");
    let out_auto = dir.join("bench_autotune.part");
    let mbpsn = match bench_run(&url, connections, false, &outn).await {
        Ok((mbps, _)) => {
            cleanup(&outn);
            println!("  {connections:>2} connections  : {mbps:>8.2} MB/s", connections = connections);
            Some(mbps)
        }
        Err(e) => {
            cleanup(&outn);
            eprintln!("  {connections:>2} connections  : FAILED — {e:#}", connections = connections);
            None
        }
    };

    match bench_run(&url, connections, true, &out_auto).await {
        Ok((mbps, conns)) => {
            cleanup(&out_auto);
            println!(
                "  autotune ({connections} max) : {mbps:>8.2} MB/s  (ended {conns} conn, best {conns})",
                connections = connections,
                conns = conns,
            );
        }
        Err(e) => {
            cleanup(&out_auto);
            eprintln!("  autotune ({connections} max) : FAILED — {e:#}", connections = connections);
        }
    }

    if let Some(mbpsn) = mbpsn {
        let speedup = if mbps1 > 0.0 { mbpsn / mbps1 } else { 0.0 };
        println!("\n  speedup         : {speedup:>8.2}x");
    } else {
        println!("\n  speedup         :     n/a (multi-connection run failed)");
    }

    // Best-effort cleanup of the bench dir if empty.
    let _ = std::fs::remove_dir(&dir);
    Ok(())
}

async fn bench_run(
    url: &str,
    connections: usize,
    autotune: bool,
    out: &std::path::Path,
) -> Result<(f64, usize)> {
    let engine = make_engine(None, autotune, true)?;
    let req = DownloadRequest::new(url.to_string(), out.to_path_buf()).connections(connections);
    let control = Control::new();
    let (tx, mut rx) = events::channel();
    let start = Instant::now();
    let dl = engine.download(req, Some(tx), control, None);
    let mut last_conns = connections;
    let consumer = async {
        while let Some(ev) = rx.recv().await {
            if let ProgressEvent::ConnectionsChanged { count, .. } = ev {
                last_conns = count;
            }
        }
    };
    let outcome = tokio::join!(dl, consumer).0?;
    let secs = start.elapsed().as_secs_f64().max(1e-9);
    let mbps = outcome.total_size as f64 / secs / 1_000_000.0;
    let conns = outcome.connections_used.max(last_conns);
    Ok((mbps, conns))
}

fn cleanup(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let _ = std::fs::remove_file(DownloadState::state_path(path));
}

/// Render progress events to stderr until the channel closes.
async fn render_events(rx: &mut events::EventReceiver) {
    while let Some(ev) = rx.recv().await {
        match ev {
            ProgressEvent::Started {
                total_size,
                connections,
                resumed_from,
                supports_ranges,
            } => {
                let size = total_size.map(human_bytes).unwrap_or_else(|| "unknown".into());
                eprintln!(
                    "  size={size} connections={connections} resumed_from={} ranges={}",
                    human_bytes(resumed_from),
                    supports_ranges
                );
            }
            ProgressEvent::Progress {
                downloaded,
                total,
                speed_bps,
                connections,
                ..
            } => {
                let pct = total
                    .map(|t| format!("{:>5.1}%", downloaded as f64 / t.max(1) as f64 * 100.0))
                    .unwrap_or_else(|| "  ?  ".into());
                eprint!(
                    "\r  {pct}  {:>10}  {:>10}/s  conns={connections}   ",
                    human_bytes(downloaded),
                    human_bytes(speed_bps as u64),
                );
                use std::io::Write;
                let _ = std::io::stderr().flush();
            }
            ProgressEvent::ConnectionsChanged { count, reason } => {
                eprintln!("\n  [autotune] connections → {count} ({reason})");
            }
            ProgressEvent::MirrorDemoted { url } => {
                eprintln!("\n  [mirror] demoted {url}");
            }
            ProgressEvent::LinkRefreshed { old, new } => {
                eprintln!("\n  [link] refreshed {old} → {new}");
            }
            ProgressEvent::Retrying { attempt, error } => {
                eprintln!("\n  [retry #{attempt}] {error}");
            }
            ProgressEvent::Failed { error } => {
                eprintln!("\n  [failed] {error}");
            }
            ProgressEvent::Completed { .. } | ProgressEvent::SegmentProgress { .. } => {}
        }
    }
}

/// Resolve the output path: honour `-o`, treating directories specially, and
/// otherwise derive a filename from the URL (probing for Content-Disposition).
async fn resolve_output(
    engine: &Downloader,
    url: &str,
    output: Option<&std::path::Path>,
) -> Result<PathBuf> {
    let parsed = url::Url::parse(url).ok();
    let url_name = parsed.as_ref().and_then(filename_from_url);

    match output {
        Some(p) if p.is_dir() => {
            let name = probe_name(engine, url).await.or(url_name).unwrap_or_else(|| "download.bin".into());
            Ok(p.join(name))
        }
        Some(p) => {
            // Treat a trailing separator as a directory request.
            let s = p.to_string_lossy();
            if s.ends_with('/') || s.ends_with('\\') {
                std::fs::create_dir_all(p).ok();
                let name = url_name.unwrap_or_else(|| "download.bin".into());
                Ok(p.join(name))
            } else {
                Ok(p.to_path_buf())
            }
        }
        None => {
            let name = probe_name(engine, url)
                .await
                .or(url_name)
                .unwrap_or_else(|| "download.bin".into());
            Ok(PathBuf::from(name))
        }
    }
}

async fn probe_name(engine: &Downloader, url: &str) -> Option<String> {
    let grabber = LinkGrabber::new(engine.client().clone());
    let info = grabber.probe_link(url).await;
    info.filename
}

// --------------------------- small helpers ---------------------------

fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(anyhow!("empty size"));
    }
    let (num, mult) = match s.chars().last().unwrap().to_ascii_uppercase() {
        'K' => (&s[..s.len() - 1], 1024u64),
        'M' => (&s[..s.len() - 1], 1024 * 1024),
        'G' => (&s[..s.len() - 1], 1024 * 1024 * 1024),
        'B' => (&s[..s.len() - 1], 1),
        _ => (s, 1),
    };
    let value: f64 = num
        .trim()
        .parse()
        .with_context(|| format!("invalid size: {s}"))?;
    Ok((value * mult as f64) as u64)
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", UNITS[i])
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_parsing() {
        assert_eq!(parse_size("1024").unwrap(), 1024);
        assert_eq!(parse_size("5M").unwrap(), 5 * 1024 * 1024);
        assert_eq!(parse_size("500K").unwrap(), 500 * 1024);
        assert_eq!(parse_size("2G").unwrap(), 2 * 1024 * 1024 * 1024);
        assert!(parse_size("abc").is_err());
    }

    #[test]
    fn human_readable() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1024), "1.00 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.00 MB");
    }
}
