; fake_pbr.asm — fake Partition Boot Record for MBR smoke test.
; Loaded by bootrec's MBR at 0000:7C00 with DL = boot drive.
; Prints "BOOTREC MBR OK\r\n" to BIOS teletype + COM1 then halts.
; Must be exactly 512 bytes with 0xAA55 signature at offset 510
; (the MBR validates the signature before chain-loading).
;
; NOT shipped to users; only built when running the integration tests.

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

    mov si, msg
.loop:
    lodsb
    or al, al
    jz .done
    ; BIOS teletype
    mov ah, 0x0E
    mov bh, 0x00
    int 0x10
    ; Serial COM1
    mov dx, 0x3F8
    out dx, al
    jmp .loop

.done:
    cli
.hang:
    hlt
    jmp .hang

msg: db 'BOOTREC MBR OK', 13, 10, 0

    times 510 - ($ - $$) db 0
    dw 0xAA55
