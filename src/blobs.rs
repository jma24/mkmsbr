//! The embedded boot-record blobs. Assembled at build time from `boot-asm/`
//! and `include_bytes!`'d here. Without the `embed-boot-asm` feature these
//! are empty slices — any code path that needs them surfaces a clear
//! error at runtime ("bootrec was built without boot blobs; rebuild with
//! --features embed-boot-asm").

/// Windows 2000/XP/2003 MBR boot code. 512 bytes. Loaded by BIOS at
/// 0000:7C00; finds the active primary partition and chain-loads its PBR.
pub const MBR_XP_BOOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mbr_xp.bin"));

/// Windows 7/8/10/11 MBR boot code. 512 bytes. Same shape as
/// [`MBR_XP_BOOT`] plus a GPT-protective-MBR refusal: if the active
/// partition has type 0xEE the MBR halts rather than chain-load.
pub const MBR_WIN7_BOOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mbr_win7.bin"));

/// FAT32 PBR boot code for the BOOTMGR-loading variant (Win 7/8/10/11
/// install media). 512 bytes, single-sector. Caller splices this through
/// [`crate::splice_fat32_pbr`] so the partition's actual BPB is preserved.
pub const FAT32_PBR_BOOTMGR_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fat32_pbr_bootmgr.bin"));

/// FAT32 PBR boot code for the NTLDR-loading variant (Win 2000/XP/2003).
/// 512 bytes, single-sector.
pub const FAT32_PBR_NTLDR_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fat32_pbr_ntldr.bin"));

pub const NTFS_PBR_BOOT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/ntfs_pbr.bin"));

/// Returns `true` if the boot blobs were embedded at build time.
pub fn embedded() -> bool {
    !MBR_XP_BOOT.is_empty()
}
