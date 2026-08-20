#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${LINKER_REPO_URL:-https://github.com/SleeplessCatty/Linker.git}"
REF="${LINKER_REF:-main}"

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

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if ! command -v git >/dev/null 2>&1; then
  echo "error: git is required" >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: Rust/Cargo is required. Install Rust from https://rustup.rs first." >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "${tmp_dir}"
}
trap cleanup EXIT

git clone --depth 1 --branch "${REF}" "${REPO_URL}" "${tmp_dir}/Linker"
exec "${tmp_dir}/Linker/scripts/install.sh" "$@"
