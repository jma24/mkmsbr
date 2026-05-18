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

/// Compare bootrec's generic MBR boot code to ms-sys's --mbr7 (Win 7 MBR).
///
/// **Status:** expected to FAIL today. The seed `boot-asm/mbr.asm` is a
/// generic MBR (find active partition + chain-load) that pre-dates the
/// variant-split between `mbr_win7` and `mbr_xp`. The v1.0 work is to
/// produce two separate variants; this eval becomes the gate for the
/// `mbr_win7` variant.
#[test]
#[ignore]
fn mbr_win7_bootcode_matches_mssys() {
    if bootrec::MBR_BOOT.is_empty() {
        panic!(
            "MBR_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    let theirs = oracle::ms_sys_mbr_win7_bootcode()
        .unwrap_or_else(|e| panic!("ms-sys oracle failed: {e}"));
    let ours: &[u8] = &bootrec::MBR_BOOT[0..440];
    if ours == theirs.as_slice() {
        return;
    }
    let diffs = ours.iter().zip(theirs.iter()).filter(|(a, b)| a != b).count();
    panic!(
        "bootrec MBR_BOOT != ms-sys --mbr7 (Hamming distance: {diffs}/440 bytes)\n\
         Diff sample (first 16 differing offsets):\n{}",
        diff_sample(ours, &theirs)
    );
}

/// Same as `mbr_win7_bootcode_matches_mssys` but for the XP MBR (`--mbr`).
///
/// **Status:** the seed MBR is closer to the generic XP shape than to the
/// Win 7 shape (Win 7's MBR has the disk-signature check + GPT-fallback
/// path that XP's doesn't). Still expected to differ; treat as the
/// eval gate for the eventual `mbr_xp` variant.
#[test]
#[ignore]
fn mbr_xp_bootcode_matches_mssys() {
    if bootrec::MBR_BOOT.is_empty() {
        panic!(
            "MBR_BOOT is empty (built without --features embed-boot-asm). \
             Re-run with --features \"embed-boot-asm compare-mssys\"."
        );
    }
    let theirs = oracle::ms_sys_mbr_xp_bootcode()
        .unwrap_or_else(|e| panic!("ms-sys oracle failed: {e}"));
    let ours: &[u8] = &bootrec::MBR_BOOT[0..440];
    if ours == theirs.as_slice() {
        return;
    }
    let diffs = ours.iter().zip(theirs.iter()).filter(|(a, b)| a != b).count();
    panic!(
        "bootrec MBR_BOOT != ms-sys --mbr (Hamming distance: {diffs}/440 bytes)\n\
         Diff sample (first 16 differing offsets):\n{}",
        diff_sample(ours, &theirs)
    );
}

// TODO: PBR byte-equality eval (fat32_pbr_bootmgr vs ms-sys --fat32pe).
// Multi-sector: ms-sys writes sectors 0, 1, and 12 (or thereabouts; needs
// confirmation against ms-sys source via the spec's "consult their output,
// never their source" rule). The oracle needs to format a FAT32 image
// first, run ms-sys against it, then read back the touched sectors and
// strip the per-partition BPB so we compare boot-code regions only.
// Wire up alongside the `fat32_pbr_bootmgr` variant implementation.

fn diff_sample(a: &[u8], b: &[u8]) -> String {
    let mut lines = Vec::new();
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x != y {
            lines.push(format!("  offset 0x{i:03X}: ours={x:02X}  mssys={y:02X}"));
            if lines.len() >= 16 {
                break;
            }
        }
    }
    lines.join("\n")
}
