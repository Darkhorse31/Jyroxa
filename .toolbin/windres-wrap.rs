//! Forward GNU resource arguments to LLVM's windres frontend and its preprocessor.

use std::path::PathBuf;
use std::process::Command;

fn backend() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("JYROXA_LLVM_WINDRES") {
        return Ok(path.into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let sibling = exe.with_file_name("llvm-windres-inner.exe");
    if sibling.is_file() {
        return Ok(sibling);
    }
    for key in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"] {
        if let Some(root) = std::env::var_os(key) {
            let path = PathBuf::from(root)
                .join("toolchains/llvm/prebuilt/windows-x86_64/bin/llvm-windres.exe");
            if path.is_file() {
                return Ok(path);
            }
        }
    }
    Err("set JYROXA_LLVM_WINDRES or ANDROID_NDK_HOME to the LLVM toolchain".into())
}

fn run() -> Result<i32, String> {
    let backend = backend()?;
    let mut cmd = Command::new(&backend);
    for arg in std::env::args_os().skip(1) {
        cmd.arg(if arg == "-V" { "--version".into() } else { arg });
    }
    // Retain both preprocessing and COFF conversion; llvm-rc alone emits .res.
    // Forward all original options, including short names and include paths.
    let preprocessor = backend.with_file_name("clang.exe");
    if preprocessor.is_file() {
        cmd.arg(format!("--preprocessor={}", preprocessor.display()));
        cmd.args([
            "--preprocessor-arg=-E",
            "--preprocessor-arg=-x",
            "--preprocessor-arg=c",
            "--preprocessor-arg=-DRC_INVOKED",
        ]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.status()
        .map(|status| status.code().unwrap_or(1))
        .map_err(|e| e.to_string())
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("windres-wrap: {error}");
            1
        }
    };
    std::process::exit(code);
}
