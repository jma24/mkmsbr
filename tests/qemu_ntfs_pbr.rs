//! Layer-2 QEMU smoke test for the NTFS PBR (multi-sector BOOTMGR variant).
//!
//! Ignored by default; run with:
//!
//!     cargo test --test qemu_ntfs_pbr --features embed-boot-asm -- --ignored
//!
//! Requires:
//!   - `nasm` to assemble the fake bootmgr stub
//!   - `qemu-system-i386` to boot the image
//!   - `docker` to format NTFS (macOS has no mkfs.ntfs; we shell out to
//!     an ephemeral Alpine container with ntfs-3g-progs installed)
//!
//! Flow:
//!   1. Build `fake_bootmgr.bin` (prints "BOOTREC OK\n" to COM1, halts),
//!      then pad to 2 KiB so NTFS stores its DATA attribute non-resident
//!      (resident DATA is unsupported by the current PBR — see
//!      boot-asm/ntfs_pbr_bootmgr/sector1.asm error code 'D').
//!   2. Format a 16 MiB raw NTFS image under Docker (mkfs.ntfs -f -F).
//!   3. Copy the padded fake bootmgr into the image as `\BOOTMGR` via
//!      `ntfscp` (no mount needed, no FUSE, no --privileged).
//!   4. Read the freshly-formatted LBA 0 + LBA 1, splice our PBR blob
//!      through `splice_ntfs_pbr_multi` (preserving BPB bytes 3..84 and
//!      the formatter's LBA 1), write the spliced 2048 bytes back over
//!      sectors 0..3 (stage 2 now lives at LBA 2..3).
//!   5. Boot the raw image under `qemu-system-i386 -serial stdio`.
//!   6. Pass if the serial output contains "BOOTREC OK".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const IMAGE_BYTES: u64 = 16 * 1024 * 1024;

#[test]
#[ignore]
fn ntfs_pbr_bootmgr_multi_chainloads_in_qemu() {
    if let Err(reason) = check_dependencies() {
        eprintln!("skipping qemu ntfs test: {reason}");
        return;
    }

    let blob = bootrec::NTFS_PBR_BOOTMGR_MULTI_BOOT;
    assert!(
        !blob.is_empty(),
        "NTFS PBR blob is empty (built without --features embed-boot-asm). \
         Re-run: cargo test --test qemu_ntfs_pbr --features embed-boot-asm -- --ignored"
    );
    assert!(
        blob.len() >= 1024 && blob.len() % 512 == 0,
        "NTFS multi blob is {} bytes; expected non-zero multiple of 512",
        blob.len()
    );

    let boot_asm = repo_root().join("boot-asm");
    let fake_loader = build_padded_fake_bootmgr(&boot_asm).expect("building padded fake_bootmgr");

    let tmp = tempdir();
    let image = tmp.join("bootrec-ntfs-pbr.img");
    create_ntfs_image(&image, &fake_loader).expect("creating NTFS image");
    splice_our_pbr(&image, blob).expect("splicing NTFS PBR");

    let serial = boot_under_qemu(&image).expect("running qemu");
    assert!(
        serial.contains("BOOTREC OK"),
        "qemu serial missing 'BOOTREC OK'. Got:\n---\n{serial}\n---"
    );
}

fn check_dependencies() -> Result<(), String> {
    for tool in &["nasm", "qemu-system-i386", "docker"] {
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

/// Assemble fake_bootmgr.bin via `make test-fixtures`, then pad it to
/// 2 KiB. Padding ensures NTFS makes the DATA attribute non-resident —
/// the resident-DATA path isn't implemented in the PBR. The padded copy
/// goes into a tempdir so the canonical bin in boot-asm/build/ stays
/// pristine for the FAT32 tests.
fn build_padded_fake_bootmgr(boot_asm: &Path) -> Result<PathBuf, String> {
    let status = Command::new("make")
        .args(["test-fixtures"])
        .current_dir(boot_asm)
        .status()
        .map_err(|e| format!("invoking make in {}: {e}", boot_asm.display()))?;
    if !status.success() {
        return Err("`make test-fixtures` failed".to_string());
    }
    let src = boot_asm.join("build").join("fake_bootmgr.bin");
    let bytes = fs::read(&src).map_err(|e| format!("read {}: {e}", src.display()))?;

    let mut padded = bytes;
    const TARGET: usize = 2048;
    if padded.len() < TARGET {
        padded.resize(TARGET, 0x90); // NOP filler — never executed
    }

    let dest = tempdir().join("fake_bootmgr_padded.bin");
    fs::write(&dest, &padded).map_err(|e| format!("write padded fake: {e}"))?;
    Ok(dest)
}

/// Create a 16 MiB raw NTFS image with `\BOOTMGR` = `fake_loader`.
/// Uses Docker because macOS has no mkfs.ntfs; the Alpine image is
/// pinned implicitly to whatever `:latest` resolves to (acceptable for
/// an ephemeral L2 fixture — no persistence, no security boundary).
fn create_ntfs_image(image: &Path, fake_loader: &Path) -> Result<(), String> {
    let f = fs::File::create(image).map_err(|e| format!("create image: {e}"))?;
    f.set_len(IMAGE_BYTES).map_err(|e| format!("set_len: {e}"))?;
    drop(f);

    let work = image.parent().expect("image has parent");
    let img_name = image.file_name().expect("image has filename").to_string_lossy().into_owned();
    let fake_name = "BOOTMGR.payload";
    let dest = work.join(fake_name);
    // Avoid the self-copy case: build_padded_fake_bootmgr stages into
    // the same tempdir, so guard against src == dst.
    if fake_loader.canonicalize().ok() != dest.canonicalize().ok() {
        fs::copy(fake_loader, &dest).map_err(|e| format!("stage fake_bootmgr: {e}"))?;
    } else {
        // already there under a different name? Read+write instead.
        let bytes = fs::read(fake_loader).map_err(|e| format!("read fake: {e}"))?;
        fs::write(&dest, &bytes).map_err(|e| format!("write fake: {e}"))?;
    }

    let mount = format!("{}:/work", work.display());
    let cmd = format!(
        "set -e; \
         apk add --no-cache ntfs-3g-progs >/dev/null 2>&1; \
         mkfs.ntfs -f -F /work/{img_name} >/dev/null; \
         ntfscp /work/{img_name} /work/{fake_name} /BOOTMGR >/dev/null"
    );
    let out = Command::new("docker")
        .args(["run", "--rm", "-v", &mount, "-w", "/work", "alpine:latest", "sh", "-c", &cmd])
        .output()
        .map_err(|e| format!("docker run: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "NTFS format via docker failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
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
    // Read LBA 0 + LBA 1. The splice preserves LBA 1 verbatim and places
    // stage 2 at LBA 2..3 (see splice_ntfs_pbr_multi docstring).
    let mut existing = [0u8; 1024];
    file.read_exact(&mut existing)
        .map_err(|e| format!("reading existing PBR + LBA 1: {e}"))?;
    let spliced = bootrec::splice_ntfs_pbr_multi(&existing, blob)
        .map_err(|e| format!("splice_ntfs_pbr_multi: {e}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    file.write_all(&spliced)
        .map_err(|e| format!("writing spliced PBR: {e}"))?;
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

    let deadline = std::time::Instant::now() + Duration::from_secs(15);
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
    let dir = std::env::temp_dir().join(format!("bootrec-ntfs-pbr-qemu-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    dir
}
