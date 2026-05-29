#!/usr/bin/env bash
set -euo pipefail

REPO_URL="${QUICKSYNC_REPO_URL:-https://github.com/SleeplessCatty/QuickSync.git}"
REF="${QUICKSYNC_REF:-main}"

usage() {
  cat <<EOF
Usage: curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash -s -- [install options]

Environment:
  QUICKSYNC_REPO_URL  Git repository to clone. Default: ${REPO_URL}
  QUICKSYNC_REF       Branch, tag, or commit to install. Default: ${REF}

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

git clone --depth 1 --branch "${REF}" "${REPO_URL}" "${tmp_dir}/QuickSync"
exec "${tmp_dir}/QuickSync/scripts/install.sh" "$@"
