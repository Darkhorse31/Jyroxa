fn main() {
    tauri_build::build();
    embed_comctl32_v6_for_tests();
}

/// Desktop unit-test harnesses are plain console EXEs without Tauri's winres
/// manifest, so comctl32 v5 is loaded and `TaskDialogIndirect` fails at process
/// start (`STATUS_ENTRYPOINT_NOT_FOUND`). Link a RT_MANIFEST so GNU `cargo test`
/// can run.
fn embed_comctl32_v6_for_tests() {
    #[cfg(windows)]
    {
        use std::path::PathBuf;
        use std::process::Command;

        let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
        let rc = manifest_dir.join("windows").join("test-harness.rc");
        let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
        let obj = out_dir.join("test-harness-manifest.o");

        let output = Command::new("windres")
            .args([
                "-i",
                rc.to_str().unwrap(),
                "-o",
                obj.to_str().unwrap(),
            ])
            .current_dir(manifest_dir.join("windows"))
            .output();

        match output {
            Ok(out) if out.status.success() => {
                // Link into every final artifact of this crate (bins + tests).
                // `rustc-link-arg-tests` is not accepted by our cargo/rustc-wrap.
                println!("cargo:rustc-link-arg={}", obj.display());
                println!("cargo:rerun-if-changed=windows/test-harness.rc");
                println!("cargo:rerun-if-changed=windows/test-harness.manifest");
            }
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stdout = String::from_utf8_lossy(&out.stdout);
                println!(
                    "cargo:warning=windres failed ({}); desktop lib tests may fail to start without comctl32 v6; stderr={stderr}; stdout={stdout}",
                    out.status
                );
            }
            Err(e) => {
                println!(
                    "cargo:warning=windres not available ({e}); desktop lib tests may fail to start without comctl32 v6"
                );
            }
        }
    }
}
