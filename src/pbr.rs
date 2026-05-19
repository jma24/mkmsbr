//! The PBR splice. The "preserve the BPB" rule from docs/BOOT_RECORDS.md
//! lives here as code, once per filesystem type. FAT32 preserves bytes
//! 3..89 (OEM + BPB + extended BPB); NTFS preserves 3..84 (OEM + BPB +
//! extended BPB; layout per Microsoft NTFS public docs).

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PbrError {
    #[error("existing PBR is {got} bytes; expected exactly 512")]
    BadExistingSize { got: usize },

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
///   bytes   3..89  = existing[3..89]  (BPB - preserved)
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
    out[90..510].copy_from_slice(&boot[90..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    Ok(out)
}

/// Multi-sector variant of [`splice_fat32_pbr`]. Given:
///   - `existing`: the freshly-formatted sector 0 (512 bytes, BPB at 3..90)
///   - `blob`: the multi-sector boot code (N × 512 bytes, N ≥ 2). The
///     first 512 bytes are stage 1 (with a BPB placeholder); bytes 512..
///     are continuation sectors (raw, no BPB).
///
/// Returns a `Vec<u8>` of length `blob.len()`:
///   bytes      0..3    = blob[0..3]       (jump from stage 1)
///   bytes      3..90   = existing[3..90]  (BPB - preserved)
///   bytes     90..510  = blob[90..510]    (stage 1 boot code)
///   bytes    510..512  = [0x55, 0xAA]     (signature)
///   bytes    512..end  = blob[512..]      (sectors 1+ verbatim)
///
/// The caller writes the result starting at sector 0 of the partition;
/// sectors 1+ land at LBAs partition_start + 1, +2, etc.
pub fn splice_fat32_pbr_multi(existing: &[u8], blob: &[u8]) -> Result<Vec<u8>, PbrError> {
    if existing.len() != 512 {
        return Err(PbrError::BadExistingSize { got: existing.len() });
    }
    if blob.is_empty() {
        return Err(PbrError::NotEmbedded);
    }
    if blob.len() < 1024 || blob.len() % 512 != 0 {
        return Err(PbrError::BadMultiBlobSize { got: blob.len() });
    }

    let mut out = vec![0u8; blob.len()];
    // Sector 0 — same splice as single-sector.
    out[0..3].copy_from_slice(&blob[0..3]);
    out[3..90].copy_from_slice(&existing[3..90]);
    out[90..510].copy_from_slice(&blob[90..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    // Sectors 1+ — verbatim.
    out[512..].copy_from_slice(&blob[512..]);
    Ok(out)
}

/// Multi-sector NTFS PBR splice. Same shape as
/// [`splice_fat32_pbr_multi`] but the preserved BPB range is bytes
/// 3..84 (OEM ID + NTFS BPB + extended BPB; layout per Microsoft NTFS
/// On-Disk Format public docs). Boot signature lives only in sector 0;
/// sectors 1+ are copied verbatim.
///
///   bytes      0..3    = blob[0..3]       (jump)
///   bytes      3..84   = existing[3..84]  (OEM + BPB; preserved)
///   bytes     84..510  = blob[84..510]    (stage 1 boot code)
///   bytes    510..512  = [0x55, 0xAA]     (boot signature)
///   bytes    512..end  = blob[512..]      (stage 2+; verbatim)
pub fn splice_ntfs_pbr_multi(existing: &[u8], blob: &[u8]) -> Result<Vec<u8>, PbrError> {
    if existing.len() != 512 {
        return Err(PbrError::BadExistingSize { got: existing.len() });
    }
    if blob.is_empty() {
        return Err(PbrError::NotEmbedded);
    }
    if blob.len() < 1024 || blob.len() % 512 != 0 {
        return Err(PbrError::BadMultiBlobSize { got: blob.len() });
    }

    let mut out = vec![0u8; blob.len()];
    out[0..3].copy_from_slice(&blob[0..3]);
    out[3..84].copy_from_slice(&existing[3..84]);
    out[84..510].copy_from_slice(&blob[84..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    out[512..].copy_from_slice(&blob[512..]);
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
        assert!(out[3..90].iter().all(|&b| b == 0xBB), "BPB from existing");
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

    fn fake_multi_blob() -> Vec<u8> {
        let mut b = vec![0u8; 1024];
        b[0] = 0xEB;
        b[1] = 0x58;
        b[2] = 0x90;
        for i in 90..510 {
            b[i] = 0xCC;
        }
        // sector 1: distinctive filler so we can assert it carried through.
        for i in 512..1024 {
            b[i] = 0xAB;
        }
        b
    }

    #[test]
    fn multi_splice_preserves_bpb_and_carries_continuation_sectors() {
        let out = splice_fat32_pbr_multi(&fake_existing(), &fake_multi_blob()).unwrap();
        assert_eq!(out.len(), 1024);
        assert_eq!(&out[0..3], &[0xEB, 0x58, 0x90]);
        assert!(out[3..90].iter().all(|&b| b == 0xBB), "BPB preserved");
        assert!(out[90..510].iter().all(|&b| b == 0xCC), "stage 1 boot code");
        assert_eq!(&out[510..512], &[0x55, 0xAA]);
        assert!(out[512..1024].iter().all(|&b| b == 0xAB), "stage 2 carried through");
    }

    fn fake_ntfs_existing() -> Vec<u8> {
        let mut e = vec![0u8; 512];
        e[3..11].copy_from_slice(b"NTFS    ");
        for i in 11..84 {
            e[i] = 0xBB;
        }
        e
    }

    fn fake_ntfs_multi_blob() -> Vec<u8> {
        let mut b = vec![0u8; 1024];
        b[0] = 0xEB;
        b[1] = 0x52;
        b[2] = 0x90;
        for i in 84..510 {
            b[i] = 0xCC;
        }
        for i in 512..1024 {
            b[i] = 0xAB;
        }
        b
    }

    #[test]
    fn ntfs_multi_splice_preserves_bpb_and_carries_stage2() {
        let out = splice_ntfs_pbr_multi(&fake_ntfs_existing(), &fake_ntfs_multi_blob()).unwrap();
        assert_eq!(out.len(), 1024);
        assert_eq!(&out[0..3], &[0xEB, 0x52, 0x90], "jump from blob");
        assert_eq!(&out[3..11], b"NTFS    ", "OEM preserved");
        assert!(out[11..84].iter().all(|&b| b == 0xBB), "BPB preserved");
        assert!(out[84..510].iter().all(|&b| b == 0xCC), "stage 1 boot code");
        assert_eq!(&out[510..512], &[0x55, 0xAA]);
        assert!(out[512..1024].iter().all(|&b| b == 0xAB), "stage 2 carried through");
    }

    #[test]
    fn ntfs_multi_splice_rejects_bad_sizes() {
        assert!(matches!(
            splice_ntfs_pbr_multi(&fake_ntfs_existing(), &vec![0u8; 512]),
            Err(PbrError::BadMultiBlobSize { got: 512 })
        ));
    }

    #[test]
    fn multi_splice_rejects_bad_sizes() {
        assert!(matches!(
            splice_fat32_pbr_multi(&fake_existing(), &vec![0u8; 512]),
            Err(PbrError::BadMultiBlobSize { got: 512 })
        ));
        assert!(matches!(
            splice_fat32_pbr_multi(&fake_existing(), &vec![0u8; 1500]),
            Err(PbrError::BadMultiBlobSize { got: 1500 })
        ));
    }
}
