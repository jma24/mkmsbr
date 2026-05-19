; fat32_pbr_ntldr/sector1.asm — Stage 2 of the multi-sector NTLDR PBR.
;
; Loaded by stage 1 (sector0.asm) at 0x7E00. Entered via far-jump from
; stage 1 with DL = boot drive. Sector 0 (with BPB) is still resident
; at 0x7C00, so BPB reads come from [0x7C00 + offset].
;
; Job: walk the FAT32 root directory, find NTLDR, follow its cluster
; chain to load it at 2000:0000, then far-jump to it.
;
; Algorithm is identical to fat32_pbr_bootmgr/sector1.asm — only the
; name string ("NTLDR      " vs "BOOTMGR    ") and the load-segment
; label (NTLDR_SEG vs BOOTMGR_SEG) differ. Both load to segment 0x2000,
; the canonical NTLDR / BOOTMGR entry point.
;
; Disk reads use INT 13h fn 0x02 (CHS) via geometry that stage 1 probed
; and saved at [GEOM_SPT] / [GEOM_HEADS]. See sector0.asm for the
; rationale (USB-FDD-emulating BIOSes reject fn 0x42 with AH=01).

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
%define BOOT_DRV         0x7B00       ; byte (shared with stage 1)
%define GEOM_SPT         0x7B01       ; byte (shared with stage 1)
%define GEOM_HEADS       0x7B02       ; byte (shared with stage 1)
%define DATA_LBA         0x7B04       ; dword
%define FAT_LBA          0x7B08       ; dword
%define READ_LBA         0x7B0C       ; dword
%define NTLDR_SEG        0x2000

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

    ; Walk root directory looking for NTLDR. EAX = current cluster.
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

    ; Load NTLDR cluster chain to NTLDR_SEG:0000.
    mov bx, NTLDR_SEG
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
    jmp NTLDR_SEG:0x0000

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

; read_one_sector: read [READ_LBA] (1 sector) into ES:DI using CHS via
; INT 13h fn 0x02. Geometry comes from stage 1's probe at [GEOM_SPT] /
; [GEOM_HEADS]. ES:DI input is preserved for the caller (the existing
; convention); we translate to ES:BX internally because fn 0x02 takes
; the buffer pointer in BX.
read_one_sector:
    mov eax, [READ_LBA]
    ; sector_idx = LBA mod SPT; track = LBA / SPT
    xor edx, edx
    movzx ecx, byte [GEOM_SPT]
    div ecx
    push dx                            ; save sector_idx for later
    ; head = track mod HEADS; cyl = track / HEADS
    xor edx, edx
    movzx ecx, byte [GEOM_HEADS]
    div ecx
    pop bx                             ; bl = sector_idx (low byte of sector_idx)
    ; Pack INT 13h fn 0x02 registers.
    ;   CH = cyl[7:0], CL[7:6] = cyl[9:8], CL[5:0] = sector (1-indexed)
    ;   DH = head, DL = drive
    mov dh, dl                         ; DH = head
    inc bl                             ; sector = sector_idx + 1
    mov ch, al                         ; CH = cyl low byte
    mov cl, ah
    and cl, 0x03                       ; CL = cyl bits 9..8
    shl cl, 6                          ; ...shifted to CL bits 7..6
    or cl, bl                          ; OR in sector
    mov bx, di                         ; ES:BX = caller's destination
    mov ax, 0x0201                     ; AH=02 read, AL=1 sector
    mov dl, [BOOT_DRV]
    int 0x13
    jc .err
    ret
.err:
    ; AH holds BIOS status. die_io prints '2' + AH + READ_LBA.
    mov al, '2'
    jmp die_io

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

; Error handlers.
;
; die: AL = single-char error code, no status. Used for '1' (NTLDR not
; found) where there's no BIOS status to surface.
; die_io: AL = error letter, AH = BIOS status. Prints '<letter><AH>'
; then the 4-byte READ_LBA (8 hex chars) so we can see which absolute
; LBA the failed read targeted. Total 11 chars on screen.
die_io:
    push ax
    call print_char                    ; prints AL = error letter
    pop ax
    mov al, ah
    call print_byte_hex                ; AH = status
    mov al, [READ_LBA + 3]
    call print_byte_hex
    mov al, [READ_LBA + 2]
    call print_byte_hex
    mov al, [READ_LBA + 1]
    call print_byte_hex
    mov al, [READ_LBA + 0]
    call print_byte_hex
    jmp halt_loop

die:
    call print_char

halt_loop:
    hlt
    jmp halt_loop

print_char:
    push ax
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    pop ax
    mov dx, 0x3F8
    out dx, al
    ret

print_byte_hex:
    push ax
    shr al, 4
    call print_hex_nibble
    pop ax
    and al, 0x0F
    call print_hex_nibble
    ret

print_hex_nibble:
    cmp al, 10
    jb .pn_digit
    add al, 7                          ; gap between '9' (0x39) and 'A' (0x41)
.pn_digit:
    add al, '0'
    jmp print_char

name:    db 'NTLDR      '

    times 512 - ($ - $$) db 0
