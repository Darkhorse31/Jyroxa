# Jyroxa benchmark notes

Date: 2026-11-10 (pre-release name SwiftGet)

Command:

```bash
jyroxa bench <url> -c 16
```

The `bench` subcommand downloads the target once with a single connection, once
with N fixed connections, and once with connection auto-tuning (up to N). It
reports throughput for each run and the single-vs-fixed speedup. Downloaded test
files are deleted after measurement (default dir `.jyroxa-bench`).

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

### fsn1-speed.hetzner.com (100 MB)

| Run                    | Throughput |
| ---------------------- | ---------- |
| 1 connection           | 10.09 MB/s |
| 16 connections         | 3.36 MB/s  |
| autotune (16 max)      | 10.56 MB/s |
| **Speedup (16 vs 1)**  | **0.33×**  |

On this path the fixed 16-connection run was slower than one connection; the
auto-tuner recovered to roughly single-connection throughput.

Historical CLI name was `swiftget`; use `jyroxa` in current builds.
