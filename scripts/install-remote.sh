#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${LINKER_REPO_URL:-https://github.com/SleeplessCatty/Linker.git}"
REF="${LINKER_REF:-main}"
LINKER_REMOTE_TMP_DIR=""

fetch_source_error() {
  echo "error: $*" >&2
}

validate_linker_ref() {
  local ref="$1"

  [[ -n "${ref}" && "${ref}" != *$'\n'* && "${ref}" != *$'\r'* ]] || return 1
  if [[ "${ref}" =~ ^[0-9a-fA-F]{7,64}$ ]]; then
    return 0
  fi
  git check-ref-format --branch "${ref}" >/dev/null 2>&1
}

fetch_linker_source() {
  local repo_url="$1"
  local ref="$2"
  local destination="$3"

  if [[ -z "${repo_url}" ]]; then
    fetch_source_error "repository URL must not be empty"
    return 1
  fi
  if ! validate_linker_ref "${ref}"; then
    fetch_source_error "invalid branch, tag, or commit: ${ref}"
    return 1
  fi
  if [[ -z "${destination}" || "${destination}" == "/" || "${destination}" != /* ]]; then
    fetch_source_error "unsafe source destination: ${destination}"
    return 1
  fi
  if [[ -e "${destination}" || -L "${destination}" ]]; then
    fetch_source_error "source destination already exists: ${destination}"
    return 1
  fi

  mkdir -p "${destination}"
  git -C "${destination}" init --quiet
  git -C "${destination}" remote add origin "${repo_url}"
  git -C "${destination}" fetch --quiet --depth 1 origin "${ref}"
  git -C "${destination}" checkout --quiet --detach FETCH_HEAD
}

cleanup_remote_install() {
  if [[ -n "${LINKER_REMOTE_TMP_DIR}" && "${LINKER_REMOTE_TMP_DIR}" != "/" ]]; then
    rm -rf "${LINKER_REMOTE_TMP_DIR}"
  fi
}

usage() {
  cat <<EOF
Usage: curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash -s -- [install options]

Environment:
  LINKER_REPO_URL  Git repository to clone. Default: ${REPO_URL}
  LINKER_REF       Branch, tag, or commit to install. Default: ${REF}

Install options are forwarded to scripts/install.sh, for example:
  --no-link
  --link-dir /usr/local/bin
EOF
}

main() {
  if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    return 0
  fi

  if ! command -v git >/dev/null 2>&1; then
    echo "error: git is required" >&2
    return 1
  fi

  if ! command -v cargo >/dev/null 2>&1; then
    echo "error: Rust/Cargo is required. Install Rust from https://rustup.rs first." >&2
    return 1
  fi

  LINKER_REMOTE_TMP_DIR="$(mktemp -d)"
  trap cleanup_remote_install EXIT INT TERM

  fetch_linker_source "${REPO_URL}" "${REF}" "${LINKER_REMOTE_TMP_DIR}/Linker"
  "${LINKER_REMOTE_TMP_DIR}/Linker/scripts/install.sh" "$@"
}

if [[ -z "${BASH_SOURCE[0]:-}" || "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
