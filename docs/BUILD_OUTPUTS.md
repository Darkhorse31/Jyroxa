# SwiftGet build outputs (Windows GNU)

Built on **2026-11-10** with host `x86_64-pc-windows-gnu`, Tauri v2.

## Desktop (Tauri)

| Artifact | Path |
|----------|------|
| Release executable | `E:\SwiftGet\target\release\swiftget-desktop.exe` |
| Frontend bundle (embedded) | `E:\SwiftGet\apps\desktop\dist\` |

### Commands used

```powershell
cd E:\SwiftGet\apps\desktop
npm install
.\build-gnu.ps1 --no-bundle
```

`build-gnu.ps1` prepends `.toolbin` (windres/ar shims) and the NDK LLVM bin directory to `PATH` so `tauri-winres` can compile the Windows manifest.

### Installer (NSIS)

Full `npm run tauri build` (with bundling) **failed** on this machine: Tauri could not finish downloading `nsis-3.13.zip` from GitHub (`timeout: global`). Use `--no-bundle` for a runnable `.exe`, or retry bundling when the network allows.

Expected installer output (when bundling succeeds):

`E:\SwiftGet\target\release\bundle\nsis\SwiftGet_*_x64-setup.exe`

## Workspace Rust binaries (reference)

| Artifact | Path |
|----------|------|
| CLI (if built) | `E:\SwiftGet\target\release\swiftget.exe` |
