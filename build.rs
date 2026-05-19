// Build script. When the `embed-boot-asm` feature is on, invokes NASM to
// assemble boot-asm/*.asm into 512-byte raw binaries and writes their byte
// contents into $OUT_DIR for include_bytes!.
//
// Without the feature, writes empty placeholder files so src/blobs.rs still
// compiles. This keeps `cargo check` working on machines without NASM during
// early scaffolding.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let asm_dir = manifest_dir.join("boot-asm");

    let blobs = ["mbr_xp", "mbr_win7", "fat32_pbr", "ntfs_pbr"];

    let embed = env::var("CARGO_FEATURE_EMBED_BOOT_ASM").is_ok();

    for blob in blobs {
        let out_path = out_dir.join(format!("{blob}.bin"));
        if embed {
            let asm_path = asm_dir.join(format!("{blob}.asm"));
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
                panic!("nasm failed for {blob}.asm");
            }
        } else {
            fs::write(&out_path, []).unwrap();
        }
    }
}
