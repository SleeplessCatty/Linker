#!/usr/bin/env bash
set -euo pipefail

APP_SUPPORT_DIR="${HOME}/Library/Application Support/QuickSync"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_LABEL="com.quicksync.qsyncd"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"
LINK_DIR="/usr/local/bin"
REMOVE_LINKS=1

usage() {
  cat <<EOF
Usage: ./scripts/uninstall.sh [--link-dir <dir>] [--no-link]

Options:
  --link-dir <dir>  Remove qsync/qsyncd symlinks from <dir>.
                   Default: /usr/local/bin
  --no-link         Do not remove command symlinks.
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
      REMOVE_LINKS=0
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

launchctl bootout "gui/$(id -u)" "${PLIST_PATH}" >/dev/null 2>&1 || true

rm -f "${PLIST_PATH}"
rm -f "${BIN_DIR}/qsync" "${BIN_DIR}/qsyncd"

if [[ "${REMOVE_LINKS}" == "1" ]]; then
  if [[ -w "${LINK_DIR}" ]]; then
    rm -f "${LINK_DIR}/qsync" "${LINK_DIR}/qsyncd"
  else
    echo "Removing command links from ${LINK_DIR} requires administrator permission."
    sudo rm -f "${LINK_DIR}/qsync" "${LINK_DIR}/qsyncd"
  fi
fi

cat <<EOF
QuickSync daemon uninstalled.

Kept local QuickSync state and logs:
  ${APP_SUPPORT_DIR}

To remove all QuickSync local state, run:
  rm -rf "${APP_SUPPORT_DIR}"

This does not delete your synced local folders or iCloud mirror folders.
EOF

if [[ "${REMOVE_LINKS}" == "1" ]]; then
  cat <<EOF

Removed command links from:
  ${LINK_DIR}
EOF
fi
