; fat32_pbr_bootmgr/sector0.asm — Stage 1 of the multi-sector PBR.
;
; Loaded by the MBR at 0000:7C00 in real mode. DL = boot drive.
;
; Job: read sector 1 of the partition (LBA = BPB.HiddSec + 1) to 0x7E00
; via INT 13h ext fn 0x42, then far-jump to 0x07E0:0x0000 = linear
; 0x7E00 where stage 2 takes over.
;
; The BPB at offsets 3..89 is filesystem state, spliced by
; bootrec::splice_fat32_pbr_multi from the existing freshly-formatted
; partition.
;
; Per docs/SPEC.md §Component breakdown, this is the v1.0 fat32_pbr_bootmgr
; variant. Real Microsoft bootmgr expects to be called from a multi-sector
; boot environment (see V0.2_PBR_STATUS history); single-sector PBRs work
; against synthetic loaders but not against real BOOTMGR on real hardware.

BITS 16
ORG 0x7C00

%define BPB_HiddSec      0x1C
%define BOOT_DRV         0x7B00       ; one byte of low-RAM scratch
%define DAP              0x0700       ; disk address packet location

start:
    jmp short body
    nop
    times 87 db 0                     ; BPB placeholder (3..89)

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

    ; Build a DAP to read sector 1 of the partition to 0x7E00.
    mov si, DAP
    mov byte [si + 0], 0x10           ; packet size
    mov byte [si + 1], 0
    mov word [si + 2], 1              ; sectors to read
    mov word [si + 4], 0x7E00         ; dest offset
    mov word [si + 6], 0x0000         ; dest segment
    mov eax, [0x7C00 + BPB_HiddSec]
    inc eax                            ; partition_LBA + 1
    mov [si + 8], eax
    mov dword [si + 12], 0

    mov dl, [BOOT_DRV]
    mov ah, 0x42
    int 0x13
    jc .io_error

    ; Hand off to stage 2. DL still holds boot drive number.
    mov dl, [BOOT_DRV]
    jmp 0x0000:0x7E00

.io_error:
    mov al, 'R'                       ; sector-1 read failed
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    mov dx, 0x3F8
    out dx, al
.h: hlt
    jmp .h

    times 510 - ($ - $$) db 0
    dw 0xAA55
