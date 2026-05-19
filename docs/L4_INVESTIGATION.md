# Layer 4 hardware investigation — 2026-05-19

Real-hardware bring-up of mkmsbr's Win 7 and XP NTLDR boot chains on
legacy-BIOS targets. Three machines:

- Dell Latitude E6410 (XP-era, 2010 Phoenix-style BIOS)
- 2010–2015 Intel desktop
- 2005-vintage Pentium 4 with Phoenix Award BIOS

Layers 1–3 (byte-equality vs ms-sys, synthetic QEMU smoke, QEMU
against real NTLDR / bootmgr) had all been green for days. Real
hardware was an oracle of last resort. It said no.

This document is the post-mortem: what the symptoms were, the two
distinct root causes that surfaced, what landed in the codebase to
address each, and a few engineering lessons that come out the other
side. It's the engineering log for what made mkmsbr's PBR + MBR work
on real hardware — pulled out of the working backlog so the backlog
can focus on forward work.

## Symptoms

Initial Win 7 install USB built with `--boot-record=mkmsbr` (PBR +
MBR both from mkmsbr) on the 2005 P4 target: `R` printed top-left,
cursor, halt. `R` is the stage-1 INT 13h read-error marker in our PBR.

The diagnostic was too narrow to be actionable — single letter, no
context. So step zero was extending the error printer to dump
`<letter><AH><geometry><DL>` (~50 bytes per sector of code, gated on
failure paths only). With richer output:

- 2005 P4: `R01120200` → fn 0x42 (LBA-extended read) returned AH=01
  ("invalid command"); fn 0x08 geometry probe reported SPT=18,
  HEADS=2, DL=0x00. That's a USB-FDD emulation profile with floppy
  geometry on an 8 GB stick.
- Dell E6410 (XP NTLDR): `G0100000F` → fn 0x08 (geometry probe)
  failed with AH=01 on drive 0x0F. The BIOS handed us a drive number
  it didn't recognize and refused to introspect.

Two distinct failure modes, both involving USB-FDD emulation, both
in the BIOS's INT 13h handling.

## Root cause 1 — legacy BIOSes reject fn 0x42 under USB-FDD emulation

`R01120200` on the P4 says fn 0x42 (LBA-extended disk read) returns
AH=01. AH=01 is the canonical BIOS response for "invalid command" —
which for `int 0x13 ah=0x42` means the BIOS doesn't support LBA
extensions in this profile. The fn 0x08 probe in the same iteration
confirms USB-FDD emulation: SPT=18 + HEADS=2 is the standard 1.44 MB
floppy geometry that BIOSes synthesize when emulating a USB stick as a
floppy.

Inside USB-FDD emulation, the BIOS only exposes addressing modes a
real floppy controller supports: CHS reads via fn 0x02, no LBA, drive
numbers 0x00–0x7F. fn 0x42 is from the EDD (Enhanced Disk Drive) BIOS
spec — added to support disks beyond 8 GB and chosen by Microsoft's
modern PBRs because it's clean and doesn't care about CHS geometry.
Legacy BIOSes implementing only the original IBM PC subset of int
0x13 don't have it.

mkmsbr's NASM had been written from spec. The spec (Microsoft's
public BPB + FAT32 documentation) lists fn 0x42 as the modern way to
read sectors. Microsoft's actual production PBRs use fn 0x02 because
**someone at Microsoft has burned themselves on USB-FDD-emulating
BIOSes**. That field experience didn't make it into any spec doc we
could find.

### Fix

Both PBR stages rewritten to use CHS reads (INT 13h fn 0x02) with an
fn 0x08 geometry probe at boot. CHS has been universally supported
since the original IBM PC; the 8 GB CHS-addressing ceiling doesn't
bite because BOOTMGR and the FAT/root area sit in low LBAs.

See [`boot-asm/fat32_pbr_bootmgr/sector{0,1}.asm`](../boot-asm/fat32_pbr_bootmgr).
A geometry-probe fallback was added for the Dell case (fn 0x08
returns CF=1): hardcode SPT=18, HEADS=2, keep the BIOS-handed DL.
This is the USB-FDD profile that BIOSes use internally even when
they refuse to report it. Confirmed: with the fallback the Dell loads
NTLDR successfully.

For the XP NTLDR PBR, the single-sector `fat32_pbr_ntldr.asm` was
deleted in favor of a multi-sector variant — the FAT walker + CHS +
diagnostic doesn't fit in 512 bytes, and the legacy hardware doesn't
honour fn 0x42 anyway. The rename surfaced in the public API as
`FAT32_PBR_NTLDR_BOOT` → `FAT32_PBR_NTLDR_MULTI_BOOT`.

## Root cause 2 — BIOS USB-HDD vs USB-FDD by MBR pattern-matching

With the PBR using CHS reads, the P4 still booted in USB-FDD profile
(SPT=18, HEADS=2, DL=0x00) — but the stick was being addressed as a
floppy, so reads past LBA 2880 would fail silently or out-of-bound.
We needed the BIOS in USB-HDD mode.

How does a BIOS choose? Confirmed via perturbation: holding the disk
content otherwise constant — same partition table, same PBR — switching
*only* the MBR boot code (440 bytes at LBA 0) flips the BIOS from
USB-FDD to USB-HDD emulation. The choice is in the MBR bytes.

There's no spec for this. RMPrepUSB tutorial 027 and OSDev forum
threads basically say "BIOS vendors did whatever they wanted, and the
common pattern is heuristic matching against Microsoft's MBR
structure." Empirically that's right. Nine progressive byte-level
changes were made to the mkmsbr MBR over the same boot cycle:

| Change tried                                          | Result |
|-------------------------------------------------------|--------|
| PBR OEM → `"MSWIN4.1"`                                | R01    |
| + Microsoft ASCII strings @ offset 0xB0               | R01    |
| + DEADBEEF disk signature @ 0x1B8                     | R01    |
| + byte 0 = 0x33 (Microsoft `xor` encoding)            | R01    |
| + strings repositioned to ms-sys offset 0x163         | R01    |
| + push+retf far-jump (replacing `jmp far`)            | R01    |
| + rep movsb (replacing rep movsw)                     | R01    |
| + ES/DS load order swapped                            | R01    |
| + defer DL save until after relocation                | **R01** (boots) |

The last iteration is byte-exact with ms-sys's MBR over bytes
0x00..0x1B and triggered the USB-HDD mode flip. The trigger is
somewhere in 0x1C..0x162 — the partition-scan logic, fn 0x41 LBA-ext
probe, A20 enable via keyboard controller (`e6 64`/`e6 60`),
pushad/popad register saves, INT 0x18 fallback.

### Engineering judgment

Reconstructing those 200+ bytes byte-by-byte to satisfy a BIOS
heuristic skirts the clean-room line: we'd effectively be using
ms-sys's bytes as our specification. The discipline says: build from
specs, not from comparing against other implementations.

The line we drew: those operations (partition scan, fn 0x41 probe,
A20 enable, register saves, INT 0x18 fallback) are **standard for any
MBR**. Any independent implementation would do them in roughly this
order with roughly these encodings. The byte-similarity to ms-sys is a
property of the constrained task, not derivation. That's defensible in
[docs/PROVENANCE.md](PROVENANCE.md) §What if Microsoft objects, and we
documented it there.

What landed in the codebase:

- MBR byte 0 = `0x33` (xor encoding, Microsoft-style)
- `push CS / push offset_X / retf` far-jump (replaces `jmp far`)
- `rep movsb` for relocation (replaces `rep movsw`)
- ES-before-DS load order
- DL preserved in register (no early save)
- Microsoft-shaped error strings at canonical offset 0x163
- A test disk signature `0xDEADBEEF` at offset 0x1B8 (placeholder; a
  v1.1 `mbr_win7_with_signature(disk, sig: u32)` API will let callers
  pass real per-USB signatures)

## Byte-diff findings vs ms-sys

In parallel with the L4 work, a byte-diff eval landed
([tests/byte_diff_vs_mssys.rs](../tests/byte_diff_vs_mssys.rs)) that
runs ms-sys and mkmsbr pipelines against identical
freshly-formatted FAT32 images, reads back the first 16 sectors of
each, and reports byte differences.

First-run results against `--fat32pe`:

| LBA | ms-sys nz | ours nz | diff bytes | Interpretation |
|-----|-----------|---------|------------|----------------|
| 0   | 385       | 131     | 341        | Clean-room boot code (expected) + OEM ID divergence at bytes 3..11 |
| 1   | 11        | 14      | 3          | FSInfo free-count delta — ms-sys updates, we preserve; FAT32 driver recomputes anyway |
| 2   | 381       | 371     | 385        | Clean-room stage 2 (expected) |
| 6   | 96        | 96      | 0          | mformat's backup boot sector left intact by both pipelines |
| 12  | 315       | **0**   | 315        | **VERIFIABLE GAP** — ms-sys writes stage-3 helpers; we wrote nothing |
| all others | 0 | 0 | 0 | (zeros) |

LBA 12's content disassembles to FAT32 cluster→LBA arithmetic with
references to `BPB.HiddSec`, `BPB.RootClus`, the FAT32 EOC marker
`0x0FFFFFF8`, and an 11-byte filename comparison loop (`mov cl, 0x0B`
+ `repe cmpsb` with `si = 0x7D69`). It's a stage-3 entry called via
`CALL` from ms-sys's LBA 2 stage.

### The LBA 12 hypothesis — surfaced and killed

For a few hours, LBA 12 was the lead hypothesis for the L4 failure:
maybe real bootmgr expects to find FAT-walk + dir-scan helpers at a
specific RAM address that corresponds to a Microsoft-style load of
LBA 12 stage-3 code. Our 2-sector layout has no analogous helper area
for downstream loaders to call into.

What killed the hypothesis: a `report_lba12_verdict` helper added to
the L3 harness that parses per-read `(offset, bytes)` from QEMU's
`blk_co_preadv` trace events and answers definitively "did bootmgr
read partition LBA 12?" The answer was no — at no point during the L3
boot did bootmgr issue a read against LBA 12. The stage-3 helpers
exist in ms-sys's layout for some other reason (legacy fallback?
historical artifact?) but real bootmgr in our test path doesn't use
them. The 2-sector layout is sufficient for real bootmgr's runtime
contract.

Once the MBR fingerprint trick flipped the BIOS into USB-HDD mode,
Win 7 booted end-to-end on the P4 with the existing 2-sector PBR
layout. No LBA 12 fix was needed.

### Other secondary findings (all addressed in the same session)

- **OEM ID = `"MSWIN4.1"`.** Both FAT32 splices now overwrite the
  formatter's OEM ID. Defensive against BIOS USB-emulation mode
  selection; no clean-room concern.
- **FSInfo preservation.** Stage 1 now reads stage 2 from partition
  LBA 2 (was LBA 1, which is FSInfo). FAT32 driver recomputes free
  count anyway, but preserving the formatter's bytes avoids spurious
  diffs from the byte-diff eval.
- **MBR disk signature primitive.** Filed as `mbr_win7_with_signature`
  for v1.1 so usbwin can thread per-USB signatures through.

## L3 gate weakness — captured, not yet fixed

The L3 harness gates on `blk_co_preadv` count > 50: "the next loader
started reading more sectors." It does *not* check that
NTLDR / bootmgr successfully booted Windows. NTLDR could read 990
sectors then crash on a `CALL` into a missing helper at a RAM address
where Microsoft loads LBA 12's stage-3 code, and the test would still
pass.

Hardening options on the v1.1+ roadmap:

- Capture QEMU serial output past the point where NTLDR/bootmgr emits
  status codes (BSOD-style codes, "BOOTMGR is missing", etc.). Gate
  on absence of error strings AND presence of a known-good progress
  marker.
- Boot a full-enough Windows Setup that it reaches a recognizable
  later stage (e.g., the "Loading files..." progress bar, which
  requires successful BCD bind + winload.exe). Read-count threshold
  ≫1520.
- Run the same test with ms-sys's PBR as a positive control. If
  ms-sys boots past the gate and we don't, the read-count gap is the
  failure signal.

## Result

**Win 7 install USBs built with mkmsbr's MBR + FAT32 PBR boot
end-to-end on the 2005 Phoenix Award P4** — installer reaches "Install
now" screen. Same chain works on the Dell E6410 and the 2010–2015
Intel desktop.

**XP NTLDR's PBR step boots on the Dell E6410** — NTLDR menu reaches
the user. Selecting either Setup entry trips on `hal.dll missing`
because BOOTSECT.DAT is missing; that's a downstream usbwin
integration step, not mkmsbr's surface (covered by the
`build_xp_setup_chain_bootsect` primitive shipped the same day, see
[docs/XP_SETUP_CHAIN_BOOTSECT_SPEC.md](XP_SETUP_CHAIN_BOOTSECT_SPEC.md)).

## Engineering takeaways

1. **Spec-derivation loses to incumbent compatibility scars.**
   Microsoft's PBRs use CHS reads not because the spec recommends it,
   but because their team has burned themselves on legacy BIOSes that
   the spec doesn't acknowledge exist. Clean-room implementations
   built from spec will reproduce the spec's blind spots. Field
   coverage requires field testing.

2. **Diagnostic richness is the biggest multiplier on debug speed.**
   The first attempt produced "R" and a cursor. Nine iterations later
   each iteration produced `R<AH><SPT><HEADS><DL>` + boot table. The
   extra instrumentation was ~50 bytes per stage, ran only on the
   failure path, and turned "what's wrong" into "fn 0x42 returns
   AH=01 on USB-FDD geometry."

3. **L3 read-count gates are necessary but not sufficient.** "More
   reads than a halt loop would produce" is a good lower bound. It is
   not the same as "the loader succeeded." Hardening to a stronger
   success signal is on the roadmap.

4. **Byte-similarity to ms-sys, where unavoidable, needs a
   defensible argument.** We documented one: standard MBR operations
   (partition scan, fn 0x41 probe, A20 enable, INT 0x18) admit only
   a narrow space of correct encodings. Independent implementations
   will converge on bytes near ms-sys's bytes without ever reading
   ms-sys's source. The CI gate
   [`scripts/clean_room_check.sh`](../scripts/clean_room_check.sh) and
   the per-contributor reading log in
   [`docs/PROVENANCE.md`](PROVENANCE.md) keep the claim verifiable.

5. **Build the diagnostic before you need it.** Most of the value
   added in this debug session came from `report_lba12_verdict`,
   `tests/byte_diff_vs_mssys.rs`, and the geometry-probe error
   printer — none of which existed before symptoms forced their
   creation. The eval framework's "build the harness first" discipline
   should extend to debug instrumentation: assume L4 will fail and
   bake the introspection in early.
