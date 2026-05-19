# NTLDR PBR findings from usbwin XP integration — 2026-05-19

Hardware-test failures surfaced two bootrec-side bugs during usbwin's
first attempt at booting a Windows XP install USB on the reference
legacy-BIOS rig (Dell E6410).

The setup: `usbwin --type windows-xp --boot-record bootrec` produced
a USB that, on boot, displays "`2_`" (literal '2' top-left + cursor)
and halts. The diagnostic '2' is recognizable from this repo's PBR
error-printer convention (stage-2 read failure marker).

Hex dumps of the resulting USB are at `/tmp/xp_bootrec_pbr.hex` on the
user's machine; the relevant findings reproduced inline below.

## Bug 1 — `splice_fat32_pbr` (NTLDR variant) does not overwrite OEM ID

The on-disk PBR sector 0, bytes 3..10 (OEM ID), are `BSD  4.4` — the
default left by macOS `newfs_msdos`. ms-sys's `--fat32nt` overwrites
these bytes with `MSWIN4.1`; bootrec's `splice_fat32_pbr` preserves
them.

Inconsistent with the BOOTMGR (multi-sector) splice, which per this
repo's BACKLOG explicitly overwrites OEM to `MSWIN4.1`:

> "PBR OEM ID overwritten to `MSWIN4.1` in `splice_fat32_pbr_multi`.
>  Defensive; no clean-room concern."

Same fix should apply to the single-sector `splice_fat32_pbr` for
both NTLDR and BOOTMGR variants. Probably one-line: after the splice,
overwrite `output[3..11] = *b"MSWIN4.1"`.

Whether the wrong OEM is *causally* tied to the boot failure is
unclear. Real NTLDR likely doesn't OEM-allowlist (XP's NTLDR boots
fine from `BSD  4.4`-OEM floppies in some configurations) but the
defensive fix is essentially free.

## Bug 2 — NTLDR PBR fails first read on real hardware (the actual boot failure)

Disassembling `boot-asm/fat32_pbr_ntldr.asm` against the on-disk PBR
sector 0, the boot code at PBR offset 0x5A-0x18F does:

1. Save DL to `[0x7b00]` (boot drive number)
2. Compute FAT start LBA = `HiddenSec + RsvdSecCnt` from BPB at 0x7b08
3. Compute data area LBA = FAT_start + NumFATs * FATSz32 at 0x7b04
4. Compute root-dir LBA = data_area + (RootClus - 2) * SecPerClus at 0x7b0c
5. Build a DAP at memory 0x0000:0x0700 for LBA-ext read (fn 0x42), 1
   sector, buffer at es:di = 0x0000:0x0500
6. `mov dl, [0x7b00]; mov ah, 0x42; int 0x13` (the read at offset 0x18C)
7. On CF=1: `mov al, '2'; jmp` to the error printer

The user reports seeing `'2_'` (just the '2' and the cursor), so:

- Stage 1 (this single-sector PBR's only stage) executes
- Geometry math executes
- DAP is set up
- `int 0x13 fn 0x42` is issued
- It returns with CF set
- '2' prints
- Diagnostic halts

The Dell E6410 supports LBA-ext fine (bootrec's BOOTMGR PBR on
identical hardware boots Win 7 without trouble via the same fn 0x42).
So the read failure isn't an LBA-ext-rejection story like the 2005
Phoenix Award P4.

Candidate causes, in order of suspicion:

1. **Computed root-dir LBA is wrong on real-formatter geometry.** The
   formula `data_start + (RootClus - 2) * SecPerClus` assumes the BPB
   `RootClus` field is the absolute cluster number, which it should be,
   but worth verifying against a `xxd` of the BPB on the failing disk.
   macOS `newfs_msdos -F 32` may pick non-default BPB fields that
   surface a latent bug.
2. **DAP setup has a 32-bit-vs-64-bit-LBA bug.** The code at offset
   0x178 reads `eax = [0x7b0c]` (only the low 32 bits of root LBA),
   writes `[si+8] = eax` and `[si+0xC] = 0`. That's correct for
   <2 TB disks (which this is — 64 GB USB). But verify the DAP size
   byte at `[si]` is 0x10 (16 bytes; required by some BIOSes) and not
   accidentally `0x18`.
3. **es:di buffer overlap.** Buffer destination is `0x0000:0x0500`.
   The relocated MBR copy on macOS pipelines lives at `0x061b..0x07ff`
   (ms-sys MBR) but our bootrec MBR lives at `0x0600..0x07ff`. If the
   DAP buffer at `0x0500` overlaps the relocated MBR's data, the
   read can succeed but trash a critical structure. Note: the read
   *failed* (CF=1) so this is less likely the cause but worth ruling
   out.

## Bug 3 — Diagnostic printer has an off-by-one for the '2' code path

Independently of bug 2, the printer reached after `'2'` is set up
incorrectly: the jump at offset 0x195 is `eb 42` (signed +0x42), PC
after = 0x197, target = 0x1d9. But the print sequence `b4 0e bb 07 00
cd 10` (set teletype function, set color, INT 0x10) starts at offset
0x1d8 — so the jump lands one byte in, executing the `0e` byte as
`push cs` and then `bb 07 00 cd 10` with AH still holding the BIOS
return code from the failed `int 0x13`. The BIOS sees a bogus
function number and does nothing visible.

This explains why the user sees just `'2'` and not `'2'` + AH-hex +
LBA-hex like the BOOTMGR PBR's richer diagnostic produces. Fix is
either:

- Change `eb 42` to `eb 41` (jump one byte earlier to land on `b4`)
- Or restructure the error printer to set AH=0x0e *after* the jump
  target

Either way: getting AH and LBA on the screen on the next test cycle
is the biggest single multiplier on debugging speed. Win 7 took nine
P4 iterations partly because the diagnostic was rich (`R<AH><SPT>
<heads><DL>`); fix this before re-running the XP test.

## Test infrastructure suggestion

The L3 NTLDR test (`tests/qemu_pbr_real.rs`) currently passes at 990
sector reads against real Microsoft NTLDR — but per the existing
"L3 gate weakness" notes in BACKLOG.md, "loader started reading lots
of sectors" is not "loader succeeded." The hardware-failure mode here
(first read fails before NTLDR loads at all) is *upstream* of where
the L3 read-count gate fires, so this should have been caught by L2 —
unless the L2 fake-NTLDR fixture uses a BPB geometry distinct from
what `newfs_msdos -F 32` produces on macOS.

Worth checking: does L2 stage the same `newfs_msdos`-produced BPB
that usbwin's pipeline produces? If L2 uses an `mtools`-formatted
or synthetic BPB, geometry-dependent bugs in the LBA math will slip
past L2 even at full fidelity.

## What usbwin is doing in parallel

usbwin is shipping a separate fix to its XP `--boot-record=ms-sys`
path: don't run `ms-sys --mbr` for XP mode (its XP-era boot code
hardcodes drive 0x80 instead of preserving the BIOS-supplied DL,
which fails on E6410-style USB-HDD emulation). usbwin will use
bootrec's MBR for both XP backends; only the PBR backend is selected
by the `--boot-record` flag.

This is an unrelated bug from the same hardware-test session, fixed
independently in the usbwin tree. Filed here for awareness because
it influences which combinations of (MBR backend × PBR backend) are
worth testing as ms-sys / bootrec each evolve.
