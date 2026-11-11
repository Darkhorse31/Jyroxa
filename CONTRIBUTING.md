# Contributing to Jyroxa

Thank you for helping improve Jyroxa. This project is MIT-licensed (Copyright Prateek Kumar);
by contributing you agree that your contributions will be released under the same license.

## Getting started

1. Fork and clone the repository.
2. Install **Rust** (1.75+). On Windows GNU targets, read
   [docs/BUILD_WINDOWS_GNU.md](docs/BUILD_WINDOWS_GNU.md) before your first build.
3. From the repo root:

   ```powershell
   cargo test
   cargo build --release
   ```

4. For UI work, see [apps/desktop](apps/desktop) (Tauri) or [android](android).

## Project layout

- `crates/jyroxa-core` — download engine and LinkGrabber
- `crates/jyroxa-cli` — command-line interface (`jyroxa` binary)
- `crates/jyroxa-ffi` — JNI/C ABI for Android
- `apps/desktop` — Tauri 2 desktop shell
- `android` — Kotlin/Compose Android app (`com.jyroxa.app`)

Keep new Rust logic in `jyroxa-core` when possible so all frontends benefit.

## Coding guidelines

- **Rust**: match existing style (`thiserror` in the library, `anyhow` in binaries).
- **TypeScript** (desktop): vanilla TS + Vite.
- **Kotlin** (Android): Jetpack Compose, Material 3.
- **Scope**: smallest correct diff.
- **Secrets**: never commit keystores, `local.properties`, API keys, or download state
  (`.jxstate`, `.sgstate`, `*.db`).

## Windows GNU notes

This repo supports `x86_64-pc-windows-gnu` with a `windows-link` patch and `.toolbin`
import libraries. MSVC developers can use `x86_64-pc-windows-msvc` and skip most of
the GNU workaround (see BUILD_WINDOWS_GNU.md).

## Tests

```powershell
cargo test
```

Integration tests for the engine live in `crates/jyroxa-core/tests/`.

## Pull requests

- One logical change per PR when possible.
- Include a short description of behavior and how you tested it.
- Do not commit build outputs (`target/`, `dist/`, `*.apk`, `.toolbin/*.exe`).
