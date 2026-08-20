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
TEST_HOME="${TEST_ROOT}/home & <upgrade>"
LINK_DIR="${TEST_ROOT}/command-links"
FAKE_BIN="${TEST_ROOT}/fake-bin"
LEGACY_ROOT="${TEST_HOME}/Library/Application Support/QuickSync"
LEGACY_PLIST="${TEST_HOME}/Library/LaunchAgents/com.quicksync.qsd.plist"
NEW_PLIST="${TEST_HOME}/Library/LaunchAgents/com.linker.linkerd.plist"
PLIST_VICTIM="${TEST_ROOT}/source-victim.txt"
LAUNCHCTL_STATE="${TEST_ROOT}/legacy-loaded"
INSTALL_LOG="${TEST_ROOT}/install.log"

mkdir -p \
  "${TEST_PROJECT}/scripts/lib" \
  "${TEST_PROJECT}/packaging/launchagent" \
  "${LEGACY_ROOT}/bin" \
  "$(dirname "${LEGACY_PLIST}")" \
  "${LINK_DIR}" \
  "${FAKE_BIN}"

cp "${ROOT_DIR}/scripts/install.sh" "${TEST_PROJECT}/scripts/install.sh"
cp "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh" \
  "${TEST_PROJECT}/scripts/lib/cleanup-legacy.sh"
cp "${ROOT_DIR}/packaging/launchagent/com.linker.linkerd.plist.in" \
  "${TEST_PROJECT}/packaging/launchagent/com.linker.linkerd.plist.in"

touch "${LEGACY_ROOT}/bin/qs" "${LEGACY_PLIST}" "${LAUNCHCTL_STATE}"
printf 'source sentinel\n' > "${PLIST_VICTIM}"
ln -s "${PLIST_VICTIM}" "${NEW_PLIST}"

cat > "${FAKE_BIN}/cargo" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "${LINKER_TEST_PROJECT}/target/release"
printf '#!/usr/bin/env bash\nexit 0\n' > "${LINKER_TEST_PROJECT}/target/release/linker"
printf '#!/usr/bin/env bash\nexit 0\n' > "${LINKER_TEST_PROJECT}/target/release/linkerd"
chmod +x \
  "${LINKER_TEST_PROJECT}/target/release/linker" \
  "${LINKER_TEST_PROJECT}/target/release/linkerd"
SH

cat > "${FAKE_BIN}/launchctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
command_name="$1"
target="${2:-}"
case "${command_name}" in
  print)
    if [[ "${target}" == *"com.quicksync.qsd" ]] \
      && [[ -e "${LINKER_TEST_LAUNCHCTL_STATE}" ]]; then
      exit 0
    fi
    echo "Bad request. Could not find service" >&2
    exit 113
    ;;
  bootout)
    if [[ "${target}" == *"com.quicksync.qsd" ]]; then
      rm -f "${LINKER_TEST_LAUNCHCTL_STATE}"
    fi
    exit 0
    ;;
  bootstrap)
    echo "simulated bootstrap failure" >&2
    exit 5
    ;;
  kickstart)
    exit 0
    ;;
esac
SH

chmod +x "${FAKE_BIN}/cargo" "${FAKE_BIN}/launchctl"

set +e
HOME="${TEST_HOME}" \
PATH="${FAKE_BIN}:${PATH}" \
LINKER_TEST_PROJECT="${TEST_PROJECT}" \
LINKER_TEST_LAUNCHCTL_STATE="${LAUNCHCTL_STATE}" \
bash "${TEST_PROJECT}/scripts/install.sh" --link-dir "${LINK_DIR}" \
  > "${INSTALL_LOG}" 2>&1
install_status=$?
set -e

[[ "${install_status}" != "0" ]] || fail "simulated LaunchAgent failure was ignored"
assert_exists "${LEGACY_ROOT}/bin/qs"
assert_exists "${LEGACY_PLIST}"
[[ "$(cat "${PLIST_VICTIM}")" == "source sentinel" ]] \
  || fail "LaunchAgent rendering overwrote a symlink target"
[[ ! -L "${NEW_PLIST}" ]] || fail "new LaunchAgent plist remained a symlink"
plutil -lint "${NEW_PLIST}" >/dev/null

echo "install safety tests passed"
