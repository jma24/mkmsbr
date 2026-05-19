# bootrec

Clean-room Rust library (and CLI) for producing Microsoft-compatible boot
records — MBR, FAT32 PBR, NTFS PBR — without depending on `ms-sys`. MIT
licensed.

**Status:** early development. The repo is seeded from boot-record work
done inside [usbwin](https://github.com/jmappleby/usbwin); see `docs/SPEC.md`
for the v1.0 plan and `docs/PROVENANCE.md` for the clean-room protocol.

## Why

`ms-sys` is the only widely-available source of correct Windows boot-record
bytes outside of Windows itself, but:

- It's GPL-2; bootrec is MIT, so consumers get a cleaner license story.
- It's distributed as source, with build-it-yourself friction.
- Its boot-code blobs in `inc/*.h` are themselves derived from Microsoft
  binaries — a long-running legal grey area that a from-the-spec
  reimplementation sidesteps.

bootrec is built **eval-first**: the verification harness (ms-sys-as-oracle
+ QEMU boot smoke + real-content boot + real hardware) is written before
the boot code. A variant ships when its eval passes. See
`docs/SPEC.md` §Verifiability hierarchy.

## What it produces (planned)

```rust
// Master Boot Records (whole-disk).
bootrec::mbr_win7(geometry, partitions) -> [u8; 512];
bootrec::mbr_xp(geometry, partitions)   -> [u8; 512];

// FAT32 Partition Boot Records.
bootrec::fat32_pbr_ntldr(bpb)   -> [u8; 512];
bootrec::fat32_pbr_bootmgr(bpb) -> PbrBytes;   // multi-sector

// NTFS Partition Boot Records.
bootrec::ntfs_pbr_bootmgr(bpb)  -> PbrBytes;
```

A CLI binary will mirror `ms-sys`'s flag names where the mapping is obvious
(`--mbr7` → `--mbr-win7`, `--fat32pe` → `--fat32-bootmgr`, etc.) so existing
shell recipes can switch with a one-line change.

## Today

What works now:

- `splice_fat32_pbr` — the BPB-preserving splice (the single most important
  primitive; see `src/pbr.rs`).
- `build_mbr` + `PartitionEntry` — single-FAT32-active MBR layout.
- Seed NASM sources for `mbr.asm`, `fat32_pbr.asm`, `ntfs_pbr.asm`,
  plus a `fake_bootmgr.asm` stub for the QEMU smoke test.
- A QEMU smoke test harness (`tests/qemu_pbr.rs`) that boots a synthetic
  FAT32 image through our PBR and asserts the chain-load worked.

Doesn't work yet:

- Real-content QEMU boot tests (Layer 3) — fixtures stage cleanly via
  `scripts/build_l3_fixtures.sh`, but the QEMU pass/fail signal for real
  NTLDR / bootmgr is still an open question (see `docs/BACKLOG.md`)
- Layer-4 (real-hardware boot) — only the user can run that pipeline
- `ntfs_pbr_bootmgr` variant
- A proper CLI binary (library + tests only for now)
- Anything in `docs/SPEC.md` §Component breakdown beyond initial sketches

Works:

- ms-sys byte-distance oracle (Layer 1) across MBR + single/multi-sector
  PBR variants — see `tests/layer1_oracle.rs`

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
```

The L3 smoke test that consumes these is still TODO — the pass/fail signal
for real Microsoft loaders is the open work. See `docs/BACKLOG.md`.

## Clean-room

bootrec is developed under a strict clean-room protocol — contributors
working on boot code may not have read ms-sys's source files (`src/*.c`,
`inc/*.h`) or any other open-source bootloader's source. See
`docs/PROVENANCE.md` for the full protocol and `docs/SPEC.md` §Clean-room
protocol for the per-PR mechanisms (reading log, forbidden-symbol grep,
similarity check) that keep the claim verifiable.

## License

MIT. See `LICENSE`.
