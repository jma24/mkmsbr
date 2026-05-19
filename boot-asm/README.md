# boot-asm

Hand-written NASM source for the boot-record blobs bootrec embeds.

| File           | What it does                                                    |
|----------------|-----------------------------------------------------------------|
| `mbr_xp.asm`   | Win 2000/XP/2003 MBR: find active primary partition, chain-load.|
| `fat32_pbr.asm`| FAT32 PBR: read BPB, walk FAT, load `bootmgr`, jump.            |
| `ntfs_pbr.asm` | NTFS PBR: same shape but walks NTFS structures.                 |

Future variants per `docs/SPEC.md` §Component breakdown: `mbr_win7.asm`,
`fat32_pbr_ntldr.asm`, `fat32_pbr_bootmgr/` (multi-sector),
`ntfs_pbr_bootmgr/`.

Each file assembles to **exactly 512 bytes** of raw binary. The build is
invoked from bootrec's top-level `build.rs` when the `embed-boot-asm`
feature is on.

## Manual build

```sh
brew install nasm
cd boot-asm
make
ls -l build/    # mbr.bin fat32_pbr.bin ntfs_pbr.bin, 512 bytes each
```

## Verification

Three layers, ordered by feedback-loop speed. See [`docs/SPEC.md`](../docs/SPEC.md) §Verifiability hierarchy for the full story.

1. **`cargo test`** — unit tests on the splice logic and partition-table encoding.
2. **Byte equality vs ms-sys** (gated): `cargo test --features compare-mssys` with `BOOTREC_MSSYS_BLOBS_DIR` set. *Eval framework in progress.*
3. **QEMU smoke test**: `cargo test --test qemu_pbr --features embed-boot-asm -- --ignored` boots a synthetic FAT32 image whose first sector uses our PBR.
4. **Real hardware**: dedicated checklist (TODO).

## Status

These files are the seed code carried over from the usbwin work. They are
**not** at v1.0 quality yet — see `docs/SPEC.md` §Component breakdown for
the v1.0 plan and the eval gates each variant must clear.
