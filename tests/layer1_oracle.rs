//! Layer-1 evals: byte-equality vs ms-sys.
//!
//! Gated behind the `compare-mssys` feature so the default `cargo test`
//! doesn't try to run ms-sys. To run these:
//!
//!     cargo test --test layer1_oracle \
//!         --features "embed-boot-asm compare-mssys" -- --ignored
//!
//! Requires:
//!   - ms-sys installed at `/tmp/ms-sys/bin/ms-sys` or via `BOOTREC_MS_SYS`
//!     env var (`git clone https://gitlab.com/cmaiolino/ms-sys.git /tmp/ms-sys && cd /tmp/ms-sys && make`).
//!   - nasm installed (for `--features embed-boot-asm`).
//!
//! These tests are expected to FAIL until each variant's library
//! implementation matches ms-sys's output. That's the methodology — the
//! eval is the binary gate that tells us when a variant is done. See
//! `docs/SPEC.md` §Eval-first workflow.

#![cfg(feature = "compare-mssys")]

mod common;

use common::oracle;

/// Doubles as the spec's "statistical similarity check" (§Clean-room
/// mechanisms #4): fails if the Hamming distance is SUSPICIOUSLY LOW
/// (< 10 bytes for a non-trivial 440-byte boot record). Too few
/// differences suggests copying. Larger distances are expected; the
/// primary correctness gate is Layer 2 (`qemu_mbr.rs`), already green.
///
/// Reports the distance via `eprintln!` so CI logs surface the trend
/// over time — a sudden jump up or down warrants a closer look.
const SUSPICIOUSLY_LOW: usize = 10;

#[test]
#[ignore]
fn mbr_xp_bootcode_distance_from_mssys() {
    if bootrec::MBR_XP_BOOT.is_empty() {
        panic!(
            "MBR_XP_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    let theirs = oracle::ms_sys_mbr_xp_bootcode()
        .unwrap_or_else(|e| panic!("ms-sys oracle failed: {e}"));
    assert_distance("mbr_xp", "--mbr", &bootrec::MBR_XP_BOOT[0..440], &theirs);
}

#[test]
#[ignore]
fn mbr_win7_bootcode_distance_from_mssys() {
    if bootrec::MBR_WIN7_BOOT.is_empty() {
        panic!(
            "MBR_WIN7_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    let theirs = oracle::ms_sys_mbr_win7_bootcode()
        .unwrap_or_else(|e| panic!("ms-sys oracle failed: {e}"));
    assert_distance("mbr_win7", "--mbr7", &bootrec::MBR_WIN7_BOOT[0..440], &theirs);
}

fn assert_distance(variant: &str, mssys_flag: &str, ours: &[u8], theirs: &[u8]) {
    assert_eq!(
        ours.len(),
        theirs.len(),
        "[{variant}] length mismatch: ours={} theirs={}",
        ours.len(),
        theirs.len()
    );
    let total = ours.len();
    let diffs = ours.iter().zip(theirs.iter()).filter(|(a, b)| a != b).count();
    eprintln!("{variant}: Hamming distance from ms-sys {mssys_flag} = {diffs}/{total} bytes");
    if diffs == 0 {
        eprintln!("  Byte-identical to ms-sys. Either remarkable parallel invention");
        eprintln!("  or the cleanroom protocol failed — review the asm source.");
    }
    if diffs < SUSPICIOUSLY_LOW {
        panic!(
            "[{variant}] Hamming distance ({diffs}) is below the suspiciously-low threshold ({SUSPICIOUSLY_LOW}). \
             Per docs/SPEC.md §Clean-room mechanisms #4, this triggers a manual review: \
             does the .asm source look copy-pasted? If parallel invention is genuine, \
             relax the threshold here with justification."
        );
    }
}

// PBR layer-1 evals: compare only the boot-code regions (bytes 0..3 +
// 90..510 = 423 bytes). The 87-byte BPB at offsets 3..90 is filesystem
// state, not boot code, and varies by the formatter — comparing it
// would just measure mformat-vs-ms-sys-formatter differences, which
// isn't what we care about.
fn pbr_bootcode_regions(sector0: &[u8; 512]) -> Vec<u8> {
    let mut v = Vec::with_capacity(423);
    v.extend_from_slice(&sector0[0..3]);
    v.extend_from_slice(&sector0[90..510]);
    v
}

// NTFS boot-code regions: bytes 0..3 (jump) + 84..510 (boot code) = 429
// bytes. The NTFS BPB is 73 bytes at offsets 11..84 (vs FAT32's 87
// bytes at 3..90), and the OEM at 3..11 is the "NTFS    " literal
// which is filesystem-state too, not boot code.
fn ntfs_pbr_bootcode_regions(sector0: &[u8; 512]) -> Vec<u8> {
    let mut v = Vec::with_capacity(429);
    v.extend_from_slice(&sector0[0..3]);
    v.extend_from_slice(&sector0[84..510]);
    v
}

#[test]
#[ignore]
fn fat32_pbr_bootmgr_distance_from_mssys() {
    if bootrec::FAT32_PBR_BOOTMGR_BOOT.is_empty() {
        panic!(
            "FAT32_PBR_BOOTMGR_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    let theirs = oracle::ms_sys_fat32_bootmgr_pbr()
        .unwrap_or_else(|e| panic!("ms-sys PBR oracle failed: {e}"));
    let mut ours_full = [0u8; 512];
    ours_full.copy_from_slice(&bootrec::FAT32_PBR_BOOTMGR_BOOT[0..512]);
    let ours = pbr_bootcode_regions(&ours_full);
    let theirs = pbr_bootcode_regions(&theirs);
    assert_distance("fat32_pbr_bootmgr", "--fat32pe (sector 0)", &ours, &theirs);
}

#[test]
#[ignore]
fn fat32_pbr_ntldr_distance_from_mssys() {
    if bootrec::FAT32_PBR_NTLDR_BOOT.is_empty() {
        panic!(
            "FAT32_PBR_NTLDR_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    let theirs = oracle::ms_sys_fat32_ntldr_pbr()
        .unwrap_or_else(|e| panic!("ms-sys PBR oracle failed: {e}"));
    let mut ours_full = [0u8; 512];
    ours_full.copy_from_slice(&bootrec::FAT32_PBR_NTLDR_BOOT[0..512]);
    let ours = pbr_bootcode_regions(&ours_full);
    let theirs = pbr_bootcode_regions(&theirs);
    assert_distance("fat32_pbr_ntldr", "--fat32nt (sector 0)", &ours, &theirs);
}

/// NTFS PBR baseline. Compares the boot-code regions of our `ntfs_pbr.asm`
/// stub against ms-sys --ntfs sector 0. Until session 3 implements an
/// actual $MFT walk, the stub is a halt loop and the Hamming distance
/// will be near-maximal — that's the point of the eval-first methodology
/// (`docs/SPEC.md` §Eval-first Step 0: "the evals fail at this point.
/// That's the point").
///
/// This test will start passing the SUSPICIOUSLY_LOW threshold check
/// trivially (distance is large), but the eprintln baseline lets us
/// track convergence as the NTFS implementation matures.
///
/// Requires Docker for the NTFS image build. Test surfaces a clear skip
/// message if Docker is unavailable.
#[test]
#[ignore]
fn ntfs_pbr_bootmgr_distance_from_mssys() {
    if bootrec::NTFS_PBR_BOOT.is_empty() {
        panic!(
            "NTFS_PBR_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    match common::ntfs_image::docker_status() {
        common::ntfs_image::DockerStatus::Available => {}
        common::ntfs_image::DockerStatus::Missing(reason) => {
            eprintln!("skipping NTFS L1 test: {reason}");
            return;
        }
    }

    let theirs = oracle::ms_sys_ntfs_pbr_sector0()
        .unwrap_or_else(|e| panic!("ms-sys NTFS PBR oracle failed: {e}"));
    let mut ours_full = [0u8; 512];
    ours_full.copy_from_slice(&bootrec::NTFS_PBR_BOOT[0..512]);
    let ours = ntfs_pbr_bootcode_regions(&ours_full);
    let theirs = ntfs_pbr_bootcode_regions(&theirs);
    assert_distance("ntfs_pbr_bootmgr", "--ntfs (sector 0)", &ours, &theirs);
}

/// Multi-sector eval. Our blob is 2 sectors; ms-sys's `--fat32pe` layout
/// is 16. Sector 0 reuses the same boot-code-regions split as the
/// single-sector tests above. Sector 1 has no BPB, so the comparison is
/// over all 512 bytes — but the alignment between *our* sector 1 and one
/// of ms-sys's sectors 1..15 is the open question the test answers: it
/// reports the Hamming distance against every non-zero ms-sys sector and
/// fails only if the closest match is suspiciously low (i.e. potential
/// copying — clean-room mechanism #4). The "true" alignment will be
/// whichever sector has the lowest non-zero distance, surfaced in the
/// eprintln log for the developer to read.
#[test]
#[ignore]
fn fat32_pbr_bootmgr_multi_distance_from_mssys() {
    if bootrec::FAT32_PBR_BOOTMGR_MULTI_BOOT.is_empty() {
        panic!(
            "FAT32_PBR_BOOTMGR_MULTI_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    assert!(
        bootrec::FAT32_PBR_BOOTMGR_MULTI_BOOT.len() >= 1024,
        "multi-sector blob is {} bytes; expected >= 1024",
        bootrec::FAT32_PBR_BOOTMGR_MULTI_BOOT.len()
    );

    let theirs_16 = oracle::ms_sys_fat32_bootmgr_pbr_multi()
        .unwrap_or_else(|e| panic!("ms-sys multi-sector PBR oracle failed: {e}"));

    // --- Sector 0: same boot-code regions as the single-sector eval. ---
    let mut our_s0 = [0u8; 512];
    our_s0.copy_from_slice(&bootrec::FAT32_PBR_BOOTMGR_MULTI_BOOT[0..512]);
    let mut their_s0 = [0u8; 512];
    their_s0.copy_from_slice(&theirs_16[0..512]);
    let ours_regions = pbr_bootcode_regions(&our_s0);
    let theirs_regions = pbr_bootcode_regions(&their_s0);
    assert_distance(
        "fat32_pbr_bootmgr_multi",
        "--fat32pe (sector 0, boot-code regions)",
        &ours_regions,
        &theirs_regions,
    );

    // --- Sector 1: full 512 bytes against each non-zero ms-sys sector. ---
    let our_s1 = &bootrec::FAT32_PBR_BOOTMGR_MULTI_BOOT[512..1024];
    let mut best: Option<(usize, usize)> = None; // (distance, sector_idx)
    eprintln!("fat32_pbr_bootmgr_multi sector 1 vs ms-sys --fat32pe sectors 1..15:");
    for i in 1..16usize {
        let ms_sector = &theirs_16[i * 512..(i + 1) * 512];
        let nz = ms_sector.iter().filter(|&&b| b != 0).count();
        if nz == 0 {
            eprintln!("  sector {i:>2}: ms-sys sector is all-zero (skipped)");
            continue;
        }
        let diffs = our_s1.iter().zip(ms_sector.iter()).filter(|(a, b)| a != b).count();
        eprintln!(
            "  sector {i:>2}: Hamming={diffs:>3}/512 (ms-sys sector has {nz} non-zero bytes)"
        );
        match best {
            Some((d, _)) if diffs >= d => {}
            _ => best = Some((diffs, i)),
        }
    }
    let (best_diffs, best_idx) = best.expect(
        "ms-sys --fat32pe produced no non-zero sectors in 1..15 — unexpected layout, \
         this test's assumptions need revisiting",
    );
    eprintln!(
        "fat32_pbr_bootmgr_multi: best sector-1 alignment is ms-sys sector {best_idx} \
         (Hamming={best_diffs}/512)"
    );
    if best_diffs < SUSPICIOUSLY_LOW {
        panic!(
            "[fat32_pbr_bootmgr_multi] our sector 1 is suspiciously close to ms-sys sector \
             {best_idx} (Hamming={best_diffs} < {SUSPICIOUSLY_LOW}). Per docs/SPEC.md \
             §Clean-room mechanisms #4 this triggers manual review of \
             boot-asm/fat32_pbr_bootmgr/sector1.asm."
        );
    }
}

