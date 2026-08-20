#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

assert_exists() {
  [[ -e "$1" || -L "$1" ]] || fail "expected path to exist: $1"
}

TEST_ROOT="$(cd "$(mktemp -d)" && pwd -P)"
trap 'rm -rf "${TEST_ROOT}"' EXIT

TEST_PROJECT="${TEST_ROOT}/project"
TEST_HOME="${TEST_ROOT}/home"
LINK_DIR="${TEST_ROOT}/command-links"
FAKE_BIN="${TEST_ROOT}/fake-bin"
NO_LAUNCHCTL_BIN="${TEST_ROOT}/no-launchctl-bin"
APP_SUPPORT_DIR="${TEST_HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
PLIST_PATH="${TEST_HOME}/Library/LaunchAgents/com.linker.linkerd.plist"
UNINSTALL_LOG="${TEST_ROOT}/uninstall.log"

mkdir -p \
  "${TEST_PROJECT}/scripts/lib" \
  "${BIN_DIR}" \
  "$(dirname "${PLIST_PATH}")" \
  "${LINK_DIR}" \
  "${FAKE_BIN}" \
  "${NO_LAUNCHCTL_BIN}"
cp "${ROOT_DIR}/scripts/uninstall.sh" "${TEST_PROJECT}/scripts/uninstall.sh"
cp "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh" \
  "${TEST_PROJECT}/scripts/lib/cleanup-legacy.sh"

touch "${BIN_DIR}/linker" "${BIN_DIR}/linkerd" "${PLIST_PATH}"
ln -s "${BIN_DIR}/linker" "${LINK_DIR}/linker"
ln -s "${BIN_DIR}/linkerd" "${LINK_DIR}/linkerd"

cat > "${FAKE_BIN}/launchctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
command_name="$1"
target="${2:-}"
case "${command_name}" in
  print)
    if [[ "${target}" == *"com.linker.linkerd" ]]; then
      exit 0
    fi
    echo "Bad request. Could not find service" >&2
    exit 113
    ;;
  bootout)
    echo "simulated bootout failure" >&2
    exit 5
    ;;
esac
SH
chmod +x "${FAKE_BIN}/launchctl"

set +e
HOME="${TEST_HOME}" PATH="${FAKE_BIN}:${PATH}" \
bash "${TEST_PROJECT}/scripts/uninstall.sh" --link-dir "${LINK_DIR}" \
  > "${UNINSTALL_LOG}" 2>&1
uninstall_status=$?
set -e

[[ "${uninstall_status}" != "0" ]] \
  || fail "uninstall ignored a running daemon that could not be stopped"
assert_exists "${BIN_DIR}/linker"
assert_exists "${BIN_DIR}/linkerd"
assert_exists "${PLIST_PATH}"
assert_exists "${LINK_DIR}/linker"
assert_exists "${LINK_DIR}/linkerd"

for utility in basename dirname id readlink; do
  ln -s "/usr/bin/${utility}" "${NO_LAUNCHCTL_BIN}/${utility}"
done

set +e
HOME="${TEST_HOME}" PATH="${NO_LAUNCHCTL_BIN}" \
/bin/bash "${TEST_PROJECT}/scripts/uninstall.sh" --link-dir "${LINK_DIR}" \
  >> "${UNINSTALL_LOG}" 2>&1
missing_launchctl_status=$?
set -e

[[ "${missing_launchctl_status}" != "0" ]] \
  || fail "uninstall continued when launchctl was unavailable"
assert_exists "${BIN_DIR}/linker"
assert_exists "${BIN_DIR}/linkerd"
assert_exists "${PLIST_PATH}"
assert_exists "${LINK_DIR}/linker"
assert_exists "${LINK_DIR}/linkerd"

echo "uninstall safety tests passed"
