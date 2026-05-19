; ntfs_pbr_bootmgr/sector1.asm — NTFS PBR stage 2.
;
; Loaded at 0x0000:0x7E00 by stage 1. DL = boot drive. DS = ES = SS = 0,
; SP = 0x7C00, direction flag clear.
;
; Algorithm:
;   1. mft_lba = BPB.HiddSec + BPB.MftLcn(low 32) * BPB.SecPerClus.
;   2. rec_sectors = decode(BPB[0x40]); signed: <0 → 1<<-x bytes /512;
;                                       >=0 → x * SecPerClus.
;   3. Read MFT record 5 (the root directory \) → BUF.
;   4. Find INDEX_ALLOCATION (0xA0) — non-resident. Parse the first run
;      header, read SecPerClus sectors of its first run into BUF; that's
;      the root INDX block.
;   5. Walk INDX entries (start at BUF+0x18+[BUF+0x18]); UTF-16 compare
;      key against "BOOTMGR" (7 chars).
;   6. Read the matched record (low 32 bits of MFT reference).
;   7. Find DATA (0x80); expect non-resident.
;   8. Walk its data runs, INT 13h-read each extent into BOOTMGR_SEG:0.
;   9. Far-JMP BOOTMGR_SEG:0 with DL preserved.
;
; Why INDEX_ALLOCATION-only: ntfs-3g's mkfs.ntfs allocates an
; INDEX_ALLOCATION attribute for the root directory even when only one
; file exists, leaving INDEX_ROOT as a single sub-node sentinel. The
; INDEX_ROOT-inline path is a Microsoft-format-specific case; supporting
; it (plus full B+tree descent) is the L3 expansion documented in
; docs/BACKLOG.md.
;
; Fixups (USA): NOT applied. For the L2 fixture's small dir, BOOTMGR's
; entry lives well before the first sector-end fixup offset (510 in the
; INDX block). L3 against real Microsoft volumes will need fixup logic.
;
; Clean-room references: Microsoft NTFS On-Disk Format public docs +
; OSDev wiki NTFS prose + Phoenix BIOS docs. See docs/PROVENANCE.md.

BITS 16
ORG 0x7E00

%define BPB_SecPerClus   0x0D
%define BPB_HiddSec      0x1C
%define BPB_MftLcn       0x30
%define BPB_MftRecBytes  0x40

%define BOOT_DRV         0x7B00
%define MFT_LBA          0x7B04
%define REC_SECTORS      0x7B08
%define READ_LBA         0x7B0C

%define DAP              0x0500
%define BUF              0x6000
%define BUF_SEG          (BUF >> 4)
%define BOOTMGR_SEG      0x2000

start:
    mov eax, [0x7C00 + BPB_MftLcn]
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    mul ecx
    add eax, [0x7C00 + BPB_HiddSec]
    mov [MFT_LBA], eax

    mov al, [0x7C00 + BPB_MftRecBytes]
    cbw
    test ax, ax
    jns .rsp
    neg al
    sub al, 9
    mov cl, al
    mov ax, 1
    shl ax, cl
    jmp .rsd
.rsp:
    movzx bx, byte [0x7C00 + BPB_SecPerClus]
    mul bx
.rsd:
    test ax, ax
    jnz .rs_ok
    inc ax
.rs_ok:
    mov [REC_SECTORS], ax

    ; Read root MFT record (5).
    mov eax, 5
    call read_mft_rec

    ; Find INDEX_ALLOCATION (0xA0). Non-resident.
    mov bx, BUF
    mov edx, 0xA0
    call find_attr
    movzx ax, word [bx + 0x20]
    add bx, ax                          ; → data runs

    ; Parse first run: header byte split into (offset_bytes<<4 | length_bytes).
    mov al, [bx]
    test al, al
    jz .nf
    inc bx
    mov ah, al
    and al, 0x0F
    shr ah, 4
    push ax
    mov cl, al
    call read_le_unsigned               ; EDX = length (clusters) (unused; we read 1 cluster)
    pop ax
    mov cl, ah
    call read_le_signed                 ; EDX = absolute LCN (delta from 0)

    ; LBA of INDX block = HiddSec + LCN * SecPerClus
    mov eax, edx
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    mul ecx
    add eax, [0x7C00 + BPB_HiddSec]
    mov [READ_LBA], eax

    ; Read SecPerClus sectors into BUF — the root INDX block.
    mov ax, BUF_SEG
    mov es, ax
    xor di, di
    movzx cx, byte [0x7C00 + BPB_SecPerClus]
.read_indx:
    push cx
    call read_one
    inc dword [READ_LBA]
    mov ax, es
    add ax, 0x20
    mov es, ax
    pop cx
    loop .read_indx

    ; Walk entries. INDX block: bytes 0x00..0x18 are header, then an
    ; index header at 0x18 whose [0x00] = first entry offset (relative).
    ; Reset ES to 0 — the read loop left it pointing at the load segment,
    ; but repe cmpsb in .scan compares DS:SI vs ES:DI and needs ES=0 so
    ; DI (the in-code `name:` label) resolves correctly.
    xor ax, ax
    mov es, ax
    mov bx, BUF
    add bx, 0x18
    movzx ax, word [bx]
    add bx, ax                          ; → first INDEX_ENTRY

.scan:
    test byte [bx + 0x0C], 0x02
    jnz .nf
    cmp byte [bx + 0x50], 7
    jne .skip
    push bx
    lea si, [bx + 0x52]
    mov di, name
    mov cx, 14
    repe cmpsb
    pop bx
    je .hit
.skip:
    add bx, [bx + 0x08]
    jmp .scan

.nf:
    mov al, 'F'
    jmp die

.hit:
    mov eax, [bx]
    call read_mft_rec

    mov bx, BUF
    mov edx, 0x80
    call find_attr
    cmp byte [bx + 0x08], 0
    je .resident_data
    movzx ax, word [bx + 0x20]
    add bx, ax

    mov ax, BOOTMGR_SEG
    mov es, ax
    xor di, di
    xor ebp, ebp

.run:
    mov al, [bx]
    test al, al
    jz .run_end
    inc bx
    mov ah, al
    and al, 0x0F
    shr ah, 4
    push ax
    mov cl, al
    call read_le_unsigned
    mov ecx, edx
    pop ax
    push cx
    mov cl, ah
    call read_le_signed
    add ebp, edx
    pop cx

    push ecx
    mov eax, ebp
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    mul ecx
    add eax, [0x7C00 + BPB_HiddSec]
    mov [READ_LBA], eax
    pop ecx
    mov eax, ecx
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    mul ecx
    mov ecx, eax

.rsec:
    push cx
    call read_one
    inc dword [READ_LBA]
    mov ax, es
    add ax, 0x20
    mov es, ax
    pop cx
    loop .rsec
    jmp .run

.run_end:
    mov dl, [BOOT_DRV]
    jmp BOOTMGR_SEG:0

.resident_data:
    mov al, 'D'
    jmp die

read_mft_rec:
    movzx ebx, word [REC_SECTORS]
    mul ebx
    add eax, [MFT_LBA]
    mov [READ_LBA], eax
    mov ax, BUF_SEG
    mov es, ax
    xor di, di
    mov cx, [REC_SECTORS]
.lp:
    push cx
    call read_one
    inc dword [READ_LBA]
    mov ax, es
    add ax, 0x20
    mov es, ax
    pop cx
    loop .lp
    ret

find_attr:
    movzx ax, word [bx + 0x14]
    add bx, ax
.next:
    mov eax, [bx]
    cmp eax, 0xFFFFFFFF
    je .miss
    cmp eax, edx
    je .got
    add bx, [bx + 4]
    jmp .next
.miss:
    mov al, 'A'
    jmp die
.got:
    ret

read_one:
    mov si, DAP
    mov byte [si], 0x10
    mov byte [si + 1], 0
    mov word [si + 2], 1
    mov [si + 4], di
    mov ax, es
    mov [si + 6], ax
    mov eax, [READ_LBA]
    mov [si + 8], eax
    mov dword [si + 12], 0
    mov dl, [BOOT_DRV]
    mov ah, 0x42
    int 0x13
    jc .err
    ret
.err:
    mov al, 'I'
    jmp die

read_le_unsigned:
    xor edx, edx
    test cl, cl
    jz .d
    push cx
    movzx ax, cl
    add bx, ax
    push bx
.lo:
    dec bx
    shl edx, 8
    mov al, [bx]
    mov dl, al
    dec cl
    jnz .lo
    pop bx
    pop cx
.d: ret

read_le_signed:
    push cx
    call read_le_unsigned
    pop cx
    test cl, cl
    jz .d
    mov ch, 32
    shl cl, 3
    sub ch, cl
    mov cl, ch
    shl edx, cl
    sar edx, cl
.d: ret

; Error codes:
;   'A' attribute missing      'I' INT 13h read failed
;   'F' BOOTMGR not in INDX    'D' DATA was resident (unsupported)
die:
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    mov dx, 0x3F8
    out dx, al
.h: hlt
    jmp .h

name:    db 'B', 0, 'O', 0, 'O', 0, 'T', 0, 'M', 0, 'G', 0, 'R', 0

    times 1024 - ($ - $$) db 0
