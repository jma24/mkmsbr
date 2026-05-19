//! Layer-2 QEMU smoke tests for the FAT32 PBR variants.
//!
//! Ignored by default. Run with:
//!
//!     cargo test --test qemu_pbr --features embed-boot-asm -- --ignored
//!
//! Requires:
//!   - `nasm` to assemble the boot blobs and the fake-loader stub
//!   - `qemu-system-i386` to boot the image
//!   - `mformat` + `mcopy` (mtools) for canonical FAT32 image construction
//!
//! Per-variant flow:
//!   1. Build the fake loader (NASM, prints "MKMSBR OK\n" to COM1, halts).
//!   2. Create a 64 MiB FAT32 image with the fake loader at root, under
//!      the filename the variant searches for (BOOTMGR for the bootmgr
//!      variant, NTLDR for the ntldr variant).
//!   3. Splice our PBR blob through `splice_fat32_pbr` (preserving the
//!      newly-formatted BPB).
//!   4. Boot under qemu-system-i386 with `-serial stdio`.
//!   5. Pass if serial contains "MKMSBR OK".
//!
//! When this passes, our PBR is byte-correct enough to chain-load an x86
//! binary by name from a FAT32 volume. That's the contract.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const IMAGE_BYTES: u64 = 64 * 1024 * 1024;

#[test]
#[ignore]
fn fat32_pbr_bootmgr_chainloads_in_qemu() {
    assert_chainload(mkmsbr::FAT32_PBR_BOOTMGR_BOOT, "BOOTMGR", "bootmgr");
}

#[test]
#[ignore]
fn fat32_pbr_ntldr_chainloads_in_qemu() {
    assert_multi_chainload(
        mkmsbr::FAT32_PBR_NTLDR_MULTI_BOOT,
        "NTLDR",
        "ntldr_multi",
    );
}

#[test]
#[ignore]
fn fat32_pbr_bootmgr_multi_chainloads_in_qemu() {
    assert_multi_chainload(
        mkmsbr::FAT32_PBR_BOOTMGR_MULTI_BOOT,
        "BOOTMGR",
        "bootmgr_multi",
    );
}

/// L2 smoke for `build_xp_setup_chain_bootsect`. Unlike the PBR tests
/// above (which boot from a FAT32 partition's spliced PBR and walk FAT
/// to find a file), this exercises the raw-LBA loader directly:
///
///   1. Format a 16 MiB FAT32 image.
///   2. Write the fake_bootmgr.bin payload to a known partition-relative
///      LBA via `dd` (bypassing FAT entirely — we're testing the loader
///      not the filesystem).
///   3. Read sector 0 to capture the formatter's BPB.
///   4. Call `build_xp_setup_chain_bootsect` with a single LbaRun
///      pointing at that LBA, target_segment = 0x2000.
///   5. Overwrite LBA 0 with the resulting bootsector (BPB preserved by
///      the splice; the formatter's PBR code is replaced).
///   6. Boot the image under qemu-system-i386.
///   7. Pass if serial contains "MKMSBR OK" — the same marker the
///      fake_bootmgr.bin payload prints.
///
/// HiddSec is 0 on a bare partition image (no MBR), so the bootsector's
/// "add HiddSec to start_lba" math reduces to a no-op; the test still
/// exercises the run-loop + CHS path. A separate test could add an MBR
/// to verify HiddSec is honored, but that's overhead the value of the
/// L4 hardware test will cover.
#[test]
#[ignore]
fn xp_setup_chain_bootsect_chainloads_in_qemu() {
    if let Err(reason) = check_dependencies() {
        eprintln!("skipping setup-chain test: {reason}");
        return;
    }
    if mkmsbr::XP_SETUP_CHAIN_BOOTSECT_BOOT.is_empty() {
        panic!(
            "XP_SETUP_CHAIN_BOOTSECT_BOOT empty (built without --features embed-boot-asm). \
             Re-run: cargo test --test qemu_pbr --features embed-boot-asm -- --ignored"
        );
    }

    const PAYLOAD_LBA: u32 = 2000; // safely past reserved + FAT region of a 16 MiB FAT32

    let boot_asm = repo_root().join("boot-asm");
    let fake_loader = build_fake_loader(&boot_asm).expect("building fake_bootmgr.bin");
    // fake_bootmgr.bin assembles to ~40 bytes (the actual code + msg + halt).
    // The PBR tests get away with the short size because they mcopy it into
    // FAT and the FAT reader pads with zeros on the last cluster. For raw
    // LBA placement we have to pad ourselves so the sector-aligned write
    // covers a full 512 bytes.
    let mut payload = std::fs::read(&fake_loader).expect("reading fake_bootmgr.bin");
    assert!(payload.len() <= 512, "fake loader fits in one sector");
    payload.resize(512, 0);

    let tmp = tempdir();
    let image = tmp.join("mkmsbr-setup-chain.img");
    create_fat32_image_only(&image).expect("creating FAT32 image");

    // Write payload at the known LBA via raw I/O (skips FAT).
    write_at_lba(&image, PAYLOAD_LBA, &payload).expect("writing payload at LBA");

    // Read sector 0 to capture the formatter's BPB.
    let mut formatter_sector0 = [0u8; 512];
    read_at_lba(&image, 0, &mut formatter_sector0).expect("reading sector 0");

    let runs = [mkmsbr::LbaRun {
        start_lba: PAYLOAD_LBA,
        sector_count: 1,
    }];
    let bootsect = mkmsbr::build_xp_setup_chain_bootsect(&formatter_sector0, 0x2000, &runs)
        .expect("build_xp_setup_chain_bootsect");

    write_at_lba(&image, 0, &bootsect).expect("writing bootsect at LBA 0");

    let serial = boot_under_qemu(&image).expect("running qemu");
    assert!(
        serial.contains("MKMSBR OK"),
        "[setup-chain] qemu serial missing 'MKMSBR OK'. Got:\n---\n{serial}\n---"
    );
}

/// Like create_fat32_image but factored to be reusable across tests that
/// don't need an mcopied payload — they place bytes via raw I/O instead.
fn create_fat32_image_only(image: &Path) -> Result<(), String> {
    let f = std::fs::File::create(image).map_err(|e| format!("create image: {e}"))?;
    f.set_len(16 * 1024 * 1024).map_err(|e| format!("set_len: {e}"))?;
    drop(f);
    let fmt = Command::new("mformat")
        .args(["-F", "-i"])
        .arg(image)
        .args(["-v", "MKMSBR", "::"])
        .output()
        .map_err(|e| format!("mformat: {e}"))?;
    if !fmt.status.success() {
        return Err(format!("mformat failed: {}", String::from_utf8_lossy(&fmt.stderr)));
    }
    Ok(())
}

fn write_at_lba(image: &Path, lba: u32, bytes: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Seek, SeekFrom, Write};
    let mut f = OpenOptions::new()
        .write(true)
        .open(image)
        .map_err(|e| format!("open for write: {e}"))?;
    f.seek(SeekFrom::Start(lba as u64 * 512))
        .map_err(|e| format!("seek: {e}"))?;
    f.write_all(bytes).map_err(|e| format!("write: {e}"))?;
    Ok(())
}

fn read_at_lba(image: &Path, lba: u32, buf: &mut [u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom};
    let mut f = OpenOptions::new()
        .read(true)
        .open(image)
        .map_err(|e| format!("open for read: {e}"))?;
    f.seek(SeekFrom::Start(lba as u64 * 512))
        .map_err(|e| format!("seek: {e}"))?;
    f.read_exact(buf).map_err(|e| format!("read: {e}"))?;
    Ok(())
}

fn assert_chainload(blob: &[u8], target_filename: &str, variant: &str) {
    if let Err(reason) = check_dependencies() {
        eprintln!("skipping qemu test ({variant}): {reason}");
        return;
    }

    if blob.is_empty() {
        panic!(
            "[{variant}] PBR blob is empty (built without --features embed-boot-asm). \
             Re-run: cargo test --test qemu_pbr --features embed-boot-asm -- --ignored"
        );
    }

    let boot_asm = repo_root().join("boot-asm");
    let fake_loader = build_fake_loader(&boot_asm).expect("building fake_bootmgr.bin");

    let tmp = tempdir();
    let image = tmp.join(format!("mkmsbr-pbr-{variant}.img"));
    create_fat32_image(&image, &fake_loader, target_filename).expect("creating FAT32 image");
    splice_our_pbr(&image, blob).expect("splicing PBR");

    let serial = boot_under_qemu(&image).expect("running qemu");
    assert!(
        serial.contains("MKMSBR OK"),
        "[{variant}] qemu serial missing 'MKMSBR OK'. Got:\n---\n{serial}\n---"
    );
}

fn assert_multi_chainload(blob: &[u8], target_filename: &str, variant: &str) {
    if let Err(reason) = check_dependencies() {
        eprintln!("skipping qemu test ({variant}): {reason}");
        return;
    }

    if blob.is_empty() {
        panic!(
            "[{variant}] PBR blob is empty (built without --features embed-boot-asm). \
             Re-run: cargo test --test qemu_pbr --features embed-boot-asm -- --ignored"
        );
    }
    assert!(
        blob.len() >= 1024 && blob.len() % 512 == 0,
        "[{variant}] multi blob is {} bytes; expected non-zero multiple of 512",
        blob.len()
    );

    let boot_asm = repo_root().join("boot-asm");
    let fake_loader = build_fake_loader(&boot_asm).expect("building fake_bootmgr.bin");

    let tmp = tempdir();
    let image = tmp.join(format!("mkmsbr-pbr-{variant}.img"));
    create_fat32_image(&image, &fake_loader, target_filename).expect("creating FAT32 image");
    splice_our_multi_pbr(&image, blob).expect("splicing multi-sector PBR");

    let serial = boot_under_qemu(&image).expect("running qemu");
    assert!(
        serial.contains("MKMSBR OK"),
        "[{variant}] qemu serial missing 'MKMSBR OK'. Got:\n---\n{serial}\n---"
    );
}

fn check_dependencies() -> Result<(), String> {
    for tool in &["nasm", "qemu-system-i386", "mformat", "mcopy"] {
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

fn build_fake_loader(boot_asm: &Path) -> Result<PathBuf, String> {
    let status = Command::new("make")
        .args(["test-fixtures"])
        .current_dir(boot_asm)
        .status()
        .map_err(|e| format!("invoking make in {}: {e}", boot_asm.display()))?;
    if !status.success() {
        return Err("`make test-fixtures` failed".to_string());
    }
    let out = boot_asm.join("build").join("fake_bootmgr.bin");
    if !out.exists() {
        return Err(format!("expected output {} missing", out.display()));
    }
    Ok(out)
}

fn create_fat32_image(image: &Path, fake_loader: &Path, target_filename: &str) -> Result<(), String> {
    let f = std::fs::File::create(image).map_err(|e| format!("create image: {e}"))?;
    f.set_len(IMAGE_BYTES).map_err(|e| format!("set_len: {e}"))?;
    drop(f);

    let fmt = Command::new("mformat")
        .args(["-F", "-i"])
        .arg(image)
        .args(["-v", "MKMSBR", "::"])
        .output()
        .map_err(|e| format!("mformat: {e}"))?;
    if !fmt.status.success() {
        return Err(format!(
            "mformat failed: {}",
            String::from_utf8_lossy(&fmt.stderr)
        ));
    }

    let cp = Command::new("mcopy")
        .arg("-i")
        .arg(image)
        .arg(fake_loader)
        .arg(format!("::{target_filename}"))
        .output()
        .map_err(|e| format!("mcopy: {e}"))?;
    if !cp.status.success() {
        return Err(format!(
            "mcopy failed: {}",
            String::from_utf8_lossy(&cp.stderr)
        ));
    }

    Ok(())
}

fn splice_our_pbr(image: &Path, blob: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(image)
        .map_err(|e| format!("opening image for splice: {e}"))?;
    let mut existing = [0u8; 512];
    file.read_exact(&mut existing)
        .map_err(|e| format!("reading existing PBR: {e}"))?;
    let spliced = mkmsbr::splice_fat32_pbr(&existing, blob)
        .map_err(|e| format!("splice_fat32_pbr: {e}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    file.write_all(&spliced)
        .map_err(|e| format!("writing spliced PBR: {e}"))?;
    Ok(())
}

fn splice_our_multi_pbr(image: &Path, blob: &[u8]) -> Result<(), String> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(image)
        .map_err(|e| format!("opening image for splice: {e}"))?;
    // Read LBA 0 + LBA 1 (FSInfo). The splice preserves FSInfo at LBA 1
    // and relocates stage 2 to LBA 2 — see splice_fat32_pbr_multi docstring.
    let mut existing = [0u8; 1024];
    file.read_exact(&mut existing)
        .map_err(|e| format!("reading existing PBR + FSInfo: {e}"))?;
    let spliced = mkmsbr::splice_fat32_pbr_multi(&existing, blob)
        .map_err(|e| format!("splice_fat32_pbr_multi: {e}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    file.write_all(&spliced)
        .map_err(|e| format!("writing spliced multi-sector PBR: {e}"))?;
    Ok(())
}

fn boot_under_qemu(image: &Path) -> Result<String, String> {
    use std::io::Read;
    use std::process::Stdio;

    let drive = format!("file={},format=raw,if=ide", image.display());
    let mut child = Command::new("qemu-system-i386")
        .args(["-drive", &drive])
        .args(["-boot", "c", "-serial", "stdio", "-display", "none", "-no-reboot"])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("spawning qemu: {e}"))?;

    let stdout = child.stdout.take().expect("piped stdout");
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let mut r = stdout;
        let _ = r.read_to_string(&mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(format!("qemu wait error: {e}")),
        }
    }
    let _ = child.kill();
    let _ = child.wait();

    Ok(reader.join().unwrap_or_default())
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("mkmsbr-pbr-qemu-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
