# blobs-prebuilt/

Pre-assembled boot-code blobs that ship in the published crate so
`cargo install mkmsbr` works on hosts without `nasm`. The bytes match
the NASM sources in `boot-asm/` at the same release tag.

Regenerating after a `boot-asm/` edit:

```sh
cargo clean
cargo build --release        # invokes nasm via build.rs
SRC=$(find target/release/build/mkmsbr-*/out -maxdepth 1 -type d | head -1)
for f in mbr_xp.bin mbr_win7.bin fat32_pbr_bootmgr.bin \
         fat32_pbr_bootmgr_multi.bin fat32_pbr_ntldr_multi.bin \
         ntfs_pbr_bootmgr_multi.bin xp_setup_chain_bootsect.bin; do
    cp "$SRC/$f" "blobs-prebuilt/$f"
done
```

Build-script behavior (`build.rs`):

1. If `nasm` is on PATH **and** `boot-asm/` sources exist → assemble
   from source (developer path; picks up any edits).
2. Otherwise → copy the prebuilt blob here into `$OUT_DIR` (install
   path; no nasm needed).
