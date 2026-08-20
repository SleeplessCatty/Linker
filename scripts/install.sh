#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh"

APP_SUPPORT_DIR="${HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LOG_DIR="${APP_SUPPORT_DIR}/logs"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_LABEL="com.linker.linkerd"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"
PLIST_TEMPLATE="${ROOT_DIR}/packaging/launchagent/${PLIST_LABEL}.plist.in"
LINK_DIR="/usr/local/bin"
CREATE_LINKS=1

usage() {
  cat <<EOF
Usage: ./scripts/install.sh [--link-dir <dir>] [--no-link]

Build and install linker/linkerd under:
  ${BIN_DIR}

Options:
  --link-dir <dir>  Create linker and linkerd symlinks in <dir>.
                   Default: /usr/local/bin
  --no-link         Do not create command symlinks.
  -h, --help       Show this help.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --link-dir)
      LINK_DIR="${2:-}"
      if [[ -z "${LINK_DIR}" ]]; then
        echo "error: --link-dir requires a directory" >&2
        exit 1
      fi
      shift 2
      ;;
    --no-link)
      CREATE_LINKS=0
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

cd "${ROOT_DIR}"
cargo build --release

if [[ "${CREATE_LINKS}" == "1" ]]; then
  ensure_link_available "${LINK_DIR}/linker" "${BIN_DIR}/linker"
  ensure_link_available "${LINK_DIR}/linkerd" "${BIN_DIR}/linkerd"
fi

cleanup_legacy_install "${HOME}" "${LINK_DIR}" "$(id -u)"
mkdir -p "${BIN_DIR}" "${LOG_DIR}" "${LAUNCH_AGENTS_DIR}"

install -m 0755 "${ROOT_DIR}/target/release/linker" "${BIN_DIR}/linker"
install -m 0755 "${ROOT_DIR}/target/release/linkerd" "${BIN_DIR}/linkerd"

if [[ "${CREATE_LINKS}" == "1" ]]; then
  install_managed_link "${LINK_DIR}/linker" "${BIN_DIR}/linker"
  install_managed_link "${LINK_DIR}/linkerd" "${BIN_DIR}/linkerd"
fi

sed \
  -e "s#__LINKERD_PATH__#${BIN_DIR}/linkerd#g" \
  -e "s#__LOG_DIR__#${LOG_DIR}#g" \
  -e "s#__APP_SUPPORT_DIR__#${APP_SUPPORT_DIR}#g" \
  "${PLIST_TEMPLATE}" > "${PLIST_PATH}"

launchctl bootout "gui/$(id -u)" "${PLIST_PATH}" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$(id -u)" "${PLIST_PATH}"
launchctl kickstart -k "gui/$(id -u)/${PLIST_LABEL}"

cat <<EOF
Linker installed.

Binaries:
  ${BIN_DIR}/linker
  ${BIN_DIR}/linkerd

LaunchAgent:
  ${PLIST_PATH}

Logs:
  ${LOG_DIR}/linkerd.out.log
  ${LOG_DIR}/linkerd.err.log

EOF

if [[ "${CREATE_LINKS}" == "1" ]]; then
  cat <<EOF

Command links:
  ${LINK_DIR}/linker -> ${BIN_DIR}/linker
  ${LINK_DIR}/linkerd -> ${BIN_DIR}/linkerd
EOF
else
  cat <<EOF

Command links were skipped.
Add this to your shell profile if you want linker on PATH:
  export PATH="\$PATH:${BIN_DIR}"
EOF
fi
