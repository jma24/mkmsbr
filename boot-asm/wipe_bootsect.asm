; wipe_bootsect.asm — single-sector destructive-wipe bootsector.
;
; Purpose: zero the first 1 MiB (LBA 0..2047) of a target disk so a
; Windows XP install lands on a virgin disk instead of one with stale
; GPT headers, foreign partition tables, or other leftovers XP's
; text-mode setup is too forgiving about. Wipes MBR + protective-MBR
; slack + the first ~1 MiB of any GPT primary header. The GPT backup
; at the disk's last LBA is left intact (XP doesn't speak GPT so it
; can't be confused by an orphan GPT backup).
;
; Loaded by NTLDR via a boot.ini bootsector-file entry. NTLDR reads
; the first 512 bytes of \WIPE.DAT from the partition root, places
; them at 0000:7C00, and far-jumps with DL = boot drive (the USB).
;
; --- Safety model ---
;
; We must never wipe the disk we're booting from (= DL). BIOS hands
; us the USB's drive number in DL; we save it and treat it as
; immutable. Target = DL XOR 1 (the other primary disk slot — flips
; 0x80 ↔ 0x81). This handles both BIOS conventions: USB-at-0x80 +
; HDD-at-0x81 (the common case) and the inverted layout some legacy
; BIOSes use.
;
; Before any destructive operation:
;   1. INT 13h fn 0x41 (check extensions) on the target. If the
;      drive doesn't exist or doesn't support extended I/O, abort
;      with "No HDD".
;   2. INT 13h fn 0x48 (get extended drive params) to read the
;      target's total sector count. Convert to MiB (sectors / 2048,
;      assuming 512-byte sectors) and display.
;   3. Display target drive number, target size in MiB, and USB
;      drive number (annotated "safe — will NOT be wiped") so the
;      user can sanity-check the read-out matches the hardware they
;      expect to wipe.
;   4. Single Y/y to confirm. Any other key cancels.
;
; --- Multi-disk limitation ---
;
; This v1 wipes exactly DL XOR 1. A machine with multiple internal
; disks (USB + two HDDs at 0x81, 0x82) gets only one of them wiped.
; The Dell E6410 reference rig has one internal HDD so this is fine
; for now; a v2 would enumerate 0x80..0x87 and present a picker
; (probably requires a multi-sector bootsector since a picker UI
; doesn't fit in 512 bytes alongside everything else).
;
; --- Size budget ---
;
; Code, data, strings, and the 16-byte DAP + 30-byte EDP all live in
; this 512-byte sector. Strings are deliberately terse.

BITS 16
ORG 0x7C00

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    cld

    ; Save BIOS-supplied DL (= USB drive, the one we MUST NOT wipe).
    mov [usb_drive], dl

    ; Compute target = DL XOR 1. The XOR flips the low bit, mapping
    ; 0x80 ↔ 0x81. Works regardless of which BIOS slot the USB
    ; landed in.
    xor dl, 1
    mov [target_drive], dl

    ; Zero the 512-byte work buffer at 0000:0500. The DAP points here;
    ; INT 13h fn 0x43 writes whatever bytes the buffer contains, so
    ; pre-zero is mandatory.
    mov di, 0x0500
    mov cx, 256                 ; 256 words = 512 bytes
    xor ax, ax
    rep stosw

    ; Print intro.
    mov si, msg_intro
    call puts

    ; --- Probe target with INT 13h fn 0x41 (check extensions) ---
    mov ah, 0x41
    mov bx, 0x55AA
    mov dl, [target_drive]
    int 0x13
    jc no_target
    cmp bx, 0xAA55
    jne no_target

    ; --- Read target's drive parameters (INT 13h fn 0x48) ---
    ; edp.size is pre-initialised to 0x1A in the data section below.
    mov ah, 0x48
    mov dl, [target_drive]
    mov si, edp
    int 0x13
    jc no_target

    ; Display "target=0xNN size=NNNN MiB"
    mov si, msg_target_eq
    call puts
    mov al, [target_drive]
    call print_hex
    mov si, msg_size_eq
    call puts

    ; Compute MiB = (low 32 bits of total sectors) / 2048, using 32-bit
    ; DIV. A 16-bit DIV (`div bx` with DX:AX/BX) overflows on any disk
    ; bigger than ~64 GB because the MiB quotient doesn't fit in AX —
    ; on real hardware the resulting #DE leaves the CPU at the INT 0
    ; vector with a flashing cursor. E6410 is 386+ so the operand-size
    ; prefix is free.
    xor edx, edx
    mov eax, [edp + 0x10]       ; low 32 bits of total sector count
    mov ebx, 2048
    div ebx                     ; EAX = sectors / 2048 = MiB
    call print_dec

    mov si, msg_mib_usb
    call puts
    mov al, [usb_drive]
    call print_hex
    mov si, msg_safe_prompt
    call puts

    ; Wait for keypress.
    xor ah, ah
    int 0x16
    or al, 0x20                 ; tolower-ish
    cmp al, 'y'
    jne cancel

    ; --- Wipe loop ---
    mov si, msg_wipe
    call puts

    mov cx, 2048                ; LBA 0..2047
.wipe_loop:
    push cx
    mov ah, 0x43                ; extended write
    mov al, 0                   ; no verify
    mov dl, [target_drive]
    mov si, dap
    int 0x13
    pop cx
    jc wipe_err
    inc word [dap_lba_lo]
    loop .wipe_loop

    mov si, msg_done
    call puts
    jmp reboot

no_target:
    mov si, msg_no_hdd
    call puts
    jmp reboot

wipe_err:
    mov si, msg_err
    call puts
    jmp reboot

cancel:
    mov si, msg_cancel
    call puts

reboot:
    mov si, msg_keyrb
    call puts
    xor ah, ah
    int 0x16
    int 0x19                    ; warm reboot

; --- BIOS teletype puts (SI -> NUL-terminated string) ---
puts:
.lp:
    lodsb
    test al, al
    jz .end
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    jmp .lp
.end:
    ret

; --- Print byte in AL as two hex digits ---
print_hex:
    push ax
    push cx
    mov cx, 2
.lp:
    rol al, 4                   ; cycle high nibble into low
    push ax
    and al, 0x0F
    add al, '0'
    cmp al, '9'
    jbe .ok
    add al, 7                   ; 'A' - '9' - 1
.ok:
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    pop ax
    loop .lp
    pop cx
    pop ax
    ret

; --- Print unsigned 32-bit decimal in EAX ---
; Uses 32-bit DIV so values up to 2^32-1 print correctly. A 16-bit
; print_dec overflows at 65536 — drives larger than ~64 GiB (any
; modern HDD) hit this in real-world use.
print_dec:
    push eax
    push ebx
    push cx
    push edx
    xor cx, cx                  ; digit count
    mov ebx, 10
.div:
    xor edx, edx
    div ebx                     ; EAX /= 10, EDX = remainder (single digit, fits in DL)
    push dx                     ; remainder is 0..9 so only DL matters
    inc cx
    test eax, eax
    jnz .div
.printloop:
    pop ax
    add al, '0'
    mov ah, 0x0E
    mov bx, 0x0007
    int 0x10
    loop .printloop
    pop edx
    pop cx
    pop ebx
    pop eax
    ret

; --- Data ---

usb_drive:    db 0
target_drive: db 0

; Disk Address Packet for INT 13h fn 0x43 (extended write).
dap:
    db 0x10                     ; size
    db 0                        ; reserved
    dw 1                        ; sector count
    dw 0x0500                   ; buffer offset
    dw 0x0000                   ; buffer segment
dap_lba_lo:   dw 0              ; bits  0..15
dap_lba_mid:  dw 0              ; bits 16..31
dap_lba_hi:   dw 0              ; bits 32..47
dap_lba_top:  dw 0              ; bits 48..63

; Extended Drive Parameters buffer for INT 13h fn 0x48.
; First word is the buffer size (pre-initialised; BIOS reads this on
; entry and writes the response into the rest). 0x1A = 26 bytes,
; the v2.x layout which is sufficient for our total-sectors read.
edp:
    dw 0x1A
    times (0x1A - 2) db 0

msg_intro:       db 13, 10, 'USBWIN WIPE', 13, 10, 0
msg_target_eq:   db 'target=0x', 0
msg_size_eq:     db ' size=', 0
msg_mib_usb:     db ' MiB', 13, 10, 'USB=0x', 0
msg_safe_prompt: db ' safe', 13, 10, 'Y=wipe? ', 0
msg_wipe:        db 13, 10, 'Wiping... ', 0
msg_done:        db 'OK', 13, 10, 0
msg_err:         db 'ERR', 13, 10, 0
msg_cancel:      db 13, 10, 'skip', 13, 10, 0
msg_no_hdd:      db 13, 10, 'no target', 13, 10, 0
msg_keyrb:       db 13, 10, 'key=reboot', 0

    times 510 - ($ - $$) db 0
    dw 0xAA55
