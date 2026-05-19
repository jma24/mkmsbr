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
}

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
