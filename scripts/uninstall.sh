#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh"

APP_SUPPORT_DIR="${HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_LABEL="com.linker.linkerd"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"
LINK_DIR="/usr/local/bin"
REMOVE_LINKS=1

usage() {
  cat <<EOF
Usage: ./scripts/uninstall.sh [--link-dir <dir>] [--no-link]

Options:
  --link-dir <dir>  Remove linker/linkerd symlinks from <dir>.
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

cleanup_legacy_install "${HOME}" "${LINK_DIR}" "$(id -u)"

launchctl bootout "gui/$(id -u)" "${PLIST_PATH}" >/dev/null 2>&1 || true

rm -f "${PLIST_PATH}"
rm -f "${BIN_DIR}/linker" "${BIN_DIR}/linkerd"

if [[ "${REMOVE_LINKS}" == "1" ]]; then
  remove_link_if_points_into "${LINK_DIR}/linker" "${BIN_DIR}"
  remove_link_if_points_into "${LINK_DIR}/linkerd" "${BIN_DIR}"
fi

cat <<EOF
Linker daemon uninstalled.

Kept local Linker state and logs:
  ${APP_SUPPORT_DIR}

To remove all Linker local state, run:
  rm -rf "${APP_SUPPORT_DIR}"

This does not delete your synced local folders or iCloud mirror folders.
EOF

if [[ "${REMOVE_LINKS}" == "1" ]]; then
  cat <<EOF

Removed command links from:
  ${LINK_DIR}
EOF
fi
