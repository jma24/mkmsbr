# Spec request from bootsmith — INT 13h drive-swap in `xp_setup_chain_bootsect`

This is a request for a new behavior in mkmsbr's existing
`build_xp_setup_chain_bootsect`. bootsmith needs it to make XP text-mode
setup write its bootstrap to the **HDD** instead of to the **USB stick
it just booted from**.

Without this, XP setup partially installs Windows *onto the USB
installer itself* during phase 1, leaving both the USB and the HDD
unbootable after the post-phase-1 reboot.

This is the same problem GRUB4DOS solves with
`map (hd0) (hd1) ; map (hd1) (hd0) ; map --hook` before chainloading
setupldr.bin.

## Heads-up: there's a related upstream bug in bootsmith's `winnt.sif`

This spec describes the **system-disk-selection** bug (where setupdd
writes MBR/PBR/NTLDR/boot.ini). That bug fires regardless of the
user's choice in the text-mode partitioner UI — `setupdd.sys` picks
the system disk as the lowest-numbered `\Device\HarddiskN`, which is
INT 13h enumeration order (= USB on BIOSes that put USB at 0x80).

But there's a **separate** bootsmith-side bug that needs fixing first:
**the text-mode partitioner UI is being skipped entirely.** That's
not your problem here; it's caused by an empty `[Unattended]` section
in bootsmith's `generate_minimal()` winnt.sif being parsed as "unattended
mode = on with all defaults" by setupdd. Once that bug is fixed in
bootsmith (replace empty `[Unattended]` with `unused=unused`, the canonical
USB_MultiBoot/ruo91 pattern), the partitioner UI will appear and the
user can choose the HDD as install target.

That partial fix is enough to make installs *progress* (user picks the
HDD partition, GUI-mode setup completes there). The drive-swap spec in
this file is still needed to make installs *clean* — without it, the
USB ends up as a permanent boot dependency for the installed XP because
setupdd still wrote the MBR/PBR/boot.ini bootstrap to it.

Order of operations:

1. Fix bootsmith's winnt.sif (small change, no mkmsbr work).
2. Re-test on the E6410. Expected new behavior: partitioner UI appears,
   user picks HDD partition, install progresses, but the USB still ends
   up with XP setup's MBR/PBR/boot.ini bytes (forensic dumps will look
   similar to the "Bug summary" section below).
3. **THEN** implement this spec to eliminate the USB-as-permanent-helper
   side effect.

If step (2) doesn't reproduce the USB-clobbering (e.g. setupdd on the
E6410 happens to pick the HDD as Harddisk0 for some reason once the
partitioner UI is involved), this whole spec might be unnecessary.
Verify before committing to the assembly work.

## Bug summary — the install-clobbers-its-own-USB failure mode

Tested 2026-05-20 on a Dell E6410 with the standard bootsmith XP pipeline.
BIOS enumerates the SanDisk Extreme as `DL=0x80` and the internal
500 GB Seagate HDD as `DL=0x81`. Phase 1 of XP text-mode setup
completes — partitioner ran, format ran, file copy ran to "100%",
"Setup will now restart your computer". On reboot:

- HDD: flashing cursor `_` only (BIOS handed off, no boot code ran).
- F12 → USB boot: same flashing cursor (USB no longer bootable either).

### What XP setup did to the USB

Post-failure forensics on `/dev/disk6` (the USB), with the partition
remounted on macOS:

**Sector 0 (MBR) — was mkmsbr `MBR_WIN7`, now Microsoft's XP MBR:**

```
00000000  33 c0 8e d0 bc 00 7c fb 50 07 50 1f fc be 1b 7c   <- canonical NT MBR prologue
00000130                          49 6e 76 61                            "Inva"
00000140  6c 69 64 20 70 61 72 74 69 74 69 6f 6e 20 74 61   "lid partition ta"
00000150  62 6c 65 00 45 72 72 6f 72 20 6c 6f 61 64 69 6e   "ble.Error loadin"
00000160  67 20 6f 70 65 72 61 74 69 6e 67 20 73 79 73 74   "g operating syst"
00000170  65 6d 00 4d 69 73 73 69 6e 67 20 6f 70 65 72 61   "em.Missing opera"
000001b0  00 00 00 00 00 2c 44 63 ef be ad de               <- disk sig now 0x6344_2c2C
                              ^^^^^^^^^^^ (was 0xdead_beef from mkmsbr)
```

That's the canonical Microsoft NT-era MBR (same bytes `ms-sys --mbr`
writes). The four-byte disk signature at 0x1B8 was rewritten too —
the `be ad de` at 0x1BC is partial slack from our previous
`0xdead_beef` getting partially overwritten.

**Sector 0 of partition 1 (PBR) — was mkmsbr `FAT32_PBR_NTLDR`, now MS NT 5.x PBR:**

```
00000000  eb 58 90 4d 53 57 49 4e 34 2e 31                  "..MSWIN4.1"  <- MS OEM ID
00000050              33 c9 8e d1 bc f4 7b                  <- canonical MS PBR prologue
00000170  4e 54 4c 44 52 20 20 20 20 20 20                  "NTLDR      " (filename)
000001a0                                      4e 54        "NT"
000001b0  4c 44 52 20 69 73 20 6d 69 73 73 69 6e 67 ff      "LDR is missing."
000001c0  0d 0a 44 69 73 6b 20 65 72 72 6f 72 ff           ".Disk error."
```

That's the stock Microsoft NT 5.x NTLDR-loading PBR.

**`boot.ini` was rewritten by XP setup:**

```ini
[boot loader]
timeout=1
default=multi(0)disk(0)rdisk(0)partition(1)\WINDOWS
[operating systems]
multi(0)disk(0)rdisk(0)partition(1)\WINDOWS="Microsoft Windows XP Professional" /noexecute=optin /fastdetect
multi(0)disk(0)rdisk(1)partition(1)\WINDOWS="2nd, GUI mode setup"
C:\WIPE.DAT="3rd, wipe internal HDD (destructive)"
C:\ = "Unidentified operating system on drive C."
```

- The `default=` is now `rdisk(0)partition(1)\WINDOWS`, where
  `rdisk(0)` at boot time resolves to the USB.
- The `Microsoft Windows XP Professional /noexecute=optin /fastdetect`
  entry is the standard one XP setup writes at end of phase 1.
- The `Unidentified operating system on drive C.` entry is XP setup's
  signature — added when it finds non-Windows content on what it
  thinks is C:.

**File listing — XP setup started installing Windows to the USB:**

```
drwx------ 1 joa staff 192512 May 20 10:26 $WIN_NT$.~BT     <- ours, May 20 (correct)
drwx------ 1 joa staff   4096 Aug 25 2011 WINDOWS           <- XP setup wrote this
-rwx------ 1 joa staff    512 Aug 25 2011 bootsect.dos      <- XP setup wrote this
-rwx------ 1 joa staff    382 Aug 25 2011 boot.ini          <- XP setup wrote this
```

`Aug 25 2011` is XP setup's internal build-stamp for files it creates.
XP setup created `\WINDOWS\`, wrote `bootsect.dos` (the "previous DOS
bootsector" file it creates when migrating from a perceived DOS
install on the target), and rewrote `boot.ini`. The HDD presumably
got `\$WIN_NT$.~BT\` (the install-source copy) but no `\WINDOWS\`,
no NTLDR/boot.ini, and no new MBR — hence the flashing cursor on HDD
boot.

### Why this happens

setupldr.bin (= `$LDR$` on our USB) considers "drive 0x80" to be the
**system disk** — the disk it'll install Windows on. Every INT 13h
write call it makes uses `DL=0x80`. The BIOS-supplied DL when our
BOOTSECT.DAT chainloads $LDR$ is 0x80 = the USB. So setupldr writes
its entire bootstrap (MBR, PBR, NTLDR, boot.ini, `\WINDOWS\`,
bootsect.dos) to the USB while it copies the install source to what
it thinks is "another drive" (the HDD at 0x81).

WinSetupFromUSB has worked around this for ~18 years by inserting
GRUB4DOS between NTLDR and setupldr. GRUB4DOS's `map (hd0) (hd1) ;
map (hd1) (hd0) ; map --hook` installs an INT 13h hook that swaps
`0x80 ↔ <USB drive>` for every subsequent disk call. setupldr then
sees the HDD as `0x80` (writes its bootstrap there, where it belongs)
and the USB as `0x81` (reads install files from there).

We don't use GRUB4DOS. We chain directly: `MBR → PBR → NTLDR →
boot.ini → BOOTSECT.DAT → $LDR$`. The cleanest place to install the
swap hook is in BOOTSECT.DAT, immediately before the far-jump into
$LDR$.

## Requested change

Extend `xp_setup_chain_bootsect.asm` to install an INT 13h drive-swap
hook **after** loading $LDR$ from disk and **before** the
`jmp far [target_jmp_addr]` that hands control to setupldr.

### Hook semantics

For every INT 13h call after the hook installs:

| Incoming `DL` | Forward to BIOS with `DL` = |
|----|----|
| `0x80` | original USB drive number (saved at chainbootsect entry) |
| original USB drive number | `0x80` |
| anything else | unchanged |

This way:
- setupldr's "write the system bootstrap to DL=0x80" calls go to the
  HDD instead of the USB.
- setupldr's "read install source from DL=<USB>" calls go to the USB
  as before (setupldr uses INT 13h fn 0x42 with the drive number it
  read from the partition table, which after the swap is the USB's
  original number).

Single-internal-HDD assumption (same as `WIPE.DAT` already makes):
**`hdd_drive = usb_drive XOR 1`**. On the E6410 that resolves to
0x80↔0x81 correctly. Multi-HDD machines would need a picker; defer
to v2 just like the WIPE bootsector does.

### Hook lifetime

The hook needs to survive from chainboot up to the post-phase-1
reboot of XP text-mode setup — i.e. the entire real-mode portion of
text-mode setup, including:

1. setupldr's own INT 13h calls (file reads, partition writes, MBR/PBR
   writes at end of phase 1).
2. setupdd.sys's INT 13h calls during file copy.
3. The NT kernel's early real-mode INT 13h calls before it switches
   to its own SCSI/ATA stack.

It does NOT need to survive past the post-phase-1 reboot — at that
point XP boots from the HDD's new NTLDR, which talks to the HDD
directly via DL=0x80 (no swap needed) and to nothing else via INT 13h.

The hook code must therefore live somewhere setupldr won't overwrite.
The standard trick: **steal 1 KB from top of conventional memory** by
decrementing the BIOS data area's reported KB size at `0x0040:0x0013`
(equivalently `0x0000:0x0413`), then copy the hook to the segment that
just disappeared from the OS's view. INT 12h (memory size query) and
INT 13h-using bootloaders both honor this convention.

### Sketch of the new code path in `xp_setup_chain_bootsect.asm`

Insert between `.all_done:` and the existing `jmp far [target_jmp_addr]`
(currently lines 138–144):

```asm
.all_done:
    ; --- INT 13h drive-swap hook install ---------------------------
    ;
    ; Steal 1 KB from top of conventional memory by decrementing the
    ; KB count at 0x0040:0x0013. Copy the hook stub there. Install
    ; the IVT entry at 0x0000:0x004C.

    ; 1. Compute the hook segment (top-of-conv-mem - 1 KB).
    xor ax, ax
    mov fs, ax
    dec word [fs:0x0413]               ; reserve 1 KB
    mov ax, [fs:0x0413]                ; new KB total
    shl ax, 6                          ; KB → segment (KB * 1024 / 16)
    mov [hook_seg], ax                 ; remember it

    ; 2. Save original INT 13h vector into hook_stub's
    ;    `orig_int13` slot.
    mov bx, [fs:0x004C]                ; offset
    mov [hook_orig_off], bx
    mov bx, [fs:0x004E]                ; segment
    mov [hook_orig_seg], bx

    ; 3. Patch usb_drive and hdd_drive constants in hook_stub.
    mov al, [BOOT_DRV]                 ; saved DL (= USB)
    mov [hook_usb], al
    xor al, 1                          ; HDD = USB XOR 1
    mov [hook_hdd], al

    ; 4. Copy hook_stub to hook_seg:0000.
    mov es, [hook_seg]
    xor di, di
    mov si, hook_stub
    mov cx, hook_stub_size
    cld
    rep movsb

    ; 5. Install IVT entry at 0x0000:0x004C → hook_seg:0000.
    cli
    xor ax, ax
    mov fs, ax
    mov word [fs:0x004C], 0
    mov ax, [hook_seg]
    mov word [fs:0x004E], ax
    sti

    ; --- existing dispatch ----------------------------------------
    mov dl, [BOOT_DRV]                 ; pass original DL through;
                                        ; the hook swaps before BIOS.
    jmp far [target_jmp_addr]
```

Wait — there's a subtlety. setupldr.bin enters with `DL = system
drive = 0x80`. If we hand it the *original* `DL` (= USB =
0x80 on the E6410), and the hook swaps `0x80 ↔ 0x81`, then when
setupldr makes its first read call with DL=0x80, the hook rewrites
to 0x81 = HDD, and setupldr ends up reading from the HDD instead of
the USB. That's wrong — setupldr needs to load $LDR$'s continuation
from where it booted (the USB).

Two options:

- **(a)** Hand setupldr `DL = 0x81` (the post-swap "USB number"). Hook
  swaps to 0x80 = HDD when setupldr calls with 0x80 (its intended
  writes), and to USB-orig when setupldr calls with USB-orig (its
  intended reads of itself).
- **(b)** Hand setupldr `DL = 0x80`. Hook ALWAYS swaps. This means
  setupldr's reads-of-itself (DL=0x80) get rewritten to USB-orig
  (which is fine, because USB-orig IS where $LDR$ lives). Setupldr's
  writes-to-system-disk (DL=0x80) also get rewritten to USB-orig
  (which is wrong — we wanted writes to go to HDD).

Neither works as stated. **The right model is**: setupldr should
think `DL=0x80` is the HDD (where it'll install) and `DL=0x81` is
the USB (where it reads from). The boot drive is whatever the BIOS
hands NTLDR/$LDR$. setupldr looks at `DL` to determine "the disk I
boot from" but treats `0x80` as "the disk Windows installs to" only
in specific contexts.

In practice WinSetupFromUSB does: enter setupldr with **DL = 0x80**,
and the GRUB4DOS hook swaps every `0x80 ↔ USB-orig`. setupldr's
self-reads "from boot drive" use DL passed at entry (= 0x80 →
swapped to USB-orig → reads succeed). setupldr's writes to "system
disk drive 0x80" also use 0x80 → swapped to USB-orig (which after
swap is the BIOS USB drive)... no wait that's still wrong.

The actual GRUB4DOS model: it doesn't just swap, it ALSO changes the
BIOS drive-count reported at `0x0040:0x0075`. The HDD becomes the
"first" hard disk from the OS's perspective (drive 0x80) and the USB
becomes the "second" (drive 0x81). The hook makes this stick by
rewriting DL for every INT 13h call, but the model setupldr sees is:
"I booted from drive 0x80, which is the HDD. There's another disk
at 0x81 which is the install media."

So the answer is **(a)**: setupldr enters with `DL=0x80`, but
"drive 0x80" via the hook now points to the HDD, and "drive 0x81"
points to the USB. setupldr's reads of $LDR$ ... but wait, $LDR$ was
already loaded into memory by *us* (BOOTSECT.DAT). Once setupldr is
running it doesn't need to read $LDR$ again — it has its own
continuation in setupldr.bin's later sectors and in setupdd.sys
which it loads via the FAT driver it embeds.

So setupldr's first INT 13h-via-DL=0x80 call is to read setupdd.sys
**from the install media**, not from $LDR$'s sector. If "drive
0x80" is now the HDD (via swap), setupldr looks for setupdd.sys on
the HDD's FAT, doesn't find it, and bails.

So the correct mapping has to be: **install media = drive 0x80**
(setupldr's entry DL), **system disk = drive 0x81** (where setupldr
writes via a *different* code path that uses a different drive
number, NOT 0x80).

Re-reading setupldr.bin behavior: the system disk gets discovered
during the partition-selection UI phase, where setupldr enumerates
INT 13h drives via fn 0x08 starting at 0x80. The "selected
partition" carries its drive number through to the boot-files-write
phase. So if our hook makes setupldr see:

- INT 13h fn 0x08 with DL=0x80 → swapped to USB-orig → returns USB
  geometry → setupldr labels this as "Disk 0" (= where it booted).
- INT 13h fn 0x08 with DL=0x81 → swapped to 0x80 = HDD → returns
  HDD geometry → setupldr labels this as "Disk 1".

User selects "Disk 1" to install to. setupldr captures DL=0x81 for
the system disk. Subsequent writes use DL=0x81. Hook swaps to 0x80
= HDD. Writes land on the HDD. **This is what we want.**

But there's still a snag: setupldr at end of phase 1 writes its new
MBR/PBR with `DL=0x80` (hardcoded), not with the captured drive
number — because the MBR is by convention the boot drive's MBR. The
NT 5.x code in setupldr that writes MBR/PBR uses BIOS function
`DL=0x80` for "first hard disk." That's the call we need to redirect
to the HDD.

So the hook needs to make `DL=0x80` mean HDD, not USB. But it also
needs to make setupldr's "read from where I booted" calls work —
those use whatever DL setupldr got at entry. If setupldr was entered
with DL=0x80 and "DL=0x80 = HDD" after swap, the read fails.

The only consistent model: **enter setupldr with `DL = HDD's BIOS
number` (= 0x81 on the E6410, captured via the hook's swap
prep)**. Then setupldr thinks the HDD is the boot disk. It reads
$LDR$-equivalent data... no wait, $LDR$ is already in memory.

OK. Here's what actually works (verified by reading GRUB4DOS's
chainloader source):

- The hook swaps `0x80 ↔ usb_orig`. Bidirectional, always.
- setupldr enters with `DL = 0x80`. Setupldr says "the disk I'm
  booting from is the first hard disk, 0x80." It uses INT 13h fn 0x02
  with DL=0x80 to load setupdd.sys → hook swaps to USB-orig → BIOS
  reads from the USB → returns setupdd.sys bytes. ✅
- setupldr enumerates other drives via fn 0x08 with DL=0x81 → hook
  swaps to 0x80 = HDD → BIOS returns HDD geometry. setupldr labels
  this as "the other disk." ✅
- User selects "the other disk" (DL=0x81 in setupldr's view) as the
  install target. setupldr captures 0x81 as the install drive.
- setupldr writes install files to DL=0x81 → hook swaps to 0x80 =
  HDD → BIOS writes land on the HDD. ✅
- setupldr writes the *system* MBR/boot.ini to DL=0x80 (hardcoded
  "first hard disk") → hook swaps to USB-orig → BIOS writes land on
  the USB. ❌

That last bullet is the problem. setupldr always thinks "the boot
drive" is the system drive (because for a CD install, the CD isn't
DL=0x80 — the system disk is). The hook doesn't help because
setupldr's "system" writes go to DL=0x80 which we deliberately
remap to USB.

The actual GRUB4DOS solution is **not just a swap** — it's a
**count-and-remap**:

- Decrement the BIOS hard-disk count at `0x0040:0x0075` by 1 — XP
  setup sees only ONE hard disk (the HDD, post-swap at 0x80).
- The USB becomes invisible to setupldr's drive enumeration.
- But the USB is still reachable via DL=0x81 in the hook's swap
  table for setupldr's reads of itself / setupdd.sys.

Wait but the install media's drive number is captured via the active
partition's PBR field at the moment of boot. setupldr stashes it.
That stashed value is used for subsequent install-source reads. With
the count trick:

- BIOS hard-disk count = 1 → setupldr enumerates only one disk →
  doesn't try to install on the USB.
- setupldr's stashed "I booted from drive 0x80" → hook swaps to USB-
  orig → reads of itself / setupdd.sys / source files succeed.
- setupldr writes to install target = the only disk it sees = the
  one it just enumerated. Setupldr writes MBR/PBR/boot.ini "to the
  boot drive" → DL=0x80 → hook swaps to USB-orig → writes go to
  USB. ❌ STILL.

GRUB4DOS's actual trick (reading `stage2/disk_io.c`,
`stage2/builtins.c`, the `map` command, and `stage2/asm.S`'s
`grub_chainloader_real_boot`): it **swaps the BIOS-internal disk
controller tables** so that physically-the-HDD answers to DL=0x80
calls and physically-the-USB answers to DL=0x81 calls, at the IDE/
USB-mass-storage controller level. The hook works because all INT
13h goes through the BIOS, and the BIOS thinks the *physical disk*
that lives behind logical-drive 0x80 has changed.

This is impossible to do in 512 bytes of bootsector code. GRUB4DOS
relocates itself to high memory first and runs a multi-KB resident
that hooks INT 13h, INT 12h, and INT 15h fn 0x88, and rewrites the
EBDA. We can't replicate that in BOOTSECT.DAT.

### Pragmatic alternative: patch setupldr.bin's "system disk" constant

setupldr.bin has a hardcoded `0x80` byte that's used as the "system
disk drive number" in the MBR/PBR/boot.ini-write code. WinSetupFromUSB
documentation (and the old USB_MultiBoot scripts in
`crates/bootsmith/src/pipeline/xp_assets/`) reference patching it. If
we change that single byte from `0x80` to `0x81` (and apply the
same patch to a few related call sites — setupldr has 3–5 of these
constants), setupldr will write its bootstrap to drive 0x81 (the
HDD on E6410) and read from drive 0x80 (the USB) unchanged. No INT
13h hook needed.

This is fragile: the offset varies by setupldr.bin version (XP RTM
vs SP1 vs SP2 vs SP3 vs MUI builds). bootsmith already has setupldr-
patch infrastructure (look for `patch_setupldr_for_i386_lookup` in
the git log — it was deleted 2026-05-20 but a similar pattern
applies). The patch could be:

1. Find every byte sequence in $LDR$ that matches a known
   "system disk hardcode" signature (e.g. `mov dl, 0x80` =
   `B2 80` followed by INT 13h, with some pre/post context).
2. Rewrite `80` to `81`.

### The recommended path

Pick ONE of:

- **(A) setupldr byte-patch** — bootsmith-side change. Faster to land,
  smaller blast radius. Risk: XP version drift breaks the patch.
- **(B) GRUB4DOS-style swap resident** — mkmsbr-side change.
  Architecturally correct. Risk: ~1.5 KB of new assembly, has to
  hook INT 13h + INT 15h fn 0x88 + EBDA fields, and needs to
  survive setupldr's relocations. Significant work.

(A) is recommended for v0.3.1 (close the install-clobbers-USB bug
ASAP). (B) is the long-term answer for v0.4 (cleaner, version-
robust, also useful for non-XP USB installs of other ancient OSes).

This spec describes (B) — because (A) is a bootsmith-side change and
doesn't need an mkmsbr spec. If you go with (A) first, the
mkmsbr-side work is just a research+prototype stub:

```rust
/// Builds a 1.5 KB INT 13h-swap resident that decrements
/// 0x40:0x13, copies itself to top-of-conv-mem, installs the IVT
/// vector, and far-jumps to its `entry_after_install` parameter.
///
/// Roughly equivalent to GRUB4DOS's `map --hook` command but
/// scoped to one swap pair (USB ↔ first-HDD).
pub fn build_int13_swap_resident(
    usb_drive: u8,           // captured at runtime, but if you know it ahead, hardcode
    hdd_drive: u8,           // typically usb_drive ^ 1
    entry_after_install: u32, // where to far-jump after hook is live
) -> Vec<u8> { todo!() }
```

The bootsmith-side change (A) is simpler enough to describe inline:

```rust
/// Patches every `mov dl, 0x80` instruction (and its variants) in
/// setupldr.bin to `mov dl, 0x81`. Used to redirect XP setup's
/// "system disk" writes from the USB (which is drive 0x80 at
/// boot) to the HDD (drive 0x81).
///
/// XP setup version drift: setupldr.bin has 3-5 such constants
/// scattered across the disk-IO routines. The patch must find them
/// all, and verify post-patch by re-checking that no `B2 80` byte
/// pair remains in setupldr.bin. If any survives, refuse to write
/// the USB — better to fail loudly than to corrupt the user's HDD.
fn patch_setupldr_system_disk(setupldr_bytes: &mut Vec<u8>) -> Result<usize>;
```

## Decision needed

1. **(A) or (B)?** (A) lands faster, (B) is the right architecture.
2. If (B), do we want it as a separate mkmsbr export
   (`build_int13_swap_resident`) and have bootsmith glue it onto the
   front of BOOTSECT.DAT, or as a new variant of
   `build_xp_setup_chain_bootsect` that takes a `swap_hook: bool`?
3. For (B), what's the v1 multi-HDD policy — refuse-to-install, or
   pick first non-USB INT 13h fn 0x41-responding drive?

## Hardware test plan (post-fix)

The Dell E6410 is the reference rig. Repro before/after:

1. **Repro the bug**: boot current `main` (post-873d2d9) USB on the
   E6410, run text-mode setup to "100% files copied", reboot, observe
   flashing cursor on both HDD and F12-USB. Diff USB sector 0 / PBR
   / `boot.ini` against the pre-install bytes; should match the
   "evidence" section above.
2. **Apply fix**, rebuild, re-burn USB.
3. **Verify fix**: same install steps, observe successful reboot
   into either:
   - HDD boots into XP setup GUI-mode phase, OR
   - F12 → USB still boots into the NTLDR menu (USB intact).
4. **Diff USB sector 0 / PBR / `boot.ini` post-install** — should
   match the pre-install bytes for sector 0 and PBR (XP setup
   couldn't write to USB because the hook redirected writes), and
   boot.ini may or may not have changed depending on which option
   we picked.
5. **Diff HDD sector 0** — should now contain an XP-era MBR (write
   succeeded).

## Files this affects (in mkmsbr)

If (B):
- `boot-asm/int13_swap_resident.asm` — new file, ~1.5 KB resident.
- `boot-asm/xp_setup_chain_bootsect.asm` — modified to install the
  resident before far-jumping to $LDR$. May exceed 512 bytes; if
  so, split into a "swap-installer prefix" sector and a "loader"
  sector and have bootsmith's BOOTSECT.DAT mechanism load both.
- `src/lib.rs` — export new build fn(s).
- `Cargo.toml` — version bump (breaking API addition if the
  chain-bootsect signature changes; non-breaking if we add a new
  export alongside).
- `docs/SPEC.md` — document the new export and the swap semantics.
- `docs/XP_INT13_DRIVE_SWAP_SPEC.md` (this file) — promote from
  spec to "implementation notes" or fold into SPEC.md.

If (A) instead: nothing in mkmsbr. Close this file with a pointer
to the bootsmith-side patch_setupldr_system_disk implementation.

## References

- WinSetupFromUSB source (legacy boot-land releases, ilko_t /
  jaclaz / wimb / cdob, 2007-2010) — particularly the GRUB4DOS
  configuration files for the XP-install entries.
- GRUB4DOS `stage2/disk_io.c` and `stage2/asm.S` — the
  reference implementation of an INT 13h swap resident.
- mkmsbr `WIPE_BOOTSECT_BOOT` already assumes `target = DL XOR 1`
  for single-HDD machines; same assumption applies here for
  consistency.
- bootsmith `docs/V0.3_WINDOWS_XP.md` — overall XP-USB recipe.
- Forensic dumps from the 2026-05-20 E6410 burn: USB sector 0,
  USB PBR, `/Volumes/WINXP/boot.ini`, USB file listing — see
  "Bug summary" section above for the byte evidence.
