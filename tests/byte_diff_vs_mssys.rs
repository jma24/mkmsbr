//! Byte-equality eval against ms-sys.
//!
//! For each variant: prepare a freshly-formatted (or zero-filled) image,
//! apply the ms-sys pipeline (branch A) and the mkmsbr pipeline (branch
//! B) against *identical* starting images, read back the first N sectors
//! of each, and report sector-by-sector byte differences.
//!
//! Why this exists: the layer-1 oracle compares our embedded *blobs*
//! against ms-sys's output. This eval goes one step further and compares
//! the *post-pipeline on-disk result* — catching bytes that ms-sys writes
//! to disk that we leave as zero (gaps), or sectors ms-sys touches that
//! we don't touch at all.
//!
//! Eval verdict (`#[ignore]`'d; run with --ignored):
//!   * Always prints a full sector-by-sector summary so the developer
//!     can read the structural differences regardless of pass/fail.
//!   * **Fails** if any sector ms-sys writes non-trivially (>= 20
//!     non-zero bytes) is left all-zero by mkmsbr. That's a verifiable
//!     gap — ms-sys puts content on disk where mkmsbr puts nothing.
//!
//! Not a pure byte-equality test. Clean-room divergence in the boot-code
//! regions (sector 0 offsets 0..3 + 90..510, sector 2 boot code, etc.)
//! is expected and ignored by the gap assertion.

#![allow(clippy::needless_range_loop)]

mod common;

use common::oracle;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const SECTOR_SIZE: usize = 512;
/// Number of leading sectors compared. ms-sys --fat32pe is known to write
/// sectors 0/1/2/6/12; 16 covers all of them with margin.
const SECTORS_TO_COMPARE: usize = 16;
/// Threshold for "ms-sys non-trivially writes this sector." Below this we
/// don't claim a gap — single stray bytes might be artifacts.
const NZ_THRESHOLD_FOR_GAP: usize = 20;

// ---------------------------------------------------------------------------
// FAT32 BOOTMGR (multi-sector PBR)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn fat32_pbr_bootmgr_multi_byte_diff_vs_mssys() {
    if let Err(e) = oracle::find_ms_sys() {
        eprintln!("skipping: {e}");
        return;
    }
    if mkmsbr::FAT32_PBR_BOOTMGR_MULTI_BOOT.is_empty() {
        panic!(
            "FAT32_PBR_BOOTMGR_MULTI_BOOT is empty — rebuild with --features embed-boot-asm"
        );
    }

    let dir = tempdir("fat32-pbr-bootmgr-multi-diff");
    let theirs = dir.join("theirs.img");
    let ours = dir.join("ours.img");

    prepare_fat32_image(&theirs).expect("seed fat32 image (theirs)");
    fs::copy(&theirs, &ours).expect("clone image for ours branch");

    oracle::run_ms_sys(&["--fat32pe", "-f"], &theirs)
        .expect("ms-sys --fat32pe on theirs.img");
    apply_mkmsbr_fat32_pbr_multi(&ours).expect("mkmsbr splice on ours.img");

    let theirs_bytes = read_first_n_sectors(&theirs, SECTORS_TO_COMPARE);
    let ours_bytes = read_first_n_sectors(&ours, SECTORS_TO_COMPARE);

    let _ = fs::remove_dir_all(&dir);

    let report = SectorDiffReport::compute(&theirs_bytes, &ours_bytes, SECTORS_TO_COMPARE);
    eprintln!(
        "\n=== fat32_pbr_bootmgr_multi vs ms-sys --fat32pe (first {} sectors) ===\n{}",
        SECTORS_TO_COMPARE, report
    );

    let gaps = report.gap_sectors(NZ_THRESHOLD_FOR_GAP);
    assert!(
        gaps.is_empty(),
        "GAPS DETECTED: mkmsbr leaves sector(s) {:?} all-zero, but ms-sys writes non-trivial \
         content there (>= {} non-zero bytes). These are verifiable gaps — content ms-sys \
         puts on disk that mkmsbr doesn't. Run with --nocapture to see the per-sector report.",
        gaps,
        NZ_THRESHOLD_FOR_GAP,
    );
}

// ---------------------------------------------------------------------------
// MBR (Win 7)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn mbr_win7_byte_diff_vs_mssys() {
    if let Err(e) = oracle::find_ms_sys() {
        eprintln!("skipping: {e}");
        return;
    }
    if mkmsbr::MBR_WIN7_BOOT.is_empty() {
        panic!("MBR_WIN7_BOOT is empty — rebuild with --features embed-boot-asm");
    }

    let dir = tempdir("mbr-win7-diff");
    let theirs = dir.join("theirs.img");
    let ours = dir.join("ours.img");

    // 16 MiB zero-filled image: large enough that mkmsbr::mbr_win7's
    // partition table is well-formed; ms-sys doesn't care about size.
    let image_bytes: usize = 16 * 1024 * 1024;
    fs::write(&theirs, vec![0u8; image_bytes]).expect("seed theirs.img");
    fs::copy(&theirs, &ours).expect("clone for ours.img");

    oracle::run_ms_sys(&["--mbr7", "-f"], &theirs).expect("ms-sys --mbr7");

    let mbr = mkmsbr::mbr_win7((image_bytes / SECTOR_SIZE) as u64).expect("mkmsbr::mbr_win7");
    write_sector0(&ours, &mbr).expect("write our MBR");

    // MBR is single-sector, so only compare sector 0.
    let theirs_bytes = read_first_n_sectors(&theirs, 1);
    let ours_bytes = read_first_n_sectors(&ours, 1);

    let _ = fs::remove_dir_all(&dir);

    let report = SectorDiffReport::compute(&theirs_bytes, &ours_bytes, 1);
    eprintln!(
        "\n=== mbr_win7 vs ms-sys --mbr7 (sector 0 only) ===\n{}\n\
         Region map:\n  \
         0x000..0x1B8 (0..440)   boot code         — clean-room divergence expected\n  \
         0x1B8..0x1BE (440..446) NT disk signature — both expected ZERO from a zero-fill start\n  \
         0x1BE..0x1FE (446..510) partition table   — ms-sys preserves (zeroes); mkmsbr writes one FAT32-LBA active entry\n  \
         0x1FE..0x200 (510..512) boot signature    — both 0x55 0xAA\n",
        report
    );

    // Disk-signature region check: from a zero-fill start, neither
    // should populate offsets 440..446. If they do, document what.
    let theirs_sig = &theirs_bytes[440..446];
    let ours_sig = &ours_bytes[440..446];
    if theirs_sig != ours_sig {
        eprintln!(
            "NOTE: disk-signature region (0x1B8..0x1BE) differs:\n  \
             ms-sys: {:02X?}\n  mkmsbr: {:02X?}",
            theirs_sig, ours_sig,
        );
    }

    // Gap assertion only applies to sectors beyond sector 0 (MBR is one
    // sector by definition), so there's no gap check here. The report
    // is the eval signal.
}

// ---------------------------------------------------------------------------
// Image setup helpers
// ---------------------------------------------------------------------------

fn prepare_fat32_image(path: &Path) -> Result<(), String> {
    fs::write(path, vec![0u8; 64 * 1024 * 1024])
        .map_err(|e| format!("seed image: {e}"))?;
    // Match tests/common/oracle.rs: mformat with FAT32 forced + a label.
    let out = std::process::Command::new("mformat")
        .args(["-F", "-i"])
        .arg(path)
        .args(["-v", "MKMSBR", "::"])
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

fn apply_mkmsbr_fat32_pbr_multi(path: &Path) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("open: {e}"))?;
    let mut existing = [0u8; 1024];
    file.read_exact(&mut existing)
        .map_err(|e| format!("read existing 1024 bytes: {e}"))?;
    let spliced = mkmsbr::splice_fat32_pbr_multi(&existing, mkmsbr::FAT32_PBR_BOOTMGR_MULTI_BOOT)
        .map_err(|e| format!("splice_fat32_pbr_multi: {e}"))?;
    file.seek(SeekFrom::Start(0))
        .map_err(|e| format!("seek: {e}"))?;
    file.write_all(&spliced)
        .map_err(|e| format!("write spliced: {e}"))?;
    Ok(())
}

fn write_sector0(path: &Path, sector: &[u8; 512]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|e| format!("open: {e}"))?;
    file.write_all(sector).map_err(|e| format!("write: {e}"))?;
    Ok(())
}

fn read_first_n_sectors(path: &Path, n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n * SECTOR_SIZE];
    let mut f = fs::File::open(path).expect("open for read");
    f.read_exact(&mut buf).expect("read_first_n_sectors");
    buf
}

fn tempdir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mkmsbr-diff-{}-{}",
        label,
        std::process::id()
    ));
    let _ = fs::create_dir_all(&dir);
    dir
}

// ---------------------------------------------------------------------------
// Diff reporting
// ---------------------------------------------------------------------------

struct SectorDiffReport {
    sectors: Vec<SectorDiff>,
}

struct SectorDiff {
    index: usize,
    theirs_nonzero: usize,
    ours_nonzero: usize,
    differing_bytes: usize,
    /// First few ranges of contiguous differing offsets within the sector.
    diff_ranges: Vec<(usize, usize)>,
}

impl SectorDiffReport {
    fn compute(theirs: &[u8], ours: &[u8], n_sectors: usize) -> Self {
        assert_eq!(theirs.len(), ours.len());
        assert!(theirs.len() >= n_sectors * SECTOR_SIZE);

        let mut sectors = Vec::with_capacity(n_sectors);
        for s in 0..n_sectors {
            let off = s * SECTOR_SIZE;
            let ts = &theirs[off..off + SECTOR_SIZE];
            let os = &ours[off..off + SECTOR_SIZE];

            let theirs_nz = ts.iter().filter(|&&b| b != 0).count();
            let ours_nz = os.iter().filter(|&&b| b != 0).count();
            let mut differing = 0usize;
            let mut ranges: Vec<(usize, usize)> = Vec::new();
            let mut cur: Option<(usize, usize)> = None;
            for i in 0..SECTOR_SIZE {
                if ts[i] != os[i] {
                    differing += 1;
                    cur = Some(match cur {
                        Some((start, _)) => (start, i + 1),
                        None => (i, i + 1),
                    });
                } else if let Some(r) = cur.take() {
                    ranges.push(r);
                }
            }
            if let Some(r) = cur {
                ranges.push(r);
            }

            sectors.push(SectorDiff {
                index: s,
                theirs_nonzero: theirs_nz,
                ours_nonzero: ours_nz,
                differing_bytes: differing,
                diff_ranges: ranges,
            });
        }
        SectorDiffReport { sectors }
    }

    /// Sectors where ms-sys writes >= threshold non-zero bytes but our
    /// output sector is entirely zero. The actionable "verifiable gap"
    /// signal.
    fn gap_sectors(&self, threshold: usize) -> Vec<usize> {
        self.sectors
            .iter()
            .filter(|s| s.theirs_nonzero >= threshold && s.ours_nonzero == 0)
            .map(|s| s.index)
            .collect()
    }
}

impl std::fmt::Display for SectorDiffReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "  LBA | theirs_nz | ours_nz | diff_bytes | diff_ranges"
        )?;
        writeln!(
            f,
            "  ----+-----------+---------+------------+------------"
        )?;
        for s in &self.sectors {
            let ranges_str = if s.diff_ranges.is_empty() {
                "(identical)".to_string()
            } else {
                let shown: Vec<String> = s
                    .diff_ranges
                    .iter()
                    .take(4)
                    .map(|(a, b)| format!("{a}..{b}"))
                    .collect();
                let suffix = if s.diff_ranges.len() > 4 {
                    format!(" (+{} more)", s.diff_ranges.len() - 4)
                } else {
                    String::new()
                };
                format!("{}{suffix}", shown.join(", "))
            };
            let gap_marker = if s.theirs_nonzero >= NZ_THRESHOLD_FOR_GAP && s.ours_nonzero == 0 {
                "  *** GAP ***"
            } else {
                ""
            };
            writeln!(
                f,
                "  {:>3} | {:>9} | {:>7} | {:>10} | {}{}",
                s.index, s.theirs_nonzero, s.ours_nonzero, s.differing_bytes, ranges_str, gap_marker
            )?;
        }
        Ok(())
    }
}
