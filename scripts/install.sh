#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_SUPPORT_DIR="${HOME}/Library/Application Support/QuickSync"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LOG_DIR="${APP_SUPPORT_DIR}/logs"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_LABEL="com.quicksync.qsyncd"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"
PLIST_TEMPLATE="${ROOT_DIR}/packaging/launchagent/${PLIST_LABEL}.plist.in"
LINK_DIR="/usr/local/bin"
CREATE_LINKS=1

usage() {
  cat <<EOF
Usage: ./scripts/install.sh [--link-dir <dir>] [--no-link]

Build and install qsync/qsyncd under:
  ${BIN_DIR}

Options:
  --link-dir <dir>  Create qsync and qsyncd symlinks in <dir>.
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

mkdir -p "${BIN_DIR}" "${LOG_DIR}" "${LAUNCH_AGENTS_DIR}"

cd "${ROOT_DIR}"
cargo build --release

install -m 0755 "${ROOT_DIR}/target/release/qsync" "${BIN_DIR}/qsync"
install -m 0755 "${ROOT_DIR}/target/release/qsyncd" "${BIN_DIR}/qsyncd"

if [[ "${CREATE_LINKS}" == "1" ]]; then
  if [[ -d "${LINK_DIR}" && -w "${LINK_DIR}" ]]; then
    ln -sf "${BIN_DIR}/qsync" "${LINK_DIR}/qsync"
    ln -sf "${BIN_DIR}/qsyncd" "${LINK_DIR}/qsyncd"
  else
    echo "Creating command links in ${LINK_DIR} requires administrator permission."
    sudo mkdir -p "${LINK_DIR}"
    sudo ln -sf "${BIN_DIR}/qsync" "${LINK_DIR}/qsync"
    sudo ln -sf "${BIN_DIR}/qsyncd" "${LINK_DIR}/qsyncd"
  fi
fi

sed \
  -e "s#__QSYNCD_PATH__#${BIN_DIR}/qsyncd#g" \
  -e "s#__LOG_DIR__#${LOG_DIR}#g" \
  -e "s#__APP_SUPPORT_DIR__#${APP_SUPPORT_DIR}#g" \
  "${PLIST_TEMPLATE}" > "${PLIST_PATH}"

launchctl bootout "gui/$(id -u)" "${PLIST_PATH}" >/dev/null 2>&1 || true
launchctl bootstrap "gui/$(id -u)" "${PLIST_PATH}"
launchctl kickstart -k "gui/$(id -u)/${PLIST_LABEL}"

cat <<EOF
QuickSync installed.

Binaries:
  ${BIN_DIR}/qsync
  ${BIN_DIR}/qsyncd

LaunchAgent:
  ${PLIST_PATH}

Logs:
  ${LOG_DIR}/qsyncd.out.log
  ${LOG_DIR}/qsyncd.err.log

EOF

if [[ "${CREATE_LINKS}" == "1" ]]; then
  cat <<EOF

Command links:
  ${LINK_DIR}/qsync -> ${BIN_DIR}/qsync
  ${LINK_DIR}/qsyncd -> ${BIN_DIR}/qsyncd
EOF
else
  cat <<EOF

Command links were skipped.
Add this to your shell profile if you want qsync on PATH:
  export PATH="\$PATH:${BIN_DIR}"
EOF
fi
