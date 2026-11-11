// Thin wrapper around LLVM's dlltool (a multicall alias of llvm-ar).
//
// rustc invokes `dlltool` with GNU-binutils-style flags
// (`-f --64 --no-leading-underscore --temp-prefix <p>`). LLVM's dlltool parses
// `-f`/`--temp-prefix` differently, which corrupts the generated import library
// and yields null IAT entries (access violations at runtime). We forward only
// the flags LLVM dlltool handles the same way GNU does: -d, -D, -l, -m, -k.
use std::path::PathBuf;
use std::process::Command;

fn llvm_dlltool_path() -> PathBuf {
    let mut exe = std::env::current_exe().expect("current_exe");
    exe.pop();
    let next = exe.join("llvm-dlltool.exe");
    if next.is_file() {
        return next;
    }
    if let Some(home) = std::env::var_os("RUSTUP_HOME") {
        let mut p = PathBuf::from(home);
        p.push(r"toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\llvm-dlltool.exe");
        if p.is_file() {
            return p;
        }
    }
    PathBuf::from("llvm-dlltool.exe")
}

fn main() {
    let real = llvm_dlltool_path();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-d" | "-D" | "-l" | "-m" | "--input-def" | "--dllname" | "--output-lib"
            | "--machine" => {
                out.push(args[i].clone());
                if i + 1 < args.len() {
                    out.push(args[i + 1].clone());
                    i += 2;
                    continue;
                }
            }
            "-k" | "--kill-at" => out.push(args[i].clone()),
            "-f" | "--temp-prefix" | "--as" | "-S" => {
                i += 2;
                continue;
            }
            "--64" | "--32" | "--no-leading-underscore" | "--leading-underscore" => {}
            _ => {}
        }
        i += 1;
    }

    let status = Command::new(&real)
        .args(&out)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {}: {e}", real.display()));
    std::process::exit(status.code().unwrap_or(1));
}
