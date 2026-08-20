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

TEST_ROOT="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "${TEST_ROOT}"' EXIT
TEST_UID="$(id -u)"

TEST_HOME="${TEST_ROOT}/home"
LINK_DIR="${TEST_ROOT}/links"
LEGACY_ROOT="${TEST_HOME}/Library/Application Support/QuickSync"
LEGACY_BIN="${LEGACY_ROOT}/bin"
LEGACY_PLIST="${TEST_HOME}/Library/LaunchAgents/com.quicksync.qsd.plist"
SOURCE_SENTINEL="${TEST_ROOT}/source/project/file.txt"
TARGET_SENTINEL="${TEST_ROOT}/target/project/file.txt"
EXTERNAL_BIN="${TEST_ROOT}/external/qsd"

mkdir -p "${LEGACY_BIN}" "$(dirname "${LEGACY_PLIST}")" "${LINK_DIR}"
mkdir -p "${LEGACY_ROOT}/logs" "${LEGACY_ROOT}/manifests" \
  "${LEGACY_ROOT}/rules" "${LEGACY_ROOT}/tmp"
mkdir -p "$(dirname "${SOURCE_SENTINEL}")" "$(dirname "${TARGET_SENTINEL}")"
mkdir -p "$(dirname "${EXTERNAL_BIN}")"
touch "${LEGACY_BIN}/qs" "${LEGACY_BIN}/qsd" "${LEGACY_PLIST}"
touch "${LEGACY_ROOT}/logs/qsd.out.log" "${LEGACY_ROOT}/logs/qsd.err.log"
touch "${LEGACY_ROOT}/logs/qsyncd.out.log" "${LEGACY_ROOT}/logs/qsyncd.err.log"
touch "${LEGACY_ROOT}/manifests/demo.json" "${LEGACY_ROOT}/rules/demo.ignore"
printf '{"workspace_dir":"%s"}\n' "${TEST_ROOT}/legacy-workspace" \
  > "${LEGACY_ROOT}/config.json"
touch "${LEGACY_ROOT}/qsd.lock" "${LEGACY_ROOT}/qsyncd.lock" \
  "${LEGACY_ROOT}/.DS_Store"
touch "${SOURCE_SENTINEL}" "${TARGET_SENTINEL}" "${EXTERNAL_BIN}"
sqlite3 "${LEGACY_ROOT}/state.sqlite" <<SQL
CREATE TABLE items (local_path TEXT NOT NULL, cloud_path TEXT NOT NULL);
INSERT INTO items VALUES ('$(dirname "${SOURCE_SENTINEL}")', '$(dirname "${TARGET_SENTINEL}")');
SQL
ln -s "${LEGACY_BIN}/qs" "${LINK_DIR}/qs"
ln -s "${EXTERNAL_BIN}" "${LINK_DIR}/qsd"

LINKER_TEST_LAUNCHCTL_LOG="${TEST_ROOT}/launchctl.log"
LINKER_TEST_SERVICE_LOADED=1
LINKER_TEST_BOOTOUT_FAIL=0
launchctl() {
  printf '%s\n' "$*" >> "${LINKER_TEST_LAUNCHCTL_LOG}"
  case "$1" in
    print)
      if [[ "${LINKER_TEST_SERVICE_LOADED}" == "1" ]]; then
        return 0
      fi
      echo "Bad request. Could not find service" >&2
      return 113
      ;;
    bootout)
      if [[ "${LINKER_TEST_BOOTOUT_FAIL}" == "1" ]]; then
        return 5
      fi
      LINKER_TEST_SERVICE_LOADED=0
      ;;
  esac
}

validate_cleanup_inputs "${TEST_HOME}" "${LINK_DIR}" "${TEST_UID}"
validate_cleanup_inputs "${TEST_HOME}/" "${LINK_DIR}/" "${TEST_UID}"

if link_points_into_dir "${LINK_DIR}/qsd" ""; then
  fail "empty managed root must not match an arbitrary command link"
fi

INVALID_INPUT_LOG="${TEST_ROOT}/invalid-input.log"
ROOT_HOME_LINK="${TEST_ROOT}/root-home-link"
ROOT_LINK_DIR="${TEST_ROOT}/root-link-dir"
ln -s / "${ROOT_HOME_LINK}"
ln -s / "${ROOT_LINK_DIR}"
for invalid_args in \
  "|${LINK_DIR}|${TEST_UID}" \
  "/|${LINK_DIR}|${TEST_UID}" \
  "//|${LINK_DIR}|${TEST_UID}" \
  "/./|${LINK_DIR}|${TEST_UID}" \
  "${TEST_HOME}/../..|${LINK_DIR}|${TEST_UID}" \
  "${ROOT_HOME_LINK}|${LINK_DIR}|${TEST_UID}" \
  "relative-home|${LINK_DIR}|${TEST_UID}" \
  "${TEST_HOME}||${TEST_UID}" \
  "${TEST_HOME}|/|${TEST_UID}" \
  "${TEST_HOME}|//|${TEST_UID}" \
  "${TEST_HOME}|/./|${TEST_UID}" \
  "${TEST_HOME}|${LINK_DIR}/../..|${TEST_UID}" \
  "${TEST_HOME}|${ROOT_LINK_DIR}|${TEST_UID}" \
  "${TEST_HOME}|relative-links|${TEST_UID}" \
  "${TEST_HOME}|${LINK_DIR}|0" \
  "${TEST_HOME}|${LINK_DIR}|invalid"; do
  IFS='|' read -r candidate_home candidate_links candidate_uid <<< "${invalid_args}"
  if validate_cleanup_inputs \
    "${candidate_home}" "${candidate_links}" "${candidate_uid}" \
    2>> "${INVALID_INPUT_LOG}"; then
    fail "unsafe cleanup inputs were accepted: ${invalid_args}"
  fi
done

READ_ONLY_LINK_DIR="${TEST_ROOT}/read-only-links"
mkdir -p "${READ_ONLY_LINK_DIR}"
chmod 0500 "${READ_ONLY_LINK_DIR}"
if validate_cleanup_inputs \
  "${TEST_HOME}" "${READ_ONLY_LINK_DIR}" "${TEST_UID}" \
  2>> "${INVALID_INPUT_LOG}"; then
  fail "a non-writable custom command-link directory was accepted"
fi
chmod 0700 "${READ_ONLY_LINK_DIR}"

TRAVERSAL_LINK_DIR="${TEST_ROOT}/traversal-links"
mkdir -p "${TRAVERSAL_LINK_DIR}"
ln -s "${LEGACY_BIN}/../../outside/qs" "${TRAVERSAL_LINK_DIR}/qs"
if link_points_into_dir "${TRAVERSAL_LINK_DIR}/qs" "${LEGACY_BIN}"; then
  fail "a lexically prefixed link that escapes the managed directory was accepted"
fi

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" "${TEST_UID}"

assert_missing "${LEGACY_ROOT}"
assert_missing "${LEGACY_PLIST}"
assert_missing "${LINK_DIR}/qs"
assert_exists "${LINK_DIR}/qsd"
assert_exists "${SOURCE_SENTINEL}"
assert_exists "${TARGET_SENTINEL}"
grep -F "bootout gui/${TEST_UID}/com.quicksync.qsd" "${LINKER_TEST_LAUNCHCTL_LOG}" >/dev/null

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
LINKER_TEST_SERVICE_LOADED=1

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" "${TEST_UID}"

assert_missing "${LEGACY_ROOT}"
assert_missing "${LINK_DIR}/qsd"
assert_exists "${SOURCE_SENTINEL}"
assert_exists "${TARGET_SENTINEL}"
grep -F "bootout gui/${TEST_UID}/com.quicksync.qsd" "${LINKER_TEST_LAUNCHCTL_LOG}" >/dev/null \
  || fail "loaded legacy daemon was not stopped after its plist disappeared"

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" "${TEST_UID}"

NESTED_HOME="${TEST_ROOT}/nested-home"
NESTED_LINK_DIR="${TEST_ROOT}/nested-links"
NESTED_ROOT="${NESTED_HOME}/Library/Application Support/QuickSync"
NESTED_SOURCE="${NESTED_ROOT}/user-source/file.txt"
mkdir -p "$(dirname "${NESTED_SOURCE}")" "${NESTED_LINK_DIR}"
touch "${NESTED_SOURCE}"
if cleanup_legacy_install \
  "${NESTED_HOME}" "${NESTED_LINK_DIR}" "${TEST_UID}" \
  2>> "${INVALID_INPUT_LOG}"; then
  fail "cleanup deleted unrecognized content that may be a source directory"
fi
assert_exists "${NESTED_SOURCE}"

DB_HOME="${TEST_ROOT}/db-home"
DB_LINK_DIR="${TEST_ROOT}/db-links"
DB_ROOT="${DB_HOME}/Library/Application Support/QuickSync"
mkdir -p "${DB_ROOT}" "${DB_LINK_DIR}"
sqlite3 "${DB_ROOT}/state.sqlite" <<SQL
CREATE TABLE items (local_path TEXT NOT NULL, cloud_path TEXT NOT NULL);
INSERT INTO items VALUES ('${DB_ROOT}', '${TEST_ROOT}/outside-target');
SQL
if cleanup_legacy_install \
  "${DB_HOME}" "${DB_LINK_DIR}" "${TEST_UID}" \
  2>> "${INVALID_INPUT_LOG}"; then
  fail "cleanup deleted a source directory recorded at the legacy root"
fi
assert_exists "${DB_ROOT}/state.sqlite"

SYMLINK_HOME="${TEST_ROOT}/symlink-home"
SYMLINK_LINK_DIR="${TEST_ROOT}/symlink-links"
EXTERNAL_SUPPORT="${TEST_ROOT}/external-support"
EXTERNAL_SENTINEL="${EXTERNAL_SUPPORT}/QuickSync/source-sentinel.txt"
mkdir -p "${SYMLINK_HOME}/Library" "${SYMLINK_LINK_DIR}" \
  "$(dirname "${EXTERNAL_SENTINEL}")"
touch "${EXTERNAL_SENTINEL}"
ln -s "${EXTERNAL_SUPPORT}" "${SYMLINK_HOME}/Library/Application Support"
if cleanup_legacy_install \
  "${SYMLINK_HOME}" "${SYMLINK_LINK_DIR}" "${TEST_UID}" \
  2>> "${INVALID_INPUT_LOG}"; then
  fail "cleanup followed a symlinked Application Support directory"
fi
assert_exists "${EXTERNAL_SENTINEL}"

STOP_HOME="${TEST_ROOT}/stop-home"
STOP_LINK_DIR="${TEST_ROOT}/stop-links"
STOP_ROOT="${STOP_HOME}/Library/Application Support/QuickSync"
STOP_PLIST="${STOP_HOME}/Library/LaunchAgents/com.quicksync.qsd.plist"
mkdir -p "${STOP_ROOT}/bin" "${STOP_LINK_DIR}" "$(dirname "${STOP_PLIST}")"
touch "${STOP_ROOT}/bin/qs" "${STOP_PLIST}"
LINKER_TEST_SERVICE_LOADED=1
LINKER_TEST_BOOTOUT_FAIL=1
if cleanup_legacy_install \
  "${STOP_HOME}" "${STOP_LINK_DIR}" "${TEST_UID}" \
  2>> "${INVALID_INPUT_LOG}"; then
  fail "cleanup continued after the legacy daemon could not be stopped"
fi
assert_exists "${STOP_ROOT}/bin/qs"
assert_exists "${STOP_PLIST}"
LINKER_TEST_BOOTOUT_FAIL=0
cleanup_legacy_install "${STOP_HOME}" "${STOP_LINK_DIR}" "${TEST_UID}"
assert_missing "${STOP_ROOT}"
assert_missing "${STOP_PLIST}"

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

PLIST_TEST_ROOT="${TEST_ROOT}/plist & <test>"
PLIST_DEST_DIR="${PLIST_TEST_ROOT}/Library/LaunchAgents"
PLIST_DEST="${PLIST_DEST_DIR}/com.linker.linkerd.plist"
PLIST_VICTIM="${PLIST_TEST_ROOT}/source-victim.txt"
mkdir -p "${PLIST_DEST_DIR}"
printf 'source sentinel\n' > "${PLIST_VICTIM}"
ln -s "${PLIST_VICTIM}" "${PLIST_DEST}"

write_launchagent_plist \
  "${ROOT_DIR}/packaging/launchagent/com.linker.linkerd.plist.in" \
  "${PLIST_DEST}" \
  "${PLIST_TEST_ROOT}/bin/linkerd & <daemon>" \
  "${PLIST_TEST_ROOT}/logs & <logs>" \
  "${PLIST_TEST_ROOT}/Application Support/Linker & <state>"

[[ ! -L "${PLIST_DEST}" ]] || fail "plist destination remained a symlink"
[[ "$(cat "${PLIST_VICTIM}")" == "source sentinel" ]] \
  || fail "plist rendering overwrote the symlink target"
grep -F '&amp;' "${PLIST_DEST}" >/dev/null \
  || fail "plist path values were not XML escaped"
grep -F '&lt;' "${PLIST_DEST}" >/dev/null \
  || fail "plist path values were not XML escaped"
plutil -lint "${PLIST_DEST}" >/dev/null

echo "legacy cleanup tests passed"
