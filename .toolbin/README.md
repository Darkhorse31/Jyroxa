# Windows GNU build helpers

Scripts and sources here support linking on `x86_64-pc-windows-gnu` when the Rust
bundled MinGW cannot run GNU `dlltool`. See [docs/BUILD_WINDOWS_GNU.md](../docs/BUILD_WINDOWS_GNU.md).

**Tracked in git:** `*.ps1`, `dlltool_wrap.rs`, this README.

**Generated locally (gitignored):** `winlibs/` import archives, `defs/` `.def` files,
`dlltool-wrap.exe`, copies of `llvm-dlltool.exe`, and other downloaded binaries.

`windres-wrap.rs` optionally forwards GNU resource flags to LLVM windres. Build
it with `rustc --edition 2021 .toolbin/windres-wrap.rs -O -o .toolbin/windres.exe`.
Set `ANDROID_NDK_HOME` or `JYROXA_LLVM_WINDRES` to the LLVM backend. The NDK's
Clang preprocesses resources before LLVM emits a COFF object; plain `llvm-rc`
output is a `.res` file and cannot replace that object.
