; ntfs_pbr_bootmgr/sector0.asm — NTFS PBR stage 1 (single sector).
;
; Body at offset 0x54, after JMP + 8-byte OEM "NTFS    " + NTFS BPB +
; Extended BPB (bytes 0x0B..0x53; spliced by
; bootrec::splice_ntfs_pbr_multi from the freshly-formatted partition).
;
; Job: read stage 2 (1 sector) from disk LBA = HiddSec + 1 into 0:7E00
; via INT 13h ext fn 0x42, far-JMP 0x0000:0x7E00. DL = boot drive
; preserved through the call.
;
; Why multi-sector: real Microsoft NTFS bootsectors are 16 sectors; the
; spec (docs/SPEC.md §Component breakdown) sizes the NTFS PBR at ~16 KB.
; Single-sector experiments overflowed 426 bytes once the MFT walker +
; data-run parser were inlined; multi-sector splits the BPB-bound 84-byte
; header from the rest of the boot code.
;
; Clean-room references: Microsoft NTFS public docs + Phoenix BIOS docs.
; See docs/PROVENANCE.md.

BITS 16
ORG 0x7C00

%define BPB_HiddSec      0x1C
%define BOOT_DRV         0x7B00
%define DAP              0x0500

start:
    jmp short body
    nop
    times 84 - ($ - $$) db 0          ; BPB + extended BPB placeholder

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

    ; Read partition LBA + 1..2 → 0:7E00. Two sectors because the NTFS
    ; MFT walker + INDEX_ALLOCATION reader doesn't fit in 512 bytes.
    mov si, DAP
    mov byte [si + 0], 0x10
    mov byte [si + 1], 0
    mov word [si + 2], 2              ; two sectors (stage 2)
    mov word [si + 4], 0x7E00
    mov word [si + 6], 0x0000
    mov eax, [0x7C00 + BPB_HiddSec]
    inc eax
    mov [si + 8], eax
    mov dword [si + 12], 0

    mov dl, [BOOT_DRV]
    mov ah, 0x42
    int 0x13
    jc .err

    mov dl, [BOOT_DRV]
    jmp 0x0000:0x7E00

.err:
    mov al, 'R'
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    mov dx, 0x3F8
    out dx, al
.h: hlt
    jmp .h

    times 510 - ($ - $$) db 0
    dw 0xAA55
