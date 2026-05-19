# mkmsbr

Clean-room Rust library (and CLI) for producing Microsoft-compatible boot
records — MBR, FAT32 PBR, NTFS PBR — without depending on `ms-sys`. MIT
licensed.

**Status:** early development. The repo is seeded from boot-record work
done inside [usbwin](https://github.com/jmappleby/usbwin); see `docs/SPEC.md`
for the v1.0 plan and `docs/PROVENANCE.md` for the clean-room protocol.

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
`docs/SPEC.md` §Verifiability hierarchy.

## What it produces (planned)

```rust
// Master Boot Records (whole-disk).
mkmsbr::mbr_win7(geometry, partitions) -> [u8; 512];
mkmsbr::mbr_xp(geometry, partitions)   -> [u8; 512];

// FAT32 Partition Boot Records (multi-sector). Both use CHS reads
// for compatibility with USB-FDD-emulating BIOSes; the single-sector
// fat32_pbr_bootmgr variant ships only as a smoke-test baseline.
mkmsbr::fat32_pbr_ntldr(bpb)   -> PbrBytes;   // 3 sectors (XP/2003)
mkmsbr::fat32_pbr_bootmgr(bpb) -> PbrBytes;   // 3 sectors (Win 7+)

// NTFS Partition Boot Record (multi-sector).
mkmsbr::ntfs_pbr_bootmgr(bpb)  -> PbrBytes;

// XP-Setup BOOTSECT.DAT chain loader. Single sector that NTLDR
// chainloads to load $LDR$ from pre-resolved LBA runs.
mkmsbr::build_xp_setup_chain_bootsect(
    formatter_sector0, target_segment, runs,
) -> [u8; 512];
```

A CLI binary will mirror `ms-sys`'s flag names where the mapping is obvious
(`--mbr7` → `--mbr-win7`, `--fat32pe` → `--fat32-bootmgr`, etc.) so existing
shell recipes can switch with a one-line change.

## Today

What works now:

- `splice_fat32_pbr` (single-sector) and `splice_fat32_pbr_multi`
  (multi-sector) — the BPB-preserving splices that are the single most
  important primitive (see `src/pbr.rs`). Both overwrite OEM ID with
  `"MSWIN4.1"` so 2000s-era BIOSes route the stick through USB-HDD
  emulation rather than USB-FDD.
- `splice_ntfs_pbr_multi` — same shape for NTFS.
- `build_mbr` + `PartitionEntry` — single-FAT32-active MBR layout.
- `build_xp_setup_chain_bootsect(formatter_sector0, target_segment,
  runs)` — XP-Setup `BOOTSECT.DAT` loader (see `src/pbr.rs`). Caller
  pre-resolves `$LDR$` LBA runs; we emit a CHS-reading bootsector.
- NASM sources for `mbr_xp.asm`, `mbr_win7.asm`, `fat32_pbr_bootmgr/`,
  `fat32_pbr_ntldr/` (both multi-sector), `ntfs_pbr_bootmgr/`, and
  `xp_setup_chain_bootsect.asm`, plus a `fake_bootmgr.asm` stub for
  the QEMU smoke tests.
- QEMU smoke test harnesses that boot synthetic FAT32 + NTFS images
  through our PBRs and assert the chain-load worked
  (`tests/qemu_pbr.rs`, `tests/qemu_ntfs_pbr.rs`,
  `tests/qemu_pbr_real.rs` for real Microsoft loaders).
- Layer-4 (real hardware): Win 7 and XP NTLDR both reach the loader
  step on the Dell Latitude E6410 reference rig (2005-vintage Phoenix
  Award P4 also works for Win 7). XP Setup phase needs the
  BOOTSECT.DAT chain to be wired up on the usbwin side (see
  `docs/USBWIN_NTLDR_FINDINGS_2026_05_19.md`).

Doesn't work yet:

- L3 (real Win 7 NTFS install) against the `ntfs_pbr_bootmgr` variant
  — code complete (USA fixups, $INDEX_ALLOCATION B+tree handling, $MFT
  extent chasing, $INDEX_ROOT inline scan all landed 2026-05-18); just
  waiting on a real NTFS image for the actual L3 run
- A proper CLI binary (library + tests only for now)
- Anything in `docs/SPEC.md` §Component breakdown beyond initial sketches

Works:

- ms-sys byte-distance oracle (Layer 1) across MBR + single/multi-sector
  PBR variants — see `tests/layer1_oracle.rs`
- Real-content QEMU boot tests (Layer 3) against real NTLDR and bootmgr
  — see `tests/qemu_pbr_real.rs`. Pass/fail signal is guest block-read
  count via `qemu -trace blk_co_preadv`. Fixtures stage via
  `scripts/build_l3_fixtures.sh`.

## Build

```sh
brew install nasm

# Library only, no boot blobs (cargo check works without nasm):
cargo build

# Library + embedded boot blobs:
cargo build --release --features embed-boot-asm
```

## Test

```sh
cargo test                                                                            # unit tests
cargo test --test qemu_pbr --features embed-boot-asm -- --ignored                     # L2 QEMU smoke
cargo test --test layer1_oracle --features "embed-boot-asm compare-mssys" -- --ignored # L1 ms-sys oracle
```

The L2 QEMU smoke test needs `qemu-system-i386`, `mformat`, `mcopy`
(`brew install qemu mtools`). The L1 oracle additionally needs ms-sys
(`git clone https://gitlab.com/cmaiolino/ms-sys.git /tmp/ms-sys && make -C /tmp/ms-sys`).

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
Tests skip gracefully if fixtures are absent.

## Clean-room

mkmsbr is developed under a strict clean-room protocol — contributors
working on boot code may not have read ms-sys's source files (`src/*.c`,
`inc/*.h`) or any other open-source bootloader's source. See
`docs/PROVENANCE.md` for the full protocol and `docs/SPEC.md` §Clean-room
protocol for the per-PR mechanisms (reading log, forbidden-symbol grep,
similarity check) that keep the claim verifiable.

## License

MIT. See `LICENSE`.
