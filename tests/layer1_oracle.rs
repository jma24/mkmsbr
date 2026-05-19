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
    let diffs = ours.iter().zip(theirs.iter()).filter(|(a, b)| a != b).count();
    eprintln!("{variant}: Hamming distance from ms-sys {mssys_flag} = {diffs}/440 bytes");
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

// TODO: PBR byte-equality eval (fat32_pbr_bootmgr vs ms-sys --fat32pe).
// Multi-sector: ms-sys writes sectors 0, 1, and 12 (or thereabouts; needs
// confirmation against ms-sys source via the spec's "consult their output,
// never their source" rule). The oracle needs to format a FAT32 image
// first, run ms-sys against it, then read back the touched sectors and
// strip the per-partition BPB so we compare boot-code regions only.
// Wire up alongside the `fat32_pbr_bootmgr` variant implementation.

