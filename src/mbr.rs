//! MBR partition table construction. Pure byte manipulation; no I/O.
//!
//! The MBR is the first 512 bytes of a partitioned disk. Layout:
//!
//! ```text
//!   offset 0x000..0x1BD   boot code (440 bytes, our mbr.asm output)
//!   offset 0x1BE..0x1FD   partition table (4 × 16-byte entries)
//!   offset 0x1FE..0x1FF   boot signature (0x55 0xAA)
//! ```
//!
//! `build_mbr` assembles a 512-byte MBR for a single-FAT32-active layout
//! (the v0.2 Windows-mode shape). It splices our boot code, writes one
//! primary partition entry covering most of the disk starting at LBA 2048
//! (1 MiB alignment, the modern convention), zeros the other three slots,
//! and adds the signature.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum MbrError {
    #[error("MBR boot code is {got} bytes; expected at least 440")]
    BootCodeTooShort { got: usize },

    #[error("disk too small for partition: {disk_sectors} sectors, need > {partition_start}")]
    DiskTooSmall { disk_sectors: u64, partition_start: u64 },

    #[error("existing MBR sector is {got} bytes; expected 512")]
    ExistingTooShort { got: usize },

    #[error("MBR boot blobs were not embedded; rebuild with --features embed-boot-asm")]
    NotEmbedded,
}

/// Standard MBR partition types. We only emit `Fat32Lba` from `build_mbr`;
/// the enum exists so callers can express intent and so tests are readable.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum PartitionType {
    /// FAT32 with LBA addressing. The right type for any FAT32 partition
    /// larger than 8 GiB or on any disk that BIOSes can't represent in CHS.
    /// Windows install USBs use this.
    Fat32Lba = 0x0C,
    /// FAT32 with CHS addressing. Don't use for new partitions.
    #[allow(dead_code)]
    Fat32Chs = 0x0B,
    /// NTFS / exFAT.
    #[allow(dead_code)]
    Ntfs = 0x07,
}

/// A single 16-byte MBR partition table entry.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct PartitionEntry {
    pub bootable: bool,
    pub partition_type: PartitionType,
    pub lba_start: u32,
    pub sector_count: u32,
}

impl PartitionEntry {
    /// Encode the 16-byte on-disk representation.
    ///
    /// CHS fields are written as 0xFE 0xFF 0xFF (the "out of CHS range,
    /// please use LBA" convention). Modern BIOSes ignore CHS when LBA is
    /// present. Some very old BIOSes refuse partitions with bogus CHS, but
    /// nothing we'd target for a Windows 7 install USB falls in that
    /// category - Win 7 itself dates to 2009 and assumes LBA.
    pub fn encode(&self) -> [u8; 16] {
        let mut bytes = [0u8; 16];
        bytes[0] = if self.bootable { 0x80 } else { 0x00 };
        bytes[1] = 0xFE; // CHS first head
        bytes[2] = 0xFF; // CHS first sector + bits 8..9 of cylinder
        bytes[3] = 0xFF; // CHS first cylinder bits 0..7
        bytes[4] = self.partition_type as u8;
        bytes[5] = 0xFE; // CHS last head
        bytes[6] = 0xFF; // CHS last sector
        bytes[7] = 0xFF; // CHS last cylinder
        bytes[8..12].copy_from_slice(&self.lba_start.to_le_bytes());
        bytes[12..16].copy_from_slice(&self.sector_count.to_le_bytes());
        bytes
    }
}

/// Standard 1 MiB partition alignment: 2048 sectors of 512 bytes each.
/// Every modern partitioning tool defaults to this; it matches SSD/flash
/// erase-block boundaries and avoids alignment pitfalls.
pub const PARTITION_START_LBA: u32 = 2048;

/// Variant entry point: Windows 2000/XP/2003 MBR with a single active
/// FAT32 partition. Suitable for booting NTLDR-chain Windows install
/// media (the pre-Vista boot path).
///
/// Wraps the embedded XP-variant boot code (`MBR_XP_BOOT`) and the
/// `build_mbr` layout primitive.
///
/// The v1.0 API target (per docs/SPEC.md) is
/// `mbr_xp(disk: DiskGeometry, partitions: &[PartitionEntry]) -> [u8; 512]`
/// — multi-partition support is future work; today this hardcodes the
/// single-FAT32-active-at-LBA-2048 shape that's what 99% of install-USB
/// recipes need.
pub fn mbr_xp(disk_sectors: u64) -> Result<[u8; 512], MbrError> {
    build_mbr(crate::MBR_XP_BOOT, disk_sectors)
}

/// Variant entry point: Windows 7/8/10/11 MBR with a single active
/// FAT32 partition. Suitable for booting BOOTMGR-chain install media.
///
/// Adds a GPT-protective-MBR refusal vs [`mbr_xp`]: an active partition
/// of type 0xEE causes the boot code to halt with an error rather than
/// blindly chain-load. Legacy BIOS booting a GPT disk is almost always
/// a misconfiguration; the user wants UEFI.
pub fn mbr_win7(disk_sectors: u64) -> Result<[u8; 512], MbrError> {
    build_mbr(crate::MBR_WIN7_BOOT, disk_sectors)
}

/// Construct the MBR for a single-FAT32-active layout. The boot code goes
/// into bytes 0..440; bytes 446..510 hold one active FAT32-LBA primary
/// partition starting at LBA 2048, slots 2-4 zeroed, signature 0x55AA at
/// the end.
///
/// Arguments:
/// - `boot_code`: an MBR boot blob (at least 440 bytes; we use the first 440).
/// - `disk_sectors`: total addressable sectors on the device.
///
/// Most callers want [`mbr_xp`] (and forthcoming `mbr_win7`) which fix the
/// boot-code variant. `build_mbr` stays public so callers can experiment
/// with custom boot code.
pub fn build_mbr(boot_code: &[u8], disk_sectors: u64) -> Result<[u8; 512], MbrError> {
    if boot_code.is_empty() {
        return Err(MbrError::NotEmbedded);
    }
    if boot_code.len() < 440 {
        return Err(MbrError::BootCodeTooShort {
            got: boot_code.len(),
        });
    }
    let partition_start = PARTITION_START_LBA as u64;
    if disk_sectors <= partition_start {
        return Err(MbrError::DiskTooSmall {
            disk_sectors,
            partition_start,
        });
    }

    let sector_count_u64 = disk_sectors - partition_start;
    let sector_count: u32 = sector_count_u64.try_into().unwrap_or(u32::MAX);

    let mut mbr = [0u8; 512];
    mbr[0..440].copy_from_slice(&boot_code[..440]);

    // NT disk signature at offset 0x1B8 (4 bytes). Windows boot manager
    // and BCD reference the boot disk by this signature; it must be
    // non-zero for Windows handoff to find its own boot drive. Some
    // 2000s-era BIOSes also key USB-FDD vs USB-HDD emulation on whether
    // this is non-zero. ms-sys writes a per-disk random value here; we
    // currently use a fixed test value (0xDEADBEEF) to probe whether the
    // BIOS cares about non-zero signature presence at all. If the L4
    // boot succeeds with this, the next step is `mbr_win7_with_signature
    // (disk, sig: u32)` so usbwin can generate a real per-USB sig.
    // TODO(v1.0): replace fixed value with caller-supplied parameter.
    mbr[0x1B8..0x1BC].copy_from_slice(&0xDEADBEEFu32.to_le_bytes());

    let active = PartitionEntry {
        bootable: true,
        partition_type: PartitionType::Fat32Lba,
        lba_start: PARTITION_START_LBA,
        sector_count,
    };
    mbr[0x1BE..0x1CE].copy_from_slice(&active.encode());
    // Slots 2, 3, 4 left as zeros (unused).

    mbr[0x1FE] = 0x55;
    mbr[0x1FF] = 0xAA;
    Ok(mbr)
}

/// Replace boot code in an existing MBR while preserving the partition
/// table, disk signature, and 0x55AA marker. Bytes 0..440 are overwritten
/// with `boot[..440]`; bytes 440..510 (NT disk signature + optional
/// copy-protect + four partition entries) are carried through from
/// `existing`; bytes 510..512 are forced to 0x55 0xAA.
///
/// This is the ms-sys-compatible "drop a boot record onto an already-
/// partitioned disk" operation. Use this when the device already has the
/// partition layout the user wants; use [`build_mbr`] / [`mbr_xp`] /
/// [`mbr_win7`] when constructing a fresh disk from scratch.
pub fn splice_mbr(existing: &[u8], boot: &[u8]) -> Result<[u8; 512], MbrError> {
    if boot.is_empty() {
        return Err(MbrError::NotEmbedded);
    }
    if boot.len() < 440 {
        return Err(MbrError::BootCodeTooShort { got: boot.len() });
    }
    if existing.len() < 512 {
        return Err(MbrError::ExistingTooShort { got: existing.len() });
    }
    let mut out = [0u8; 512];
    out[..440].copy_from_slice(&boot[..440]);
    out[440..510].copy_from_slice(&existing[440..510]);
    out[510] = 0x55;
    out[511] = 0xAA;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_boot() -> Vec<u8> {
        // 440 bytes of distinctive filler so we can assert the boot code
        // was copied through and not overwritten by partition table or
        // signature.
        vec![0xCC; 440]
    }

    #[test]
    fn entry_encodes_active_fat32_with_lba_only() {
        let e = PartitionEntry {
            bootable: true,
            partition_type: PartitionType::Fat32Lba,
            lba_start: 2048,
            sector_count: 131072,
        };
        let b = e.encode();
        assert_eq!(b[0], 0x80, "bootable flag");
        assert_eq!(b[1..4], [0xFE, 0xFF, 0xFF], "CHS first = LBA-marker");
        assert_eq!(b[4], 0x0C, "FAT32 LBA type");
        assert_eq!(b[5..8], [0xFE, 0xFF, 0xFF], "CHS last = LBA-marker");
        assert_eq!(&b[8..12], &2048u32.to_le_bytes());
        assert_eq!(&b[12..16], &131072u32.to_le_bytes());
    }

    #[test]
    fn entry_inactive_clears_boot_flag() {
        let e = PartitionEntry {
            bootable: false,
            partition_type: PartitionType::Fat32Lba,
            lba_start: 0,
            sector_count: 0,
        };
        assert_eq!(e.encode()[0], 0x00);
    }

    #[test]
    fn mbr_has_signature_and_active_partition() {
        let mbr = build_mbr(&fake_boot(), 131072).unwrap();
        assert_eq!(mbr.len(), 512);
        assert_eq!(&mbr[0x1FE..], &[0x55, 0xAA], "boot signature");
        assert_eq!(mbr[0x1BE], 0x80, "partition 1 active");
        assert_eq!(mbr[0x1BE + 4], 0x0C, "FAT32 LBA");
        // LBA start at offset 0x1BE + 8
        let lba = u32::from_le_bytes(mbr[0x1C6..0x1CA].try_into().unwrap());
        assert_eq!(lba, 2048);
        // Sector count fills the rest of the disk
        let count = u32::from_le_bytes(mbr[0x1CA..0x1CE].try_into().unwrap());
        assert_eq!(count, 131072 - 2048);
    }

    #[test]
    fn mbr_preserves_boot_code() {
        let mbr = build_mbr(&fake_boot(), 131072).unwrap();
        assert!(mbr[0..440].iter().all(|&b| b == 0xCC), "boot code from input");
    }

    #[test]
    fn mbr_zeros_unused_partition_slots() {
        let mbr = build_mbr(&fake_boot(), 131072).unwrap();
        for slot in 1..4 {
            let offset = 0x1BE + 16 * slot;
            for i in 0..16 {
                assert_eq!(mbr[offset + i], 0, "slot {slot} byte {i} should be 0");
            }
        }
    }

    #[test]
    fn mbr_rejects_short_boot_code() {
        let short = vec![0u8; 100];
        assert!(matches!(
            build_mbr(&short, 131072),
            Err(MbrError::BootCodeTooShort { got: 100 })
        ));
    }

    #[test]
    fn mbr_rejects_empty_boot_code() {
        assert!(matches!(build_mbr(&[], 131072), Err(MbrError::NotEmbedded)));
    }

    #[test]
    fn mbr_rejects_disk_too_small() {
        let err = build_mbr(&fake_boot(), 1024).unwrap_err();
        assert!(matches!(err, MbrError::DiskTooSmall { .. }));
    }

    #[test]
    fn splice_mbr_replaces_boot_code_only() {
        let mut existing = [0u8; 512];
        // Distinctive partition table + disk signature in bytes 440..510.
        for (i, b) in existing[440..510].iter_mut().enumerate() {
            *b = (i as u8).wrapping_add(0x40);
        }
        existing[510] = 0x55;
        existing[511] = 0xAA;

        let boot: Vec<u8> = (0..440).map(|i| (i & 0xFF) as u8).collect();
        let out = splice_mbr(&existing, &boot).unwrap();

        assert_eq!(&out[..440], &boot[..]);
        assert_eq!(&out[440..510], &existing[440..510]);
        assert_eq!(out[510], 0x55);
        assert_eq!(out[511], 0xAA);
    }

    #[test]
    fn splice_mbr_forces_signature_even_if_missing() {
        let mut existing = [0u8; 512];
        for b in existing[440..510].iter_mut() {
            *b = 0x42;
        }
        // existing[510..512] left as zeros
        let out = splice_mbr(&existing, &fake_boot()).unwrap();
        assert_eq!(out[510], 0x55);
        assert_eq!(out[511], 0xAA);
    }

    #[test]
    fn splice_mbr_rejects_short_existing() {
        assert!(matches!(
            splice_mbr(&[0u8; 100], &fake_boot()),
            Err(MbrError::ExistingTooShort { got: 100 })
        ));
    }

    #[test]
    fn splice_mbr_rejects_short_boot() {
        let existing = [0u8; 512];
        assert!(matches!(
            splice_mbr(&existing, &[0u8; 100]),
            Err(MbrError::BootCodeTooShort { got: 100 })
        ));
    }

    #[test]
    fn splice_mbr_rejects_empty_boot() {
        let existing = [0u8; 512];
        assert!(matches!(splice_mbr(&existing, &[]), Err(MbrError::NotEmbedded)));
    }

    #[test]
    fn mbr_clamps_huge_disks_to_u32_max() {
        // 5 TB disk: 9_765_625_000 sectors. Single FAT32 partition can't
        // address that much; we clamp to u32::MAX and let the user partition
        // smarter or use a different filesystem.
        let mbr = build_mbr(&fake_boot(), 9_765_625_000).unwrap();
        let count = u32::from_le_bytes(mbr[0x1CA..0x1CE].try_into().unwrap());
        assert_eq!(count, u32::MAX);
    }
}
