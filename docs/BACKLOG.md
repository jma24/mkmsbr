# bootrec v1.0 backlog

Where we are vs. what `docs/SPEC.md` calls v1.0. Internal-honest tone:
"shipped" means the eval gates the spec required for that row are
green; "unproven" means tested only against synthetic loaders, not
real Microsoft files; "blocked" means waiting on external input.

The spec's 14-week timeline (§Timeline estimate) was conservative —
the eval-first methodology is paying compounding interest. As of
this writing, 4 of 5 variants ship at their `boot-asm/` Layer-2
target in a single development arc.

Last updated: 2026-05-18, after `ntfs_pbr_bootmgr` (multi-sector)
landed USA fixups, $INDEX_ALLOCATION B+tree handling (linear scan over
every INDX block in every data run), $MFT extent chasing (run table
populated from $MFT record 0's $DATA), and $INDEX_ROOT inline scanning.
All four pre-existing "known L2 limitations" are addressed; stage 2 is
943/1024 bytes, L2 still green in ~16 s.

## Variant matrix

Per `docs/SPEC.md` §Component breakdown. "L1" = byte-distance vs ms-sys
oracle (informational, gated by `< SUSPICIOUSLY_LOW` Hamming threshold).
"L2" = QEMU smoke against fake loader. "L3" = QEMU smoke against real
Microsoft files. "L4" = real-hardware boot.

| Variant                     | L1                       | L2 (fake)  | L3 (real) | L4 (HW) | Spec target | Status |
|-----------------------------|--------------------------|------------|-----------|---------|-------------|--------|
| `mbr_xp`                    | ✓ 373/440 vs `--mbr`     | ✓          | n/a       | —       | L1+L2       | shipped at spec target |
| `mbr_win7`                  | ✓ 396/440 vs `--mbr7`    | ✓          | n/a       | —       | L1+L2       | shipped at spec target |
| `fat32_pbr_ntldr`           | ✓ 398/423 vs `--fat32nt` | ✓          | ✓ 990 reads | —     | L1+L2+L3    | shipped at spec target |
| `fat32_pbr_bootmgr` (single)| ✓ 392/423 vs `--fat32pe` | ✓          | unproven  | unproven| —           | legacy / smoke baseline |
| `fat32_pbr_bootmgr` (multi) | ✓ ≥378/512 vs `--fat32pe` s1..15 | ✓    | ✓ 1520 reads | —  | L2+L3+L4    | L4 pending |
| `ntfs_pbr_bootmgr` (multi)  | TODO                     | ✓ (ntfs-3g, all limitations addressed) | unproven  | —       | L2+L3       | L2 green; L1 + L3 pending |

The single-sector `fat32_pbr_bootmgr` is kept as a smoke-test baseline.
The multi-sector variant is the v1.0 target (`docs/SPEC.md` line 132).

## Per-variant remaining work

### `fat32_pbr_ntldr` to spec target
- ~~L3 smoke test~~ — **shipped** in `tests/qemu_pbr_real.rs`. Gates on
  guest block-read count via `qemu -trace blk_co_preadv`; threshold 50
  reads is well above any error-halt path. Observed: 990 reads against
  real XP NTLDR + NTDETECT.COM (≈490 sectors of NTLDR loaded by our PBR,
  ≈500 more issued by NTLDR after handoff).

### `fat32_pbr_bootmgr` multi-sector to spec target
- ~~L3 smoke test~~ — **shipped** alongside the ntldr L3. Observed: 1520
  reads against real Win 7 bootmgr + BCD (≈750 sectors of bootmgr loaded
  by our PBR, ≈770 more from bootmgr's own self-load + BCD walk). Open
  question from the L1 multi oracle — "does our 2-sector layout satisfy
  the real bootmgr contract or do we need ms-sys's full 0/1/2/6/12
  layout?" — is now resolved empirically: 2 sectors is enough to reach
  bootmgr's BCD-reading phase.
- **L4 real-hardware verification.** Dell E6410 + 2010-2015 Intel
  desktop + 2005-vintage P4. *Blocks: spec L4 target / 1.0 release.*

### `ntfs_pbr_bootmgr` to spec target
- ~~L2 NASM clean-room~~ — **shipped** as a multi-sector PBR
  (3 sectors: 512-byte stage 1 + 1024-byte stage 2). Stage 2 walks
  $MFT record 5, reads INDEX_ALLOCATION's first INDX block, scans for
  "BOOTMGR" (UTF-16, namespace-agnostic), then chases the matched
  record's $DATA runs into segment 0x2000. Validated against an
  ntfs-3g-formatted 16 MiB image under QEMU via
  `tests/qemu_ntfs_pbr.rs` (Docker is the macOS workaround for the
  missing `mkfs.ntfs`).
- **Known L2 limitations** (all addressed 2026-05-18; smoke-validated
  against the ntfs-3g L2 fixture, full real-volume validation gated on
  the L3 image arriving 2026-05-19):
  - ~~INDEX_ROOT inline path not implemented~~ — **shipped** 2026-05-18.
    Stage 2 now scans $INDEX_ROOT's inline entries first; if not found
    and the INDEX_HEADER's LARGE_INDEX flag (0x01) is clear, dies with
    'F' (small dir, no $INDEX_ALLOCATION to walk); if set, falls
    through into the existing $INDEX_ALLOCATION walk. Single code
    addition reuses the same entry-scan layout; converges on the
    common `.load_bootmgr` path before the $DATA walker.
  - ~~INDEX_ALLOCATION B+tree descent not implemented~~ —
    **shipped** 2026-05-18 as a linear scan over every INDX block in
    every data run of $INDEX_ALLOCATION. Avoids true sub-node descent:
    interior-node separator entries copy the leaf-level key, so a
    name surfaces in some block regardless of tree level. Assumes
    IndexBlockSize == ClusterSize (holds for ntfs-3g default + Win 7
    Setup's 4 KiB cluster layout). L2 still green.
  - ~~USA (Update Sequence Array) fixups skipped~~ — **shipped**
    2026-05-18 as `apply_fixups` in `sector1.asm`; called after every
    `read_mft_rec` and after the root INDX read. Restores the last
    2 bytes of each 512-byte sector from the in-record USA. L2 still
    green; the L2 fixture's BOOTMGR entry was before offset 510 so
    the fixup is a no-op there, but real Win 7 INDX entries straddle
    sector boundaries and would have been corrupted without it.
  - ~~$MFT's own data runs not walked~~ — **shipped** 2026-05-18.
    Init now reads MFT record 0, parses its $DATA runs into a small
    table at 0x7B20 (LCN + length per extent, terminator-zeroed),
    and `read_mft_rec` walks that table to map any record N → LBA.
    Bootstrapped with a synthetic huge entry at BPB.MftLcn so the
    record-0 read itself goes through the same code path. New error
    codes: 'M' = $MFT $DATA was resident; 'O' = record number past
    end of run table. L2 still green.
  - Resident $DATA unsupported — fake bootmgr must be padded past
    NTFS's resident-attribute threshold (~700 B) in the L2 test.
- **L1 oracle.** ms-sys `--ntfs` byte-distance comparison TODO.
- **L3 fixture** from a real Win 7 NTFS install. Same shape as the
  bootmgr L3, but the four "Known L2 limitations" above will need
  to be addressed first.
- *Blocks: spec L3 target.*

## Eval framework

| Item                                | Spec ref           | Status |
|-------------------------------------|--------------------|--------|
| Layer 1 oracle (ms-sys subprocess)  | §Verifiability     | ✓ MBR + PBR sector 0 |
| Layer 1 oracle for multi-sector PBR | §Verifiability     | ✓ — ms-sys populates sectors 0,1,2,6,12; best sector-1 match Hamming 378/512 |
| Layer 2 QEMU harness (FAT32 PBR)    | §Eval-first        | ✓ single + multi |
| Layer 2 QEMU harness (MBR)          | §Eval-first        | ✓ both variants |
| Layer 2 QEMU harness (NTFS)         | §Eval-first        | ✓ `tests/qemu_ntfs_pbr.rs` (Docker mkfs.ntfs + ntfscp fixture) |
| Layer 3 fixture build script        | §Real-content      | ✓ `scripts/build_l3_fixtures.sh` (XP + Win 7) |
| Layer 3 QEMU harness (read-count gate) | §Real-content   | ✓ `tests/qemu_pbr_real.rs` — gates on `blk_co_preadv` count > 50 |
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
- **L3 signal detection — resolved.** `qemu -trace blk_co_preadv,file=…`
  produces one line per guest read; counting lines and gating > 50 cleanly
  separates "PBR halted before chainload" (single- to double-digit reads
  in practice) from "real loader took over and self-loaded" (hundreds to
  thousands). QEMU 11 renamed the classic `bdrv_aio_readv`; the harness
  picks the first advertised name from a preference list so older qemu
  builds still work without code changes.
- **Open data point from the L1 multi-sector oracle:** ms-sys `--fat32pe`
  populates sectors 0, 1, 2, 6, 12 — not "0/1/12" as the old TODO
  speculated. Stage-2 code lives in sectors 2/6/12; sector 1 carries
  only 11 non-zero bytes. The L3 result above shows our 2-sector layout
  is enough to reach bootmgr's BCD-reading phase under QEMU; whether
  real hardware needs more (L4) is the remaining open question.
- **Next session candidates** now that `ntfs_pbr_bootmgr` is L2-complete:
  1. NTFS L3 fixture against the real Win 7 image (arriving 2026-05-19)
     — exercises USA fixups / multi-block scan / extent chasing /
     INDEX_ROOT inline against real Microsoft-formatted bytes for the
     first time.
  2. NTFS L1 ms-sys `--ntfs` oracle (last informational gap in the
     variant matrix).
  3. CI / packaging push (GitHub Actions workflow, `src/bin/bootrec.rs`,
     README install section, crates.io reservation) — none of which
     individually need bootrec internals knowledge.
  4. L4 hardware verification — gated on user, not on the codebase.
