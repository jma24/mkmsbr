//! Layer-1 eval helper: ms-sys subprocess invocation + byte extraction.
//!
//! See `docs/SPEC.md` §Verifiability hierarchy / §Clean-room protocol.
//! ms-sys appears in the codebase ONLY here — as a black-box subprocess
//! the test harness uses to obtain the reference bytes a known-correct
//! implementation produces. The library source files under `src/` and
//! `boot-asm/` have no awareness of ms-sys at all.

use std::path::PathBuf;
use std::process::Command;

/// Locate the ms-sys binary. Resolution order:
///   1. `BOOTREC_MS_SYS` env var (full path)
///   2. `/tmp/ms-sys/bin/ms-sys` (developer's local checkout — common case)
///   3. `/usr/local/bin/ms-sys`
///   4. `/opt/homebrew/bin/ms-sys`
///   5. PATH lookup via `which`
///
/// Returns `Err` with a clear message if none resolve, so individual tests
/// can choose between `panic!()` ("this test requires ms-sys") and graceful
/// skip ("eprintln! and return").
pub fn find_ms_sys() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("BOOTREC_MS_SYS") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
        return Err(format!("BOOTREC_MS_SYS={} does not exist", p.display()));
    }
    for candidate in &[
        "/tmp/ms-sys/bin/ms-sys",
        "/usr/local/bin/ms-sys",
        "/opt/homebrew/bin/ms-sys",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return Ok(p);
        }
    }
    let out = Command::new("/usr/bin/env")
        .args(["which", "ms-sys"])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !path.is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    Err(
        "ms-sys not found. Install with `git clone https://gitlab.com/cmaiolino/ms-sys.git \
         /tmp/ms-sys && cd /tmp/ms-sys && make`, or set BOOTREC_MS_SYS."
            .to_string(),
    )
}

/// Run ms-sys with the given args against an image file. The image must
/// already exist and be appropriately prepared (zero-filled for MBR
/// variants; FAT32-formatted for PBR variants).
pub fn run_ms_sys(args: &[&str], image: &std::path::Path) -> Result<(), String> {
    let bin = find_ms_sys()?;
    let mut cmd = Command::new(&bin);
    cmd.args(args).arg(image);
    let out = cmd.output().map_err(|e| format!("spawning ms-sys: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "ms-sys {args:?} failed: {}\nstdout: {}",
            String::from_utf8_lossy(&out.stderr),
            String::from_utf8_lossy(&out.stdout)
        ));
    }
    Ok(())
}

/// Run ms-sys --mbr7 and return its 440-byte MBR boot code (offset 0..440
/// of the resulting sector 0). The partition-table area (440..510) and the
/// boot signature (510..512) are excluded because ms-sys preserves
/// pre-existing partition-table bytes — that's outside what `bootrec`'s
/// `mbr_win7` boot-code variant produces.
pub fn ms_sys_mbr_win7_bootcode() -> Result<[u8; 440], String> {
    mbr_boot_code(&["--mbr7"])
}

/// Run ms-sys --mbr (XP MBR) and return its 440-byte boot-code area.
pub fn ms_sys_mbr_xp_bootcode() -> Result<[u8; 440], String> {
    mbr_boot_code(&["--mbr"])
}

fn mbr_boot_code(args: &[&str]) -> Result<[u8; 440], String> {
    use std::io::Read;
    let tmp = std::env::temp_dir().join(format!(
        "bootrec-oracle-{}-{}",
        args[0].trim_start_matches('-'),
        std::process::id()
    ));
    // Pre-fill with zeros so ms-sys has a target to write to. 1 MiB is
    // ample for whole-disk MBR variants (we only read sector 0 back).
    std::fs::write(&tmp, vec![0u8; 1024 * 1024]).map_err(|e| format!("seed image: {e}"))?;
    // `-f` forces ms-sys to write to a regular file (otherwise it refuses
    // with "does not seem to be a disk device").
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("-f");
    run_ms_sys(&full_args, &tmp)?;
    let mut f = std::fs::File::open(&tmp).map_err(|e| format!("open: {e}"))?;
    let mut buf = [0u8; 512];
    f.read_exact(&mut buf).map_err(|e| format!("read: {e}"))?;
    let _ = std::fs::remove_file(&tmp);
    let mut out = [0u8; 440];
    out.copy_from_slice(&buf[0..440]);
    Ok(out)
}
