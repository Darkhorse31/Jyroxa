# Benchmark and distribution scripts

Run from the repository root in Windows PowerShell 5.1 with the build tools on PATH.

```powershell
cargo build --release -p jyroxa-cli -j 3
.\scripts\bench.ps1 -CsvPath bench.csv
# With a separate Cargo target directory:
.\scripts\bench.ps1 -CliPath "$env:CARGO_TARGET_DIR\release\jyroxa.exe" -Repeats 3 -IncludeFixed16 -TimeoutSeconds 180
.\scripts\test-bench.ps1
.\scripts\build-dist.ps1 -OutputDir C:\Users\lenovo\Jyroxa-dist
```

`bench.ps1` accepts `-Urls` as a string array. Each repeat runs every selected
mode before the next repeat, rotating the first mode. The default modes are
one fixed connection and autotune with a maximum of 16. `-IncludeFixed16` adds
16 fixed connections. Each measurement starts with fresh host tuning history.
The CSV is replaced on invocation and flushed after every measurement. Times
include process startup, probing and download completion; MB/s uses decimal
megabytes. Failed and timed-out runs have blank bytes and speed fields and are
excluded from medians. Exit code 124 indicates the harness timeout; -1 indicates
a launch or output error. Download data, state and logs are removed after each
run. Scratch directories live under the repository's ignored `.jyroxa-bench`
directory. Notes retain CLI diagnostics for failures. The summary includes failed counts.

`build-dist.ps1` builds the CLI, installs locked desktop dependencies with
`npm ci`, builds the NSIS installer and portable archive, then builds native
Android libraries and the APK. Android uses release signing when
`android/keystore.properties` exists and debug otherwise. It requires the Rust
Android targets, cargo-ndk, Java, Android SDK/NDK, Node/npm and desktop GNU tools.
Set `ANDROID_NDK_HOME` to the NDK root. `CARGO_TARGET_DIR` is respected and
normalized before entering nested directories. Cargo builds use three jobs.
`SHA256SUMS.txt` lists only the artifacts produced by the completed invocation.
Any failed build stops packaging with an error; a partial output directory is
not a complete distribution.
