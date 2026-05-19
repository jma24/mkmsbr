// Build script. With the `embed-boot-asm` feature on (default):
//
//   1. If `nasm` is on PATH and `boot-asm/` sources are present, assemble
//      from source — the dev path.
//   2. Otherwise, copy from `blobs-prebuilt/` — the `cargo install` path.
//      Prebuilt blobs are the bytes the maintainer published; they match
//      the `boot-asm/` source at the same tag.
//
// Without the feature, write empty placeholder files so `cargo check` still
// works on hosts that don't need the bytes (e.g., crate-API consumers that
// don't actually invoke the splice functions).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let asm_dir = manifest_dir.join("boot-asm");
    let prebuilt_dir = manifest_dir.join("blobs-prebuilt");

    let single_sector_blobs = [
        "mbr_xp",
        "mbr_win7",
        "fat32_pbr_bootmgr",
        "xp_setup_chain_bootsect",
    ];

    let embed = env::var("CARGO_FEATURE_EMBED_BOOT_ASM").is_ok();
    let nasm_ok = embed && nasm_available();

    for blob in single_sector_blobs {
        let out_path = out_dir.join(format!("{blob}.bin"));
        let asm_path = asm_dir.join(format!("{blob}.asm"));
        let prebuilt = prebuilt_dir.join(format!("{blob}.bin"));
        build_single(&asm_path, &out_path, &prebuilt, embed, nasm_ok);
    }

    for variant in ["fat32_pbr_bootmgr", "fat32_pbr_ntldr", "ntfs_pbr_bootmgr"] {
        let multi_dir = asm_dir.join(variant);
        let multi_out = out_dir.join(format!("{variant}_multi.bin"));
        let prebuilt = prebuilt_dir.join(format!("{variant}_multi.bin"));
        build_multi(&multi_dir, &multi_out, &prebuilt, embed, nasm_ok);
    }
}

fn nasm_available() -> bool {
    Command::new("nasm")
        .arg("-v")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn build_single(asm_path: &Path, out_path: &Path, prebuilt: &Path, embed: bool, nasm_ok: bool) {
    if !embed {
        fs::write(out_path, []).unwrap();
        return;
    }

    if nasm_ok && asm_path.exists() {
        println!("cargo:rerun-if-changed={}", asm_path.display());
        let status = Command::new("nasm")
            .args([
                "-f",
                "bin",
                "-o",
                out_path.to_str().unwrap(),
                asm_path.to_str().unwrap(),
            ])
            .status()
            .expect("nasm invocation failed unexpectedly");
        if !status.success() {
            panic!("nasm failed for {}", asm_path.display());
        }
    } else {
        copy_prebuilt(prebuilt, out_path);
    }
}

fn build_multi(dir: &Path, out_path: &Path, prebuilt: &Path, embed: bool, nasm_ok: bool) {
    if !embed {
        fs::write(out_path, []).unwrap();
        return;
    }

    let sectors = ["sector0", "sector1"];
    let all_sources = sectors.iter().all(|s| dir.join(format!("{s}.asm")).exists());

    if nasm_ok && all_sources {
        let stem = out_path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_suffix("_multi"))
            .expect("multi blob out_path must end in _multi.bin");
        let mut combined = Vec::with_capacity(1024);
        for sector in &sectors {
            let asm = dir.join(format!("{sector}.asm"));
            let bin = out_path.with_file_name(format!("{stem}_{sector}.bin"));
            println!("cargo:rerun-if-changed={}", asm.display());
            let status = Command::new("nasm")
                .args(["-f", "bin", "-o", bin.to_str().unwrap(), asm.to_str().unwrap()])
                .status()
                .expect("nasm spawn");
            if !status.success() {
                panic!("nasm failed for {}", asm.display());
            }
            let bytes = fs::read(&bin).unwrap_or_else(|_| panic!("read {}", bin.display()));
            if bytes.is_empty() || bytes.len() % 512 != 0 {
                panic!(
                    "{} assembled to {} bytes, expected non-zero multiple of 512",
                    asm.display(),
                    bytes.len()
                );
            }
            combined.extend_from_slice(&bytes);
        }
        fs::write(out_path, &combined).unwrap();
    } else {
        copy_prebuilt(prebuilt, out_path);
    }
}

fn copy_prebuilt(prebuilt: &Path, out_path: &Path) {
    println!("cargo:rerun-if-changed={}", prebuilt.display());
    if !prebuilt.exists() {
        panic!(
            "neither nasm nor a prebuilt blob is available for {}. \
             Install nasm (`brew install nasm` on macOS, your distro's \
             package on Linux), or install mkmsbr from crates.io which \
             ships prebuilt blobs.",
            out_path.display()
        );
    }
    fs::copy(prebuilt, out_path).unwrap_or_else(|e| {
        panic!(
            "failed to copy prebuilt blob {} → {}: {e}",
            prebuilt.display(),
            out_path.display()
        )
    });
}
