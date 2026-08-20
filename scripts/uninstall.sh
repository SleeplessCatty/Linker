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

USER_UID="$(id -u)"
USER_HOME="$(trim_trailing_slash "${HOME}")"
APP_SUPPORT_DIR="${USER_HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LAUNCH_AGENTS_DIR="${USER_HOME}/Library/LaunchAgents"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"

validate_cleanup_inputs "${USER_HOME}" "${LINK_DIR}" "${USER_UID}"
LINK_DIR="$(trim_trailing_slash "${LINK_DIR}")"
validate_safe_directory_path "${APP_SUPPORT_DIR}"
validate_safe_directory_path "${BIN_DIR}"
validate_safe_directory_path "${LAUNCH_AGENTS_DIR}"

if { [[ "${REMOVE_LINKS}" == "1" ]] \
    && { link_points_into_dir "${LINK_DIR}/linker" "${BIN_DIR}" \
      || link_points_into_dir "${LINK_DIR}/linkerd" "${BIN_DIR}"; }; } \
  || legacy_managed_links_present "${USER_HOME}" "${LINK_DIR}"; then
  authorize_link_directory "${LINK_DIR}"
fi

stop_launchagent "${USER_UID}" "${PLIST_LABEL}" "${PLIST_PATH}"
cleanup_legacy_install "${USER_HOME}" "${LINK_DIR}" "${USER_UID}"

if [[ "${REMOVE_LINKS}" == "1" ]]; then
  remove_link_if_points_into "${LINK_DIR}/linker" "${BIN_DIR}"
  remove_link_if_points_into "${LINK_DIR}/linkerd" "${BIN_DIR}"
fi

remove_managed_file "${PLIST_PATH}"
remove_managed_file "${BIN_DIR}/linker"
remove_managed_file "${BIN_DIR}/linkerd"

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
