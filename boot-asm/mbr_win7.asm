; mbr_win7.asm — bootrec Master Boot Record, Windows 7/8/10/11 variant.
;
; Spec-compliance map (clean-room — see docs/PROVENANCE.md):
;   - INT 13h fn 0x42 (LBA): Phoenix BIOS Interface Reference
;   - Partition table layout: winioctl.h / MBR convention
;   - GPT protective MBR partition type 0xEE: UEFI spec §5.2 Table 5-3
;   - Relocation 0x7C00 -> 0x600: standard MBR pattern
;
; Differences vs mbr_xp.asm:
;   - GPT-protective check: if the active partition has type 0xEE (GPT
;     protective MBR), refuse to boot. Legacy BIOS booting a GPT disk
;     skips past the actual GPT structures and is almost always a
;     misconfiguration. The user wants UEFI in that case.
;   - Different error strings (smaller, terser — Win 7 MBRs ship with
;     short single-line errors rather than the verbose XP ones).
;
; Loaded by BIOS at 0000:7C00 in real mode. The BIOS hands us:
;   DL = boot drive number (e.g. 0x80 for first hard disk / USB stick)
;
; Algorithm:
;   1. Set up segments + stack, save DL.
;   2. Relocate ourselves 0x7C00 -> 0x0600 (so the loaded PBR fits at 0x7C00).
;   3. Scan the partition table for an active entry (byte 0 = 0x80).
;   4. If active partition is type 0xEE (GPT protective), halt with 'G'.
;   5. Otherwise read its first sector to 0x7C00 via INT 13h ext fn 0x42.
;   6. Verify the loaded sector ends with 0x55 0xAA.
;   7. Far-jump to 0:7C00 with DL preserved.
;
; Output: exactly 512 bytes. Bytes 446..509 are the partition table
; (zeroed by nasm; written by bootrec at install time). Bytes 510..511
; = 0x55 0xAA.

BITS 16
ORG 0x7C00

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00          ; stack just below the load address
    sti

    ; Save boot drive number; we'll need it for INT 13h reads.
    mov [boot_drive], dl

    ; Relocate ourselves to 0000:0600 so the loaded PBR can occupy 0000:7C00.
    cld
    mov si, 0x7C00
    mov di, 0x0600
    mov cx, 256             ; 256 words = 512 bytes
    rep movsw
    ; Far-jump into the RELOCATED copy. Without the bias, NASM resolves
    ; `relocated` to its ORG-base address (~0x7C2x), and we'd continue
    ; in the ORIGINAL at 0x7C00 — see mbr_xp.asm for the cautionary tale.
    jmp 0x0000:(relocated - 0x7C00 + 0x0600)

relocated:
    ; Partition table in the relocated copy at 0x0600 + 0x1BE = 0x07BE.
    mov si, 0x07BE
    mov cx, 4               ; four 16-byte primary partition entries
.scan:
    cmp byte [si], 0x80     ; active flag
    je .check_gpt
    add si, 16
    loop .scan

    ; No active partition.
    mov al, 'A'
    jmp die

.check_gpt:
    ; Active partition found. Reject if it's a GPT protective MBR
    ; (type 0xEE) — legacy BIOS booting a GPT disk is misconfigured.
    cmp byte [si + 4], 0xEE
    je .gpt_refuse

    ; SI -> active partition entry. Bytes 8..11 = LBA start (little-endian).
    ; Build a disk address packet for INT 13h extended read.
    push si
    mov si, dap
    mov word [si + 0], 0x10         ; packet size
    mov word [si + 2], 1            ; sectors to read = 1 (just the PBR)
    mov word [si + 4], 0x7C00       ; dest offset
    mov word [si + 6], 0x0000       ; dest segment
    pop bx                          ; partition entry pointer
    mov ax, [bx + 8]
    mov [si + 8], ax                ; LBA low 16
    mov ax, [bx + 10]
    mov [si + 10], ax               ; LBA next 16
    mov word [si + 12], 0           ; LBA bits 32..47
    mov word [si + 14], 0           ; LBA bits 48..63

    mov dl, [boot_drive]
    mov ah, 0x42                    ; extended read
    int 0x13
    jc .io_error

    ; Check the loaded sector's boot signature.
    cmp word [0x7C00 + 510], 0xAA55
    jne .bad_signature

    ; Hand off to the PBR. DL still holds the boot drive number.
    mov dl, [boot_drive]
    jmp 0x0000:0x7C00

.gpt_refuse:
    mov al, 'G'
    jmp die

.io_error:
    mov al, 'I'
    jmp die

.bad_signature:
    mov al, 'S'
    jmp die

; die: AL = single-character error code. Print to BIOS teletype + COM1
; (the latter so the QEMU smoke harness can scrape failures), then halt.
;
; Error code legend:
;   'A' = no Active partition
;   'G' = active partition is a GPT protective MBR (use UEFI)
;   'I' = INT 13h disk read failed
;   'S' = loaded sector lacked 0x55AA boot signature
die:
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    mov dx, 0x3F8
    out dx, al
.h: hlt
    jmp .h

; Data
boot_drive: db 0

dap:
    times 16 db 0

; Pad to the partition table location (offset 0x1BE = 446).
    times 446 - ($ - $$) db 0

; Partition table: 4 × 16-byte entries, zeroed. bootrec writes the
; real partition entries during pipeline execution.
    times 64 db 0

; Boot signature.
    dw 0xAA55
