# Building on Windows GNU (x86_64-pc-windows-gnu)

This machine does **not** use MSVC. Rust is installed under `E:\.swiftget-tools` with host target `x86_64-pc-windows-gnu`.

## raw-dylib workaround

GNU `dlltool` cannot run (no bundled `as.exe`), and LLVM `dlltool` can emit corrupt import tables for large `windows-sys` surfaces. The project therefore:

1. Patches `windows-link` (see `vendor/windows-link` and root `Cargo.toml` `[patch.crates-io]`) to use classic `dylib` + `+verbatim` linking.
2. Adds prebuilt import archives in `.toolbin/winlibs` (generate with `.toolbin/gen-winlibs.ps1`).
3. Uses `.toolbin/dlltool-wrap.exe` as `-Cdlltool` for small direct `raw-dylib` crates (e.g. `getrandom`).

See `.cargo/config.toml` for `rustflags`.

## Build commands

From repo root (core / CLI):

```powershell
cargo build --release
cargo test --release
```

Desktop (Tauri) from `apps/desktop`:

```powershell
npm install
.\build-gnu.ps1
```

`build-gnu.ps1` puts `E:\SwiftGet\.toolbin` (copy `llvm-windres` / `llvm-ar` from the project NDK as `windres.exe` / `ar.exe` if missing) and the NDK LLVM `bin` directory on `PATH` for the build. Tauri’s build script uses `windres` to embed the application manifest.

### Extra import libraries for Tauri / WebView2

Tauri pulls a larger `windows-sys` surface than the CLI. After the first link failure, generate any missing DLL import archives with:

```powershell
$env:RUSTUP_HOME = "E:\.swiftget-tools\rustup"
# One-off: many DLLs (see build log), then comctl32 v6 + path API set:
powershell -ExecutionPolicy Bypass -File .toolbin\gen-winlibs.ps1 <dll-names...>
powershell -ExecutionPolicy Bypass -File .toolbin\gen-from-path.ps1 `
  "C:\Windows\WinSxS\amd64_microsoft.windows.common-controls_6595b64144ccf1df_6.0.19041.6456_none_60b8a6cb71f64256\comctl32.dll" `
  comctl32.dll
powershell -ExecutionPolicy Bypass -File .toolbin\gen-apiset-path.ps1
```

`gen-apiset-path.ps1` builds `api-ms-win-core-path-l1-1-0.dll` import symbols from `KernelBase.dll` (`PathCch*`).

If installer bundling fails (e.g. NSIS download timeout), produce the binary only:

```powershell
.\build-gnu.ps1 --no-bundle
```

Output paths are recorded in [BUILD_OUTPUTS.md](./BUILD_OUTPUTS.md).
