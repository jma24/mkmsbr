; fat32_pbr_bootmgr/sector1.asm — Stage 2 of the multi-sector PBR.
;
; Loaded by stage 1 (sector0.asm) at 0x7E00. Entered via far-jump from
; stage 1 with DL = boot drive. Sector 0 (with BPB) is still resident
; at 0x7C00, so BPB reads come from [0x7C00 + offset].
;
; Job: walk the FAT32 root directory, find BOOTMGR, follow its cluster
; chain to load it at 2000:0000, then far-jump to it.
;
; Algorithm matches the legacy single-sector fat32_pbr_bootmgr.asm — the
; FAT walking logic is identical; only the load address differs.
;
; Clean-room: written from FAT32 spec (FATGEN103) + Phoenix BIOS docs.

BITS 16
ORG 0x7E00

%define BPB_BytsPerSec   0x0B
%define BPB_SecPerClus   0x0D
%define BPB_RsvdSecCnt   0x0E
%define BPB_NumFATs      0x10
%define BPB_HiddSec      0x1C
%define BPB_FATSz32      0x24
%define BPB_RootClus     0x2C

%define BUF              0x0500       ; 1-sector scratch
%define DAP              0x0700       ; disk address packet
%define BOOT_DRV         0x7B00       ; byte (shared with stage 1)
%define DATA_LBA         0x7B04       ; dword
%define FAT_LBA          0x7B08       ; dword
%define READ_LBA         0x7B0C       ; dword
%define BOOTMGR_SEG      0x2000

stage2:
    ; Setup is already done by stage 1, but DS/ES might be stale after
    ; the far-jump. Re-zero them for safety.
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    sti
    cld

    ; FAT_LBA = HiddSec + RsvdSecCnt
    mov eax, [0x7C00 + BPB_HiddSec]
    movzx ebx, word [0x7C00 + BPB_RsvdSecCnt]
    add eax, ebx
    mov [FAT_LBA], eax

    ; DATA_LBA = FAT_LBA + NumFATs * FATSz32
    mov cl, [0x7C00 + BPB_NumFATs]
    mov ebx, [0x7C00 + BPB_FATSz32]
.dmul:
    add eax, ebx
    dec cl
    jnz .dmul
    mov [DATA_LBA], eax

    ; Walk root directory looking for BOOTMGR. EAX = current cluster.
    mov eax, [0x7C00 + BPB_RootClus]

.dir_cluster:
    push eax                          ; save dir cluster
    sub eax, 2
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    push ecx
    mul ecx
    add eax, [DATA_LBA]
    mov [READ_LBA], eax
    pop ecx                           ; sectors remaining in cluster

.dir_sector:
    push cx
    xor bx, bx
    mov es, bx
    mov di, BUF
    call read_one_sector
    mov si, BUF
    mov cx, 16                        ; 16 entries per 512-byte sector
.scan:
    mov al, [si]
    test al, al
    jz .nf                            ; end-of-dir
    cmp al, 0xE5
    je .skip
    cmp byte [si + 11], 0x0F          ; LFN entry
    je .skip
    push si
    push cx
    mov di, name
    mov cx, 11
    repe cmpsb
    pop cx
    pop si
    je .found
.skip:
    add si, 32
    dec cx
    jnz .scan

    pop cx
    inc dword [READ_LBA]
    dec cx
    jnz .dir_sector

    pop eax                           ; dir cluster
    call next_cluster
    cmp eax, 0x0FFFFFF8
    jb .dir_cluster

.nf:
    mov al, '1'
    jmp die

.found:
    ; SI -> dir entry. Stack: dir_cluster (4B) + sectors-remaining (2B).
    add sp, 6

    ; cluster = (high << 16) | low
    movzx eax, word [si + 26]
    movzx ebx, word [si + 20]
    shl ebx, 16
    or eax, ebx

    ; Load BOOTMGR cluster chain to BOOTMGR_SEG:0000.
    mov bx, BOOTMGR_SEG
    mov es, bx
    xor di, di
.load:
    push eax
    call read_cluster
    pop eax
    call next_cluster
    cmp eax, 0x0FFFFFF8
    jb .load

    mov dl, [BOOT_DRV]
    jmp BOOTMGR_SEG:0x0000

read_cluster:
    sub eax, 2
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    mul ecx
    add eax, [DATA_LBA]
    mov [READ_LBA], eax
.lr:
    push cx
    call read_one_sector
    inc dword [READ_LBA]
    mov bx, es
    add bx, 0x20                       ; ES += 512 linear
    mov es, bx
    pop cx
    loop .lr
    ret

read_one_sector:
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
    mov al, '2'
    jmp die

next_cluster:
    push ecx
    push edx
    push es
    push di

    shl eax, 2
    movzx ecx, word [0x7C00 + BPB_BytsPerSec]
    xor edx, edx
    div ecx
    add eax, [FAT_LBA]
    mov [READ_LBA], eax

    xor bx, bx
    mov es, bx
    mov di, BUF
    push edx
    call read_one_sector
    pop edx
    mov eax, [BUF + edx]
    and eax, 0x0FFFFFFF

    pop di
    pop es
    pop edx
    pop ecx
    ret

; die: AL = single-char error code printed to BIOS teletype + COM1.
;   '1' = BOOTMGR not found in FAT32 root
;   '2' = INT 13h disk read failed
die:
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    mov dx, 0x3F8
    out dx, al
.h: hlt
    jmp .h

name:    db 'BOOTMGR    '

    times 512 - ($ - $$) db 0
