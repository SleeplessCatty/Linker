#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEGACY_PATTERN='QuickSync|qsync|(^|[^[:alnum:]])qs([^[:alnum:]]|$)|(^|[^[:alnum:]])qsd([^[:alnum:]]|$)|com\.quicksync|QUICKSYNC_'

set +e
CONTENT_MATCHES="$(
  git -C "${ROOT_DIR}" grep -n -I -i -E "${LEGACY_PATTERN}" -- \
    . \
    ':(exclude)scripts/lib/cleanup-legacy.sh' \
    ':(exclude)scripts/tests/legacy-cleanup.sh' \
    ':(exclude)scripts/tests/install-safety.sh' \
    ':(exclude)scripts/tests/branding-residue.sh' \
    ':(exclude)docs/superpowers/plans/2026-08-21-linker-rename.md' \
)"
content_status=$?
set -e

if [[ "${content_status}" -gt 1 ]]; then
  echo "error: git grep failed while checking branding residue" >&2
  exit "${content_status}"
fi

TRACKED_PATHS="$(git -C "${ROOT_DIR}" ls-files)" || {
  echo "error: git ls-files failed while checking branding residue" >&2
  exit 1
}

set +e
PATH_MATCHES="$(printf '%s\n' "${TRACKED_PATHS}" | grep -i -E "${LEGACY_PATTERN}")"
path_status=$?
set -e

if [[ "${path_status}" -gt 1 ]]; then
  echo "error: path scan failed while checking branding residue" >&2
  exit "${path_status}"
fi

if [[ -n "${CONTENT_MATCHES}" || -n "${PATH_MATCHES}" ]]; then
  echo "Legacy branding remains:" >&2
  [[ -z "${CONTENT_MATCHES}" ]] || echo "${CONTENT_MATCHES}" >&2
  [[ -z "${PATH_MATCHES}" ]] || echo "${PATH_MATCHES}" >&2
  exit 1
fi

echo "branding residue test passed"
