# Spec request from usbwin — `build_xp_setup_chain_bootsect`

This is a request for a new mkmsbr public function. usbwin needs it to
finish the WinSetupFromUSB XP boot chain. Without it, the chain breaks
at the "BOOTSECT.DAT loads `$LDR$`" step and NTLDR falls through to its
default Windows-load path → classic
`<Windows root>\system32\hal.dll missing` error.

## Context

XP-USB boot chain (WinSetupFromUSB-style):

```
MBR  → PBR  → NTLDR  → boot.ini  → BOOTSECT.DAT  → $LDR$  → text-mode setup
 ✓     ✓      ✓        ✓           NEEDED          ✓        (works once chain is intact)
```

`BOOTSECT.DAT` is a single 512-byte file at `\$WIN_NT$.~BT\BOOTSECT.DAT`
on the FAT32 partition. NTLDR's "bootsector entry" mechanism loads it
to `0x0000:0x7C00`, jumps to it, and that sector is expected to do the
work of finding and loading `$LDR$` (a 5-char-renamed copy of
`setupldr.bin`, ~260 KB).

The naïve approach — patch a copy of the partition's PBR with
`NTLDR` → `$LDR$` — worked historically for single-sector PBRs that
embedded the filename string in sector 0. mkmsbr's current
`fat32_pbr_ntldr` is multi-sector (per the 2026-05-19 CHS rewrite) and
puts the literal `NTLDR` filename string at offset 0x5D0 (sector 2 /
stage 2), unreachable from a single-sector BOOTSECT.DAT load. So the
patch-the-PBR approach can't work for mkmsbr's PBR.

The clean fix: usbwin walks FAT in advance (already implemented in
`crates/usbwin/src/pipeline/fat32.rs`), finds `$LDR$`'s LBAs, and hands
them to a mkmsbr function that emits a single-sector loader. The loader
doesn't need a FAT walker, doesn't need a filename string — it just
CHS-reads pre-resolved LBAs and jumps. That fits in 512 bytes with room
to spare.

## API

```rust
/// Build a single-sector BOOTSECT.DAT that NTLDR loads via boot.ini's
/// bootsector-entry mechanism. When chainloaded to 0x0000:0x7C00, this
/// sector CHS-reads `$LDR$`'s contents from the given LBA runs into
/// memory at `target_segment:0x0000` and far-jumps there.
///
/// No FAT walker, no filename lookup — usbwin walks FAT in advance and
/// hands us pre-resolved LBAs. Just geometry-probe + CHS-read +
/// far-jmp + the standard error strings.
///
/// `formatter_sector0` is the partition's existing sector 0 — we preserve
/// the BPB at bytes 3..90 and the 0xAA55 boot signature so the resulting
/// 512-byte blob looks like a valid FAT boot sector to anything that
/// inspects it (even though it doesn't use the BPB at runtime). HiddSec
/// at BPB offset 0x1C IS read at runtime to convert partition-relative
/// LBAs to absolute.
pub fn build_xp_setup_chain_bootsect(
    formatter_sector0: &[u8; 512],
    target_segment: u16,          // canonical: 0x2000 (setupldr's expected load addr)
    runs: &[LbaRun],              // partition-relative LBAs, coalesced
) -> Result<[u8; 512], Error>;

#[derive(Debug, Clone, Copy)]
pub struct LbaRun {
    /// Partition-relative starting LBA (i.e. LBA within the FAT32
    /// partition; not absolute disk LBA). The bootsector adds BPB.HiddSec
    /// at runtime to get the absolute disk LBA for INT 13h.
    pub start_lba: u32,
    /// Number of consecutive sectors in this run.
    pub sector_count: u16,
}
```

### Errors

- `runs` empty → error ("nothing to load").
- `runs.len()` exceeds whatever cap the implementation picks (suggest 8
  or 16 — see below) → error with a "fragmentation too high" message so
  usbwin can fall back or surface to the user.
- `formatter_sector0` lacks 0xAA55 signature at offset 0x1FE → error.
- `target_segment` < 0x0050 or > 0x9000 → error (sanity).
- Total sectors > some bound (256 = 128 KB? probably need ~520 for a
  260 KB `$LDR$`, so pick maybe 1024) → error.

### Why `LbaRun` instead of a flat `[u32]`

`$LDR$` ≈ 260 KB ≈ 520 sectors. A flat array of 520 × `u32` = 2 KB
won't fit in a 512-byte sector. FAT32 typically allocates a freshly-
staged file in 1–3 contiguous runs, so `LbaRun { u32 + u16 } = 6 bytes`
per run gives 6–18 bytes of LBA table, leaving ~480 bytes for boot code.

usbwin's `pipeline::fat32::find_file_extent` returns a flat LBA list;
usbwin coalesces consecutive LBAs into runs before calling this
function. If we get unlucky and a 260 KB file is fragmented into >8
runs, usbwin can either retry-with-different-formatter-params or fail
with a clear message — we don't expect this in practice because the
file is the very first thing on a freshly-formatted partition.

### Why partition-relative LBAs

Two equivalent choices:
1. **Partition-relative**: usbwin computes LBAs relative to the start
   of the partition (the natural output of a FAT walk). The bootsector
   adds `BPB.HiddSec` at runtime to get absolute disk LBAs for INT 13h.
2. **Absolute disk LBAs**: usbwin adds `HiddSec` before calling this.
   Simpler sector code, no BPB read at runtime.

Preference: (1) partition-relative. Matches how PBRs conventionally
work, keeps the bootsector self-contained (it knows where it is on
disk via the BPB), and the extra "add HiddSec" is ~6 bytes of code.

## Runtime behavior the emitted sector must implement

Pseudocode for the sector code (everything after the BPB at bytes 90+):

```
entry:
    cli
    xor ax, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    cld

    mov [boot_dl], dl          ; save BIOS drive

    ; Geometry probe via INT 13h fn 0x08.
    push es
    mov ah, 0x08
    int 0x13
    jc error_R                 ; print 'R', halt
    and cl, 0x3F               ; SPT in low 6 bits of CL
    mov [spt], cl
    inc dh                     ; heads = DH + 1
    mov [heads], dh
    pop es

    ; Load destination ES:DI = target_segment:0
    mov ax, target_segment
    mov es, ax
    xor di, di

    ; For each run in run_table:
    ;   abs_lba = run.start_lba + [bpb_hiddsec]  ; bpb_hiddsec at offset 0x1C
    ;   for i in 0..run.sector_count:
    ;     CHS-read 1 sector to ES:DI
    ;     add di, 512 (handle ES wrap at 64 KB)
    ;     inc abs_lba
    ; next run

    ; All runs loaded → far-jmp to target_segment:0
    mov dl, [boot_dl]          ; setupldr expects DL = boot drive
    db 0xEA                    ; jmp far
    dw 0x0000                  ; offset
    dw target_segment          ; segment

read_one_sector:
    ; Inputs: ABS_LBA in DX:AX, ES:DI = dest.
    ; CHS conversion:
    ;   sector = (LBA % SPT) + 1
    ;   tmp    = LBA / SPT
    ;   head   = tmp % heads
    ;   cyl    = tmp / heads
    ; Then INT 13h fn 0x02 with AL=1.
    ; On CF, error_2 (print '2', halt).
    ...
    ret

error_R / error_2:
    ; Print single char + AH (hex), halt.
    ...

run_table:
    ; Inlined at build time. Each LbaRun = 6 bytes:
    ;   dd start_lba
    ;   dw sector_count
    ; Terminated by a zero entry or count field.

boot_dl: db 0
spt:     db 0
heads:   db 0
```

Stage-2 fits comfortably in ~256 bytes of code + 6×N bytes of run
table. Standard error printing adds maybe 60 bytes. Plenty of room.

## Why this matches existing mkmsbr patterns

The CHS-read + geometry-probe + error-print scaffolding is the same shape
that `boot-asm/fat32_pbr_bootmgr/sector{0,1}.asm` already uses after
the 2026-05-19 P4 rewrite. This new function is structurally simpler than
those (no FAT walker, no multi-stage). Realistic implementation: ~150
lines of NASM, sharing most of its primitives with the existing PBR work.

## Testing the new function

Suggested L2 harness, analogous to existing PBR smoke tests:

1. usbwin (or the test) builds a synthetic FAT32 image:
   - Format a 16 MiB disk image (mformat / mkfs.vfat)
   - Write a 64 KB stub file at a known location with a payload that
     prints `BOOTSECT.DAT-OK\r\n` on COM1 and halts (or just `jmp $`)
   - Place a copy of an XP-shape PBR at sector 0 of the partition
2. Call `build_xp_setup_chain_bootsect`:
   - `formatter_sector0` = sector 0 of the synthetic partition
   - `target_segment` = 0x2000
   - `runs` = a single `LbaRun { start_lba: ..., sector_count: 128 }`
     pointing at the stub
3. Write the returned 512 bytes as BOOTSECT.DAT at the agreed FAT
   location (or just feed it directly to QEMU)
4. Boot under QEMU with the bootsector loaded as if NTLDR had done it
   (i.e. load it to 0x0000:0x7C00 and jump)
5. Gate on the `BOOTSECT.DAT-OK` marker on serial

Same `qemu -trace blk_co_preadv` read-count gate could be used as a
fallback (count > N), but the marker-string gate is tighter.

## What usbwin will do with this

Once mkmsbr ships this, usbwin's `pipeline/xp_staging.rs` will gain a
new code path:

```rust
match mkmsbr::build_xp_setup_chain_bootsect(
    &formatter_sector0_array,
    0x2000,
    &runs,
) {
    Ok(bytes) => xp_staging::write_bootsect_dat(&usb_mount, &bytes)?,
    Err(e) => {
        // Fall back to the existing patch-PBR approach (works for
        // ms-sys --fat32nt PBR; fails for mkmsbr NTLDR multi).
        // Eventually retire the fallback once this is the canonical path.
        ...
    }
}
```

with the LBA list coming from `pipeline::fat32::find_file_extent` +
a small run-coalescing helper.

## Out of scope (intentional)

- Loading anything other than a single contiguous-or-runs file.
- Looking up the file by name (caller pre-resolves).
- Long filenames / non-8.3 names (caller deals with FAT entry layout).
- Anything beyond XP setup. If Vista+ ever needs an analogous chain, it
  uses BCD, not boot.ini-bootsector-entries.

## Rough timeline ask

~1 hour of focused work by the agent who just rewrote
`fat32_pbr_bootmgr` for CHS — same patterns, simpler problem. usbwin's
side is already substantially built (see `pipeline/fat32.rs`); the only
remaining usbwin work is the run-coalescing helper (~20 lines) and the
glue to call the new function.

## Provenance note

The "BOOTSECT.DAT loads $LDR$" technique is documented in
WinSetupFromUSB / boot-land tooling (canonical implementation:
`github.com/ruo91/USB_MultiBoot/blob/master/USB_MultiBoot_10/makebt/MakeBS3.cmd`,
jaclaz 2007). Their approach byte-patched the existing PBR; ours
replaces the PBR-patch with a purpose-built loader. The byte-level
behavior is different (we hardcode LBAs vs walking FAT), but the
end-to-end role in the boot chain — NTLDR loads us, we load `$LDR$`,
we jump there — is identical. Clean-room: design is derived from public
boot-loader patterns (FAT spec, BIOS INT 13h docs), not from reading
WinSetupFromUSB's `gsar`-patched PBR bytes.
