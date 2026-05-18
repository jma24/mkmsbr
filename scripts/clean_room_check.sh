#!/usr/bin/env bash
# Forbidden-symbol grep for the clean-room boot-code regions.
# See docs/SPEC.md §Clean-room mechanisms #3 (forbidden-symbol grep).
#
# Fails the build if any of the patterns below appear in src/ or boot-asm/.
# Catches the dumbest "I copied a block of bytes from that .h file" leaks.
#
# Usage:
#   ./scripts/clean_room_check.sh
#
# Exit 0 = clean; exit 1 = forbidden pattern hit (with location printed).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CHECK_DIRS=("$REPO_ROOT/src" "$REPO_ROOT/boot-asm")

# Patterns that should NEVER appear in bootrec source files. The grep is
# case-insensitive. Test files under tests/oracle/ are excluded because
# they legitimately wrap ms-sys as a black-box subprocess.
PATTERNS=(
  "ms-sys"
  "mssys"
  "syslinux"
  "ldlinux"
  "grub4dos"
  "ilko-y"           # ms-sys's WaitBT author; flag any reference
  "/\\* extracted from"
  "// extracted from"
  "; extracted from"
)

failed=0
# Restrict to source-code extensions. README.md and other docs may
# legitimately name ms-sys when describing the eval methodology; the
# forbidden-symbol gate is for the actual code that ships.
INCLUDE_GLOBS=(
  --include='*.rs'
  --include='*.asm'
  --include='*.S'
  --include='*.s'
  --include='*.c'
  --include='*.h'
)
for dir in "${CHECK_DIRS[@]}"; do
  if [[ ! -d "$dir" ]]; then
    continue
  fi
  for pattern in "${PATTERNS[@]}"; do
    if matches=$(grep -RIin --color=never "${INCLUDE_GLOBS[@]}" "$pattern" "$dir" 2>/dev/null); then
      echo "FORBIDDEN PATTERN: '$pattern'"
      echo "$matches" | sed 's/^/  /'
      failed=1
    fi
  done
done

if [[ $failed -ne 0 ]]; then
  echo
  echo "Clean-room check FAILED. Forbidden patterns above must be removed"
  echo "from src/ or boot-asm/. (Test harness files under tests/ may reference"
  echo "ms-sys legitimately — those are not scanned.)"
  exit 1
fi

echo "Clean-room check: src/ and boot-asm/ are clean."
