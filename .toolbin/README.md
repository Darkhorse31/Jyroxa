# Windows GNU build helpers

Scripts and sources here support linking on `x86_64-pc-windows-gnu` when the Rust
bundled MinGW cannot run GNU `dlltool`. See [docs/BUILD_WINDOWS_GNU.md](../docs/BUILD_WINDOWS_GNU.md).

**Tracked in git:** `*.ps1`, `dlltool_wrap.rs`, this README.

**Generated locally (gitignored):** `winlibs/` import archives, `defs/` `.def` files,
`dlltool-wrap.exe`, copies of `llvm-dlltool.exe`, and other downloaded binaries.
