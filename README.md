# mkmsbr

Clean-room Rust library for producing Microsoft-compatible boot
records — MBR, FAT32 PBR, NTFS PBR — without depending on `ms-sys`. MIT
licensed.

**Status:** v1.0 ready. 4 of 5 boot-record variants ship at their
spec-defined eval target (see [docs/SPEC.md](docs/SPEC.md) §Component
breakdown); the 5th (`ntfs_pbr_bootmgr`) is L2-green and waiting on a
real NTFS fixture for its L3 run. mkmsbr is the default boot-record
backend in [usbwin](https://github.com/jma24/usbwin) v1.0, where Win 7
install USBs built with mkmsbr's MBR + FAT32 PBR boot end-to-end on real
legacy-BIOS hardware (Dell E6410, verified 2026-05-19). See
[docs/PROVENANCE.md](docs/PROVENANCE.md) for the clean-room protocol and
[docs/BACKLOG.md](docs/BACKLOG.md) for the full variant matrix.

## Why

`ms-sys` is the only widely-available source of correct Windows boot-record
bytes outside of Windows itself, but:

- It's GPL-2; mkmsbr is MIT, so consumers get a cleaner license story.
- It's distributed as source, with build-it-yourself friction.
- Its boot-code blobs in `inc/*.h` are themselves derived from Microsoft
  binaries — a long-running legal grey area that a from-the-spec
  reimplementation sidesteps.

mkmsbr is built **eval-first**: the verification harness (ms-sys-as-oracle
+ QEMU boot smoke + real-content boot + real hardware) is written before
the boot code. A variant ships when its eval passes. See
[docs/SPEC.md](docs/SPEC.md) §Verifiability hierarchy.

## Install

```sh
# From crates.io (CLI binary + library):
cargo install mkmsbr

# As a library dependency:
cargo add mkmsbr
```

Building from source needs [NASM](https://nasm.us/) for the boot-code
assembly (`brew install nasm`); `cargo install` invokes it via the build
script.

## Usage

### Command line

The `mkmsbr` binary is a drop-in for `ms-sys` for the five variants
mkmsbr supports. Flags accept both the mkmsbr-style long names and the
ms-sys aliases:

```sh
# Win 7+ install USB: write boot records to a freshly-partitioned disk.
sudo mkmsbr --mbr-win7      /dev/rdisk6      # alias: --mbr7
sudo mkmsbr --fat32-bootmgr /dev/rdisk6s1    # alias: --fat32pe

# XP install USB:
sudo mkmsbr --mbr-xp        /dev/rdisk6      # alias: --mbr
sudo mkmsbr --fat32-ntldr   /dev/rdisk6s1    # alias: --fat32nt

# NTFS PBR (experimental — L3 against real bootmgr not yet validated):
sudo mkmsbr --ntfs-bootmgr  /dev/rdisk6s1    # alias: --ntfs
```

Each invocation reads the existing first sectors of the target, splices
in mkmsbr's clean-room boot code while preserving the formatter's BPB
(for PBR variants) or partition table (for MBR variants), and writes the
result back. `mkmsbr --help` for the full flag list.

### Rust library

```rust
use mkmsbr::{splice_mbr, splice_fat32_pbr_multi, FAT32_PBR_BOOTMGR_MULTI_BOOT, MBR_WIN7_BOOT};

// ms-sys-compatible MBR boot-code replacement (preserves partition
// table + disk signature at bytes 440..510):
let mbr = splice_mbr(&existing_sector0, MBR_WIN7_BOOT)?;

// FAT32 BOOTMGR multi-sector PBR splice (preserves BPB + FSInfo;
// overwrites OEM ID with "MSWIN4.1" for USB-HDD-emulation BIOSes):
let pbr = splice_fat32_pbr_multi(&existing_1024_bytes, FAT32_PBR_BOOTMGR_MULTI_BOOT)?;
```

Full API:

```rust
// Master Boot Records — splice into an existing partitioned disk, or
// build from scratch with a single-FAT32-active partition layout.
mkmsbr::splice_mbr(existing: &[u8], boot: &[u8])    -> Result<[u8; 512], MbrError>;
mkmsbr::mbr_xp(disk_sectors: u64)                   -> Result<[u8; 512], MbrError>;
mkmsbr::mbr_win7(disk_sectors: u64)                 -> Result<[u8; 512], MbrError>;

// FAT32 / NTFS PBR splices. Preserve bytes 3..89 (FAT32) / 3..84
// (NTFS) of `existing` (the BPB) and the FSInfo sector at LBA 1.
// FAT32 splices overwrite OEM ID with "MSWIN4.1" so 2005-era BIOSes
// route the stick through USB-HDD emulation rather than USB-FDD.
mkmsbr::splice_fat32_pbr(existing: &[u8], boot: &[u8])       -> Result<[u8; 512], PbrError>;
mkmsbr::splice_fat32_pbr_multi(existing: &[u8], blob: &[u8]) -> Result<Vec<u8>, PbrError>;
mkmsbr::splice_ntfs_pbr_multi(existing: &[u8], blob: &[u8])  -> Result<Vec<u8>, PbrError>;

// XP-Setup BOOTSECT.DAT chain loader. NTLDR chainloads this; it reads
// $LDR$ from pre-resolved LBA runs into target_segment:0 via CHS.
mkmsbr::build_xp_setup_chain_bootsect(
    formatter_sector0: &[u8; 512],
    target_segment: u16,
    runs: &[LbaRun],
) -> Result<[u8; 512], PbrError>;

// Pre-assembled boot-code blobs (NASM sources at boot-asm/*.asm).
mkmsbr::{MBR_XP_BOOT, MBR_WIN7_BOOT,
         FAT32_PBR_NTLDR_MULTI_BOOT,
         FAT32_PBR_BOOTMGR_BOOT, FAT32_PBR_BOOTMGR_MULTI_BOOT,
         NTFS_PBR_BOOTMGR_MULTI_BOOT,
         XP_SETUP_CHAIN_BOOTSECT_BOOT};
```

The higher-level spec-target API (`DiskGeometry` + `Fat32Bpb` typed
inputs from [docs/SPEC.md:99](docs/SPEC.md) §Library scope) is filed as
API polish in [docs/BACKLOG.md](docs/BACKLOG.md) §API polish.

## Variant status

"L1" = byte-distance vs ms-sys oracle. "L2" = synthetic QEMU smoke
against a fake loader. "L3" = QEMU against real Microsoft NTLDR /
bootmgr. "L4" = real legacy-BIOS hardware.

| Variant                      | L1                              | L2 | L3              | L4                                            | Spec target | Status |
|------------------------------|---------------------------------|----|-----------------|-----------------------------------------------|-------------|--------|
| `mbr_xp`                     | 373/440 vs `--mbr`              | ✓  | n/a             | ✓ ships in production via usbwin XP mode      | L1+L2       | shipped |
| `mbr_win7`                   | 396/440 vs `--mbr7`             | ✓  | n/a             | ✓ Win 7 install USB boots end-to-end          | L1+L2       | shipped |
| `fat32_pbr_ntldr` (multi)    | vs `--fat32nt` s0 only          | ✓  | 987 reads       | ✓ NTLDR loads on Dell E6410                   | L1+L2+L3+L4 | shipped |
| `fat32_pbr_bootmgr` (multi)  | ≥378/512 vs `--fat32pe` s1..15  | ✓  | 1520 reads      | ✓ Win 7 install USB boots end-to-end          | L2+L3+L4    | shipped |
| `ntfs_pbr_bootmgr` (multi)   | TODO                            | ✓  | pending fixture | —                                             | L2+L3       | L2 green; L3 awaiting real NTFS image |

The single-sector `fat32_pbr_bootmgr` is retained as a smoke-test
baseline. The multi-sector variant is the v1.0 target.

## Used by

mkmsbr ships in [usbwin](https://github.com/jma24/usbwin) v1.0 as the
default `--boot-record=mkmsbr` backend. usbwin's Win 7 and Windows XP
install-USB pipelines link mkmsbr in-process for MBR + FAT32 PBR bytes
and the XP-Setup BOOTSECT.DAT chain loader; ms-sys is now an opt-in
`--boot-record=ms-sys` fallback retained for byte-equality auditing.

## Build

```sh
brew install nasm

# Default build (library + CLI + embedded boot blobs):
cargo build --release

# Library only, without boot blobs — cargo check on machines without NASM:
cargo check --no-default-features
```

The `embed-boot-asm` feature is on by default and invokes NASM to
assemble `boot-asm/*.asm`. The CLI binary requires it; the library can
be built without it for hosts that only need the API types.

## Test

Integration tests are `#[ignore]` by default because they depend on
external tools. The full set:

```sh
# Layer 1 — byte-equality vs ms-sys. Needs ms-sys + nasm.
cargo test --test layer1_oracle --features compare-mssys -- --ignored

# Layer 1 — byte-diff gap detection vs ms-sys (catches "sectors ms-sys
# writes but we don't"; see docs/BACKLOG.md §Byte-diff findings).
cargo test --test byte_diff_vs_mssys --features compare-mssys -- --ignored

# Layer 2 — synthetic QEMU boot smoke.
cargo test --test qemu_mbr      -- --ignored
cargo test --test qemu_pbr      -- --ignored
cargo test --test qemu_ntfs_pbr -- --ignored

# Layer 3 — real NTLDR / bootmgr chain-load under QEMU (skips
# gracefully if fixtures are absent; see below).
cargo test --test qemu_pbr_real -- --ignored
```

L2 and L3 need `qemu-system-i386`, `mformat`, `mcopy`
(`brew install qemu mtools`). `qemu_ntfs_pbr` additionally needs Docker
as a macOS workaround for the missing `mkfs.ntfs`. The L1 oracle and
byte-diff eval need ms-sys:

```sh
git clone https://gitlab.com/cmaiolino/ms-sys.git /tmp/ms-sys
make -C /tmp/ms-sys
```

See [tests/README.md](tests/README.md) for the full test architecture.

### Layer 3 (real Microsoft boot binaries)

The L3 fixture script extracts NTLDR / NTDETECT.COM / bootmgr / BCD from
install ISOs you hold a license for, into `tests/real_content/` (gitignored,
never redistributed):

```sh
scripts/build_l3_fixtures.sh \
    --xp-iso /path/to/winxp_sp3.iso \
    --win7-iso /path/to/win7.iso

cargo test --test qemu_pbr_real --features embed-boot-asm -- --ignored
```

The L3 harness boots a FAT32 image with the real loader file under
qemu-system-i386, records `blk_co_preadv` trace events, and passes if
the recorded read count exceeds the threshold — strong evidence the
real loader took over from our PBR rather than the PBR halting first.
Set `MKMSBR_L3_MIN_READS=<n>` to override the default threshold.

## Hardware compatibility notes

Two adjustments learned the hard way during the 2026-05-19 real-hardware
investigation (see [docs/BACKLOG.md](docs/BACKLOG.md) §L4 investigation
for the full nine-iteration debug log):

- **CHS reads, not LBA-ext.** PBR stages use INT 13h fn 0x02 rather
  than fn 0x42 because legacy BIOSes that USB-FDD-emulate reject fn 0x42
  with AH=01. Stage 1 probes geometry via fn 0x08 and falls back to the
  USB-FDD profile (SPT=18, HEADS=2) if the probe is refused — needed on
  the Dell E6410 where DL is handed as 0x0F rather than 0x80.
- **OEM ID = `"MSWIN4.1"`.** Both FAT32 PBR splices overwrite the
  formatter's OEM ID so 2005-era BIOSes route the stick through USB-HDD
  emulation rather than USB-FDD.
- **MBR fingerprint.** The mkmsbr MBR's instruction sequence is shaped
  to fingerprint as Microsoft-style (`xor` byte 0, push+retf far-jump,
  error strings at canonical offset 0x163) for the same BIOS
  USB-HDD-mode trigger. These operations are standard for any MBR; the
  similarity is a property of the constrained task, not derivation.

## Clean-room

mkmsbr is developed under a strict clean-room protocol — contributors
working on boot code may not have read ms-sys's source files (`src/*.c`,
`inc/*.h`) or any other open-source bootloader's source. See
[docs/PROVENANCE.md](docs/PROVENANCE.md) for the full protocol and
[docs/SPEC.md](docs/SPEC.md) §Clean-room protocol for the per-PR
mechanisms (reading log, forbidden-symbol grep, similarity check) that
keep the claim verifiable.

## License

MIT. See [LICENSE](LICENSE).
