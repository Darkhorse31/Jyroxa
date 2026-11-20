# HTTP transfer review

Reference: Jyroxa revision `96d9d1b` and [Ghost Downloader revision
`0c56e253df731fd3b0628135cca10e373f0b5260`](https://github.com/XiaoYouChR/Ghost-Downloader-3/tree/0c56e253df731fd3b0628135cca10e373f0b5260),
inspected on 2026-11-20. The latter identifies itself as version 4.5.1;
it is the current source in the repository named Ghost-Downloader-3.
No Ghost source was incorporated into Jyroxa.

Before creating a transfer, Ghost's `features/http_pack/pack.py:41-125` performs
a `GET` with `Range: bytes=1-1`, identity encoding, and a 30-second total timeout.
If that returns 200 with unknown length or length one, it additionally tries
`bytes=0-0`. It does not use HEAD here. The initial transfer request and the
eight-way split described below happen after that metadata preparation.

## Source comparison

Ghost references below are relative to its pinned repository. Jyroxa baseline
line numbers refer to the revision above; function names identify the changed code.

| Concern | Ghost source and exact behavior | Jyroxa baseline | Change |
| --- | --- | --- | --- |
| Metadata/start | `features/http_pack/task.py:171-179,393-429`: one initial range, confirm the first response, split its remainder into the configured count (default eight). | Prior: HEAD then `bytes=0-0`. Now: `bytes=0-` lead stream on the winning mirror while mirrors race; segment zero continues on the same connection; other workers start as soon as size is known. Fallback `bytes=0-0` when open range fails. | No separate metadata-only round trip on capable servers. |
| Initial concurrency | `task.py:100,428`: eight default subworkers; resumed tasks fill up to that count at `523-525`. | `tuner.rs` + `hoststats.rs`: default eight; optional start at persisted best count per host; clamp by throttle cap and `max_connections`. | `.jyroxa/hoststats.json` remembers best count/throughput (7-day decay) and throttle caps (1-hour decay). |
| Idle worker | `task.py:188-207,216-223,477`: split the largest unreceived remainder in half, create a replacement task when a worker completes. Minimum reassignment remainder is 512 KiB (`app/config/cfg.py:257-258`). | `segment.rs:305-440`: largest-remainder stealing, with a shared bounds lock preventing overlap with buffered network bytes. Default minimum is 512 KiB; the second stealing pass permits a half smaller than that. | Preserve this allocator. Notify readers when a new worker claims work so a shortened, completed range can stop reading promptly. |
| Connection growth | `task.py:225-265,290-291`: one-second samples; five samples must be within 15% of their mean. Add four splits; after more than five seconds, stop accelerating if relative speed gain is below 0.8 times relative subworker growth. Existing successful workers remain. | `tuner.rs`: median samples; ~2 s to first 8→16 trial at 500 ms tick; 10% aggregate gain required to accept 32; 3% for earlier doublings; noise never retires connections. | Faster ramp than Ghost on fast links without early stop on single slow ticks. |
| Buffers/chunks | `task.py:316-322,430-443`: consume `response.stream()` chunks; no fixed application chunk size or aggregation buffer configured. | `download.rs:139,1310-1347`: aggregate 2 MiB per segment, bounded queue of 64 buffers, one persistent blocking positional writer. `mem::take` leaves each producer with zero capacity after each flush. | Recycle completed 2 MiB allocations through a bounded pool. Single-stream fallback also batches 2 MiB writes asynchronously and limits progress notifications. |
| Output allocation | `app/platform/sysio.py:27-36`: Windows sparse-file flag then file-end information; Unix `ftruncate`. `task.py:509-513` sets final size before transfer. No merge pass. | `download.rs:1807-1817`: set file length before positioned writes; committed disk cursors separate from received cursors. | Preserve preallocation and disk-confirmed checkpoints. Reject a resume checkpoint when its payload is absent or has the wrong length. |
| HTTP client | `app/client.py:96-150`: wreq client, browser emulation, redirect cap 10, configured proxy or explicit no-proxy. Each subworker request builds and closes its own client (`task.py:392,474`). No explicit HTTP version, HTTP/2 window, pool, TCP keepalive, or connect-timeout value is set here: those are library/emulation defaults. | `lib.rs:132-154`: shared HTTP/1.1 segment client, idle pool 32, idle lifetime 90 s, TCP_NODELAY, keepalive 30 s, connect timeout 20 s, read timeout 30 s. General client supports HTTP/2 adaptive windows. | Preserve transport settings; metadata now uses the segment pool. Force `Accept-Encoding: identity` on range requests to keep byte offsets stable. TLS certificate verification stays enabled. |
| Read timeout | `task.py:21,392`: 30-second stream read timeout. | `download.rs:141,1497-1505`: five-second per-segment watchdog plus client read timeout. Original absolute watchdog includes time spent flushing/rate-limiting/paused before the next read. | Bound network idle time by the smaller of five seconds and configured read timeout; exclude local buffering/rate limiting and pauses. Re-split stalled remainders and retry with jittered backoff. |
| Retry | `task.py:456-474`: permanent statuses fail; untouched rejected ranges may be merged by their left neighbor; other transient errors on a resumable range sleep five seconds and retry without a fixed retry count. | `download.rs:1436-1468`: 429/503 requeue then return success, resetting the local retry count indefinitely. `1594`: clean short EOF resets retries even when zero bytes arrived. | Throttle count belongs to the segment across handoffs. Empty successful responses consume retries with backoff. Exhaustion participates in the existing download failure budget. |
| Range integrity | `task.py:33-43,421-426`: check Content-Range and fall back on mismatch. | Segment responses were accepted on status alone; no Content-Range validation. | Validate start, inclusive end, and representation size before accepting data. No wrong-offset response reaches the writer. |
| Shutdown/fallback | `task.py:519-546`: structured task group exits before fallback truncates the file. | `download.rs:943-967`: bounded polling of workers, then attempted removal while the writer could still own the output handle; removal errors ignored. | Abort/join remaining network workers, close the queue, and join the writer before fallback or cancellation changes the output. Checkpoints after failure contain completed disk writes. |
| Early control | Ghost uses task status and structured cancellation. | `download.rs:59-66`: watch-channel `send` discards pause/cancel while there are no receivers, including before the first worker starts. Metadata probing was not cancellation-aware. | Retain control state with `send_replace` and observe it while probing. A cancellation issued before download startup remains effective. |

## Reproductions and measurement

`crates/jyroxa-core/tests/http_reliability.rs` supplies permanent 429/503,
empty partial responses, wrong range headers, stalled reads, stalled metadata,
slow HEAD, missing resume payload, and connection-cap cases. Every network
operation in these tests has an outer deadline. Existing integration tests cover
buffered range splitting, disconnect recovery, pause/cancel, checksums and fallback.

Build `http_measure` in release mode, preserve an executable built against the
original source, then run `scripts/http-benchmark.py` against each executable.
The script keeps individual JSON measurements and SHA-256 digests, supports
public URLs and deterministic local unlimited/per-connection-limited sources,
and can kill its own transfer process and verify a resumed payload. MB means
1,000,000 bytes. First-50MB timing comes from progress events, so resolution is
up to one default 500 ms supervisor tick. End-to-end throughput includes
metadata/startup; the harness's extra hash calculation occurs after timing.

Ghost's CLI controls its desktop runtime (`app/cli.py:56-60`,
`Ghost-Downloader-3.py:395-418`), and the checked source requires Python 3.13
plus Qt/wreq dependencies. It was inspected without launching a desktop window.
The comparison above is a source comparison, not a measured speed claim about
Ghost. Public throughput claims require the same source, route and machine;
loopback results establish local engine capacity rather than Internet capacity.

The default metadata client shares the segment pool; `with_client` continues to
override metadata and single-stream requests, preserving caller configuration.
Metadata probes race up to four mirrors within the configured connection limit,
so an unresponsive primary cannot serialize healthy alternatives behind its
retry budget. Header-free transfers reuse the selected mirror's resolved URL
instead of repeating redirects on every segment; requests carrying custom
headers retain the original redirect behavior.
Segment response headers and body reads both have an idle deadline; completed
requests have no total deadline that would cut off a long but healthy transfer.
When growing from eight to sixteen workers, reconciliation now prepares eight
queued tails for the eight new workers, rather than fifteen. This avoids
unnecessary range fragmentation and extra requests during ramp-up.
Consecutive retry counts follow each range across worker handoffs and reset
only after received bytes advance its cursor. The global worker-failure limit
cannot multiply an exhausted range's retry budget. Expired-link refresh hooks
also have a deadline and consume that budget, including hooks returning the same
expired URL repeatedly. Error responses and stalled streams are dropped before
backoff so they do not hold server connection slots while sleeping.

Torrent source, tuning and transport settings are unchanged. Workspace tests and
the separate task report record the actual verification results and limits.

The GNU compiler wrapper now appends its linker arguments, preserving a leading
nested compiler path. This permits Cargo's workspace compiler wrapper used by
Clippy to identify the real compiler correctly. The earlier argument insertion
made even Clippy's target-information query fail with multiple input filenames.
