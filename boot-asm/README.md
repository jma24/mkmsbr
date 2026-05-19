# boot-asm

Hand-written NASM source for the boot-record blobs mkmsbr embeds.
Pre-assembled blobs live in `../blobs-prebuilt/`; the top-level
`build.rs` prefers `nasm` (this directory) when it's on PATH and falls
back to the prebuilt bytes otherwise.

## Files

Single-sector blobs (512 bytes each):

| File                          | What it does                                                          |
|-------------------------------|-----------------------------------------------------------------------|
| `mbr_xp.asm`                  | Win 2000/XP/2003 MBR: find active primary, chain-load its PBR.        |
| `mbr_win7.asm`                | Win 7/8/10/11 MBR: same shape as XP plus a GPT-protective refusal.    |
| `fat32_pbr_bootmgr.asm`       | Single-sector FAT32 BOOTMGR PBR — smoke-test baseline only.           |
| `xp_setup_chain_bootsect.asm` | XP-Setup BOOTSECT.DAT chain loader; reads `$LDR$` LBA runs via CHS.   |

Multi-sector variants (one subdirectory per variant; `sector0.asm` =
stage 1, `sector1.asm` = stage 2):

| Directory             | What it does                                                    |
|-----------------------|-----------------------------------------------------------------|
| `fat32_pbr_bootmgr/`  | FAT32 PBR that loads `bootmgr` (Win 7+ install media).          |
| `fat32_pbr_ntldr/`    | FAT32 PBR that loads NTLDR (Win 2000/XP/2003 install media).    |
| `ntfs_pbr_bootmgr/`   | NTFS PBR that loads `bootmgr` (walks $MFT with USA fixups).     |

Test fixtures (not shipped, only used by `tests/qemu_*.rs`):

| File              | What it does                                       |
|-------------------|----------------------------------------------------|
| `fake_bootmgr.asm`| Stub that takes over after the PBR's chain-load.   |
| `fake_pbr.asm`    | Stub that takes over after the MBR's chain-load.   |

## Build

The canonical build runs from mkmsbr's top-level `build.rs` whenever
the `embed-boot-asm` feature is on (default). For manual iteration
during NASM development:

```sh
brew install nasm     # if not already installed
cd boot-asm
make                  # → build/*.bin (and build/<variant>/sector{0,1}.bin)
make test-fixtures    # → build/fake_bootmgr.bin + build/fake_pbr.bin
```

After editing a `.asm` source and re-running `cargo build`, refresh
the prebuilt blobs in `../blobs-prebuilt/` per the recipe at
[blobs-prebuilt/README.md](../blobs-prebuilt/README.md) so end-user
`cargo install` keeps getting the latest bytes.

## Verification

Four eval layers, ordered by feedback-loop speed. See
[`../docs/SPEC.md`](../docs/SPEC.md) §Verifiability hierarchy for the
full story and [`../docs/BACKLOG.md`](../docs/BACKLOG.md) §Current
state for which variants pass which layers today.

1. **Unit (`cargo test`)** — splice logic and partition-table encoding.
2. **L1 — byte-distance vs ms-sys** — `cargo test --test layer1_oracle --features compare-mssys -- --ignored` (needs ms-sys on PATH). The `byte_diff_vs_mssys` companion test surfaces sectors ms-sys writes but mkmsbr doesn't.
3. **L2 — QEMU boot smoke** — `cargo test --test qemu_mbr -- --ignored` (and `qemu_pbr`, `qemu_ntfs_pbr`). Synthetic disk image + fake loader.
4. **L3 — QEMU against real Microsoft loaders** — `cargo test --test qemu_pbr_real -- --ignored`, gated on `blk_co_preadv` trace event count.
5. **L4 — real legacy-BIOS hardware** — manual; Win 7 install USBs built with these blobs boot end-to-end on the Dell E6410 reference rig.
