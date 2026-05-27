# Boot-blob provenance

bootsmith embeds three small (~512-byte) chunks of x86 real-mode boot code: an MBR loader, a FAT32 PBR loader, and an NTFS PBR loader. This document records where those bytes come from and why we're confident shipping them under MIT.

## Source: hand-written NASM in this repo

The bytes shipped in `target/release/bootsmith` are produced at build time from the NASM source files under [`boot-asm/`](../boot-asm/). Those files are original work by the bootsmith authors and are licensed MIT, identical to the rest of this codebase.

`boot-asm/Makefile` invokes `nasm` and emits 512-byte raw binaries. `crates/bootsmith-boot/build.rs` runs the makefile and `include_bytes!`s the results into the compiled binary. There is no external dependency at runtime; NASM is a build-time tool only.

## Clean-room development protocol

The boot record source files are developed under a strict clean-room protocol. Anyone working on `boot-asm/*.asm` (or on `crates/bootsmith-boot/src/pbr.rs`) may consult **only** these sources:

**Allowed references:**
- Microsoft's *FAT32 File System Specification* (FATGEN103.doc / FATGEN102.pdf, publicly published by Microsoft in 2000 and republished many times since).
- IBM/Phoenix *BIOS Interface Reference* and equivalent public BIOS documentation (covering INT 10h, INT 13h, INT 13h extensions, the boot process).
- The OSDev wiki's algorithmic descriptions (text and pseudocode only, never their example code blocks).
- Generic textbook x86 assembly references (e.g. Intel software developer manuals).
- Output bytes from third-party tools, used **for verification only** (see "Cross-check" below).

**Disallowed references:**
- Source code from ms-sys, syslinux, GRUB, GRUB4DOS, Linux kernel boot code, BSD bootloaders, or any other open-source bootloader project.
- Any leaked, reverse-engineered, or disassembled Microsoft, Apple, or third-party proprietary code.
- Stack Overflow or blog-post code that itself derives from any of the above (when in doubt, treat external code blocks as tainted and use only the prose/pseudocode portions).

The rule is: **read others' output for validation; derive nothing from others' source.** Validation tells us "is the byte we produced equivalent to what's already in the field?" Derivation imports authorship questions we don't want to inherit.

When in doubt about a reference's status, stop and document the question in a PR comment rather than incorporating uncertain code.

## Cross-check: ms-sys equivalence test

[ms-sys](https://ms-sys.sourceforge.net/) ships boot record blobs that are functionally identical (and historically derived from Microsoft binaries). We do **not** redistribute ms-sys's bytes. We do, however, run an optional test that asserts our NASM output is byte-equal to ms-sys's reference blobs:

```sh
# one-time: clone ms-sys somewhere
git clone https://gitlab.com/cmaiolino/ms-sys.git /tmp/ms-sys
export BOOTSMITH_MSSYS_BLOBS_DIR=/tmp/ms-sys/inc

cargo test --features compare-mssys
```

This test is gated behind a feature flag and an env var, so the default `cargo test` invocation neither depends on ms-sys nor accesses it. The check exists because byte-equality vs ms-sys is the tightest possible "does our NASM work?" feedback loop — if our hand-written assembly produces the same bytes as code that's shipped to millions of users for two decades, we're done verifying.

## Why not just ship ms-sys's bytes?

Three reasons, in increasing order of importance:

1. **License clarity.** ms-sys is GPL-2.0. The boot record blobs inside its source tree are header-file byte arrays derived from Microsoft binaries (XP/Vista/7 era). The exact license status of those arrays — when extracted from ms-sys and embedded in someone else's project — has been argued for years without a clean answer. Writing our own NASM sidesteps the question.
2. **Maintainability.** Shipping opaque blobs means future bug fixes require reverse-engineering. Shipping NASM source means future fixes are diffs.
3. **Pride.** This is a tool that solves a real, persistent gap. It deserves its own first-party boot code.

## SHA-256 of expected bytes

These are the SHA-256 hashes of the boot blobs as of the v1.0.1 tag.
They match the bytes shipped in [`blobs-prebuilt/`](../blobs-prebuilt)
and the bytes produced by `nasm` against the `boot-asm/` sources at
the same tag.

```
mbr_xp.bin                    884328001d1ada748453b0d670252058bd18e00ca4eebf2d5f026cbda4b7a07b
mbr_win7.bin                  25eb0b47025d4db19b20bfae1c70d0efde6dd504277e0c2a0c0b752bb5ebaa67
fat32_pbr_bootmgr.bin         dc65a7a25725e251a579b5e4929603939f544eebfff93f97c854c91372429e4a
fat32_pbr_bootmgr_multi.bin   023bb589b05bda3b67684caaee68adc39f98740a1d7cceb37f28c8d30fb709e2
fat32_pbr_ntldr_multi.bin     28550dbe96c3049ffb36a15122c3f93da3ad7f95eba7a24fb069289e0c838b6f
ntfs_pbr_bootmgr_multi.bin    0b7e479a72ecd020133c0b1eaddfe6b6e54337124a97a133e42c17f53a5c3dac
xp_setup_chain_bootsect.bin   a1f10692b88e64b5a4b7aedec8dd2f1276eb8f671fa8c103c52ab809fd5c729c
```

Regenerate from a fresh checkout:

```sh
cd blobs-prebuilt && shasum -a 256 *.bin
```

If those hashes don't match what's printed above, either `boot-asm/`
changed (intentional — bump the version + update this table) or
something drifted in NASM (unintentional — investigate).

## What if Microsoft objects?

Their boot code is ~440 bytes of x86 that does the obvious thing — read BPB, walk FAT, find `bootmgr`, load it. The space of "correct implementations" is small enough that two competent engineers writing this code independently will produce nearly-identical bytes. The bytes are not creative expression; they're the unique correct way to do a constrained task. We're confident in the originality of our NASM source.

If we're ever asked to take it down, the audit trail (NASM source, git history, this document) demonstrates we wrote it ourselves.
