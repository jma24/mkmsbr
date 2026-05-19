# tests/ — eval framework

Mkmsbr is developed eval-first: the verification harness exists before
each variant's boot-code implementation. See `docs/SPEC.md`
§Verifiability hierarchy for the design and §Eval-first workflow for how
to use it.

## File layout

| File / dir                  | Purpose                                                            |
|------------------------------|-------------------------------------------------------------------|
| `common/oracle.rs`           | Layer-1 helper: ms-sys subprocess + byte extraction.              |
| `common/qemu_trace.rs`       | Layer-3 helper: QEMU spawn with `blk_co_preadv` trace + counter.  |
| `common/mod.rs`              | Module file Cargo includes from each integration test.            |
| `layer1_oracle.rs`           | Layer 1: byte-equality vs ms-sys (`#[ignore]` by default).        |
| `qemu_mbr.rs`                | Layer 2: synthetic chain-load smoke for the MBR variants.         |
| `qemu_pbr.rs`                | Layer 2: synthetic FAT32 boot smoke for the PBR variants.         |
| `qemu_pbr_real.rs`           | Layer 3: real NTLDR / bootmgr chain-load under QEMU + trace gate. |
| `real_content/`              | Layer-3 fixture binaries (gitignored; staged by the L3 script).   |

`qemu_pbr.rs` still owns its own QEMU-spawn helper because it gates on
serial output. `qemu_pbr_real.rs` uses `common/qemu_trace.rs` instead,
which gates on guest block-read count. Future NTFS variants will reuse
both helpers.

## How to run

```sh
# Layer 1 — byte-equality vs ms-sys. Needs ms-sys + nasm.
cargo test --test layer1_oracle \
    --features "embed-boot-asm compare-mssys" -- --ignored

# Layer 2 — synthetic QEMU boot smoke for MBR + FAT32 PBR variants.
# Needs nasm + qemu-system-i386 + mtools (mformat, mcopy).
cargo test --test qemu_mbr --features embed-boot-asm -- --ignored
cargo test --test qemu_pbr --features embed-boot-asm -- --ignored

# Layer 3 — real NTLDR / bootmgr chain-load under QEMU.
# Additionally needs mmd (mtools) + fixtures staged by
# scripts/build_l3_fixtures.sh. Tests skip if fixtures are absent.
cargo test --test qemu_pbr_real --features embed-boot-asm -- --ignored
```

### ms-sys resolution

The Layer-1 oracle searches for ms-sys in this order:

1. `MKMSBR_MS_SYS` env var (full path).
2. `/tmp/ms-sys/bin/ms-sys`.
3. `/usr/local/bin/ms-sys`.
4. `/opt/homebrew/bin/ms-sys`.
5. `which ms-sys` on PATH.

Install:

```sh
git clone https://gitlab.com/cmaiolino/ms-sys.git /tmp/ms-sys
cd /tmp/ms-sys && make
```

### Expected status today

All Layer-1, Layer-2 and Layer-3 tests for the four implemented
variants (`mbr_xp`, `mbr_win7`, `fat32_pbr_ntldr`,
`fat32_pbr_bootmgr` single + multi-sector) pass. Variant-by-variant
status — including L1 byte-distances and L3 observed read counts — is
maintained in `docs/BACKLOG.md` §Variant matrix.

The remaining `ntfs_pbr_bootmgr` variant has no eval coverage yet
(L2 NTFS harness is the prerequisite).

## Clean-room boundary

`tests/common/oracle.rs` is the only place ms-sys appears in the mkmsbr
codebase, and only as a black-box subprocess. Library source files
(`src/`) and boot-code source (`boot-asm/`) have no ms-sys awareness —
they're derived from FAT32/NTFS specs and BIOS docs only. See
`docs/PROVENANCE.md` and `docs/SPEC.md` §Clean-room protocol.

`scripts/clean_room_check.sh` greps `src/` + `boot-asm/` for forbidden
references (ms-sys, syslinux, etc.) and fails on hit. Run before each PR;
intended for CI.
