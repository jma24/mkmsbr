#!/usr/bin/env bash
# Forbidden-symbol grep for the clean-room boot-code regions.
# See docs/SPEC.md §Clean-room mechanisms #3 (forbidden-symbol grep).
#
# Two tiers:
#
#   HARD patterns — forbidden anywhere, including comments. These are
#     smoking-gun indicators of code derivation (a contributor cut & pasted
#     bytes and didn't fully redact the provenance trail). Comments that
#     mention "extracted from <upstream>" are caught at this tier.
#
#   SOFT patterns — forbidden in code/data but allowed in comments. Project
#     names like "ms-sys" appear legitimately in engineering notes that
#     compare byte-distance against ms-sys output, document why we made a
#     particular byte-level choice for compatibility, etc. The script
#     strips line comments (`;` for NASM, `//` for Rust/C) before grepping
#     so those notes don't trip the gate. The names must NOT appear in
#     code identifiers, string literals, or data — only in the natural-
#     language documentation strands around the clean-room work.
#
# Usage:
#   ./scripts/clean_room_check.sh
#
# Exit 0 = clean; exit 1 = forbidden pattern hit (with location printed).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECK_DIRS=("$REPO_ROOT/src" "$REPO_ROOT/boot-asm")

HARD_PATTERNS=(
  "/\\* extracted from"
  "// extracted from"
  "; extracted from"
  "ilko-y"           # ms-sys's WaitBT author; any reference is a leak
)

SOFT_PATTERNS=(
  "ms-sys"
  "mssys"
  "syslinux"
  "ldlinux"
  "grub4dos"
)

INCLUDE_GLOBS=(
  --include='*.rs'
  --include='*.asm'
  --include='*.S'
  --include='*.s'
  --include='*.c'
  --include='*.h'
)

# Strip line comments. Block comments (/* */) in Rust/C aren't handled —
# we don't use them for clean-room notes (doc comments are //... or ///...).
# Conservative bias: a SOFT pattern inside an unhandled block comment
# would trip the gate, which we'd rather see than miss.
strip_comments() {
  local file="$1"
  case "$file" in
    *.asm|*.S|*.s) sed 's/;.*//' "$file" ;;
    *.rs|*.c|*.h)  sed 's|//.*||' "$file" ;;
    *)             cat "$file" ;;
  esac
}

failed=0

# HARD tier: grep raw files (comments included).
for dir in "${CHECK_DIRS[@]}"; do
  [[ ! -d "$dir" ]] && continue
  for pattern in "${HARD_PATTERNS[@]}"; do
    if matches=$(grep -RIin --color=never "${INCLUDE_GLOBS[@]}" "$pattern" "$dir" 2>/dev/null); then
      echo "FORBIDDEN PATTERN (any context): '$pattern'"
      echo "$matches" | sed 's/^/  /'
      failed=1
    fi
  done
done

# SOFT tier: grep comment-stripped views. Walk files explicitly so we can
# strip comments per-file before grepping.
for dir in "${CHECK_DIRS[@]}"; do
  [[ ! -d "$dir" ]] && continue
  while IFS= read -r -d '' file; do
    stripped=$(strip_comments "$file")
    for pattern in "${SOFT_PATTERNS[@]}"; do
      if hits=$(echo "$stripped" | grep -in --color=never -- "$pattern"); then
        echo "FORBIDDEN PATTERN (code/data, comments stripped): '$pattern' in $file"
        echo "$hits" | sed 's/^/  /'
        failed=1
      fi
    done
  done < <(find "$dir" \
    \( -name '*.rs' -o -name '*.asm' -o -name '*.S' \
       -o -name '*.s' -o -name '*.c' -o -name '*.h' \) \
    -print0)
done

if [[ $failed -ne 0 ]]; then
  echo
  echo "Clean-room check FAILED. Forbidden patterns above must be removed"
  echo "from src/ or boot-asm/. (Test harness files under tests/ may reference"
  echo "ms-sys legitimately — those are not scanned.)"
  exit 1
fi

echo "Clean-room check: src/ and boot-asm/ are clean."
