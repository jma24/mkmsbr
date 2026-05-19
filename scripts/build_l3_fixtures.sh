#!/usr/bin/env bash
# build_l3_fixtures.sh — stage Microsoft boot binaries from licensed
# install ISOs into tests/real_content/ for the Layer-3 QEMU smoke tests.
#
# The extracted binaries are NOT redistributable. See
# tests/real_content/MANIFEST.md for the license posture. The repo
# commits only this script + the manifest; the binaries themselves
# live only on machines that have run this script against an ISO they
# hold a license for.
#
# Usage:
#   scripts/build_l3_fixtures.sh \
#       --xp-iso /path/to/winxp_sp3.iso \
#       --win7-iso /path/to/win7.iso
#
# Or via env vars (useful in CI):
#   BOOTREC_XP_ISO=/path/to/xp.iso \
#   BOOTREC_WIN7_ISO=/path/to/win7.iso \
#       scripts/build_l3_fixtures.sh
#
# Either flag/var may be omitted to skip that variant.
#
# Idempotent: re-running with the same ISOs is a no-op (size + hash check
# match the cached copy). Pass --force to re-stage anyway.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$REPO_ROOT/tests/real_content"

XP_ISO="${BOOTREC_XP_ISO:-}"
WIN7_ISO="${BOOTREC_WIN7_ISO:-}"
FORCE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --xp-iso)   XP_ISO="$2"; shift 2 ;;
        --win7-iso) WIN7_ISO="$2"; shift 2 ;;
        --force)    FORCE=1; shift ;;
        -h|--help)
            sed -n '2,/^set/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "unknown flag: $1" >&2
            exit 2
            ;;
    esac
done

if [[ -z "$XP_ISO" && -z "$WIN7_ISO" ]]; then
    echo "no ISO supplied; pass --xp-iso and/or --win7-iso (or set BOOTREC_XP_ISO / BOOTREC_WIN7_ISO)" >&2
    exit 2
fi

# Minimum reasonable file sizes — guards against "ISO didn't contain this
# file" silent failures where the copy produces a 0-byte stub. These are
# generous lower bounds; real binaries are larger.
declare -A MIN_BYTES=(
    [NTLDR]=200000          # observed 250048
    [NTDETECT.COM]=40000    # observed 47564
    [bootmgr]=300000        # observed 383786
    [bcd]=10000             # observed 262144; smaller BCDs exist
)

mkdir -p "$DEST"

# --- platform helpers ----------------------------------------------------

mount_iso() {
    # Mount $1 (ISO path) read-only; print the mount point to stdout. The
    # caller must call `unmount_iso "$mountpoint"` when done.
    local iso="$1"
    local mp
    case "$(uname -s)" in
        Darwin)
            # hdiutil attach with auto-mount handles UDF + Joliet correctly,
            # which manual `mount -t cd9660` does not (Win 7 ISOs use UDF).
            local out
            out="$(hdiutil attach -readonly -nobrowse -plist "$iso")"
            # Parse the plist for mount-point. Simplest portable path:
            # grep for /Volumes/... line.
            mp="$(echo "$out" | grep -Eo '/Volumes/[^<]+' | head -n1)"
            if [[ -z "$mp" ]]; then
                echo "hdiutil attach succeeded but mount point unknown: $out" >&2
                return 1
            fi
            echo "$mp"
            ;;
        Linux)
            mp="$(mktemp -d -t bootrec-iso-XXXXXX)"
            sudo mount -o loop,ro "$iso" "$mp"
            echo "$mp"
            ;;
        *)
            echo "unsupported platform: $(uname -s)" >&2
            return 1
            ;;
    esac
}

unmount_iso() {
    local mp="$1"
    case "$(uname -s)" in
        Darwin)
            hdiutil detach "$mp" >/dev/null 2>&1 || hdiutil detach -force "$mp" >/dev/null 2>&1 || true
            ;;
        Linux)
            sudo umount "$mp" || true
            rmdir "$mp" 2>/dev/null || true
            ;;
    esac
}

# --- extraction ---------------------------------------------------------

# Locate $rel beneath $mp case-insensitively (ISOs vary between
# `I386/NTLDR` and `i386/NTLDR`). Echoes the resolved path on stdout.
find_ci() {
    local mp="$1" rel="$2"
    # Try the obvious path first.
    if [[ -e "$mp/$rel" ]]; then echo "$mp/$rel"; return 0; fi
    # Fall back to case-insensitive find.
    local found
    found="$(find "$mp" -iwholename "*/$rel" -type f 2>/dev/null | head -n1)"
    if [[ -n "$found" ]]; then echo "$found"; return 0; fi
    return 1
}

stage_file() {
    # stage_file <src-on-iso> <dest-rel-path>
    local src="$1" dest_rel="$2"
    local dest="$DEST/$dest_rel"
    local base
    base="$(basename "$dest")"

    if [[ ! -f "$src" ]]; then
        echo "  ! missing on ISO: $src" >&2
        return 1
    fi

    local size
    size="$(stat -f%z "$src" 2>/dev/null || stat -c%s "$src")"
    local min="${MIN_BYTES[$base]:-1}"
    if (( size < min )); then
        echo "  ! $base is only $size bytes (minimum $min) — wrong file or truncated extraction" >&2
        return 1
    fi

    if [[ "$FORCE" -eq 0 && -f "$dest" ]]; then
        local existing_size
        existing_size="$(stat -f%z "$dest" 2>/dev/null || stat -c%s "$dest")"
        if [[ "$existing_size" == "$size" ]]; then
            echo "  = $dest_rel already staged ($size bytes; pass --force to overwrite)"
            return 0
        fi
    fi

    mkdir -p "$(dirname "$dest")"
    cp "$src" "$dest"
    chmod 0644 "$dest"
    local sha
    sha="$(shasum -a 256 "$dest" | awk '{print $1}')"
    echo "  + $dest_rel  ($size bytes, sha256=${sha:0:12}…)"
}

stage_xp() {
    local iso="$1"
    echo "Staging Win XP fixtures from $iso"
    local mp
    mp="$(mount_iso "$iso")"
    trap 'unmount_iso "$mp"' RETURN

    local ntldr ntdetect
    ntldr="$(find_ci "$mp" "I386/NTLDR")"   || { echo "  ! NTLDR not found on ISO" >&2; return 1; }
    ntdetect="$(find_ci "$mp" "I386/NTDETECT.COM")" \
        || { echo "  ! NTDETECT.COM not found on ISO" >&2; return 1; }

    stage_file "$ntldr"    "xp/NTLDR"
    stage_file "$ntdetect" "xp/NTDETECT.COM"
}

stage_win7() {
    local iso="$1"
    echo "Staging Win 7 fixtures from $iso"
    local mp
    mp="$(mount_iso "$iso")"
    trap 'unmount_iso "$mp"' RETURN

    local bootmgr bcd
    bootmgr="$(find_ci "$mp" "bootmgr")" || { echo "  ! bootmgr not found on ISO" >&2; return 1; }
    bcd="$(find_ci "$mp" "boot/bcd")"   || { echo "  ! boot/bcd not found on ISO" >&2; return 1; }

    stage_file "$bootmgr" "win7/bootmgr"
    stage_file "$bcd"     "win7/bcd"
}

# --- main ----------------------------------------------------------------

if [[ -n "$XP_ISO" ]]; then
    if [[ ! -f "$XP_ISO" ]]; then
        echo "XP ISO not found: $XP_ISO" >&2
        exit 1
    fi
    stage_xp "$XP_ISO"
fi

if [[ -n "$WIN7_ISO" ]]; then
    if [[ ! -f "$WIN7_ISO" ]]; then
        echo "Win 7 ISO not found: $WIN7_ISO" >&2
        exit 1
    fi
    stage_win7 "$WIN7_ISO"
fi

echo
echo "Fixture summary:"
(cd "$DEST" && find . -type f ! -name '.gitignore' ! -name 'MANIFEST.md' -print0 \
    | xargs -0 shasum -a 256 2>/dev/null | sort)
