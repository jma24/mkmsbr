//! Boot record assembly. Pure byte manipulation; no I/O.
//!
//! The single most important function in this crate is `splice_fat32_pbr`:
//! it takes the existing PBR (which a freshly-formatted partition holds,
//! e.g. what `newfs_msdos` writes) and splices in our boot code while
//! preserving bytes 3..89 (the BPB — the filesystem-geometry block that
//! describes this specific volume). Replacing the BPB with a template
//! breaks boot; preserving it is the whole point of this crate.

pub mod blobs;
pub mod mbr;
pub mod pbr;

pub use blobs::{
    FAT32_PBR_BOOTMGR_BOOT, FAT32_PBR_BOOTMGR_MULTI_BOOT, FAT32_PBR_NTLDR_BOOT, MBR_WIN7_BOOT,
    MBR_XP_BOOT, NTFS_PBR_BOOT,
};
pub use mbr::{
    build_mbr, mbr_win7, mbr_xp, MbrError, PartitionEntry, PartitionType, PARTITION_START_LBA,
};
pub use pbr::{splice_fat32_pbr, splice_fat32_pbr_multi};
