; fat32_pbr_ntldr/sector0.asm — Stage 1 of the multi-sector NTLDR PBR.
;
; Loaded by the MBR at 0000:7C00 in real mode. DL = boot drive.
;
; Job: read stage 2 (1 sector at partition LBA 2 = BPB.HiddSec + 2) to
; 0x7E00 via INT 13h fn 0x02 (CHS), then far-jump to 0x07E0:0x0000 =
; linear 0x7E00 where stage 2 takes over.
;
; Why CHS and not LBA-ext (fn 0x42): 2000s-era BIOSes that emulate USB
; sticks as USB-FDD reject fn 0x42 with AH=01 (invalid command).
; Confirmed 2026-05-19 on the Dell rig used for XP testing — diagnostic
; from the prior single-sector NTLDR PBR returned exactly that code.
; CHS via fn 0x02 works on every BIOS since the original IBM PC. The
; 8 GB CHS addressing limit doesn't bite us because NTLDR sits in the
; low LBAs of the partition (well under 8 GB), and once NTLDR loads it
; takes over the disk-access path itself.
;
; Why LBA+2 and not LBA+1: partition LBA 1 is the FAT32 FSInfo sector
; (BPB.FSInfo = 1 in newfs_msdos defaults). Clobbering it would
; invalidate the FSInfo signatures and force the FS driver to recompute
; free-cluster counts on first mount. ms-sys does the same: --fat32nt
; leaves LBA 1 alone and places stage-2 code at LBA 2. We mirror that.
;
; The BPB at offsets 3..89 is filesystem state, spliced by
; bootrec::splice_fat32_pbr_multi from the existing freshly-formatted
; partition.
;
; Identical in structure to fat32_pbr_bootmgr/sector0.asm — only the
; stage-2 payload differs (NTLDR string instead of BOOTMGR; same load
; segment 0x2000:0000).

BITS 16
ORG 0x7C00

%define BPB_HiddSec      0x1C
%define BOOT_DRV         0x7B00       ; one byte of low-RAM scratch
%define GEOM_SPT         0x7B01       ; sectors per track (1..63)
%define GEOM_HEADS       0x7B02       ; number of heads (1..256)

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

    ; Probe drive geometry via INT 13h fn 0x08. Spec says ES:DI should be
    ; 0:0 on entry to defend against a Phoenix BIOS bug; we already
    ; cleared ES above. fn 0x08 returns CL[5:0] = sectors per track and
    ; DH = max head index (heads = DH + 1). Stage 2 reuses the saved
    ; values via the same GEOM_* addresses, so this probe only runs once.
    ;
    ; Fallback: some legacy BIOSes (observed 2026-05-19 on the Dell XP
    ; rig — diagnostic G0100000F) reject fn 0x08 with AH=01 when the
    ; BIOS-handed drive number is one of their internal USB-emulation
    ; values (DL=0x0F in that case). The MBR's fn 0x42 read still works
    ; on the same drive number, so reads aren't dead — only the geometry
    ; query is. Fall back to the standard USB-FDD profile (SPT=18,
    ; HEADS=2) so stage 2's CHS reads have a geometry to convert with,
    ; and keep [BOOT_DRV] as the BIOS-handed value.
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

    ; Read stage 2: partition LBA 2 = HiddSec + 2.
    mov eax, [0x7C00 + BPB_HiddSec]
    add eax, 2
    mov bx, 0x7E00                    ; ES:BX = 0000:7E00
    call read_one_sector_chs
    jc io_error

    ; Hand off to stage 2. DL still holds boot drive number.
    mov dl, [BOOT_DRV]
    jmp 0x0000:0x7E00

; read_one_sector_chs: read 1 sector at absolute LBA EAX into ES:BX.
; Uses GEOM_SPT / GEOM_HEADS from low RAM. Trashes EAX, ECX, EDX. On
; return CF reflects INT 13h CF; on error AH = BIOS status code.
read_one_sector_chs:
    push bx                           ; preserve destination offset
    ; sector_idx = LBA mod SPT; track = LBA / SPT
    xor edx, edx
    movzx ecx, byte [GEOM_SPT]
    div ecx                           ; eax = LBA / SPT, edx = LBA mod SPT
    mov bx, dx                        ; bx (low byte) = sector_idx
    ; head = track mod HEADS; cylinder = track / HEADS
    xor edx, edx
    movzx ecx, byte [GEOM_HEADS]
    div ecx                           ; eax = cyl, edx = head (0..255)
    ; Pack INT 13h fn 0x02 register layout:
    ;   AH = 02, AL = 1
    ;   CH = cyl[7:0]; CL[7:6] = cyl[9:8]; CL[5:0] = sector (1-indexed)
    ;   DH = head, DL = drive
    ;   ES:BX = buffer
    mov dh, dl                        ; DH = head
    inc bl                            ; bl = sector (1..SPT)
    mov ch, al                        ; CH = cyl low
    mov cl, ah                        ; CL temp = cyl bits 15..8
    and cl, 0x03                      ; mask to cyl bits 9..8
    shl cl, 6                         ; move to CL bits 7..6
    or cl, bl                         ; CL = (cyl_hi << 6) | sector
    pop bx                            ; restore destination offset
    mov ax, 0x0201                    ; AH=02 read, AL=1 sector
    mov dl, [BOOT_DRV]
    int 0x13
    ret

; Error handlers. AH on entry holds the BIOS status code from the failed
; INT 13h call. Diagnostic format mirrors fat32_pbr_bootmgr/sector0.asm:
; '<letter><AH><SPT><heads><DL>' = 9 chars on screen. Letter identifies
; which call failed (G = geometry probe, R = stage-2 read).
geom_error:
    push ax
    mov al, 'G'
    call print_char
    jmp print_status

io_error:
    push ax
    mov al, 'R'
    call print_char
print_status:
    pop ax                            ; AH = BIOS status
    mov al, ah
    call print_byte_hex
    mov al, [GEOM_SPT]
    call print_byte_hex
    mov al, [GEOM_HEADS]
    call print_byte_hex
    mov al, [BOOT_DRV]
    call print_byte_hex
halt_loop:
    hlt
    jmp halt_loop

print_byte_hex:
    push ax
    shr al, 4
    call print_hex_nibble
    pop ax
    and al, 0x0F
    call print_hex_nibble
    ret

print_char:
    push ax
    mov ah, 0x0E
    mov bx, 7
    int 0x10
    pop ax
    mov dx, 0x3F8
    out dx, al
    ret

print_hex_nibble:
    cmp al, 10
    jb .pn_digit
    add al, 7                         ; gap between '9' (0x39) and 'A' (0x41)
.pn_digit:
    add al, '0'
    jmp print_char

    times 510 - ($ - $$) db 0
    dw 0xAA55
