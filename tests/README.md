# tests/ — eval framework

Bootrec is developed eval-first: the verification harness exists before
each variant's boot-code implementation. See `docs/SPEC.md`
§Verifiability hierarchy for the design and §Eval-first workflow for how
to use it.

## File layout

| File / dir                  | Purpose                                                    |
|------------------------------|-----------------------------------------------------------|
| `common/oracle.rs`           | Layer-1 helper: ms-sys subprocess + byte extraction.      |
| `common/mod.rs`              | Module file Cargo includes from each integration test.    |
| `layer1_oracle.rs`           | Layer 1: byte-equality vs ms-sys (`#[ignore]` by default).|
| `qemu_pbr.rs`                | Layer 2: synthetic FAT32 boot smoke for the PBR variant.  |

Future variants will add `layer2_qemu_mbr.rs`, `layer2_qemu_ntfs.rs`, etc.
The common-harness pieces in `qemu_pbr.rs` (QEMU spawn, serial scrape,
fake-bootmgr build, mformat image creation) will move to
`common/qemu.rs` once a second variant needs them.

## How to run

```sh
# Layer 1 — byte-equality vs ms-sys. Needs ms-sys + nasm.
cargo test --test layer1_oracle \
    --features "embed-boot-asm compare-mssys" -- --ignored

# Layer 2 — synthetic QEMU boot smoke for the FAT32 PBR.
# Needs nasm + qemu-system-i386 + mtools (mformat, mcopy).
cargo test --test qemu_pbr --features embed-boot-asm -- --ignored
```

### ms-sys resolution

The Layer-1 oracle searches for ms-sys in this order:

1. `BOOTREC_MS_SYS` env var (full path).
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

Both layer tests are expected to **fail** against the seed code carried
over from usbwin. That's the methodology: the eval is the binary gate.
A variant ships when its eval passes.

- `layer1_oracle::mbr_win7_bootcode_matches_mssys`: ~405/440 bytes differ.
  The seed `boot-asm/mbr.asm` is a generic MBR, not specifically the Win 7
  variant. Becomes the gate for the future `mbr_win7` variant.
- `layer1_oracle::mbr_xp_bootcode_matches_mssys`: similar story.
- `qemu_pbr::fat32_pbr_loads_bootmgr_in_qemu`: passes against the seed
  single-sector FAT32 PBR, but the seed PBR is not byte-equivalent to
  ms-sys's multi-sector `--fat32pe`. The Layer-1 PBR oracle (TODO in
  `layer1_oracle.rs`) is what gates the `fat32_pbr_bootmgr` variant.

## Clean-room boundary

`tests/common/oracle.rs` is the only place ms-sys appears in the bootrec
codebase, and only as a black-box subprocess. Library source files
(`src/`) and boot-code source (`boot-asm/`) have no ms-sys awareness —
they're derived from FAT32/NTFS specs and BIOS docs only. See
`docs/PROVENANCE.md` and `docs/SPEC.md` §Clean-room protocol.

`scripts/clean_room_check.sh` greps `src/` + `boot-asm/` for forbidden
references (ms-sys, syslinux, etc.) and fails on hit. Run before each PR;
intended for CI.
