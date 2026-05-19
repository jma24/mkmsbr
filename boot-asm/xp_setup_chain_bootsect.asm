; xp_setup_chain_bootsect.asm — single-sector bootsector emitted by
; bootrec::build_xp_setup_chain_bootsect.
;
; Loaded by NTLDR at 0x0000:0x7C00 via boot.ini's bootsector-entry
; mechanism (the same `C:\$WIN_NT$.~BT\BOOTSECT.DAT="..."` line that
; WinSetupFromUSB has used for two decades). DL = boot drive.
;
; Role in the XP install chain:
;
;   MBR → PBR → NTLDR → boot.ini → BOOTSECT.DAT (this) → $LDR$ → text-mode setup
;
; Job: read pre-resolved LBA runs (the on-disk extents of $LDR$) into
; memory at target_segment:0, then far-jump there. usbwin walks FAT
; ahead of time and patches the run table + target segment into this
; sector before writing it as BOOTSECT.DAT. No FAT walker, no filename
; string — just geometry probe + CHS reads + far-jmp.
;
; Why CHS (fn 0x02) and not LBA-ext (fn 0x42): same reason
; fat32_pbr_ntldr/sector0.asm uses CHS — 2000s-era BIOSes that emulate
; USB sticks as USB-FDD reject fn 0x42 with AH=01. Confirmed on the Dell
; Latitude E6410 reference rig 2026-05-19 (diagnostic G0100000F from the
; PBR's probe path). Same geometry fallback (SPT=18, HEADS=2) applies
; when fn 0x08 itself returns CF — some BIOSes route reads on weird
; drive numbers (DL=0x0F seen on the same Dell) but refuse geometry
; queries on them.
;
; Layout (512 bytes total):
;   0x000..0x002  jmp short body + nop
;   0x003..0x059  BPB placeholder (spliced from formatter sector 0).
;                 Only BPB.HiddSec at offset 0x1C is read at runtime;
;                 the rest is preserved for FAT-driver compatibility
;                 so the file looks like a normal FAT boot sector.
;   0x05A..0x17F  Boot code (geom probe, run loop, CHS reader,
;                 error printer).
;   0x180..0x183  target_jmp_addr (4 bytes: offset=0, segment patched
;                 by Rust). `jmp far [target_jmp_addr]` dispatches.
;   0x184         run_count (1 byte, patched).
;   0x185..0x1E4  run_table (96 bytes = 16 × LbaRun{u32+u16}, patched).
;   0x1E5..0x1FD  pad
;   0x1FE..0x1FF  0xAA55
;
; Clean-room: derived from the FAT spec (HiddSec semantics), Phoenix
; BIOS INT 13h docs, and the existing fat32_pbr_bootmgr CHS work in this
; repo. The general technique (NTLDR-loads-bootsector-which-loads-$LDR$)
; is from public WinSetupFromUSB documentation; the byte-level loader
; is purpose-built (raw LBAs, no FAT walk), not derived from theirs.

BITS 16
ORG 0x7C00

%define BPB_HiddSec 0x1C

%define BOOT_DRV    0x7B00       ; saved BIOS DL
%define GEOM_SPT    0x7B01       ; sectors per track (fn 0x08 result or 18)
%define GEOM_HEADS  0x7B02       ; head count (fn 0x08 result or 2)
%define ABS_LBA     0x7B04       ; dword: current absolute LBA being read

start:
    jmp short body
    nop
    times 87 db 0                ; BPB at offsets 3..89 (spliced by Rust)

body:
    cli
    xor ax, ax
    mov ss, ax
    mov ds, ax
    mov es, ax
    mov sp, 0x7C00
    sti
    cld
    mov [BOOT_DRV], dl

    ; Geometry probe via INT 13h fn 0x08. Same fallback policy as
    ; fat32_pbr_ntldr/sector0.asm: on CF, use USB-FDD profile (18/2)
    ; and continue. Probe failure isn't fatal — the BIOS may still
    ; honor reads on the BIOS-handed drive number.
    mov ah, 0x08
    mov dl, [BOOT_DRV]
    push es
    xor di, di
    int 0x13
    pop es
    jc .geom_fallback
    mov al, cl
    and al, 0x3F
    mov [GEOM_SPT], al
    mov al, dh
    inc al
    mov [GEOM_HEADS], al
    jmp .geom_done
.geom_fallback:
    mov byte [GEOM_SPT], 18
    mov byte [GEOM_HEADS], 2
.geom_done:

    ; Destination segment from the patchable far-jump address. ES:DI
    ; tracks the cursor; we advance ES by 0x20 (= 512 bytes linear)
    ; per sector so DI stays 0 — no 64 KB wrap headaches for files
    ; up to 640 KB (well over $LDR$'s 260 KB).
    mov ax, [target_jmp_addr + 2]
    mov es, ax
    xor di, di

    ; Run loop. BL = remaining runs, SI -> current LbaRun.
    xor bx, bx
    mov bl, [run_count]
    test bl, bl
    jz .all_done
    mov si, run_table

.next_run:
    ; abs_lba = run.start_lba + BPB.HiddSec
    mov eax, [si]
    add eax, [0x7C00 + BPB_HiddSec]
    mov [ABS_LBA], eax
    mov cx, [si + 4]              ; sector_count for this run

.read_loop:
    push si
    push bx
    push cx
    call read_one_sector
    pop cx
    pop bx
    pop si
    ; Advance ES by 0x20 paragraphs (= 512 bytes); DI stays 0.
    mov ax, es
    add ax, 0x20
    mov es, ax
    inc dword [ABS_LBA]
    loop .read_loop

    add si, 6                     ; next LbaRun
    dec bl
    jnz .next_run

.all_done:
    ; Far-jump to target_segment:0. $LDR$ (setupldr) expects DL = boot
    ; drive on entry. The indirect form (jmp far [mem]) lets Rust patch
    ; the segment word at a fixed offset without re-encoding the
    ; instruction.
    mov dl, [BOOT_DRV]
    jmp far [target_jmp_addr]

; read_one_sector: reads sector [ABS_LBA] into ES:0 via INT 13h fn 0x02.
; CHS conversion follows the standard formula:
;   sector_idx = LBA mod SPT     ; sector # is sector_idx + 1 (1-indexed)
;   track      = LBA / SPT
;   head       = track mod HEADS
;   cyl        = track / HEADS
; Then INT 13h fn 0x02 with the packed CHS in CH/CL/DH and ES:BX = ES:0.
read_one_sector:
    mov eax, [ABS_LBA]
    xor edx, edx
    movzx ecx, byte [GEOM_SPT]
    div ecx                       ; EAX = LBA/SPT, EDX = LBA mod SPT
    push dx                       ; save sector_idx
    xor edx, edx
    movzx ecx, byte [GEOM_HEADS]
    div ecx                       ; EAX = cyl, EDX = head
    pop bx                        ; BL = sector_idx
    inc bl                        ; sector = sector_idx + 1 (1-indexed)
    mov ch, al                    ; CH = cyl low byte
    mov cl, ah
    and cl, 0x03                  ; CL = cyl bits 9..8
    shl cl, 6                     ; ...shifted to CL bits 7..6
    or cl, bl                     ; CL = cyl_hi:sector
    mov dh, dl                    ; DH = head
    mov dl, [BOOT_DRV]
    mov ax, 0x0201                ; AH=02 read, AL=1 sector
    xor bx, bx                    ; ES:BX = ES:0 (DI is logically 0)
    int 0x13
    jc .err
    ret
.err:
    ; AH = BIOS status. Print 'B' (BOOTSECT.DAT-stage marker, distinct
    ; from PBR's 'R'/'2'), then AH as two hex chars, then halt.
    push ax
    mov al, 'B'
    call pchar
    pop ax
    mov al, ah
    call pbyte
.halt:
    hlt
    jmp .halt

; pbyte/pnib/pchar: AL printer chain. Same DAS-trick layout as
; fat32_pbr_ntldr/sector0.asm — fall-through saves bytes.
pbyte:
    push ax
    shr al, 4
    call pnib
    pop ax
    and al, 0x0F
pnib:
    add al, 0x90
    daa
    adc al, 0x40
    daa
pchar:
    xor bh, bh
    mov ah, 0x0E
    int 0x10
    ret

; --- Patchable area at fixed offset 0x180 ---------------------------
;
; Rust writes here after the asm assembles:
;   0x180..0x181  target_jmp_addr offset (kept 0 by Rust)
;   0x182..0x183  target_jmp_addr segment (patched: usually 0x2000)
;   0x184         run_count (patched)
;   0x185..0x1E4  run_table: up to 16 × 6-byte LbaRun {u32 start, u16 count}
;
times 0x180 - ($ - $$) db 0
target_jmp_addr:
    dw 0x0000                     ; offset (kept 0 — $LDR$ entrypoint)
    dw 0x2000                     ; segment (default, Rust patches)
run_count:
    db 0
run_table:
    times 96 db 0

    times 0x1FE - ($ - $$) db 0
    dw 0xAA55
