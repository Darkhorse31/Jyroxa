# SwiftGet Benchmark

Date: 2026-11-10

Command:

```
swiftget bench <url> -c 16
```

The `bench` subcommand downloads the target once with a single connection, once
with N fixed connections, and once with connection auto-tuning (up to N). It
reports throughput for each run and the single-vs-fixed speedup. If the fixed
multi-connection run fails, the single-connection result is still printed and
the failure is reported clearly. Downloaded test files are deleted immediately
after measurement.

## Results

Host: Windows 10, `x86_64-pc-windows-gnu` toolchain. Release build
(`cargo build --release`, `opt-level = 3`, thin LTO).

### sin-speed.hetzner.com (100 MB)

| Run                    | Throughput |
| ---------------------- | ---------- |
| 1 connection           | 0.55 MB/s  |
| 16 connections         | 1.41 MB/s  |
| autotune (16 max)      | 6.06 MB/s  |
| **Speedup (16 vs 1)**  | **2.54×**  |

Singapore test file; high latency from the benchmark host. Auto-tuning ramps
connections while backing off after transport errors, which helps on this path.

### fsn1-speed.hetzner.com (100 MB)

| Run                    | Throughput |
| ---------------------- | ---------- |
| 1 connection           | 10.09 MB/s |
| 16 connections         | 3.36 MB/s  |
| autotune (16 max)      | 1.47 MB/s  |
| **Speedup (16 vs 1)**  | **0.33×**  |

Falkenstein (EU) mirror. A single HTTP/2 stream already achieves most of the
available throughput from this host; opening 16 range requests concurrently
contended and slowed the transfer. Auto-tune reduced parallelism after errors but
did not beat the single-connection run on this sample.

### proof.ovh.net (100 MB) — historical (2026-11-11)

Earlier run when this origin was reachable from the test host:

| Run             | Throughput |
| --------------- | ---------- |
| 1 connection    | 16.45 MB/s |
| 16 connections  | 18.97 MB/s |
| **Speedup**     | **1.15×**  |

On that well-provisioned origin a single stream nearly saturated downstream
bandwidth, so the multi-connection gain was modest.

## Notes

- `proof.ovh.net` and `tele2.net` were unreachable from the 2026-11-10 host;
  Hetzner speedtest binaries were used instead.
- Numbers depend heavily on the test host's uplink, time of day, and the origin's
  per-connection policy; treat them as indicative rather than absolute.
