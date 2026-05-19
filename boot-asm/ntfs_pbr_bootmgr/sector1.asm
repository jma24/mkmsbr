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
; B+tree handling: rather than descending via subnode pointers, we scan
; entries breadth-agnostic — first the inline entries in $INDEX_ROOT,
; then (if LARGE_INDEX flag is set and we still haven't matched) every
; INDX block in every run of $INDEX_ALLOCATION. Interior-node separator
; entries copy the leaf-level key, so the BOOTMGR filename surfaces in
; some block regardless of which level it lives at. Assumes
; IndexBlockSize == ClusterSize, which holds for the ntfs-3g default
; and Win 7 Setup's 4 KiB cluster layout.
;
; Fixups (USA): applied to every multi-sector record after read. NTFS
; stores a sentinel (the Update Sequence Number) at the last 2 bytes of
; each 512-byte sector of a FILE/INDX record and stashes the originals
; in the Update Sequence Array at header offset 0x04, sized in words at
; 0x06 (= 1 USN + N sector fixups). On the L2 fixture BOOTMGR's entry
; lives before offset 510 so the fixup is a no-op, but real Win 7 INDX
; blocks have entries straddling sector boundaries.
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
%define MFT_RUNS         0x7B20       ; 8 bytes/entry: LCN (4) + length_clusters (4);
                                      ; zero-length entry terminates. Up to ~28 runs
                                      ; before hitting 0x7C00.

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

    ; Bootstrap MFT_RUNS with a single huge entry mapping LCN = BPB.MftLcn
    ; so the first read_mft_rec (for record 0 itself) finds $MFT at the
    ; canonical contiguous-layout LBA. After we read record 0 we parse its
    ; $DATA runs and overwrite this table with the real extent map.
    mov di, MFT_RUNS
    mov eax, [0x7C00 + BPB_MftLcn]
    mov [di], eax                       ; bootstrap LCN
    mov dword [di + 4], 0x7FFFFFFF      ; pretend it spans the world
    xor eax, eax
    mov [di + 8], eax                   ; terminator LCN
    mov [di + 12], eax                  ; terminator length (the field
                                        ; read_mft_rec checks for end-of-table)

    ; Read $MFT's own self-describing record (MFT record 0), then walk
    ; its $DATA runs into MFT_RUNS so subsequent reads work for $MFTs
    ; that span multiple extents.
    xor eax, eax
    call read_mft_rec
    mov bx, BUF
    mov edx, 0x80
    call find_attr
    cmp byte [bx + 0x08], 0
    je mft_data_resident
    movzx ax, word [bx + 0x20]
    add bx, ax                          ; → $DATA run list
    mov di, MFT_RUNS
    xor ebp, ebp                        ; running absolute LCN
.mft_run:
    mov al, [bx]
    test al, al
    jz .mft_runs_done
    inc bx
    mov ah, al
    and al, 0x0F
    shr ah, 4
    push ax
    mov cl, al
    call read_le_unsigned               ; EDX = length (clusters)
    mov ecx, edx
    pop ax
    push ecx
    mov cl, ah
    call read_le_signed                 ; EDX = signed delta LCN
    add ebp, edx                        ; → absolute LCN of this run
    pop ecx
    mov [di], ebp
    mov [di + 4], ecx
    add di, 8
    jmp .mft_run
.mft_runs_done:
    xor eax, eax
    mov [di], eax
    mov [di + 4], eax                   ; terminator

    ; Read root MFT record (5).
    mov eax, 5
    call read_mft_rec

    ; Try $INDEX_ROOT (0x90) first. Small directories with no
    ; $INDEX_ALLOCATION keep every entry here; large ones use
    ; $INDEX_ROOT as the B+tree root only and set LARGE_INDEX.
    mov bx, BUF
    mov edx, 0x90
    call find_attr
    movzx ax, word [bx + 0x14]          ; ValueOffset (resident attr)
    add bx, ax                          ; BX → INDEX_ROOT value
    mov al, [bx + 0x1C]                 ; INDEX_HEADER.Flags
    push ax                             ; stash for fallthrough decision
    add bx, 0x10                        ; → INDEX_HEADER
    movzx ax, word [bx]                 ; EntriesOffset (rel to header)
    add bx, ax                          ; → first INDEX_ENTRY

.root_scan:
    test byte [bx + 0x0C], 0x02
    jnz .root_done
    cmp byte [bx + 0x50], 7
    jne .root_skip
    push bx
    lea si, [bx + 0x52]
    mov di, name
    mov cx, 14
    repe cmpsb
    pop bx
    je .root_hit
.root_skip:
    add bx, [bx + 0x08]
    jmp .root_scan

.root_hit:
    add sp, 2                           ; drop stashed flags
    mov eax, [bx]
    call read_mft_rec
    jmp .load_bootmgr

.root_done:
    pop ax                              ; recover INDEX_HEADER.Flags
    test al, 0x01                       ; LARGE_INDEX → INDEX_ALLOCATION exists
    jz .nf                              ; small dir, inline only, no BOOTMGR

    ; Find INDEX_ALLOCATION (0xA0). Non-resident.
    mov bx, BUF
    mov edx, 0xA0
    call find_attr
    movzx ax, word [bx + 0x20]
    add bx, ax                          ; BX → start of data-run list

    xor ebp, ebp                        ; running absolute LCN (= start LCN of
                                        ; the most recently parsed run; each
                                        ; run's signed delta is relative to it)

.idx_run:
    mov al, [bx]
    test al, al
    jz .nf                              ; runs exhausted, BOOTMGR not found
    inc bx
    mov ah, al
    and al, 0x0F
    shr ah, 4
    push ax                             ; save (length_bytes, offset_bytes)
    mov cl, al
    call read_le_unsigned               ; EDX = run length in clusters
    mov ecx, edx                        ; ECX = run length
    pop ax
    push cx                             ; save length (16-bit; assumes < 65536)
    mov cl, ah
    call read_le_signed                 ; EDX = signed delta LCN
    add ebp, edx                        ; EBP = absolute LCN of this run
    pop cx                              ; ECX low = clusters in run

    push bx                             ; preserve run-list pointer
    push ebp                            ; preserve run-start LCN

.idx_cluster:
    push cx                             ; outer cluster counter

    mov eax, ebp
    movzx ecx, byte [0x7C00 + BPB_SecPerClus]
    mul ecx
    add eax, [0x7C00 + BPB_HiddSec]
    mov [READ_LBA], eax

    mov ax, BUF_SEG
    mov es, ax
    xor di, di
    movzx cx, byte [0x7C00 + BPB_SecPerClus]
.idx_read:
    push cx
    call read_one
    inc dword [READ_LBA]
    mov ax, es
    add ax, 0x20
    mov es, ax
    pop cx
    loop .idx_read

    ; INDX block in BUF. Reset ES=0 so the `name:` label resolves via
    ; ES:DI in repe cmpsb below.
    xor ax, ax
    mov es, ax
    mov bx, BUF
    call apply_fixups
    add bx, 0x18
    movzx ax, word [bx]
    add bx, ax                          ; → first INDEX_ENTRY in this block

.idx_scan:
    test byte [bx + 0x0C], 0x02
    jnz .idx_block_done                 ; end-of-block marker; next block
    cmp byte [bx + 0x50], 7
    jne .idx_skip
    push bx
    lea si, [bx + 0x52]
    mov di, name
    mov cx, 14
    repe cmpsb
    pop bx
    je .idx_hit
.idx_skip:
    add bx, [bx + 0x08]
    jmp .idx_scan

.idx_block_done:
    inc ebp                             ; next cluster within this run
    pop cx                              ; restore outer counter
    loop .idx_cluster

    pop ebp                             ; restore run-start LCN for next delta
    pop bx                              ; restore run-list pointer
    jmp .idx_run

.nf:
    mov al, 'F'
    jmp die

.idx_hit:
    ; BX → matched INDEX_ENTRY in BUF; stack still holds outer-counter CX
    ; (2), run-start EBP (4), run-list BX (2). Discard before clobbering
    ; BX/EBP via read_mft_rec.
    mov eax, [bx]
    add sp, 8
    call read_mft_rec

.load_bootmgr:
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
    ; IN: EAX = MFT record number.
    ; Translates record → LBA via MFT_RUNS (populated at init from
    ; $MFT's own $DATA attribute) so this works for $MFTs that span
    ; multiple extents on disk.
    movzx ebx, word [REC_SECTORS]
    mul ebx                             ; EAX = sector index within $MFT
    mov si, MFT_RUNS
.find_run:
    mov ecx, [si + 4]                   ; length (clusters); 0 = terminator
    test ecx, ecx
    jz .mft_oob
    movzx ebx, byte [0x7C00 + BPB_SecPerClus]
    push eax
    mov eax, ecx
    mul ebx                             ; EAX = run length in sectors
    mov ecx, eax
    pop eax
    cmp eax, ecx
    jb .in_run
    sub eax, ecx
    add si, 8
    jmp .find_run
.in_run:
    ; LBA = HiddSec + run.LCN * SecPerClus + (sector offset within run).
    mov ecx, eax
    mov eax, [si]
    movzx ebx, byte [0x7C00 + BPB_SecPerClus]
    mul ebx
    add eax, ecx
    add eax, [0x7C00 + BPB_HiddSec]
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
    xor ax, ax
    mov es, ax
    mov bx, BUF
    call apply_fixups
    ret
.mft_oob:
    mov al, 'O'
    jmp die

; apply_fixups: walk the Update Sequence Array of a multi-sector NTFS
; record (FILE or INDX) and restore the original last-2-bytes of each
; 512-byte sector.
;
; IN:  BX = record offset (DS-relative), DS = 0.
; OUT: record at [BX] has last 2 bytes of each sector replaced by the
;      corresponding USA entry.
; Clobbers: nothing visible (saves via pusha + ES).
apply_fixups:
    pusha
    push es
    xor ax, ax
    mov es, ax
    mov si, bx
    mov ax, [bx + 4]                    ; USA offset within record
    add si, ax                          ; SI → USA[0] (the USN)
    mov cx, [bx + 6]                    ; USA size in words (USN + N)
    test cx, cx
    jz .done
    dec cx                              ; → N fixup entries
    jz .done
    add si, 2                           ; → USA[1] (first fixup)
    mov di, bx
    add di, 510                         ; → last 2 bytes of sector 0
.fx:
    movsw                               ; copy USA[i] over sector-end sentinel
    add di, 510                         ; advance to next sector-end - 2
    loop .fx
.done:
    pop es
    popa
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
;   'F' BOOTMGR not in INDX    'D' BOOTMGR $DATA resident (unsupported)
;   'M' $MFT $DATA resident    'O' MFT rec past end of run table
mft_data_resident:
    mov al, 'M'
    jmp die
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
