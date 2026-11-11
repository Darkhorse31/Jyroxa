# SwiftGet Benchmark

Date: 2026-11-11

Command:

```
swiftget bench https://proof.ovh.net/files/100Mb.dat -c 16
```

The `bench` subcommand downloads the target once with a single connection and
once with N connections, reporting the throughput of each and the resulting
speedup. Downloaded test files are deleted immediately after measurement.

## Results

Host: Windows 10, `x86_64-pc-windows-gnu` toolchain. Release build
(`cargo build --release`, `opt-level = 3`, thin LTO).

### proof.ovh.net (100 MB)

| Run             | Throughput |
| --------------- | ---------- |
| 1 connection    | 16.45 MB/s |
| 16 connections  | 18.97 MB/s |
| **Speedup**     | **1.15x**  |

On this well-provisioned origin a single HTTP/2 stream already comes close to
saturating the available downstream bandwidth, so the multi-connection gain is
modest. The advantage of segmented downloading is larger against origins that
rate-limit or throttle per connection, or over lossy links where a stalled
segment is isolated from the others.

### ash-speed.hetzner.com (100 MB) — not completed

The secondary mirror was attempted as well. Its single-connection run measured
~0.53 MB/s (the server is geographically distant from the test host), and the
multi-connection run was rejected with `HTTP 429 Too Many Requests` — the server
throttles concurrent range requests from one client. SwiftGet surfaced the 429
correctly (demoting the mirror and ultimately reporting the status), which is the
intended behaviour; no partial files were left behind after cleanup.

## Notes

- `tele2.net` (`http://speedtest.tele2.net/100MB.zip`) is listed in the task as a
  further fallback if the two above are unreachable.
- Numbers depend heavily on the test host's uplink, time of day, and the origin's
  per-connection policy; treat them as indicative rather than absolute.
