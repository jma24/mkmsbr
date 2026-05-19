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
///
/// **Limitation:** the single-sector variant boots fake bootmgrs in QEMU
/// fine but does not satisfy real Microsoft BOOTMGR's multi-sector boot
/// environment contract on real hardware. For production install media
/// use [`FAT32_PBR_BOOTMGR_MULTI_BOOT`].
pub const FAT32_PBR_BOOTMGR_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fat32_pbr_bootmgr.bin"));

/// FAT32 PBR boot code, multi-sector BOOTMGR variant. 1024 bytes
/// (sector 0 stage-1 + sector 1 stage-2). Caller splices through
/// [`crate::splice_fat32_pbr_multi`]. This is the v1.0 production
/// variant; see `boot-asm/fat32_pbr_bootmgr/` for the per-sector NASM.
pub const FAT32_PBR_BOOTMGR_MULTI_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fat32_pbr_bootmgr_multi.bin"));

/// FAT32 PBR boot code, multi-sector NTLDR variant (Win 2000/XP/2003).
/// 1024 bytes (sector 0 stage-1 + sector 1 stage-2). Caller splices
/// through [`crate::splice_fat32_pbr_multi`].
///
/// Multi-sector layout is mandatory: legacy BIOSes that emulate USB
/// sticks as USB-FDD reject INT 13h fn 0x42 with AH=01, so stage 1
/// uses CHS reads (fn 0x02) for the stage-2 load and stage 2 uses CHS
/// for every FAT walk read. Single-sector FAT-walk + CHS reads + name
/// search doesn't fit in 512 bytes. See `boot-asm/fat32_pbr_ntldr/`
/// for the per-sector NASM.
pub const FAT32_PBR_NTLDR_MULTI_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/fat32_pbr_ntldr_multi.bin"));

/// NTFS PBR boot code, multi-sector BOOTMGR variant. 1024 bytes
/// (sector 0 stage 1 + sector 1 stage 2). Caller splices through
/// [`crate::splice_ntfs_pbr_multi`]. Single-sector NTFS PBRs do not
/// fit the MFT walker + data-run parser in 426 bytes, so NTFS is
/// multi-sector from day one (see `boot-asm/ntfs_pbr_bootmgr/`).
pub const NTFS_PBR_BOOTMGR_MULTI_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/ntfs_pbr_bootmgr_multi.bin"));

/// Single-sector loader template for XP Setup's BOOTSECT.DAT slot. Built
/// at runtime by [`crate::build_xp_setup_chain_bootsect`]: the patcher
/// splices the partition's BPB into bytes 3..90 and fills the run-table
/// + target-segment placeholders at offsets 0x180..0x1E5. Pre-patch the
/// blob is non-functional (run_count = 0); always go through the
/// builder.
pub const XP_SETUP_CHAIN_BOOTSECT_BOOT: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/xp_setup_chain_bootsect.bin"));

/// Returns `true` if the boot blobs were embedded at build time.
pub fn embedded() -> bool {
    !MBR_XP_BOOT.is_empty()
}
