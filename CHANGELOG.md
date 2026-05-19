# Changelog

All notable changes to mkmsbr are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); this project
adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] — 2026-05-19

### Fixed

- **`cargo install mkmsbr` no longer requires `nasm` on the user's
  host.** The published crate now ships pre-assembled boot-code blobs
  in `blobs-prebuilt/`; `build.rs` uses nasm if available (developer
  path) and falls back to the prebuilt bytes otherwise (install path).
  Prior 1.0.0 hard-failed during `cargo install` on any host without
  nasm. The bytes shipped match the `boot-asm/` source at the same tag.

## [1.0.0] — 2026-05-19

First published release. Clean-room MIT replacement for `ms-sys` for
the boot-record variants needed by Windows 7+ and Windows XP install
USB pipelines.

### Variants shipped

Four of five v1.0 variants ship at their spec-defined eval target (see
[docs/SPEC.md](docs/SPEC.md) §Component breakdown); the fifth
(`ntfs_pbr_bootmgr`) ships L2-green and awaits a real-NTFS L3 fixture.

- `mbr_xp` — Windows 2000/XP/2003 MBR. L1 + L2 + ships in production
  via [usbwin](https://github.com/jma24/usbwin) XP mode.
- `mbr_win7` — Windows 7/8/10/11 MBR. L1 + L2 + Win 7 install USBs
  boot end-to-end on the Dell E6410 reference rig.
- `fat32_pbr_ntldr` (multi-sector) — FAT32 PBR loading NTLDR. L1 + L2
  + L3 (987 real-NTLDR reads under QEMU) + L4 (NTLDR loads on Dell
  E6410 real hardware).
- `fat32_pbr_bootmgr` (multi-sector) — FAT32 PBR loading bootmgr.
  L2 + L3 (1520 real-bootmgr reads under QEMU) + L4 (Win 7 install
  USB boots end-to-end on legacy-BIOS hardware).
- `ntfs_pbr_bootmgr` (multi-sector) — NTFS PBR loading bootmgr. L2
  green against an ntfs-3g-formatted volume. L1 ms-sys byte-distance
  and L3 against a real Win 7 NTFS install are pending.

### Added

- **Public library API** (`src/lib.rs`):
  - `mbr_xp(disk_sectors)` / `mbr_win7(disk_sectors)` — whole-disk MBR
    builders for single-FAT32-active layouts.
  - `splice_mbr(existing, boot)` — ms-sys-compatible MBR boot-code
    replacement that preserves the partition table + disk signature
    at bytes 440..510.
  - `splice_fat32_pbr` / `splice_fat32_pbr_multi` — FAT32 PBR splices
    that preserve the formatter-written BPB.
  - `splice_ntfs_pbr_multi` — NTFS PBR splice with USA-fixup-aware
    MFT walker.
  - `build_xp_setup_chain_bootsect(formatter_sector0, target_segment,
    runs)` — XP-Setup BOOTSECT.DAT chain loader for the
    NTLDR → BOOTSECT.DAT → `$LDR$` path.
  - Pre-assembled boot-code blobs (`MBR_XP_BOOT`, `MBR_WIN7_BOOT`,
    `FAT32_PBR_NTLDR_MULTI_BOOT`, `FAT32_PBR_BOOTMGR_BOOT`,
    `FAT32_PBR_BOOTMGR_MULTI_BOOT`, `NTFS_PBR_BOOTMGR_MULTI_BOOT`,
    `XP_SETUP_CHAIN_BOOTSECT_BOOT`).

- **CLI binary** (`mkmsbr`): drop-in replacement for `ms-sys` for the
  five shipped variants. Accepts both mkmsbr-style flags
  (`--mbr-win7`, `--fat32-bootmgr`) and ms-sys aliases (`--mbr7`,
  `--fat32pe`).

- **Eval framework** (4-layer hierarchy):
  - Layer 1 — byte-distance vs ms-sys subprocess oracle
    (`tests/layer1_oracle.rs`).
  - Layer 1 — byte-diff gap detection
    (`tests/byte_diff_vs_mssys.rs`).
  - Layer 2 — synthetic QEMU boot smoke for MBR + FAT32 + NTFS
    (`tests/qemu_mbr.rs`, `tests/qemu_pbr.rs`, `tests/qemu_ntfs_pbr.rs`).
  - Layer 3 — real NTLDR / bootmgr chain-load under QEMU with
    `blk_co_preadv` trace gating (`tests/qemu_pbr_real.rs`).

- **Hardware compatibility:** CHS reads (INT 13h fn 0x02) with fn 0x08
  geometry probe + USB-FDD fallback for legacy BIOSes that reject
  LBA-ext; OEM ID = `"MSWIN4.1"` in both FAT32 splices; MBR
  instruction sequence shaped to fingerprint as Microsoft-style for
  BIOS USB-HDD mode detection.

- **Clean-room infrastructure**: [docs/PROVENANCE.md](docs/PROVENANCE.md)
  protocol, `scripts/clean_room_check.sh` forbidden-symbol grep,
  statistical similarity check in the L1 oracle.

### Known limitations

- `ntfs_pbr_bootmgr` has no L1 ms-sys byte-distance comparison and
  no L3 real-content validation. L2 green against ntfs-3g fixtures;
  any consumer should be aware that real Win 7 NTFS install bytes
  are not yet exercised.
- MBR disk signature at offset 0x1B8 is currently a fixed
  `0xDEADBEEF` test value; `mbr_win7_with_signature(disk, sig: u32)`
  is filed as the v1.1 API addition that lets callers thread a real
  per-USB signature through.
- Multi-sector splice output type is `Vec<u8>`; the `PbrBytes`
  newtype from [docs/SPEC.md](docs/SPEC.md) §Library scope is filed
  as cosmetic polish.

[1.0.1]: https://github.com/jma24/mkmsbr/releases/tag/v1.0.1
[1.0.0]: https://github.com/jma24/mkmsbr/releases/tag/v1.0.0
