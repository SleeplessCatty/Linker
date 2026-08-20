#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

assert_exists() {
  [[ -e "$1" || -L "$1" ]] || fail "expected path to exist: $1"
}

assert_missing() {
  [[ ! -e "$1" && ! -L "$1" ]] || fail "expected path to be absent: $1"
}

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT

TEST_HOME="${TEST_ROOT}/home"
LINK_DIR="${TEST_ROOT}/links"
LEGACY_ROOT="${TEST_HOME}/Library/Application Support/QuickSync"
LEGACY_BIN="${LEGACY_ROOT}/bin"
LEGACY_PLIST="${TEST_HOME}/Library/LaunchAgents/com.quicksync.qsd.plist"
SOURCE_SENTINEL="${TEST_ROOT}/source/project/file.txt"
TARGET_SENTINEL="${TEST_ROOT}/target/project/file.txt"
EXTERNAL_BIN="${TEST_ROOT}/external/qsd"

mkdir -p "${LEGACY_BIN}" "$(dirname "${LEGACY_PLIST}")" "${LINK_DIR}"
mkdir -p "$(dirname "${SOURCE_SENTINEL}")" "$(dirname "${TARGET_SENTINEL}")"
mkdir -p "$(dirname "${EXTERNAL_BIN}")"
touch "${LEGACY_BIN}/qs" "${LEGACY_BIN}/qsd" "${LEGACY_PLIST}"
touch "${SOURCE_SENTINEL}" "${TARGET_SENTINEL}" "${EXTERNAL_BIN}"
ln -s "${LEGACY_BIN}/qs" "${LINK_DIR}/qs"
ln -s "${EXTERNAL_BIN}" "${LINK_DIR}/qsd"

LINKER_TEST_LAUNCHCTL_LOG="${TEST_ROOT}/launchctl.log"
launchctl() {
  printf '%s\n' "$*" >> "${LINKER_TEST_LAUNCHCTL_LOG}"
}

validate_cleanup_inputs "${TEST_HOME}" "${LINK_DIR}" 501

if link_points_into_dir "${LINK_DIR}/qsd" ""; then
  fail "empty managed root must not match an arbitrary command link"
fi

INVALID_INPUT_LOG="${TEST_ROOT}/invalid-input.log"
for invalid_args in \
  "|${LINK_DIR}|501" \
  "/|${LINK_DIR}|501" \
  "relative-home|${LINK_DIR}|501" \
  "${TEST_HOME}||501" \
  "${TEST_HOME}|/|501" \
  "${TEST_HOME}|relative-links|501" \
  "${TEST_HOME}|${LINK_DIR}|invalid"; do
  IFS='|' read -r candidate_home candidate_links candidate_uid <<< "${invalid_args}"
  if validate_cleanup_inputs \
    "${candidate_home}" "${candidate_links}" "${candidate_uid}" \
    2>> "${INVALID_INPUT_LOG}"; then
    fail "unsafe cleanup inputs were accepted: ${invalid_args}"
  fi
done

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" 501

assert_missing "${LEGACY_ROOT}"
assert_missing "${LEGACY_PLIST}"
assert_missing "${LINK_DIR}/qs"
assert_exists "${LINK_DIR}/qsd"
assert_exists "${SOURCE_SENTINEL}"
assert_exists "${TARGET_SENTINEL}"
grep -F "bootout gui/501 ${LEGACY_PLIST}" "${LINKER_TEST_LAUNCHCTL_LOG}" >/dev/null

LINK_ERROR_LOG="${TEST_ROOT}/link-error.log"
if ensure_link_available \
  "${LINK_DIR}/qsd" "${TEST_ROOT}/managed/qsd" 2>> "${LINK_ERROR_LOG}"; then
  fail "unrelated command link must block installation"
fi
if ensure_link_available "${LINK_DIR}/missing" "" 2>> "${LINK_ERROR_LOG}"; then
  fail "empty managed link target must be rejected"
fi
grep -F "refusing to replace existing command path" "${LINK_ERROR_LOG}" >/dev/null

rm -f "${LINK_DIR}/qsd"
mkdir -p "${LEGACY_BIN}"
touch "${LEGACY_BIN}/qsd"
ln -s "${LEGACY_BIN}/qsd" "${LINK_DIR}/qsd"
: > "${LINKER_TEST_LAUNCHCTL_LOG}"

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" 501

assert_missing "${LEGACY_ROOT}"
assert_missing "${LINK_DIR}/qsd"
assert_exists "${SOURCE_SENTINEL}"
assert_exists "${TARGET_SENTINEL}"
grep -F "bootout gui/501/com.quicksync.qsd" "${LINKER_TEST_LAUNCHCTL_LOG}" >/dev/null \
  || fail "loaded legacy daemon was not stopped after its plist disappeared"

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" 501

MANAGED_ROOT="${TEST_ROOT}/managed/bin"
mkdir -p "${MANAGED_ROOT}"
touch "${MANAGED_ROOT}/linker"

ensure_link_available "${LINK_DIR}/linker" "${MANAGED_ROOT}/linker"
ln -s "${MANAGED_ROOT}/linker" "${LINK_DIR}/linker"
ensure_link_available "${LINK_DIR}/linker" "${MANAGED_ROOT}/linker"
remove_link_if_points_into "${LINK_DIR}/linker" "${MANAGED_ROOT}"
assert_missing "${LINK_DIR}/linker"

install_managed_link "${LINK_DIR}/linker" "${MANAGED_ROOT}/linker"
assert_exists "${LINK_DIR}/linker"
[[ "$(readlink "${LINK_DIR}/linker")" == "${MANAGED_ROOT}/linker" ]] \
  || fail "managed command link points to the wrong target"
install_managed_link "${LINK_DIR}/linker" "${MANAGED_ROOT}/linker"

ln -s "${EXTERNAL_BIN}" "${LINK_DIR}/qsd"
if install_managed_link \
  "${LINK_DIR}/qsd" "${MANAGED_ROOT}/linkerd" 2>> "${LINK_ERROR_LOG}"; then
  fail "managed link installation replaced an unrelated command"
fi
[[ "$(readlink "${LINK_DIR}/qsd")" == "${EXTERNAL_BIN}" ]] \
  || fail "unrelated command link changed during installation"

echo "legacy cleanup tests passed"
