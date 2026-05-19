# Layer-3 real-content fixtures

Layer-3 evals (`docs/SPEC.md` §Real-content fixtures) need real Microsoft
boot binaries to confirm mkmsbr's PBR can chain-load them — not just
the fake stubs Layer 2 uses. The binaries themselves are **copyrighted
Microsoft software and not redistributable**: every developer and CI
runner stages their own copy from an install ISO they hold a license for,
via `scripts/build_l3_fixtures.sh`.

The repo commits the script + this manifest. The binaries live only in
this directory, only on machines that ran the script. `.gitignore` here
excludes everything except the manifest itself.

## What gets staged

After running `scripts/build_l3_fixtures.sh`, this directory contains:

```
xp/
  NTLDR              # Win XP / 2000 / 2003 NT loader
  NTDETECT.COM       # NTLDR's hardware-detection sidecar
win7/
  bootmgr            # Win 7 / 8 / 10 / 11 boot manager
  bcd                # Boot Configuration Data store (referenced by bootmgr)
```

These files are FAT32-volume contents the loaders expect to find by
name. Their on-disk paths in the install ISO they came from are noted
in `scripts/build_l3_fixtures.sh`.

## Reference hashes

For reproducibility we note SHA-256 hashes of the binaries extracted from
**specific** ISO editions. Different language editions, service packs, or
volume-license vs retail builds produce different hashes. The fixture
script does not gate on these; it gates only on file size being non-trivial
(reject 0-byte / "ISO doesn't have this file" failures).

| File                       | Source ISO (label / sha-256-of-iso)                                                         | SHA-256 of file                                                    |
|----------------------------|---------------------------------------------------------------------------------------------|--------------------------------------------------------------------|
| `xp/NTLDR`                 | `en_windows_xp_professional_with_service_pack_3_x86_cd_vl_x14-73974.iso` (XPSP3 VLK)        | `644335c778eed2c2acb701657fb337cef93bde486650878d3c6ebc2ac4d4a447` |
| `xp/NTDETECT.COM`          | same as above                                                                               | `8f7186a71684dd114e89cc908ed9400192bc3a47fb288cce4c5c27d0f5d3afa4` |
| `win7/bootmgr`             | `Win7_Ult_SP1_English_x32.iso` (Win 7 Ultimate SP1 x86 English)                             | `0769a292114dfe181dc4931159c24cd7adb6a3f3823177e40eb45ee59688ea4a` |
| `win7/bcd`                 | same as above                                                                               | `b3357aa4b3fb0f1dc2a9acd5787d3be7a36d8494ac52b8d385699c376a76af90` |

Reference hashes are informational. If yours differ but file sizes are
reasonable (NTLDR ≈ 245 KiB, NTDETECT ≈ 47 KiB, bootmgr ≈ 380 KiB,
BCD ≈ 256 KiB), you almost certainly have a valid alternate edition.

## How the fixtures are consumed

The Layer-3 QEMU smoke tests (`tests/qemu_pbr_real.rs`) format a FAT32
image, `mcopy` these real binaries onto it, splice our PBR through
`splice_fat32_pbr` / `splice_fat32_pbr_multi`, and boot in QEMU.
Success criterion: guest block-read count via `qemu -trace blk_co_preadv`
exceeds a threshold (default 50; override with `MKMSBR_L3_MIN_READS`).
A successful chainload reads the loader file off FAT plus the real
loader's own subsequent reads — hundreds to thousands in practice.
A halted PBR issues only single- to double-digit reads.

## Why we don't ship our own minimal NTLDR substitute

The Layer-2 fake loader (`boot-asm/fake_bootmgr.asm`) already proves our
PBR can chain-load *something* by name. The point of Layer 3 is to
detect "ms-sys's real PBR works against real NTLDR but ours doesn't" —
which can only happen with real Microsoft binaries on the other end of
the jump. A more-faithful substitute wouldn't catch the kinds of bugs
L3 exists to catch.
