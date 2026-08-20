#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${ROOT_DIR}/scripts/install-remote.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

TEST_ROOT="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "${TEST_ROOT}"' EXIT
SOURCE_REPO="${TEST_ROOT}/source repo & origin"

mkdir -p "${SOURCE_REPO}"
git -C "${SOURCE_REPO}" init -q
git -C "${SOURCE_REPO}" checkout -q -b main
git -C "${SOURCE_REPO}" config user.name "Linker Test"
git -C "${SOURCE_REPO}" config user.email "linker-test@example.invalid"
printf 'main\n' > "${SOURCE_REPO}/version.txt"
git -C "${SOURCE_REPO}" add version.txt
git -C "${SOURCE_REPO}" commit -q -m main
MAIN_SHA="$(git -C "${SOURCE_REPO}" rev-parse HEAD)"
git -C "${SOURCE_REPO}" tag v0.2.0

git -C "${SOURCE_REPO}" checkout -q -b feature
printf 'feature\n' > "${SOURCE_REPO}/version.txt"
git -C "${SOURCE_REPO}" commit -q -am feature
FEATURE_SHA="$(git -C "${SOURCE_REPO}" rev-parse HEAD)"

for ref_and_sha in \
  "main|${MAIN_SHA}" \
  "v0.2.0|${MAIN_SHA}" \
  "${FEATURE_SHA}|${FEATURE_SHA}"; do
  IFS='|' read -r requested_ref expected_sha <<< "${ref_and_sha}"
  destination="${TEST_ROOT}/checkout-${requested_ref}"
  fetch_linker_source "${SOURCE_REPO}" "${requested_ref}" "${destination}"
  actual_sha="$(git -C "${destination}" rev-parse HEAD)"
  [[ "${actual_sha}" == "${expected_sha}" ]] \
    || fail "${requested_ref} resolved to ${actual_sha}, expected ${expected_sha}"
done

if fetch_linker_source \
  "${SOURCE_REPO}" "--upload-pack=unexpected" "${TEST_ROOT}/unsafe-ref"; then
  fail "an option-like ref was accepted"
fi

echo "remote ref tests passed"
