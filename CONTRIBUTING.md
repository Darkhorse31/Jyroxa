# Contributing to SwiftGet

Thank you for helping improve SwiftGet. This project is MIT-licensed; by contributing
you agree that your contributions will be released under the same license.

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

- `crates/jyroxa-core` — download engine and LinkGrabber (most logic lives here)
- `crates/jyroxa-cli` — command-line interface
- `crates/jyroxa-ffi` — JNI/C ABI for Android
- `apps/desktop` — Tauri v2 desktop shell
- `android` — Kotlin/Compose Android app

Keep new Rust logic in `jyroxa-core` when possible so all frontends benefit.

## Coding guidelines

- **Rust**: match existing style (`thiserror` in the library, `anyhow` in binaries).
  Prefer focused unit tests in the module and integration tests under `tests/`.
- **TypeScript** (desktop): vanilla TS + Vite; no new heavy UI frameworks without
  discussion.
- **Kotlin** (Android): Jetpack Compose, Material 3, follow existing package layout.
- **Scope**: smallest correct diff; avoid unrelated refactors in the same PR.
- **Secrets**: never commit keystores, `local.properties`, API keys, or download state
  (`.sgstate`, `*.db`).

## Windows GNU notes

This repo supports `x86_64-pc-windows-gnu` with a `windows-link` patch and `.toolbin`
import libraries. If you develop on MSVC, you can use `x86_64-pc-windows-msvc` and skip
most of the GNU workaround (see BUILD_WINDOWS_GNU.md).

## Tests

```powershell
cargo test
```

Integration tests use a local in-process HTTP server (no network required). When
changing download semantics, add or update tests in `crates/jyroxa-core`.

Android unit tests (when SDK is available):

```powershell
cd android
.\gradlew.bat testReleaseUnitTest
```

## Commits and pull requests

- Use clear, conventional messages (`feat:`, `fix:`, `docs:`, `test:`, `chore:`).
- One logical change per commit when practical.
- PR description: what changed, why, and how you verified it (commands run).
- Link related issues when applicable.

## Reporting bugs

Include OS, Rust target (`rustup show active-toolchain`), SwiftGet version/commit,
steps to reproduce, and logs or `.sgstate` snippets if relevant (redact URLs/tokens).

## Code of conduct

Be respectful and constructive. We optimize for maintainable, auditable download
software—clarity and tests matter more than cleverness.
