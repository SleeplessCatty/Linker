#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEGACY_PATTERN='QuickSync|qsync|(^|[^[:alnum:]])qs([^[:alnum:]]|$)|(^|[^[:alnum:]])qsd([^[:alnum:]]|$)|com\.quicksync|QUICKSYNC_'

MATCHES="$(
  git -C "${ROOT_DIR}" grep -n -I -i -E "${LEGACY_PATTERN}" -- \
    . \
    ':(exclude)scripts/lib/cleanup-legacy.sh' \
    ':(exclude)scripts/tests/legacy-cleanup.sh' \
    ':(exclude)scripts/tests/branding-residue.sh' \
    ':(exclude)docs/superpowers/plans/2026-08-21-linker-rename.md' \
    || true
)"

if [[ -n "${MATCHES}" ]]; then
  echo "Legacy branding remains:" >&2
  echo "${MATCHES}" >&2
  exit 1
fi

echo "branding residue test passed"
