# bootrec v1.0 backlog

Where we are vs. what `docs/SPEC.md` calls v1.0. Internal-honest tone:
"shipped" means the eval gates the spec required for that row are
green; "unproven" means tested only against synthetic loaders, not
real Microsoft files; "blocked" means waiting on external input.

The spec's 14-week timeline (§Timeline estimate) was conservative —
the eval-first methodology is paying compounding interest. As of
this writing, 4 of 5 variants ship at their `boot-asm/` Layer-2
target in a single development arc.

Last updated: 2026-05-18, after commit `3348398`.

## Variant matrix

Per `docs/SPEC.md` §Component breakdown. "L1" = byte-distance vs ms-sys
oracle (informational, gated by `< SUSPICIOUSLY_LOW` Hamming threshold).
"L2" = QEMU smoke against fake loader. "L3" = QEMU smoke against real
Microsoft files. "L4" = real-hardware boot.

| Variant                     | L1                       | L2 (fake)  | L3 (real) | L4 (HW) | Spec target | Status |
|-----------------------------|--------------------------|------------|-----------|---------|-------------|--------|
| `mbr_xp`                    | ✓ 373/440 vs `--mbr`     | ✓          | n/a       | —       | L1+L2       | shipped at spec target |
| `mbr_win7`                  | ✓ 396/440 vs `--mbr7`    | ✓          | n/a       | —       | L1+L2       | shipped at spec target |
| `fat32_pbr_ntldr`           | ✓ 398/423 vs `--fat32nt` | ✓          | —         | —       | L1+L2+L3    | L3 pending |
| `fat32_pbr_bootmgr` (single)| ✓ 392/423 vs `--fat32pe` | ✓          | unproven  | unproven| —           | legacy / smoke baseline |
| `fat32_pbr_bootmgr` (multi) | —                        | ✓          | —         | —       | L2+L3+L4    | L1 + L3 + L4 pending |
| `ntfs_pbr_bootmgr`          | —                        | —          | —         | —       | L2+L3       | not started |

The single-sector `fat32_pbr_bootmgr` is kept as a smoke-test baseline.
The multi-sector variant is the v1.0 target (`docs/SPEC.md` line 132).

## Per-variant remaining work

### `fat32_pbr_ntldr` to spec target
- **L3 fixture from a Win XP ISO** — `tests/real_content/` per spec
  §Real-content fixtures. Needs the user to supply an ISO path; the
  fixture-build script extracts a minimal subset and a real `NTLDR` so
  the smoke test can confirm chain-load works against actual Microsoft
  files. *Blocks: spec L3 target.*

### `fat32_pbr_bootmgr` multi-sector to spec target
- **L1 oracle for multi-sector.** Current oracle only compares sector 0.
  Extend to compare sector 1 too (bytes-equality against ms-sys's
  sectors 1 and 12 — note ms-sys is 16 sectors, ours is 2; alignment
  question to resolve). *Not gating but a missing eval signal.*
- **L3 fixture from a Win 7 ISO.** Same shape as ntldr L3 but with
  real `bootmgr` + `Boot/BCD`. The known hard question: does our
  2-sector layout satisfy real `bootmgr`'s contract, or does it need
  the full 16-sector Microsoft layout? Will discover at this gate.
  *Blocks: spec L3 target.*
- **L4 real-hardware verification.** Dell E6410 + 2010-2015 Intel
  desktop + 2005-vintage P4. *Blocks: spec L4 target / 1.0 release.*

### `ntfs_pbr_bootmgr` from scratch
- **NASM clean-room.** Walks $MFT instead of FAT. Spec calls
  high-complexity; comparable lift to the FAT32 PBRs. Layer 2 against
  a fake bootmgr on a synthetic NTFS image (mkfs.ntfs / mtools-ntfs
  toolchain needed; uncertain on macOS dev environment).
- **L3 fixture from a Win 7 NTFS install.** Same shape as bootmgr L3.
- *Blocks: spec L2+L3 target.*

## Eval framework

| Item                                | Spec ref           | Status |
|-------------------------------------|--------------------|--------|
| Layer 1 oracle (ms-sys subprocess)  | §Verifiability     | ✓ MBR + PBR sector 0 |
| Layer 1 oracle for multi-sector PBR | §Verifiability     | TODO (note above) |
| Layer 2 QEMU harness (FAT32 PBR)    | §Eval-first        | ✓ single + multi |
| Layer 2 QEMU harness (MBR)          | §Eval-first        | ✓ both variants |
| Layer 2 QEMU harness (NTFS)         | §Eval-first        | TODO |
| Layer 3 fixture build script        | §Real-content      | TODO |
| Layer 4 hardware checklist          | §Layer 4           | TODO |
| Statistical similarity check        | §Clean-room mech 4 | ✓ in layer1_oracle.rs |
| Forbidden-symbol grep               | §Clean-room mech 3 | ✓ scripts/clean_room_check.sh |
| COVERAGE.md (variant × layer)       | §Verifiable        | TODO |
| Determinism check (`tests/determinism.sh`) | §Verifiable | TODO |
| SPEC_TRACE.md                        | §Verifiable        | TODO |
| Regression golden fixtures           | §Verifiable        | TODO |

## CI / packaging / release

| Item                                | Notes                                                      | Status |
|-------------------------------------|------------------------------------------------------------|--------|
| GitHub Actions workflow             | Run clean_room_check + cargo test on every PR              | TODO |
| Layer 1/2 in CI (ignored gate)      | Needs nasm + qemu + mtools + ms-sys on runner              | TODO |
| Layer 3 in CI                       | Depends on fixture-build infrastructure                    | TODO |
| CLI binary (`src/bin/bootrec.rs`)   | Clap wrapper, ms-sys flag aliases (§Form factor)           | TODO |
| Cargo features clean-up             | `embed-boot-asm` default-on once stable                    | TODO |
| `crates.io` publish                 | Reserve name; first release                                | TODO |
| Homebrew formula                    | `brew install bootrec` (§Audience and packaging)           | TODO |
| README user-install instructions    | Polished install + usage section                           | TODO |

## Clean-room process

| Item                                | Spec ref              | Status |
|-------------------------------------|-----------------------|--------|
| `docs/PROVENANCE.md`                | §Clean-room protocol  | ✓ inherited from usbwin |
| `CONTRIBUTORS_READING.md`           | §Clean-room mech 2    | TODO (currently single-contributor; add when 2nd joins) |
| Per-PR clean-room declaration       | §Clean-room mech 1    | TODO (PR template) |
| Independent code review per release | §Clean-room mech 5    | process-only |
| Public legal review pre-1.0         | §Clean-room mech 6    | TODO before tag |

## API polish

| Item                                                                 | Status |
|----------------------------------------------------------------------|--------|
| `mbr_xp` / `mbr_win7` take `disk_sectors: u64`                       | provisional |
| Spec target: `mbr_xp(disk: DiskGeometry, partitions: &[...]) -> [u8; 512]` | TODO |
| `splice_fat32_pbr` is the only PBR entry point                       | provisional |
| Spec target: `fat32_pbr_bootmgr(bpb) -> PbrBytes`                    | TODO |
| `splice_fat32_pbr_multi` returns `Vec<u8>` not `PbrBytes` newtype    | TODO (cosmetic) |
| Doc comments / rustdoc landing pages                                 | partial |

## Documentation

| Item                                  | Status |
|---------------------------------------|--------|
| `README.md`                            | ✓ scaffold |
| `docs/SPEC.md`                         | ✓ frozen v1.0 plan |
| `docs/PROVENANCE.md`                   | ✓ inherited |
| `docs/BACKLOG.md` (this file)         | ✓ |
| `docs/BOOT_RECORDS.md` (BPB rationale) | TODO — copy/adapt from usbwin |
| `CHANGELOG.md`                         | TODO before first tagged release |
| `COVERAGE.md` (machine-checked)        | TODO (in eval framework section) |
| `SPEC_TRACE.md` (spec → code links)    | TODO |

## Tracking notes

- The spec was written assuming we'd be working from spec → NASM with no
  Layer-2 harness initially; we inverted that order (harness first), which
  is why variants are landing faster than the spec estimated.
- The session-by-session cadence so far has been ~1 variant or
  ~1 framework piece per 60–90 minute focused session. The remaining
  variants (`ntfs_pbr_bootmgr`, multi-sector L3 against real BOOTMGR) are
  expected to take longer because of unknown filesystem/contract details.
- v1.0 ship date is gated by L4 (real hardware) — the user runs that
  pipeline; everything else is bootrec-side work.
