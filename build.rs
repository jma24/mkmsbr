// Build script. When the `embed-boot-asm` feature is on, invokes NASM to
// assemble boot-asm/*.asm into 512-byte raw binaries and writes their byte
// contents into $OUT_DIR for include_bytes!.
//
// Without the feature, writes empty placeholder files so src/blobs.rs still
// compiles. This keeps `cargo check` working on machines without NASM during
// early scaffolding.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let asm_dir = manifest_dir.join("boot-asm");

    let single_sector_blobs = [
        "mbr_xp",
        "mbr_win7",
        "fat32_pbr_bootmgr",
        "fat32_pbr_ntldr",
        "ntfs_pbr",
    ];

    let embed = env::var("CARGO_FEATURE_EMBED_BOOT_ASM").is_ok();

    for blob in single_sector_blobs {
        let out_path = out_dir.join(format!("{blob}.bin"));
        let asm_path = asm_dir.join(format!("{blob}.asm"));
        build_single(&asm_path, &out_path, embed);
    }

    // Multi-sector fat32_pbr_bootmgr: sector0 + sector1 concatenated to
    // a 1024-byte blob. The variant lives in its own subdirectory per
    // docs/SPEC.md §Project layout.
    let multi_dir = asm_dir.join("fat32_pbr_bootmgr");
    let multi_out = out_dir.join("fat32_pbr_bootmgr_multi.bin");
    build_multi(&multi_dir, &multi_out, embed);
}

fn build_single(asm_path: &Path, out_path: &Path, embed: bool) {
    if embed {
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
            .expect(
                "failed to invoke nasm. Install with `brew install nasm`, \
                 or build without --features embed-boot-asm",
            );
        if !status.success() {
            panic!("nasm failed for {}", asm_path.display());
        }
    } else {
        fs::write(out_path, []).unwrap();
    }
}

fn build_multi(dir: &Path, out_path: &Path, embed: bool) {
    if !embed {
        fs::write(out_path, []).unwrap();
        return;
    }

    let mut combined = Vec::with_capacity(1024);
    for sector in &["sector0", "sector1"] {
        let asm = dir.join(format!("{sector}.asm"));
        let bin = out_path.with_file_name(format!("fat32_pbr_bootmgr_{sector}.bin"));
        println!("cargo:rerun-if-changed={}", asm.display());
        let status = Command::new("nasm")
            .args(["-f", "bin", "-o", bin.to_str().unwrap(), asm.to_str().unwrap()])
            .status()
            .expect("nasm spawn");
        if !status.success() {
            panic!("nasm failed for {}", asm.display());
        }
        let bytes = fs::read(&bin).unwrap_or_else(|_| panic!("read {}", bin.display()));
        if bytes.len() != 512 {
            panic!(
                "{} assembled to {} bytes, expected exactly 512",
                asm.display(),
                bytes.len()
            );
        }
        combined.extend_from_slice(&bytes);
    }
    fs::write(out_path, &combined).unwrap();
}
