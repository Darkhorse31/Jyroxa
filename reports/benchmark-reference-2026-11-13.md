# Historical benchmark references — 2026-11-13

These measurements predate the v0.1.3 release confirmation. They were imported from the existing local reports under `C:\Users\lenovo\.jyroxa-tools\reports`; IDM was not launched during this release task.

| Server | Jyroxa v0.1.2 auto (decimal MB/s) | IDM 6.42 reference (MiB/s) | IDM converted (decimal MB/s) |
| --- | ---: | ---: | ---: |
| Singapore (`sin-speed.hetzner.com`) | 5.52 | 10.08 | 10.56964608 |
| Falkenstein (`fsn1-speed.hetzner.com`) | 5.36 | 6.72 | 7.04643072 |

Same Windows PC, same Hetzner `100MB.bin` files (104,857,600 bytes each), three runs per tool/mode, ten-second cooldowns. The IDM version and same-PC comparison are supplied release references. The old Jyroxa CLI identified as 0.1.2 at revision `09a9553`.

Source `bench-2026-11-13.md` labels its table MB/s but says "1024² B/s unless noted". The archived IDM CSV confirms that its throughput was computed as 100 MiB divided by elapsed seconds, whereas Jyroxa's CLI divides bytes by 1,000,000. Preserve the requested IDM numbers as MiB/s; multiply by 1.048576 before calculating cross-tool speedups or drawing bars in decimal MB/s. The [six original IDM CSV records](idm-reference-2026-11-13.csv) are copied unchanged, including their historical `mb_per_s` column name. Conversion uses the published two-decimal reference medians, so ratios are approximate.

The [earlier auto-mode report](beat-idm-2026-11-13.md) measured Singapore 21.60 and Falkenstein 10.37 decimal MB/s, with 10–30 second cooldowns. Its original IDM comparisons did not account for the unit mismatch. Its [CLI output](beat-idm-2026-11-13-bench.log) is preserved as readable text with NUL bytes from mixed PowerShell log encodings removed. Those earlier values are context, not substitutions for the release confirmation results.

Public internet throughput varies with routing, server load and throttling; these separately scheduled historical references do not establish a universal speed advantage. Ghost Downloader could not be benchmarked headlessly yet; no Ghost throughput is reported.
