# Development tools log (Jyroxa build)

Historical notes for reproducible builds on the original host. Paths may differ on your machine.

## Pre-existing on the reference host

- Windows 10, PowerShell 5.1
- Git, Node.js, Microsoft Edge WebView2 runtime
- rustc/cargo via isolated `E:\.swiftget-tools` (optional)

## Isolated toolchain layout (example)

When using the bundled tooling directory:

| Variable | Example |
| -------- | ------- |
| `RUSTUP_HOME` | `E:\.swiftget-tools\rustup` |
| `CARGO_HOME` | `E:\.swiftget-tools\cargo` |
| `JAVA_HOME` | `E:\.swiftget-tools\jdk17` |
| `ANDROID_HOME` | `E:\.swiftget-tools\android-sdk` |
| `ANDROID_NDK_HOME` | `...\ndk\27.2.12479018` |
| `GRADLE_USER_HOME` | `E:\.swiftget-tools\gradle-home` |

The name **swiftget** appears in this file only as a legacy folder label (`E:\.swiftget-tools`).

## Windows GNU raw-dylib linker workaround

See `.toolbin/README.md` and [docs/BUILD_WINDOWS_GNU.md](docs/BUILD_WINDOWS_GNU.md). Internal env vars `SWIFTGET_DLLTOOL_WRAP` and `SWIFTGET_WINLIBS` in `.cargo/config.toml` are build plumbing, not user-facing product names.
