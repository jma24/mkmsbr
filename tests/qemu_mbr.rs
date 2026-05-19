//! Layer-2 QEMU smoke test for `mkmsbr::mbr_xp`.
//!
//! Ignored by default. Run with:
//!
//!     cargo test --test qemu_mbr --features embed-boot-asm -- --ignored
//!
//! Requires:
//!   - `nasm` to assemble the fake-PBR stub.
//!   - `qemu-system-i386` to boot the image.
//!
//! Flow:
//!   1. Build the fake PBR (NASM, prints "MKMSBR MBR OK\r\n" to COM1, halts).
//!   2. Allocate a 4 MiB raw disk image, zero-filled.
//!   3. Write `mkmsbr::mbr_xp(8192)` to sector 0 (8192-sector image, so
//!      the partition covers LBA 2048..8192 = 6144 sectors).
//!   4. Write `fake_pbr.bin` at LBA 2048 (the partition start).
//!   5. Boot under qemu-system-i386 with `-drive if=ide -serial stdio`.
//!   6. Pass if serial output contains "MKMSBR MBR OK".
//!
//! Contract proven on pass: mkmsbr's MBR correctly relocates itself,
//! scans the partition table, reads the active partition's first sector
//! via INT 13h ext, validates the signature, and chain-loads.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const SECTOR: u64 = 512;
const DISK_SECTORS: u64 = 8192; // 4 MiB
const PARTITION_LBA: u64 = 2048; // matches PARTITION_START_LBA

#[test]
#[ignore]
fn mbr_xp_chainloads_active_partition_in_qemu() {
    let mbr = mkmsbr::mbr_xp(DISK_SECTORS).expect("mbr_xp");
    assert_chainload(&mbr, "mbr_xp");
}

#[test]
#[ignore]
fn mbr_win7_chainloads_active_partition_in_qemu() {
    let mbr = mkmsbr::mbr_win7(DISK_SECTORS).expect("mbr_win7");
    assert_chainload(&mbr, "mbr_win7");
}

fn assert_chainload(mbr: &[u8; 512], variant: &str) {
    if let Err(reason) = check_dependencies() {
        eprintln!("skipping qemu test ({variant}): {reason}");
        return;
    }

    if mkmsbr::MBR_XP_BOOT.is_empty() {
        panic!(
            "MBR blobs are empty (built without --features embed-boot-asm). \
             Re-run: cargo test --test qemu_mbr --features embed-boot-asm -- --ignored"
        );
    }

    let boot_asm = repo_root().join("boot-asm");
    let fake_pbr = build_fake_pbr(&boot_asm).expect("building fake_pbr.bin");

    let tmp = tempdir();
    let image = tmp.join(format!("mkmsbr-{variant}.img"));
    create_image(&image, mbr, &fake_pbr).expect("creating disk image");

    let serial = boot_under_qemu(&image).expect("running qemu");
    assert!(
        serial.contains("MKMSBR MBR OK"),
        "[{variant}] qemu serial missing 'MKMSBR MBR OK'. Got:\n---\n{serial}\n---"
    );
}

fn check_dependencies() -> Result<(), String> {
    for tool in &["nasm", "qemu-system-i386"] {
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

fn build_fake_pbr(boot_asm: &Path) -> Result<PathBuf, String> {
    let status = Command::new("make")
        .args(["test-fixtures"])
        .current_dir(boot_asm)
        .status()
        .map_err(|e| format!("invoking make in {}: {e}", boot_asm.display()))?;
    if !status.success() {
        return Err("`make test-fixtures` failed".to_string());
    }
    let out = boot_asm.join("build").join("fake_pbr.bin");
    if !out.exists() {
        return Err(format!("expected output {} missing", out.display()));
    }
    Ok(out)
}

fn create_image(image: &Path, mbr: &[u8; 512], fake_pbr: &Path) -> Result<(), String> {
    use std::io::{Seek, SeekFrom, Write};

    let mut f = std::fs::File::create(image).map_err(|e| format!("create image: {e}"))?;
    f.set_len(DISK_SECTORS * SECTOR)
        .map_err(|e| format!("set_len: {e}"))?;

    f.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek 0: {e}"))?;
    f.write_all(mbr).map_err(|e| format!("write MBR: {e}"))?;

    let pbr = std::fs::read(fake_pbr).map_err(|e| format!("read fake_pbr: {e}"))?;
    f.seek(SeekFrom::Start(PARTITION_LBA * SECTOR))
        .map_err(|e| format!("seek partition: {e}"))?;
    f.write_all(&pbr)
        .map_err(|e| format!("write fake PBR: {e}"))?;
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
    let dir = std::env::temp_dir().join(format!("mkmsbr-mbr-qemu-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    dir
}
