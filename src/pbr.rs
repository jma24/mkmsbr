//! The PBR splice. The "preserve the BPB" rule from docs/BOOT_RECORDS.md
//! lives here as code, once per filesystem type. FAT32 preserves bytes
//! 3..89 (OEM + BPB + extended BPB); NTFS preserves 3..84 (OEM + BPB +
//! extended BPB; layout per Microsoft NTFS public docs).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PbrError {
    #[error("existing PBR is {got} bytes; expected exactly 512")]
    BadExistingSize { got: usize },

    #[error("existing reserved area is {got} bytes; expected exactly 1024 (sector 0 + sector 1)")]
    BadExistingMultiSize { got: usize },

    #[error("boot blob is {got} bytes; expected exactly 512")]
    BadBlobSize { got: usize },

    #[error("multi-sector blob is {got} bytes; expected non-zero multiple of 512")]
    BadMultiBlobSize { got: usize },

    #[error("boot blobs were not embedded; rebuild with --features embed-boot-asm")]
    NotEmbedded,

    #[error("run list is empty; need at least one LbaRun")]
    EmptyRuns,

    #[error("run list has {got} entries; bootsect template caps at {max}")]
    TooManyRuns { got: usize, max: usize },

    #[error(
        "total sector count {got} exceeds bootsect cap of {max} \
         (≈{} KB at 512 B/sector)",
        max * 512 / 1024
    )]
    TooManySectors { got: usize, max: usize },

    #[error("formatter sector 0 missing 0xAA55 boot signature at offset 510")]
    MissingBootSignature,

    #[error(
        "target segment {got:#06x} out of sane range [{min:#06x}, {max:#06x}] \
         (too low: overlaps IVT/BDA; too high: wraps real-mode address space)"
    )]
    BadTargetSegment { got: u16, min: u16, max: u16 },
}

/// One contiguous range of sectors on disk, in partition-relative LBA
/// coordinates (the natural output of a FAT walk). The bootsector code
/// adds `BPB.HiddSec` at runtime to get the absolute disk LBA before
/// issuing INT 13h reads.
///
/// Used by [`build_xp_setup_chain_bootsect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LbaRun {
    /// Partition-relative starting LBA.
    pub start_lba: u32,
    /// Number of consecutive sectors in this run. Must be > 0.
    pub sector_count: u16,
}

/// Build the 512-byte BOOTSECT.DAT that NTLDR chainloads via
/// `boot.ini`'s bootsector-entry mechanism during XP Setup. The emitted
/// sector reads the runs into `target_segment:0` via CHS and far-jumps
/// there — typically into `$LDR$` (setupldr.bin renamed) at 0x2000:0.
///
/// `formatter_sector0` is the partition's existing sector 0; the BPB at
/// bytes 3..90 is preserved (only `BPB.HiddSec` at offset 0x1C is read
/// at runtime, but preserving the full BPB keeps the result valid to FS
/// drivers that inspect it).
///
/// `target_segment` is where the loaded payload starts executing. The
/// canonical XP Setup value is 0x2000 (setupldr's expected load
/// address). Sane range is [0x0050, 0x9000] — below 0x0050 collides
/// with the IVT/BDA, above 0x9000 starts to wrap the real-mode address
/// space depending on payload size.
///
/// `runs` is the list of LBA extents to read, partition-relative,
/// pre-coalesced. The caller (usbwin's FAT walker) collapses adjacent
/// LBAs into runs so an unfragmented 260 KB `$LDR$` fits in 1–3 runs
/// rather than 520 separate entries.
///
/// Errors: empty `runs`, more than [`MAX_SETUP_CHAIN_RUNS`] runs,
/// total sectors > [`MAX_SETUP_CHAIN_SECTORS`], missing boot
/// signature, or target segment out of range.
pub fn build_xp_setup_chain_bootsect(
    formatter_sector0: &[u8; 512],
    target_segment: u16,
    runs: &[LbaRun],
) -> Result<[u8; 512], PbrError> {
    // Template-blob check first so a misbuilt crate fails before input
    // validation (which would mask the real cause).
    let template = crate::blobs::XP_SETUP_CHAIN_BOOTSECT_BOOT;
    if template.is_empty() {
        return Err(PbrError::NotEmbedded);
    }

    if runs.is_empty() {
        return Err(PbrError::EmptyRuns);
    }
    if runs.len() > MAX_SETUP_CHAIN_RUNS {
        return Err(PbrError::TooManyRuns {
            got: runs.len(),
            max: MAX_SETUP_CHAIN_RUNS,
        });
    }
    let total_sectors: usize = runs.iter().map(|r| r.sector_count as usize).sum();
    if total_sectors > MAX_SETUP_CHAIN_SECTORS {
        return Err(PbrError::TooManySectors {
            got: total_sectors,
            max: MAX_SETUP_CHAIN_SECTORS,
        });
    }
    if formatter_sector0[510] != 0x55 || formatter_sector0[511] != 0xAA {
        return Err(PbrError::MissingBootSignature);
    }
    const MIN_TARGET_SEG: u16 = 0x0050;
    const MAX_TARGET_SEG: u16 = 0x9000;
    if target_segment < MIN_TARGET_SEG || target_segment > MAX_TARGET_SEG {
        return Err(PbrError::BadTargetSegment {
            got: target_segment,
            min: MIN_TARGET_SEG,
            max: MAX_TARGET_SEG,
        });
    }

    let mut out = [0u8; 512];
    // Splice: same shape as splice_fat32_pbr — jmp + BPB + boot code +
    // signature. The patchable area lives inside the [90..510] boot-code
    // window so it comes through verbatim from the template; we then
    // overwrite the placeholders below.
    out[0..3].copy_from_slice(&template[0..3]);
    out[3..90].copy_from_slice(&formatter_sector0[3..90]);
    out[90..510].copy_from_slice(&template[90..510]);
    out[510] = 0x55;
    out[511] = 0xAA;

    // Patchable region offsets (see boot-asm/xp_setup_chain_bootsect.asm
    // header comment). target_jmp_addr lives at 0x180 with offset=0,
    // segment immediately after.
    const TARGET_SEG_OFFSET: usize = 0x182;
    const RUN_COUNT_OFFSET: usize = 0x184;
    const RUN_TABLE_OFFSET: usize = 0x185;

    out[TARGET_SEG_OFFSET..TARGET_SEG_OFFSET + 2]
        .copy_from_slice(&target_segment.to_le_bytes());
    out[RUN_COUNT_OFFSET] = runs.len() as u8;
    for (i, run) in runs.iter().enumerate() {
        let off = RUN_TABLE_OFFSET + i * 6;
        out[off..off + 4].copy_from_slice(&run.start_lba.to_le_bytes());
        out[off + 4..off + 6].copy_from_slice(&run.sector_count.to_le_bytes());
    }
    Ok(out)
}

/// Maximum number of LbaRuns that fit in the BOOTSECT.DAT template.
/// The patchable run table is 96 bytes at 6 bytes per entry → 16 runs.
pub const MAX_SETUP_CHAIN_RUNS: usize = 16;

/// Maximum total sectors the BOOTSECT.DAT loader will read. 1024 sectors
/// = 512 KB, comfortably above the ~260 KB needed for $LDR$/setupldr.bin
/// and well within a single real-mode segment range (0x2000..0xB000).
pub const MAX_SETUP_CHAIN_SECTORS: usize = 1024;

/// Splice the FAT32 PBR. Given:
///   - `existing`: the 512-byte sector currently at /dev/rdiskNs1 offset 0,
///     i.e. what newfs_msdos just wrote (BPB at 3..89 is what we keep).
///   - `boot`: the 512-byte blob from `boot-asm/build/fat32_pbr.bin`
///
/// Returns a new 512-byte sector ready to be written back to the partition:
///   bytes   0..2   = boot[0..2]       (jump)
///   bytes   3..10  = "MSWIN4.1"       (OEM ID - overwritten; see below)
///   bytes  11..89  = existing[11..89] (BPB body - preserved)
///   bytes  90..509 = boot[90..509]    (boot code)
///   bytes 510..511 = [0x55, 0xAA]     (signature)
pub fn splice_fat32_pbr(existing: &[u8], boot: &[u8]) -> Result<[u8; 512], PbrError> {
    if existing.len() != 512 {
        return Err(PbrError::BadExistingSize { got: existing.len() });
    }
    if boot.is_empty() {
        return Err(PbrError::NotEmbedded);
    }
    if boot.len() != 512 {
        return Err(PbrError::BadBlobSize { got: boot.len() });
    }

    let mut out = [0u8; 512];
    out[0..3].copy_from_slice(&boot[0..3]);
    out[3..90].copy_from_slice(&existing[3..90]);
    // OEM ID overwrite — see splice_fat32_pbr_multi for the full empirical
    // story. Short version: 2005-era BIOSes route USB media through USB-FDD
    // emulation (CHS-only, ~2880-sector cap, DL=0x00) when the boot-sector
    // OEM ID isn't a Microsoft string, and through USB-HDD emulation
    // otherwise. The NTLDR/XP pipeline used to inherit mformat's "BSD  4.4"
    // here, which sent the same legacy hardware that needs the multi-sector
    // bootmgr variant's MSWIN4.1 patch to die with '2' (INT 13h read fail)
    // inside fat32_pbr_ntldr's root-directory walk.
    out[3..11].copy_from_slice(b"MSWIN4.1");
    out[90..510].copy_from_slice(&boot[90..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    Ok(out)
}

/// Multi-sector variant of [`splice_fat32_pbr`]. Given:
///   - `existing`: the freshly-formatted reserved area — exactly 1024
///     bytes covering partition LBA 0 (boot sector + BPB at bytes 3..90)
///     and LBA 1 (FSInfo sector that newfs_msdos placed there).
///   - `blob`: the multi-sector boot code (N × 512 bytes, N ≥ 2). The
///     first 512 bytes are stage 1 (with a BPB placeholder); bytes 512..
///     are stage-2+ continuation sectors (raw, no BPB).
///
/// Returns a `Vec<u8>` of length `blob.len() + 512`:
///   bytes      0..3       = blob[0..3]        (jump from stage 1)
///   bytes      3..90      = existing[3..90]   (BPB - preserved)
///   bytes     90..510     = blob[90..510]     (stage 1 boot code)
///   bytes    510..512     = [0x55, 0xAA]      (boot signature)
///   bytes    512..1024    = existing[512..1024] (FSInfo - preserved)
///   bytes   1024..end     = blob[512..]       (stage 2+ verbatim)
///
/// The caller writes the result starting at partition LBA 0; stage 2
/// lands at LBA 2, matching the layout stage 1 reads (HiddSec + 2).
/// LBA 1 carries the formatter's FSInfo unchanged, which is what
/// ms-sys `--fat32pe` does (its sector 1 contains only the three
/// FSInfo signatures totaling ~10 non-zero bytes).
pub fn splice_fat32_pbr_multi(existing: &[u8], blob: &[u8]) -> Result<Vec<u8>, PbrError> {
    if existing.len() != 1024 {
        return Err(PbrError::BadExistingMultiSize { got: existing.len() });
    }
    if blob.is_empty() {
        return Err(PbrError::NotEmbedded);
    }
    if blob.len() < 1024 || blob.len() % 512 != 0 {
        return Err(PbrError::BadMultiBlobSize { got: blob.len() });
    }

    let mut out = vec![0u8; blob.len() + 512];
    // Sector 0 — same splice as single-sector.
    out[0..3].copy_from_slice(&blob[0..3]);
    out[3..90].copy_from_slice(&existing[3..90]);
    // Overwrite OEM ID at bytes 3..11 with the Microsoft FAT32 signature
    // "MSWIN4.1". 2000s-era BIOSes scan the boot sector's OEM ID at boot
    // and switch between USB-FDD emulation (1.44MB cap, CHS only, DL=0x00)
    // and USB-HDD emulation (full disk, LBA-ext available, DL=0x80+) based
    // on what they find. Anything non-"MSWIN" keeps them in USB-FDD mode
    // and our reads of partition LBA 2+ fail with AH=01 because effective
    // addressable space ends mid-partition. Confirmed empirically on a
    // 2005-vintage P4 BIOS where bootrec-built USB with newfs_msdos's
    // "BSD  4.4" OEM gave R01120200 (USB-FDD, drive=0x00, geom 80/2/18),
    // while the same hardware booted an ms-sys-built USB (OEM "MSWIN4.1")
    // without issue. See docs/BACKLOG.md "Byte-diff findings vs ms-sys".
    out[3..11].copy_from_slice(b"MSWIN4.1");
    out[90..510].copy_from_slice(&blob[90..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    // Sector 1 — preserve FSInfo from the formatter.
    out[512..1024].copy_from_slice(&existing[512..1024]);
    // Sectors 2+ — stage 2 and beyond, verbatim from blob[512..].
    out[1024..].copy_from_slice(&blob[512..]);
    Ok(out)
}

/// Multi-sector NTFS PBR splice. Same shape as
/// [`splice_fat32_pbr_multi`] but the preserved BPB range is bytes
/// 3..84 (OEM ID + NTFS BPB + extended BPB; layout per Microsoft NTFS
/// On-Disk Format public docs).
///
///   bytes      0..3       = blob[0..3]        (jump)
///   bytes      3..84      = existing[3..84]   (OEM + BPB; preserved)
///   bytes     84..510     = blob[84..510]     (stage 1 boot code)
///   bytes    510..512     = [0x55, 0xAA]      (boot signature)
///   bytes    512..1024    = existing[512..1024] (LBA 1; preserved)
///   bytes   1024..end     = blob[512..]       (stage 2+; verbatim)
///
/// `existing` must be 1024 bytes (LBA 0 + LBA 1). NTFS sectors 0..15
/// are reserved by $Boot, so LBA 1 has no fixed-purpose payload — but
/// preserving the formatter's bytes there keeps the splice signature
/// parallel to the FAT32 variant and avoids gratuitously zeroing a
/// sector Microsoft considers part of the bootloader region.
pub fn splice_ntfs_pbr_multi(existing: &[u8], blob: &[u8]) -> Result<Vec<u8>, PbrError> {
    if existing.len() != 1024 {
        return Err(PbrError::BadExistingMultiSize { got: existing.len() });
    }
    if blob.is_empty() {
        return Err(PbrError::NotEmbedded);
    }
    if blob.len() < 1024 || blob.len() % 512 != 0 {
        return Err(PbrError::BadMultiBlobSize { got: blob.len() });
    }

    let mut out = vec![0u8; blob.len() + 512];
    out[0..3].copy_from_slice(&blob[0..3]);
    out[3..84].copy_from_slice(&existing[3..84]);
    out[84..510].copy_from_slice(&blob[84..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    out[512..1024].copy_from_slice(&existing[512..1024]);
    out[1024..].copy_from_slice(&blob[512..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_formatter_sector0() -> [u8; 512] {
        // Minimal valid FAT32 boot sector for the setup-chain tests.
        // We only care about the jump, OEM/BPB area being copyable, and
        // the boot signature; the loader code area gets overwritten.
        let mut s = [0u8; 512];
        s[0..3].copy_from_slice(&[0xEB, 0x58, 0x90]);
        s[3..11].copy_from_slice(b"BSD  4.4");
        // Set a non-zero HiddSec so we can verify the bootsect's runtime
        // would add it (caller doesn't have to compute absolute LBAs).
        s[0x1C..0x20].copy_from_slice(&2048u32.to_le_bytes());
        s[510] = 0x55;
        s[511] = 0xAA;
        s
    }

    fn one_run(start: u32, count: u16) -> Vec<LbaRun> {
        vec![LbaRun {
            start_lba: start,
            sector_count: count,
        }]
    }

    #[test]
    fn setup_chain_emits_512_bytes_with_signature() {
        let s0 = synthetic_formatter_sector0();
        let out = build_xp_setup_chain_bootsect(&s0, 0x2000, &one_run(1000, 128)).unwrap();
        assert_eq!(out.len(), 512);
        assert_eq!(&out[510..512], &[0x55, 0xAA]);
        // BPB preserved (bytes 3..90 from formatter).
        assert_eq!(&out[3..90], &s0[3..90]);
    }

    #[test]
    fn setup_chain_patches_target_segment_and_run_table() {
        let s0 = synthetic_formatter_sector0();
        let runs = vec![
            LbaRun { start_lba: 1000, sector_count: 64 },
            LbaRun { start_lba: 1100, sector_count: 32 },
        ];
        let out = build_xp_setup_chain_bootsect(&s0, 0x2500, &runs).unwrap();

        // target_jmp_addr offset at 0x180 stays 0 (offset within target seg).
        assert_eq!(u16::from_le_bytes([out[0x180], out[0x181]]), 0);
        // target_jmp_addr segment at 0x182 = the patched target_segment.
        assert_eq!(u16::from_le_bytes([out[0x182], out[0x183]]), 0x2500);
        // run_count at 0x184.
        assert_eq!(out[0x184], 2);
        // run_table at 0x185: two 6-byte entries.
        assert_eq!(u32::from_le_bytes(out[0x185..0x189].try_into().unwrap()), 1000);
        assert_eq!(u16::from_le_bytes([out[0x189], out[0x18A]]), 64);
        assert_eq!(u32::from_le_bytes(out[0x18B..0x18F].try_into().unwrap()), 1100);
        assert_eq!(u16::from_le_bytes([out[0x18F], out[0x190]]), 32);
        // Subsequent entry slot remains zeroed.
        assert!(out[0x191..0x191 + 6].iter().all(|&b| b == 0));
    }

    #[test]
    fn setup_chain_rejects_empty_runs() {
        let s0 = synthetic_formatter_sector0();
        let err = build_xp_setup_chain_bootsect(&s0, 0x2000, &[]).unwrap_err();
        assert!(matches!(err, PbrError::EmptyRuns), "got {err:?}");
    }

    #[test]
    fn setup_chain_rejects_too_many_runs() {
        let s0 = synthetic_formatter_sector0();
        let many: Vec<LbaRun> = (0..(MAX_SETUP_CHAIN_RUNS + 1) as u32)
            .map(|i| LbaRun { start_lba: i * 100, sector_count: 1 })
            .collect();
        let err = build_xp_setup_chain_bootsect(&s0, 0x2000, &many).unwrap_err();
        assert!(matches!(err, PbrError::TooManyRuns { .. }), "got {err:?}");
    }

    #[test]
    fn setup_chain_rejects_too_many_sectors() {
        let s0 = synthetic_formatter_sector0();
        let runs = vec![LbaRun {
            start_lba: 0,
            sector_count: (MAX_SETUP_CHAIN_SECTORS + 1) as u16,
        }];
        let err = build_xp_setup_chain_bootsect(&s0, 0x2000, &runs).unwrap_err();
        assert!(matches!(err, PbrError::TooManySectors { .. }), "got {err:?}");
    }

    #[test]
    fn setup_chain_rejects_missing_boot_signature() {
        let mut s0 = synthetic_formatter_sector0();
        s0[510] = 0;
        let err = build_xp_setup_chain_bootsect(&s0, 0x2000, &one_run(1000, 1)).unwrap_err();
        assert!(matches!(err, PbrError::MissingBootSignature), "got {err:?}");
    }

    #[test]
    fn setup_chain_rejects_target_segment_out_of_range() {
        let s0 = synthetic_formatter_sector0();
        for bad in [0x0000u16, 0x0049, 0x9001, 0xFFFF] {
            let err = build_xp_setup_chain_bootsect(&s0, bad, &one_run(1000, 1)).unwrap_err();
            assert!(
                matches!(err, PbrError::BadTargetSegment { .. }),
                "seg {bad:#06x} → {err:?}"
            );
        }
    }

    #[test]
    fn setup_chain_accepts_max_runs_at_cap() {
        let s0 = synthetic_formatter_sector0();
        let runs: Vec<LbaRun> = (0..MAX_SETUP_CHAIN_RUNS as u32)
            .map(|i| LbaRun { start_lba: i * 100, sector_count: 1 })
            .collect();
        let out = build_xp_setup_chain_bootsect(&s0, 0x2000, &runs).unwrap();
        assert_eq!(out[0x184], MAX_SETUP_CHAIN_RUNS as u8);
    }

    fn fake_blob() -> Vec<u8> {
        let mut b = vec![0u8; 512];
        // Distinctive markers so we can assert what came from where.
        b[0] = 0xEB;
        b[1] = 0x58;
        b[2] = 0x90;
        for i in 90..510 {
            b[i] = 0xCC; // "code" filler
        }
        b
    }

    fn fake_existing() -> Vec<u8> {
        let mut e = vec![0u8; 512];
        // BPB filler so we can detect preservation.
        for i in 3..90 {
            e[i] = 0xBB;
        }
        e
    }

    #[test]
    fn splice_preserves_bpb() {
        let out = splice_fat32_pbr(&fake_existing(), &fake_blob()).unwrap();
        assert_eq!(&out[0..3], &[0xEB, 0x58, 0x90], "jump from blob");
        assert_eq!(&out[3..11], b"MSWIN4.1", "OEM ID overwritten");
        assert!(
            out[11..90].iter().all(|&b| b == 0xBB),
            "BPB body past OEM preserved from existing"
        );
        assert!(out[90..510].iter().all(|&b| b == 0xCC), "boot code from blob");
        assert_eq!(&out[510..512], &[0x55, 0xAA], "boot signature");
    }

    #[test]
    fn splice_rejects_wrong_sizes() {
        assert!(splice_fat32_pbr(&vec![0u8; 256], &fake_blob()).is_err());
        assert!(splice_fat32_pbr(&fake_existing(), &vec![0u8; 256]).is_err());
    }

    #[test]
    fn splice_errors_when_blob_missing() {
        match splice_fat32_pbr(&fake_existing(), &[]) {
            Err(PbrError::NotEmbedded) => {}
            other => panic!("expected NotEmbedded, got {other:?}"),
        }
    }

    fn fake_existing_multi() -> Vec<u8> {
        // Sector 0: BPB filler at 3..90; sector 1: distinct FSInfo-like filler
        // so we can assert it round-trips into the spliced output.
        let mut e = vec![0u8; 1024];
        for i in 3..90 {
            e[i] = 0xBB;
        }
        for i in 512..1024 {
            e[i] = 0xF5; // "FSInfo" filler
        }
        e
    }

    fn fake_multi_blob() -> Vec<u8> {
        let mut b = vec![0u8; 1024];
        b[0] = 0xEB;
        b[1] = 0x58;
        b[2] = 0x90;
        for i in 90..510 {
            b[i] = 0xCC;
        }
        // blob bytes 512..1024 are stage 2 — distinctive filler so we can
        // confirm they land at output offset 1024.. (= partition LBA 2).
        for i in 512..1024 {
            b[i] = 0xAB;
        }
        b
    }

    #[test]
    fn multi_splice_preserves_bpb_fsinfo_and_relocates_stage2() {
        let out = splice_fat32_pbr_multi(&fake_existing_multi(), &fake_multi_blob()).unwrap();
        // Output is one sector larger than blob: stage 2 shifted from
        // LBA 1 → LBA 2 to clear room for the preserved FSInfo at LBA 1.
        assert_eq!(out.len(), 1536);
        assert_eq!(&out[0..3], &[0xEB, 0x58, 0x90]);
        // OEM ID at bytes 3..11 is overwritten with "MSWIN4.1" so 2000s-era
        // BIOSes switch from USB-FDD to USB-HDD emulation. See the
        // comment in splice_fat32_pbr_multi for the empirical rationale.
        assert_eq!(&out[3..11], b"MSWIN4.1");
        // BPB after the OEM ID (offsets 11..90) is preserved from the
        // formatter — that's the filesystem state we mustn't clobber.
        assert!(out[11..90].iter().all(|&b| b == 0xBB), "BPB preserved past OEM");
        assert!(out[90..510].iter().all(|&b| b == 0xCC), "stage 1 boot code");
        assert_eq!(&out[510..512], &[0x55, 0xAA]);
        assert!(
            out[512..1024].iter().all(|&b| b == 0xF5),
            "FSInfo sector preserved from existing"
        );
        assert!(
            out[1024..1536].iter().all(|&b| b == 0xAB),
            "stage 2 lives at LBA 2"
        );
    }

    #[test]
    fn multi_splice_rejects_wrong_existing_size() {
        // 512-byte existing (sector 0 only) is no longer valid — caller
        // must read both LBA 0 and LBA 1.
        assert!(matches!(
            splice_fat32_pbr_multi(&vec![0u8; 512], &fake_multi_blob()),
            Err(PbrError::BadExistingMultiSize { got: 512 })
        ));
    }

    fn fake_ntfs_existing_multi() -> Vec<u8> {
        let mut e = vec![0u8; 1024];
        e[3..11].copy_from_slice(b"NTFS    ");
        for i in 11..84 {
            e[i] = 0xBB;
        }
        for i in 512..1024 {
            e[i] = 0xF5; // formatter's LBA 1 bytes
        }
        e
    }

    fn fake_ntfs_multi_blob() -> Vec<u8> {
        // Three sectors: stage 1 + stage 2 spans 2 sectors.
        let mut b = vec![0u8; 1536];
        b[0] = 0xEB;
        b[1] = 0x52;
        b[2] = 0x90;
        for i in 84..510 {
            b[i] = 0xCC;
        }
        for i in 512..1536 {
            b[i] = 0xAB;
        }
        b
    }

    #[test]
    fn ntfs_multi_splice_preserves_bpb_lba1_and_relocates_stage2() {
        let out =
            splice_ntfs_pbr_multi(&fake_ntfs_existing_multi(), &fake_ntfs_multi_blob()).unwrap();
        // Output is one sector larger than blob (1536 + 512 = 2048).
        assert_eq!(out.len(), 2048);
        assert_eq!(&out[0..3], &[0xEB, 0x52, 0x90], "jump from blob");
        assert_eq!(&out[3..11], b"NTFS    ", "OEM preserved");
        assert!(out[11..84].iter().all(|&b| b == 0xBB), "BPB preserved");
        assert!(out[84..510].iter().all(|&b| b == 0xCC), "stage 1 boot code");
        assert_eq!(&out[510..512], &[0x55, 0xAA]);
        assert!(
            out[512..1024].iter().all(|&b| b == 0xF5),
            "LBA 1 preserved from existing"
        );
        assert!(
            out[1024..2048].iter().all(|&b| b == 0xAB),
            "stage 2 (2 sectors) lives at LBA 2..3"
        );
    }

    #[test]
    fn ntfs_multi_splice_rejects_bad_sizes() {
        // 512-byte existing rejected — caller must supply LBA 0 + LBA 1.
        assert!(matches!(
            splice_ntfs_pbr_multi(&vec![0u8; 512], &fake_ntfs_multi_blob()),
            Err(PbrError::BadExistingMultiSize { got: 512 })
        ));
        // Bad blob size.
        assert!(matches!(
            splice_ntfs_pbr_multi(&fake_ntfs_existing_multi(), &vec![0u8; 512]),
            Err(PbrError::BadMultiBlobSize { got: 512 })
        ));
    }

    #[test]
    fn multi_splice_rejects_bad_blob_sizes() {
        assert!(matches!(
            splice_fat32_pbr_multi(&fake_existing_multi(), &vec![0u8; 512]),
            Err(PbrError::BadMultiBlobSize { got: 512 })
        ));
        assert!(matches!(
            splice_fat32_pbr_multi(&fake_existing_multi(), &vec![0u8; 1500]),
            Err(PbrError::BadMultiBlobSize { got: 1500 })
        ));
    }
}
