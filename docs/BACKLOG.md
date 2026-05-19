# mkmsbr backlog

Post-v1.0.1 roadmap. v1.0 shipped on 2026-05-19 with four of five
boot-record variants at their spec-defined eval target; this doc
tracks what's left for v1.1 and beyond.

Engineering history (L4 real-hardware investigation, byte-diff
findings, BIOS USB-emulation discovery) lives in
[L4_INVESTIGATION.md](L4_INVESTIGATION.md). For shipping decisions
and version notes, see [CHANGELOG.md](../CHANGELOG.md). For the v1.0
plan and eval-framework design, see [SPEC.md](SPEC.md).

## Current state

Per [SPEC.md](SPEC.md) §Component breakdown. "L1" = byte-distance vs
ms-sys oracle. "L2" = synthetic QEMU smoke. "L3" = QEMU against real
Microsoft NTLDR / bootmgr. "L4" = real legacy-BIOS hardware.

| Variant                      | L1 | L2 | L3 | L4 | Spec target | Status |
|------------------------------|----|----|----|----|-------------|--------|
| `mbr_xp`                     | ✓  | ✓  | n/a | ✓ (via usbwin XP)   | L1+L2       | shipped |
| `mbr_win7`                   | ✓  | ✓  | n/a | ✓ Win 7 end-to-end  | L1+L2       | shipped |
| `fat32_pbr_ntldr` (multi)    | ✓  | ✓  | ✓  | ✓ NTLDR loads       | L1+L2+L3+L4 | shipped |
| `fat32_pbr_bootmgr` (multi)  | ✓  | ✓  | ✓  | ✓ Win 7 end-to-end  | L2+L3+L4    | shipped |
| `ntfs_pbr_bootmgr` (multi)   | —  | ✓  | —  | —                   | L2+L3       | L2 green; L1 + L3 pending |

The single-sector `fat32_pbr_bootmgr` is retained as a smoke-test
baseline only.

## Variant completion

### `ntfs_pbr_bootmgr` to spec target

The remaining variant from v1.0 scope. Stage 2 walks $MFT with USA
fixups, B+tree-style linear INDX scan, $MFT extent chasing, and
inline $INDEX_ROOT scanning. L2 green against an ntfs-3g-formatted
volume; the open work is real-content validation.

| Item                                 | Notes                                                              | Status |
|--------------------------------------|--------------------------------------------------------------------|--------|
| L1 byte-distance oracle              | `ms-sys --ntfs` companion to the existing `--mbr` / `--fat32*` oracles | TODO |
| L3 fixture against real Win 7 NTFS   | Same shape as `fat32_pbr_bootmgr` L3; needs a real Win 7 NTFS image to extract from | TODO |
| Resident `$DATA` support             | Current stage 2 assumes non-resident; fake bootmgr in L2 must be padded past ~700 B | TODO (edge case) |

## API polish

The shipping 1.0 API is splice-based and takes raw byte slices; the
SPEC.md §Library scope target is typed-input.

| Item                                                                   | Notes                                                              | Status |
|------------------------------------------------------------------------|--------------------------------------------------------------------|--------|
| `mbr_win7_with_signature(disk_sectors, sig: u32)`                      | Per-USB NT disk signature. Today: hardcoded 0xDEADBEEF placeholder | TODO |
| Typed-input MBR API: `mbr_xp(DiskGeometry, &[PartitionEntry])`         | SPEC.md target; current `mbr_*(disk_sectors: u64)` is provisional  | TODO |
| Typed-input PBR API: `fat32_pbr_bootmgr(Fat32Bpb) -> PbrBytes`         | SPEC.md target; current splice API is provisional                  | TODO |
| `PbrBytes` newtype to replace `Vec<u8>` from multi splices             | Cosmetic                                                           | TODO |
| Polished rustdoc landing pages                                         | Partial — types are documented; module-level docs are thin         | partial |

## Test coverage gaps

| Item                                                                   | Notes                                                              | Status |
|------------------------------------------------------------------------|--------------------------------------------------------------------|--------|
| Hardened L3 harness (post-handoff success)                             | Capture QEMU serial / boot to recognizable later stage / ms-sys positive control. See [L4_INVESTIGATION.md](L4_INVESTIGATION.md) §L3 gate weakness | TODO |
| CHS-only QEMU variant                                                  | Boot via `-drive if=floppy` so SeaBIOS rejects fn 0x42; would have caught the LBA-ext deviation before L4 | TODO |
| Regression golden fixtures                                             | Byte-for-byte fixtures committed in `tests/golden/`; refresh script | TODO |
| `tests/determinism.sh`                                                 | Build twice, compare bytes; SPEC.md §Verifiable                    | TODO |
| Layer 4 hardware checklist                                             | Reference rig list + per-target boot result                        | TODO |

## CI & packaging

| Item                                                                   | Notes                                                              | Status |
|------------------------------------------------------------------------|--------------------------------------------------------------------|--------|
| GitHub Actions workflow                                                | clean_room_check + `cargo test` on every PR                        | TODO |
| L1/L2 in CI (`#[ignore]` gate)                                         | Needs nasm + qemu + mtools + ms-sys on runner image                | TODO |
| L3 in CI                                                               | Depends on fixture-build infrastructure + license-friendly runner  | TODO |
| Homebrew tap (`jma24/homebrew-mkmsbr`)                                 | Formula drafted; needs the tap repo + sha256 from a tagged release | TODO |
| Prebuilt binaries on GitHub Releases                                   | macOS arm64/x64 + Linux x64; cross-build via release workflow      | TODO |

## Clean-room process

| Item                                                                   | Notes                                                              | Status |
|------------------------------------------------------------------------|--------------------------------------------------------------------|--------|
| `docs/PROVENANCE.md` clean-room protocol                               | Inherited; SHA-256s filled in at v1.0.1                            | ✓ |
| `scripts/clean_room_check.sh` forbidden-symbol grep                    | Run before each PR; gated in v1.0 commits                          | ✓ |
| Statistical similarity check (in `tests/layer1_oracle.rs`)             | `< SUSPICIOUSLY_LOW` Hamming threshold                             | ✓ |
| Public legal review                                                    | Completed pre-v1.0                                                 | ✓ |
| `CONTRIBUTORS_READING.md`                                              | Per-contributor reading declaration; add when second contributor joins | deferred |
| Per-PR clean-room declaration template                                 | GitHub PR template                                                 | TODO |

## Documentation

| Item                                                                   | Notes                                                              | Status |
|------------------------------------------------------------------------|--------------------------------------------------------------------|--------|
| `README.md`                                                            | Install + usage; v1.0 shape                                        | ✓ |
| `CHANGELOG.md`                                                         | Keep a Changelog format                                            | ✓ |
| `docs/SPEC.md`                                                         | Frozen v1.0 plan                                                   | ✓ |
| `docs/PROVENANCE.md`                                                   | Clean-room protocol + blob hashes                                  | ✓ |
| `docs/L4_INVESTIGATION.md`                                             | Real-hardware bring-up post-mortem                                 | ✓ |
| `docs/XP_SETUP_CHAIN_BOOTSECT_SPEC.md`                                 | XP Setup chain primitive design                                    | ✓ |
| `docs/BOOT_RECORDS.md` (BPB rationale)                                 | Why we splice rather than build; carryover from usbwin             | TODO |
| `COVERAGE.md` (machine-checked variant × layer)                        | SPEC.md §Verifiable                                                | TODO |
| `SPEC_TRACE.md` (spec → code links)                                    | SPEC.md §Verifiable                                                | TODO |

## Resolved engineering questions

One-line answers to questions that surfaced during v1.0 development;
links go to the doc where the reasoning lives.

- **Why CHS reads instead of fn 0x42 LBA-ext?** Legacy BIOSes reject
  fn 0x42 under USB-FDD emulation. See [L4_INVESTIGATION.md](L4_INVESTIGATION.md)
  §Root cause 1.
- **Why does the MBR fingerprint as Microsoft-style?** Legacy BIOSes
  choose USB-HDD vs USB-FDD emulation by MBR pattern-matching. See
  [L4_INVESTIGATION.md](L4_INVESTIGATION.md) §Root cause 2.
- **Why OEM ID = "MSWIN4.1" in the FAT32 splices?** Same BIOS
  USB-emulation heuristic, applied at the PBR layer.
- **Does mkmsbr's 2-sector PBR satisfy real bootmgr's runtime
  contract under QEMU and on real hardware?** Yes. The LBA-12
  stage-3-helpers hypothesis was surfaced and killed: real bootmgr
  never issues a read against partition LBA 12 in our boot path. See
  [L4_INVESTIGATION.md](L4_INVESTIGATION.md) §Byte-diff findings.
- **How is byte-similarity to ms-sys MBR defensible as clean-room?**
  Standard MBR operations (partition scan, fn 0x41 probe, A20 enable,
  INT 0x18) admit only a narrow space of correct encodings. See
  [PROVENANCE.md](PROVENANCE.md) §What if Microsoft objects, and
  [L4_INVESTIGATION.md](L4_INVESTIGATION.md) §Engineering judgment.
