//! GNU `windres` API over host `llvm-rc` when only LLVM is available (no clang).

use std::process::Command;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-V" || a == "/?" || a == "--version") {
        println!("GNU windres (GNU Binutils) 2.41.0");
        return;
    }

    let exe = std::env::current_exe().expect("exe");
    let llvm_rc = exe.parent().expect("parent").join("llvm-windres-inner.exe");

    let mut input: Option<String> = None;
    let mut output: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--input" => {
                input = Some(args[i + 1].clone());
                i += 2;
            }
            "--output" => {
                output = Some(args[i + 1].clone());
                i += 2;
            }
            "--include-dir" | "--target" | "-c" | "--output-format=coff" | "--output-format" => {
                i += 2;
            }
            s if s.starts_with("--output-format=") => i += 1,
            _ => i += 1,
        }
    }

    let Some(input) = input else {
        eprintln!("windres-wrap: missing --input");
        std::process::exit(1);
    };
    let Some(output) = output else {
        eprintln!("windres-wrap: missing --output");
        std::process::exit(1);
    };

    let status = Command::new(&llvm_rc)
        .args(["-no-preprocess", &input, "-fo", &output])
        .status()
        .unwrap_or_else(|e| {
            eprintln!("windres-wrap: {e}");
            std::process::exit(1);
        });
    std::process::exit(status.code().unwrap_or(1));
}
