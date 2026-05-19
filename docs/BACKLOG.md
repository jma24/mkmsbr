# bootrec v1.0 backlog

Where we are vs. what `docs/SPEC.md` calls v1.0. Internal-honest tone:
"shipped" means the eval gates the spec required for that row are
green; "unproven" means tested only against synthetic loaders, not
real Microsoft files; "blocked" means waiting on external input.

The spec's 14-week timeline (§Timeline estimate) was conservative —
the eval-first methodology is paying compounding interest. As of
this writing, 4 of 5 variants ship at their `boot-asm/` Layer-2
target in a single development arc.

Last updated: 2026-05-18, after the L1-multi-oracle + L3-fixture-infra
session (post-commit `069935b`, uncommitted).

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
| `fat32_pbr_bootmgr` (multi) | ✓ ≥378/512 vs `--fat32pe` s1..15 | ✓    | —         | —       | L2+L3+L4    | L3 + L4 pending |
| `ntfs_pbr_bootmgr`          | —                        | —          | —         | —       | L2+L3       | not started |

The single-sector `fat32_pbr_bootmgr` is kept as a smoke-test baseline.
The multi-sector variant is the v1.0 target (`docs/SPEC.md` line 132).

## Per-variant remaining work

### `fat32_pbr_ntldr` to spec target
- **L3 smoke test (`tests/qemu_pbr_real.rs`)** — fixtures already staged
  by `scripts/build_l3_fixtures.sh` (NTLDR + NTDETECT.COM under
  `tests/real_content/xp/`). Open question: what's the pass/fail signal?
  Real NTLDR doesn't write to COM1, so the L2 "BOOTREC OK" trick doesn't
  carry over. Candidate signals: (1) qemu `-trace bdrv_aio_readv` and
  assert N > some threshold (NTLDR self-loads many sectors; our PBR alone
  reads ~3); (2) `pmemsave` after timeout, grep for NTLDR's embedded
  strings; (3) VGA framebuffer screendump + ASCII recognition. Option 1
  is probably cleanest. *Blocks: spec L3 target.*

### `fat32_pbr_bootmgr` multi-sector to spec target
- **L3 smoke test** — fixtures already staged (`tests/real_content/win7/`
  `bootmgr` + `bcd`). Same signal-detection question as the ntldr L3.
  Additional unknown: does our 2-sector layout satisfy real `bootmgr`'s
  contract, or does it need the full 16-sector Microsoft layout? The L1
  oracle showed ms-sys populates sectors 0, 1, 2, 6, 12 — stage 2+ code
  is in 2/6/12, not in 1. Test will discover this. *Blocks: spec L3 target.*
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
| Layer 1 oracle for multi-sector PBR | §Verifiability     | ✓ — ms-sys populates sectors 0,1,2,6,12; best sector-1 match Hamming 378/512 |
| Layer 2 QEMU harness (FAT32 PBR)    | §Eval-first        | ✓ single + multi |
| Layer 2 QEMU harness (MBR)          | §Eval-first        | ✓ both variants |
| Layer 2 QEMU harness (NTFS)         | §Eval-first        | TODO |
| Layer 3 fixture build script        | §Real-content      | ✓ `scripts/build_l3_fixtures.sh` (XP + Win 7) |
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
- **Next session candidate: L3 smoke test signal detection.** Fixtures
  are staged; what's missing is a way to tell whether QEMU is running
  *our PBR's error halt* vs *real NTLDR / bootmgr code*. Leading approach:
  `qemu -trace bdrv_aio_readv` and gate on disk-read count (real loaders
  self-load → many reads; our PBR alone does ~3). Alternates: `pmemsave`
  + string grep, VGA screendump. Pick one in a focused session.
- **Open data point from this session's L1 multi-sector oracle:** ms-sys
  `--fat32pe` populates sectors 0, 1, 2, 6, 12 — not "0/1/12" as the old
  TODO speculated. Stage-2 code lives in sectors 2/6/12; sector 1 carries
  only 11 non-zero bytes. Affects any future multi-sector layout debate.
