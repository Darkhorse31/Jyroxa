//! HLS / DASH manifest parsing and parallel segment download.

mod dash;
mod hls;
mod runner;

pub use dash::{dash_segment_urls, parse_mpd, DashVariant, MpdInfo};
pub use hls::{parse_m3u8, HlsPlaylist, HlsSegment, HlsVariant};
pub use runner::{is_stream_manifest_url, stream_download, StreamDownloadResult};
