//! Layer-3 QEMU smoke tests: boot against *real* Microsoft loaders.
//!
//! Ignored by default. Run with:
//!
//!     cargo test --test qemu_pbr_real --features embed-boot-asm -- --ignored
//!
//! Prerequisites:
//!   - `nasm` + `qemu-system-i386` + `mtools` (mformat, mcopy, mmd).
//!   - L3 fixtures staged via `scripts/build_l3_fixtures.sh` (XP ISO for
//!     the ntldr test, Win 7 ISO for the bootmgr-multi test). Tests skip
//!     gracefully if the corresponding fixtures are absent.
//!
//! Pass criterion. Unlike Layer 2 (which gates on a "BOOTREC OK" string
//! the fake loader emits over COM1), real NTLDR / bootmgr don't speak to
//! the serial port. Instead we record block-device read events via QEMU's
//! `-trace` subsystem and gate on the count. Empirical floor:
//!
//!   - Our PBR error-halt path issues only the reads it took to discover
//!     the failure (file-not-found scans, etc.) — single- to double-digit
//!     reads in practice.
//!   - A successful chainload reads the loader file off FAT (NTLDR alone
//!     is ~490 sectors; bootmgr is ~750), then the real loader does its
//!     own further reads on top.
//!
//! `L3_READ_THRESHOLD` is set conservatively. Override with the env var
//! `BOOTREC_L3_MIN_READS` after observing real numbers on your platform.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use common::qemu_trace::{boot_with_trace, TracedBoot};

const IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const BOOT_TIMEOUT: Duration = Duration::from_secs(15);

/// Conservative default — well above any error-halt-path read count, well
/// below the read count of even a partial successful chainload. Override
/// with `BOOTREC_L3_MIN_READS` once empirically tuned.
const L3_READ_THRESHOLD_DEFAULT: usize = 50;

#[test]
#[ignore]
fn fat32_pbr_ntldr_loads_real_ntldr_in_qemu() {
    let fixtures = repo_root().join("tests/real_content/xp");
    let ntldr = fixtures.join("NTLDR");
    let ntdetect = fixtures.join("NTDETECT.COM");
    if !ntldr.exists() || !ntdetect.exists() {
        eprintln!(
            "skipping L3 ntldr test: fixtures missing under {}. Run \
             scripts/build_l3_fixtures.sh --xp-iso <path>",
            fixtures.display()
        );
        return;
    }

    if let Err(reason) = check_dependencies() {
        eprintln!("skipping L3 ntldr test: {reason}");
        return;
    }

    let blob = bootrec::FAT32_PBR_NTLDR_BOOT;
    if blob.is_empty() {
        panic!(
            "FAT32_PBR_NTLDR_BOOT is empty (built without --features embed-boot-asm). \
             Re-run: cargo test --test qemu_pbr_real --features embed-boot-asm -- --ignored"
        );
    }

    let tmp = tempdir();
    let image = tmp.join("bootrec-l3-ntldr.img");
    create_fat32_image(&image).expect("formatting FAT32 image");
    mcopy_to_root(&image, &ntldr, "NTLDR").expect("mcopy NTLDR");
    mcopy_to_root(&image, &ntdetect, "NTDETECT.COM").expect("mcopy NTDETECT.COM");
    splice_pbr_single(&image, blob).expect("splicing single-sector PBR");

    let result = boot_with_trace(&image, BOOT_TIMEOUT).expect("running qemu");
    assert_chainloaded("ntldr", result);
}

#[test]
#[ignore]
fn fat32_pbr_bootmgr_multi_loads_real_bootmgr_in_qemu() {
    let fixtures = repo_root().join("tests/real_content/win7");
    let bootmgr = fixtures.join("bootmgr");
    let bcd = fixtures.join("bcd");
    if !bootmgr.exists() || !bcd.exists() {
        eprintln!(
            "skipping L3 bootmgr test: fixtures missing under {}. Run \
             scripts/build_l3_fixtures.sh --win7-iso <path>",
            fixtures.display()
        );
        return;
    }

    if let Err(reason) = check_dependencies() {
        eprintln!("skipping L3 bootmgr test: {reason}");
        return;
    }

    let blob = bootrec::FAT32_PBR_BOOTMGR_MULTI_BOOT;
    if blob.is_empty() {
        panic!(
            "FAT32_PBR_BOOTMGR_MULTI_BOOT is empty (built without --features embed-boot-asm). \
             Re-run: cargo test --test qemu_pbr_real --features embed-boot-asm -- --ignored"
        );
    }
    assert!(
        blob.len() >= 1024 && blob.len() % 512 == 0,
        "multi blob is {} bytes; expected non-zero multiple of 512",
        blob.len()
    );

    let tmp = tempdir();
    let image = tmp.join("bootrec-l3-bootmgr.img");
    create_fat32_image(&image).expect("formatting FAT32 image");
    mcopy_to_root(&image, &bootmgr, "bootmgr").expect("mcopy bootmgr");
    mmd_dir(&image, "boot").expect("mmd ::/boot");
    mcopy_to(&image, &bcd, "boot/bcd").expect("mcopy bcd");
    splice_pbr_multi(&image, blob).expect("splicing multi-sector PBR");

    let result = boot_with_trace(&image, BOOT_TIMEOUT).expect("running qemu");
    assert_chainloaded("bootmgr_multi", result);
}

fn assert_chainloaded(variant: &str, result: TracedBoot) {
    let threshold = std::env::var("BOOTREC_L3_MIN_READS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(L3_READ_THRESHOLD_DEFAULT);

    eprintln!(
        "[{variant}] L3 read count = {} (threshold {}, event {})",
        result.read_count, threshold, result.event_name
    );
    if !result.serial.is_empty() {
        eprintln!("[{variant}] serial output:\n---\n{}\n---", result.serial);
    }

    assert!(
        result.read_count >= threshold,
        "[{variant}] only {} block reads recorded (need >= {}). \
         Suggests our PBR failed to chainload before the real loader could self-load. \
         Serial tail: {:?}",
        result.read_count,
        threshold,
        result.serial.lines().rev().take(5).collect::<Vec<_>>()
    );
}

// --- helpers ------------------------------------------------------------

fn check_dependencies() -> Result<(), String> {
    for tool in &["qemu-system-i386", "mformat", "mcopy", "mmd"] {
        which(tool).map_err(|e| format!("missing `{tool}`: {e}"))?;
    }
    Ok(())
}

fn which(tool: &str) -> Result<(), String> {
    let out = Command::new("/usr/bin/env")
        .args(["which", tool])
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("`{tool}` not found in PATH"));
    }
    Ok(())
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn create_fat32_image(image: &Path) -> Result<(), String> {
    let f = std::fs::File::create(image).map_err(|e| format!("create image: {e}"))?;
    f.set_len(IMAGE_BYTES).map_err(|e| format!("set_len: {e}"))?;
    drop(f);

    let out = Command::new("mformat")
        .args(["-F", "-i"])
        .arg(image)
        .args(["-v", "BOOTREC", "::"])
        .output()
        .map_err(|e| format!("mformat: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "mformat failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn mcopy_to_root(image: &Path, src: &Path, name: &str) -> Result<(), String> {
    mcopy_to(image, src, name)
}

fn mcopy_to(image: &Path, src: &Path, dest_rel: &str) -> Result<(), String> {
    let out = Command::new("mcopy")
        .arg("-i")
        .arg(image)
        .arg(src)
        .arg(format!("::{dest_rel}"))
        .output()
        .map_err(|e| format!("mcopy: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "mcopy {} -> {}: {}",
            src.display(),
            dest_rel,
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn mmd_dir(image: &Path, dir: &str) -> Result<(), String> {
    let out = Command::new("mmd")
        .arg("-i")
        .arg(image)
        .arg(format!("::{dir}"))
        .output()
        .map_err(|e| format!("mmd: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "mmd ::{dir}: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

fn splice_pbr_single(image: &Path, blob: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(image)
        .map_err(|e| format!("opening image: {e}"))?;
    let mut existing = [0u8; 512];
    file.read_exact(&mut existing)
        .map_err(|e| format!("reading existing PBR: {e}"))?;
    let spliced = bootrec::splice_fat32_pbr(&existing, blob)
        .map_err(|e| format!("splice_fat32_pbr: {e}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    file.write_all(&spliced)
        .map_err(|e| format!("writing PBR: {e}"))?;
    Ok(())
}

fn splice_pbr_multi(image: &Path, blob: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(image)
        .map_err(|e| format!("opening image: {e}"))?;
    let mut existing = [0u8; 512];
    file.read_exact(&mut existing)
        .map_err(|e| format!("reading existing PBR: {e}"))?;
    let spliced = bootrec::splice_fat32_pbr_multi(&existing, blob)
        .map_err(|e| format!("splice_fat32_pbr_multi: {e}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    file.write_all(&spliced)
        .map_err(|e| format!("writing multi-sector PBR: {e}"))?;
    Ok(())
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bootrec-l3-qemu-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
