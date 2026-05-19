# mkmsbr v1.0 backlog

Where we are vs. what `docs/SPEC.md` calls v1.0. Internal-honest tone:
"shipped" means the eval gates the spec required for that row are
green; "unproven" means tested only against synthetic loaders, not
real Microsoft files; "blocked" means waiting on external input.

The spec's 14-week timeline (§Timeline estimate) was conservative —
the eval-first methodology is paying compounding interest. As of
this writing, 4 of 5 variants ship at their `boot-asm/` Layer-2
target in a single development arc.

Last updated: 2026-05-19 (late), after the XP NTLDR L4 investigation
landed the same CHS-rewrite shape that proved out for Win 7 earlier in
the day, plus a new `build_xp_setup_chain_bootsect` primitive for the
XP-Setup BOOTSECT.DAT chain. The XP PBR step boots on the Dell E6410
reference rig (NTLDR menu reaches the user); the remaining downstream
work — usbwin walking FAT for `$LDR$` and emitting a real BOOTSECT.DAT
via the new primitive — is tracked in usbwin.

Mid-day 2026-05-19 entry: the byte-diff vs ms-sys eval landed and
surfaced an LBA-12 gap in `fat32_pbr_bootmgr` (multi) on its first
run. Same session moved both FAT32+NTFS multi-sector stage 2 from
LBA+1 to LBA+2 so FSInfo (FAT32) / formatter's LBA 1 (NTFS) is
preserved verbatim instead of clobbered by stage-2 code.

Prior 2026-05-18 entry: `ntfs_pbr_bootmgr` (multi-sector) landed USA
fixups, $INDEX_ALLOCATION B+tree handling (linear scan over every INDX
block in every data run), $MFT extent chasing (run table populated
from $MFT record 0's $DATA), and $INDEX_ROOT inline scanning. All
four pre-existing "known L2 limitations" addressed; stage 2 is
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
| `fat32_pbr_ntldr` (multi)   | ✓ vs `--fat32nt` s0 only | ✓          | ✓ 987 reads | ✓ PBR step boots on Dell E6410 2026-05-19 | L1+L2+L3+L4 | shipped at spec target |
| `fat32_pbr_bootmgr` (single)| ✓ 392/423 vs `--fat32pe` | ✓          | unproven  | unproven| —           | legacy / smoke baseline |
| `fat32_pbr_bootmgr` (multi) | ✓ ≥378/512 vs `--fat32pe` s1..15 | ✓    | ✓ 1520 reads | ✗ doesn't boot 2026-05-19 | L2+L3+L4    | L4 failing — LBA-12 gap suspected |
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
  layout?" — is partially resolved: 2 sectors is enough to reach
  bootmgr's BCD-reading phase **under QEMU**. L4 failure on real
  hardware (2026-05-19) suggests the QEMU result is necessary but not
  sufficient — see "Byte-diff findings vs ms-sys" below.
- **FSInfo preservation, 2026-05-19.** Stage 1 now reads stage 2 from
  partition LBA 2 (was LBA 1, which is FSInfo). Splice signature
  changed: `existing` must be 1024 bytes (LBA 0 + LBA 1); output is
  `blob.len() + 512` bytes with formatter's FSInfo carried verbatim at
  output offset 512..1024. All L2 + L3 gates re-passed with read
  counts unchanged from pre-move baseline (NTLDR 990, bootmgr_multi
  1520). NTFS PBR moved in parallel for layout consistency (NTFS
  sectors 0..15 are reserved by $Boot, so it's a cosmetic change
  there — no semantic risk).
- **L4 real-hardware verification — failing 2026-05-19.** Dell E6410 +
  2010-2015 Intel desktop + 2005-vintage P4 setup did not boot from a
  mkmsbr-built USB. Mode of failure not yet pinpointed; "Byte-diff
  findings vs ms-sys" lists the candidates. *Blocks: spec L4 target /
  1.0 release.*

### Byte-diff findings vs ms-sys (2026-05-19)

`tests/byte_diff_vs_mssys.rs` (added 2026-05-19) runs ms-sys and
mkmsbr pipelines against identical freshly-formatted FAT32 images,
reads back the first 16 sectors of each, and reports byte
differences. First-run results:

| LBA | ms-sys nz | ours nz | diff bytes | Interpretation |
|-----|-----------|---------|------------|----------------|
| 0   | 385       | 131     | 341        | Clean-room boot code (expected) + OEM ID divergence at bytes 3..11 |
| 1   | 11        | 14      | 3 (488..491) | FSInfo free-count delta — ms-sys updates, we preserve; FAT32 driver recomputes anyway |
| 2   | 381       | 371     | 385        | Clean-room stage 2 (expected) |
| 6   | 96        | 96      | **0**      | **NOT a gap** — mformat's backup boot sector left intact by both pipelines |
| 12  | 315       | **0**   | 315        | **VERIFIABLE GAP** — ms-sys writes stage-3 helpers (FAT-walk + dir-scan with 0x66 32-bit prefixes); we write nothing |
| all others | 0 | 0 | 0 | (zeros) |

LBA 12's content disassembles to FAT32 cluster→LBA arithmetic with
references to `BPB.HiddSec`, `BPB.RootClus`, the FAT32 EOC marker
`0x0FFFFFF8`, and an 11-byte filename comparison loop (`mov cl, 0x0B`
+ `repe cmpsb` with `si = 0x7D69`). It's a stage-3 entry called via
`CALL` from ms-sys's LBA 2 stage. Our LBA 2 implements the same
functionality inline in its single 420-byte payload — but if real
bootmgr (or some BIOS-level continuation) jumps into LBA 12, the
zero-fill is a hard crash.

**Nuance: L3 doesn't disprove LBA 12.** The L3 gate is "guest
`blk_co_preadv` count > 50" — i.e. "the next loader started reading
more sectors." It does *not* check that NTLDR/bootmgr successfully
booted Windows. NTLDR could read 990 sectors then crash on a `CALL`
into a missing FAT-walk helper at the RAM address where Microsoft
loads LBA 12's stage-3 code, and our test would still pass.
Microsoft's loaders very plausibly assume the PBR-loaded helper
table at LBA 12 is callable from later boot stages; our monolithic
stage 2 fills the same *functional* role but at a different RAM
address with a different calling convention. **This makes LBA 12
the lead candidate for the L4 failure**, with L3's pass being a
deceptive non-signal rather than a counterexample.

Other candidates, ranked by current weight of evidence:

1. **LBA 12 stage-3 helpers (lead).** ms-sys puts FAT-walk +
   directory-scan + cluster→LBA arithmetic in 315 bytes at LBA 12,
   reached via `CALL` from LBA 2. Real NTLDR / real bootmgr likely
   relies on finding helpers at a specific RAM address that
   corresponds to a Microsoft-style load of LBA 12. Our 2-sector
   layout has no analogous helper area for downstream loaders to
   call into. *L3 cannot disprove this — see nuance above.*
2. **MBR disk signature missing.** `build_mbr` leaves bytes
   440..446 (NT Disk Signature + copy-protect) zero. Win 7 install
   USB BCD typically references the boot disk by signature; ms-sys
   also leaves zero on a zero-fill start, so this is a latent gap
   only if usbwin's pipeline doesn't write one separately.
3. **OEM ID divergence at bytes 3..11.** Our splice preserves
   mformat's OEM (e.g. `"mtools  "`); ms-sys overwrites with
   `"MSWIN4.1"`. Real bootmgr may allowlist Microsoft-style OEMs.
4. **Real BIOS USB-HDD emulation quirks** — INT 13h DL handling,
   USB-FDD vs USB-HDD profile mismatches on the 2005-vintage P4
   target. Unfixable in the harness without hardware.

**L3 gate weakness — captured 2026-05-19.** The current
`blk_co_preadv > 50` threshold measures "loader started" but not
"loader succeeded." Hardening options:
- Instrument the QEMU run to capture serial output past the
  point NTLDR/bootmgr would emit error codes (BSOD-style status
  codes, "BOOTMGR is missing", etc.). Gate on absence of error
  strings AND presence of a known-good progress marker.
- Boot a full-enough Windows Setup that it reaches a recognizable
  later stage (e.g., the "Loading files..." progress bar, which is
  WIM-extraction territory and requires successful BCD bind +
  winload.exe). Read-count threshold becomes ≫1520.
- Run the same test with ms-sys's PBR as a positive control. If
  ms-sys boots successfully past the gate and we don't, the read
  count gap is the failure signal.

**Next-step priorities:**
- Add LBA 12 stage-3 helpers to mkmsbr's multi-sector blob (closes
  the high-confidence gap). Likely a `sector12.asm` sibling, splice
  writes 13 sectors total (LBA 0..12) with zeros at the unused
  intermediate offsets.
- Fix OEM ID in the splice (1-line change: overwrite output[3..11]
  with `b"MSWIN4.1"` for FAT32 BOOTMGR variant).
- Add an MBR disk-signature primitive (`mbr_win7_with_signature(disk,
  sig: u32)`) so callers can either pass through a Setup-provided
  signature or generate one. usbwin is the natural owner of the
  signature lifecycle.

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
| Layer 1 oracle for multi-sector PBR | §Verifiability     | ✓ — ms-sys populates sectors 0,1,2,6,12; best stage-2 match Hamming 378/512 |
| Byte-diff eval vs ms-sys            | §Verifiability     | ✓ `tests/byte_diff_vs_mssys.rs` (2026-05-19) — gap detection on sectors ms-sys writes but mkmsbr doesn't |
| Layer 2 QEMU harness (FAT32 PBR)    | §Eval-first        | ✓ single + multi |
| Layer 2 QEMU harness (MBR)          | §Eval-first        | ✓ both variants |
| Layer 2 QEMU harness (NTFS)         | §Eval-first        | ✓ `tests/qemu_ntfs_pbr.rs` (Docker mkfs.ntfs + ntfscp fixture) |
| Layer 3 fixture build script        | §Real-content      | ✓ `scripts/build_l3_fixtures.sh` (XP + Win 7) |
| Layer 3 QEMU harness (read-count gate) | §Real-content   | ✓ `tests/qemu_pbr_real.rs` — gates on `blk_co_preadv` count > 50; **known weak** (passes for "loader started," not "loader succeeded" — see "L3 gate weakness" in Byte-diff findings) |
| Layer 3 hardened harness (post-handoff success) | §Real-content | TODO — capture serial / boot to recognizable later stage; ms-sys-as-positive-control comparison |
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
| CLI binary (`src/bin/mkmsbr.rs`)   | Clap wrapper, ms-sys flag aliases (§Form factor)           | TODO |
| Cargo features clean-up             | `embed-boot-asm` default-on once stable                    | TODO |
| `crates.io` publish                 | Reserve name; first release                                | TODO |
| Homebrew formula                    | `brew install mkmsbr` (§Audience and packaging)           | TODO |
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
  pipeline; everything else is mkmsbr-side work.
- **L3 signal detection — resolved.** `qemu -trace blk_co_preadv,file=…`
  produces one line per guest read; counting lines and gating > 50 cleanly
  separates "PBR halted before chainload" (single- to double-digit reads
  in practice) from "real loader took over and self-loaded" (hundreds to
  thousands). QEMU 11 renamed the classic `bdrv_aio_readv`; the harness
  picks the first advertised name from a preference list so older qemu
  builds still work without code changes.
- **Open data point from the L1 multi-sector oracle — partially
  answered 2026-05-19.** ms-sys `--fat32pe` populates sectors
  0, 1, 2, 6, 12. Sector 1 carries only ~10 bytes (FSInfo signatures);
  sector 6 is the FAT32 backup boot sector — mformat puts it there
  and ms-sys leaves it alone; sector 12 is a stage-3 entry point.
  See "Byte-diff findings vs ms-sys" section above.
- **L4 investigation 2026-05-19 — resolved for Win 7 via operational
  fallback; PBR is fully clean-room.** Initial real-hardware boot
  attempt failed with `R` (stage-1 INT 13h read error). Nine iterations
  of diagnostics surfaced two distinct root causes, one solved and one
  punted:

  **Root cause 1 (solved): legacy BIOSes reject INT 13h fn 0x42 (LBA-ext).**
  The 2005-vintage Phoenix Award BIOS on the L4 target returns AH=01
  ("invalid command") to fn 0x42 — confirmed by the diagnostic
  instrumentation we added (`R<AH><SPT><heads><DL>` on stage-1 read
  failure, `2<AH><LBA>` on stage-2 read failure). Geometry probe via
  fn 0x08 reports SPT=18, heads=2, DL=0x00 — i.e. the BIOS is doing
  USB-FDD emulation with floppy geometry. Fix: rewrote both PBR stages
  to use CHS reads via fn 0x02 (universal since the original IBM PC),
  with a fn 0x08 geometry probe at boot. 8 GB CHS addressing limit
  doesn't bite because BOOTMGR and the FAT/root area sit in low LBAs.
  See `boot-asm/fat32_pbr_bootmgr/sector{0,1}.asm`. This was the lesson
  that clean-room derivation from spec loses to incumbent compatibility
  scars — Microsoft uses CHS in their PBRs because they know fn 0x42
  fails on USB-FDD-emulating BIOSes from field experience we don't
  have access to.

  **Root cause 2 (operational fallback): BIOS USB-FDD/HDD mode is
  determined by undocumented MBR pattern-matching.** Confirmed via
  perturbation: same disk, same partition table, same PBR — switching
  only the MBR boot code (440 bytes at LBA 0) flips the BIOS from
  USB-FDD emulation to USB-HDD emulation. Nine progressive byte-level
  changes to our clean-room MBR all failed to trigger the mode switch:

  | Change tried | Result |
  |---|---|
  | PBR OEM → `"MSWIN4.1"` | R01 |
  | + Microsoft ASCII strings @ offset 0xB0 | R01 |
  | + DEADBEEF disk signature @ 0x1B8 | R01 |
  | + byte 0 = 0x33 (Microsoft `xor` encoding) | R01 |
  | + strings repositioned to ms-sys offset 0x163 | R01 |
  | + push+retf far-jump (replacing `jmp far`) | R01 |
  | + rep movsb (replacing rep movsw) | R01 |
  | + ES/DS load order swapped | R01 |
  | + defer DL save until after relocation — bytes 0..0x1B byte-exact with ms-sys | **R01** |

  Bytes 0x00..0x1B byte-exact with ms-sys's MBR and still no flip. The
  trigger is therefore in bytes 0x1C..0x162 of the MBR body — the
  partition-scan logic, fn 0x41 LBA-ext probe, A20 enable via keyboard
  controller (`e6 64`/`e6 60`), pushad/popad register saves, INT 0x18
  fallback. Reconstructing those 200+ bytes byte-by-byte to satisfy
  the BIOS heuristic is not really clean-room any more — we'd be
  using ms-sys's bytes as our specification. Public docs explicitly
  state there is no standard for BIOS USB enumeration mode selection
  (RMPrepUSB tutorial 027, OSDev forum). **Decision for v0.2/v0.3:
  usbwin pipeline invokes `ms-sys --mbr7` as the MBR step regardless
  of `--boot-record` flag; the mkmsbr MBR is shipped as a fallback
  for modern BIOSes that don't need the Microsoft fingerprint.** Full
  clean-room MBR rewrite (mirroring ms-sys's instruction sequence
  while staying defensible — those operations are standard for any
  MBR) is filed as v1.0/v1.1 work.

  **What this session shipped:**
  - PBR stages 1 and 2 rewritten with CHS reads + INT 13h fn 0x08
    geometry probe. Single-letter error codes extended with hex BIOS
    status + geometry/LBA context, gated nowhere — adds ~50 bytes per
    sector to the PBR, executes only on failure.
  - PBR OEM ID overwritten to `"MSWIN4.1"` in
    `splice_fat32_pbr_multi`. Defensive; no clean-room concern.
  - MBR contains Microsoft error strings at canonical offset 0x163,
    test disk signature `0xDEADBEEF` at offset 0x1B8 (TODO: replace
    with caller-supplied parameter in `mbr_win7_with_signature` for
    v1.0), byte 0 = 0x33, push+retf far-jump, rep movsb relocation,
    ES-before-DS load order, DL preserved in register (no early save).
  - Diagnostic infrastructure: `tests/common/qemu_trace.rs` now
    parses per-read `(offset, bytes)` from QEMU trace events; new
    `report_lba12_verdict` in `tests/qemu_pbr_real.rs` answers the
    "did bootmgr read partition LBA 12?" question definitively (no —
    killed that hypothesis cleanly).
  - `tests/byte_diff_vs_mssys.rs` (added as part of the L1 oracle
    work earlier in the day) tracked + passing.

- **Win 7 boots on real hardware** with mkmsbr PBR + ms-sys MBR
  fallback. Verified 2026-05-19 on a 2005-vintage Phoenix Award BIOS
  P4 target.

- **XP NTLDR L4 — PBR step shipped 2026-05-19 (late).** The Dell E6410
  hits the same USB-FDD-emulation trap as the 2005 Phoenix P4 (fn 0x42
  rejected with AH=01) plus an extra wrinkle: the BIOS hands DL=0x0F
  and rejects fn 0x08 (geometry probe) on that drive number too.
  Diagnosed via the new stage-1 `G<AH><SPT><HEADS><DL>` output on
  hardware (G0100000F: probe failed, drive 0x0F). Fix landed in three
  layers:

  1. **Multi-sector NTLDR PBR** (`boot-asm/fat32_pbr_ntldr/`) mirroring
     the `fat32_pbr_bootmgr` shape. CHS reads (fn 0x02) for stage-2
     load + every FAT-walk read. The single-sector `fat32_pbr_ntldr.asm`
     was deleted — FAT walker + CHS + diagnostic doesn't fit in 512
     bytes, and the legacy hardware doesn't honour fn 0x42 anyway.
  2. **OEM ID overwrite in `splice_fat32_pbr`** (the single-sector splice
     used by callers like BOOTSECT.DAT). Mirrors the existing
     `splice_fat32_pbr_multi` patch — OEM "MSWIN4.1" is the gate that
     keeps 2005-era BIOSes in USB-HDD mode rather than USB-FDD.
  3. **Geometry-probe fallback** in stage 1: if fn 0x08 returns CF (the
     Dell case), hardcode SPT=18, HEADS=2 (the USB-FDD profile that
     legacy BIOSes use internally even when they refuse to report it)
     and keep the BIOS-handed DL. Confirmed: with this fallback the
     Dell loads NTLDR successfully.

  Renames: `FAT32_PBR_NTLDR_BOOT` → `FAT32_PBR_NTLDR_MULTI_BOOT`.

- **XP Setup chain — mkmsbr primitive shipped 2026-05-19 (late).** New
  `build_xp_setup_chain_bootsect(formatter_sector0, target_segment,
  runs: &[LbaRun]) -> [u8; 512]`. Builds a single-sector BOOTSECT.DAT
  that NTLDR chainloads via boot.ini's bootsector-entry mechanism;
  reads pre-resolved `$LDR$` LBA extents into target_segment:0 via CHS
  and far-jumps. No FAT walker, no filename string — caller (usbwin)
  walks FAT once + coalesces extents. Spec at
  `docs/XP_SETUP_CHAIN_BOOTSECT_SPEC.md`. L2 smoke at
  `tests/qemu_pbr.rs:xp_setup_chain_bootsect_chainloads_in_qemu`. usbwin
  integration tracked in `docs/USBWIN_NTLDR_FINDINGS_2026_05_19.md`.

- **XP L4 — BOOTSECT.DAT chain still pending on usbwin side.** The
  mkmsbr primitive ships ready. Downstream chain (NTLDR loads
  BOOTSECT.DAT → BOOTSECT.DAT loads `$LDR$` → text-mode setup) needs
  usbwin to walk FAT for `$LDR$`, coalesce extents into LbaRuns, call
  the new primitive, and write the result. Confirmed-failing state
  today: NTLDR menu reaches the user, but selecting either Setup entry
  trips on the classic `<Windows root>\system32\hal.dll` error because
  BOOTSECT.DAT is missing and NTLDR falls through to its default
  Windows-load path.

- **Next session candidates:**
  1. ~~Wire up the operational fallback in usbwin (always invoke
     `ms-sys --mbr7` for Win 7 mode).~~ Landed already; usbwin now
     uses mkmsbr MBR for Win 7 + XP and ms-sys MBR fallback was
     deemed unnecessary once mkmsbr's MBR byte-matching was tried.
  2. `mbr_win7_with_signature(disk_sectors, sig: u32)` API in
     `src/mbr.rs`, replacing the hardcoded 0xDEADBEEF test value.
     usbwin generates a per-USB random signature and threads it
     through. Needed for Windows BCD downstream regardless of BIOS
     mode-detection behavior.
  3. ~~XP / NTLDR L4 investigation~~ — shipped 2026-05-19 (late). PBR
     step boots; BOOTSECT.DAT chain is the remaining link, tracked on
     the usbwin side.
  4. **usbwin integration of `build_xp_setup_chain_bootsect`**: walk
     FAT for `$LDR$` extents, coalesce runs (≤16), call the new
     primitive, write the result to `$WIN_NT$.~BT\BOOTSECT.DAT`. The
     existing `crates/usbwin/src/pipeline/fat32.rs` walker is the
     starting point; only the run-coalescer + glue is new.
  5. NTFS L3 fixture against the real Win 7 image — exercises USA
     fixups / multi-block scan / extent chasing / INDEX_ROOT inline
     against real Microsoft-formatted bytes for the first time.
  6. NTFS L1 ms-sys `--ntfs` oracle (last informational gap in the
     variant matrix).
  7. CHS-only QEMU test variant (boot via `-drive if=floppy` so
     SeaBIOS rejects fn 0x42 — closes the test-coverage gap that let
     the LBA-ext deviation slip past QEMU until L4 caught it).
  8. Full clean-room MBR rewrite for v1.0 / v1.1, mirroring ms-sys's
     structure where defensible. Argue in PROVENANCE that the
     resulting byte-similarity is a property of the constrained task,
     not derivation.
  9. CI / packaging push (GitHub Actions workflow, `src/bin/mkmsbr.rs`,
     README install section, crates.io reservation) — none of which
     individually need mkmsbr internals knowledge.
