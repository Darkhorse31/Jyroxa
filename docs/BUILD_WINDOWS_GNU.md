# Building on Windows GNU (`x86_64-pc-windows-gnu`)

This optional path is for hosts that use the **GNU** Rust target without a full
MinGW C toolchain (for example rustup's `x86_64-pc-windows-gnu` with LLVM binutils).

**MSVC users:** install the `x86_64-pc-windows-msvc` toolchain and build normally.
Comment out or remove the `[build] rustc-wrapper` and `[env] SWIFTGET_*` entries in
`.cargo/config.toml` (they exist only for the GNU `dlltool` workaround). The
`[target.x86_64-pc-windows-gnu]` block is ignored when you build for MSVC.

## Phase 1 (core, CLI, FFI)

From the repo root:

```powershell
# One-time on GNU hosts (after cloning):
powershell -ExecutionPolicy Bypass -File .toolbin\build-dlltool-wrap.ps1
powershell -ExecutionPolicy Bypass -File .toolbin\gen-winlibs.ps1 kernel32.dll advapi32.dll user32.dll

cargo build --release
cargo test
```

`cargo build` / `cargo test` use workspace `default-members` (core, CLI, FFI only).
The Tauri crate is a workspace member for `cargo metadata`, but is built separately (below).

## raw-dylib workaround

GNU `dlltool` from rustup often cannot run (no bundled `as.exe`), and LLVM `dlltool`
can emit corrupt import tables for large `windows-sys` surfaces. The project therefore:

1. Patches `windows-link` (see `vendor/windows-link` and root `Cargo.toml`
   `[patch.crates-io]`) to use classic `dylib` + `+verbatim` linking.
2. Adds prebuilt import archives in `.toolbin/winlibs/` (generate with
   `.toolbin/gen-winlibs.ps1` and related scripts).
3. Uses `.toolbin/dlltool-wrap.exe` as `-Cdlltool` for small direct `raw-dylib` crates
   (e.g. `getrandom`). Build it with `.toolbin/build-dlltool-wrap.ps1`.

Paths in `.cargo/config.toml` are **workspace-relative** (`.toolbin/...`).

See `.toolbin/README.md` for what is tracked in git vs generated locally.

## Desktop (Tauri)

From `apps/desktop`:

```powershell
npm install
.\build-gnu.ps1
```

`build-gnu.ps1` prepends the repo `.toolbin` directory and the Rust GNU LLVM `bin`
directory to `PATH` for the build (`windres`, `ar`, etc.).

### Extra import libraries for Tauri / WebView2

Tauri pulls a larger `windows-sys` surface than the CLI. After a link error listing a
missing DLL, generate import archives, for example:

```powershell
powershell -ExecutionPolicy Bypass -File .toolbin\gen-winlibs.ps1 <dll-names...>
powershell -ExecutionPolicy Bypass -File .toolbin\gen-from-path.ps1 `
  "C:\Windows\WinSxS\...\comctl32.dll" `
  comctl32.dll
powershell -ExecutionPolicy Bypass -File .toolbin\gen-apiset-path.ps1
```

`gen-apiset-path.ps1` builds `api-ms-win-core-path-l1-1-0.dll` import symbols from
`KernelBase.dll` (`PathCch*`).

If installer bundling fails (e.g. NSIS download timeout), produce the binary only:

```powershell
.\build-gnu.ps1 --no-bundle
```

Output paths are recorded in [BUILD_OUTPUTS.md](./BUILD_OUTPUTS.md).
