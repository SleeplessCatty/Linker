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

USER_UID="$(id -u)"
USER_HOME="$(trim_trailing_slash "${HOME}")"
APP_SUPPORT_DIR="${USER_HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LOG_DIR="${APP_SUPPORT_DIR}/logs"
LAUNCH_AGENTS_DIR="${USER_HOME}/Library/LaunchAgents"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"

validate_cleanup_inputs "${USER_HOME}" "${LINK_DIR}" "${USER_UID}"
LINK_DIR="$(trim_trailing_slash "${LINK_DIR}")"

cd "${ROOT_DIR}"
cargo build --release

validate_legacy_cleanup "${USER_HOME}" "${LINK_DIR}" "${USER_UID}"

if [[ "${CREATE_LINKS}" == "1" ]] \
  || legacy_managed_links_present "${USER_HOME}" "${LINK_DIR}"; then
  authorize_link_directory "${LINK_DIR}"
fi

if [[ "${CREATE_LINKS}" == "1" ]]; then
  ensure_link_available "${LINK_DIR}/linker" "${BIN_DIR}/linker"
  ensure_link_available "${LINK_DIR}/linkerd" "${BIN_DIR}/linkerd"
fi

ensure_safe_directory "${BIN_DIR}"
ensure_safe_directory "${LOG_DIR}"
ensure_safe_directory "${LAUNCH_AGENTS_DIR}"

install -m 0755 "${ROOT_DIR}/target/release/linker" "${BIN_DIR}/linker"
install -m 0755 "${ROOT_DIR}/target/release/linkerd" "${BIN_DIR}/linkerd"

if [[ "${CREATE_LINKS}" == "1" ]]; then
  install_managed_link "${LINK_DIR}/linker" "${BIN_DIR}/linker"
  install_managed_link "${LINK_DIR}/linkerd" "${BIN_DIR}/linkerd"
fi

write_launchagent_plist \
  "${PLIST_TEMPLATE}" \
  "${PLIST_PATH}" \
  "${BIN_DIR}/linkerd" \
  "${LOG_DIR}" \
  "${APP_SUPPORT_DIR}"

stop_launchagent "${USER_UID}" "${PLIST_LABEL}" "${PLIST_PATH}"

if ! launchctl bootstrap "gui/${USER_UID}" "${PLIST_PATH}"; then
  legacy_cleanup_error "could not install LaunchAgent ${PLIST_LABEL}; legacy state was preserved"
  exit 1
fi
if ! launchctl kickstart -k "gui/${USER_UID}/${PLIST_LABEL}"; then
  stop_launchagent "${USER_UID}" "${PLIST_LABEL}" "${PLIST_PATH}" || true
  legacy_cleanup_error "could not start LaunchAgent ${PLIST_LABEL}; legacy state was preserved"
  exit 1
fi
if ! wait_for_linker_daemon \
  "${USER_UID}" \
  "${PLIST_LABEL}" \
  "${BIN_DIR}/linker" \
  "${APP_SUPPORT_DIR}"; then
  stop_launchagent "${USER_UID}" "${PLIST_LABEL}" "${PLIST_PATH}" || true
  exit 1
fi
if ! stop_legacy_daemon "${USER_HOME}" "${USER_UID}"; then
  stop_launchagent "${USER_UID}" "${PLIST_LABEL}" "${PLIST_PATH}" || true
  legacy_cleanup_error "could not stop the legacy daemon; legacy state was preserved"
  exit 1
fi

remove_legacy_artifacts "${USER_HOME}" "${LINK_DIR}" "${USER_UID}"

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
