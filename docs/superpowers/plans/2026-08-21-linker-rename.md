# Linker Full Rename Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rename the complete application to Linker, ship `linker` and `linkerd`, clean the previous local installation safely, and leave no active legacy identifiers outside the cleanup implementation and its tests.

**Architecture:** Keep the existing three-crate Rust architecture and sync behavior, but rename every public and internal product identifier. Put destructive legacy cleanup behind a small, idempotent Shell interface with path and symlink guards; drive the Rust and Shell changes test-first, then update packaging, documentation, CI, and residue checks.

**Tech Stack:** Rust 2021, Cargo workspace, Clap, rusqlite, notify, Bash, launchd plist, Homebrew Ruby Formula, GitHub Actions.

**Spec:** `docs/superpowers/specs/2026-08-21-linker-rename-design.md`

## Global Constraints

- Final product and repository name: `Linker` and `SleeplessCatty/Linker`.
- Final binaries: `linker` and `linkerd`; do not ship command aliases.
- Final crates: `linker-cli`, `linker-core`, and `linker-daemon`; Rust imports use `linker_core`.
- Final runtime root: `~/Library/Application Support/Linker`; tests override it with `LINKER_APP_SUPPORT_DIR`.
- Final LaunchAgent label: `com.linker.linkerd`; final Homebrew Formula: `linker.rb` with class `Linker`.
- Workspace version: `0.2.0`.
- Do not change the sync algorithm, conflict policy, rules, or `remove` and `delete` semantics.
- Do not migrate old SQLite state or associations, and never delete source or target directories during legacy cleanup.
- Old identifiers may remain only in the isolated cleanup implementation, cleanup test, residue test, and this temporary execution plan.
- Do not rewrite Git history, create a release tag, push, change `origin`, or rename the external GitHub repository.

## File Structure

**Rename directories:**

- `crates/qsync-cli/` → `crates/linker-cli/`
- `crates/qsync-core/` → `crates/linker-core/`
- `crates/qsync-daemon/` → `crates/linker-daemon/`

**Rename packaging files:**

- `packaging/homebrew/quicksync.rb` → `packaging/homebrew/linker.rb`
- `packaging/launchagent/com.quicksync.qsd.plist.in` → `packaging/launchagent/com.linker.linkerd.plist.in`

**Create focused Shell files:**

- `scripts/lib/cleanup-legacy.sh`: guarded legacy-state, LaunchAgent, and managed-link cleanup functions.
- `scripts/tests/legacy-cleanup.sh`: isolated filesystem tests for cleanup and link safety.
- `scripts/tests/branding-residue.sh`: allowlisted scan for legacy identifiers.

**Modify Rust workspace and sources:**

- `Cargo.toml`, `Cargo.lock`: members, package version, package names, dependency names.
- `crates/linker-cli/Cargo.toml`, `src/main.rs`, `tests/cli_e2e.rs`: binary, imports, help, errors, E2E expectations.
- `crates/linker-core/Cargo.toml`, `src/error.rs`, `src/lib.rs`, `src/health.rs`, `src/ops.rs`, `src/paths.rs`, `src/rules.rs`, `src/state.rs`, `src/sync.rs`: package, error type, paths, daemon discovery, lock and temp names, tests.
- `crates/linker-daemon/Cargo.toml`, `src/main.rs`, `tests/daemon_once.rs`: binary, imports, logs, lock behavior, tests.

**Modify installation and packaging:**

- `scripts/install.sh`, `scripts/uninstall.sh`, `scripts/install-remote.sh`
- `packaging/homebrew/linker.rb`
- `packaging/launchagent/com.linker.linkerd.plist.in`

**Modify documentation and automation:**

- `README.md`, `PRODUCT.md`, `SPEC.md`, `TASKS.md`, `USAGE.md`, `INSTALL.md`, `LICENSE`
- `docs/GITHUB_RELEASE.md`, `docs/PROJECT_AUDIT.md`, `docs/REPOSITORY_FILES.md`
- `.github/workflows/ci.yml`

---

### Task 1: Rename the Rust workspace, binaries, and runtime identifiers

**Files:**

- Rename: `crates/qsync-cli/` → `crates/linker-cli/`
- Rename: `crates/qsync-core/` → `crates/linker-core/`
- Rename: `crates/qsync-daemon/` → `crates/linker-daemon/`
- Modify: `Cargo.toml:1-26`
- Modify: `Cargo.lock`
- Modify: `crates/linker-cli/Cargo.toml:1-15`
- Modify: `crates/linker-cli/src/main.rs:1-332`
- Test: `crates/linker-cli/tests/cli_e2e.rs`
- Modify: `crates/linker-core/Cargo.toml:1-24`
- Modify: `crates/linker-core/src/error.rs:1-57`
- Modify: `crates/linker-core/src/lib.rs:1-10`
- Modify: `crates/linker-core/src/health.rs:1-206`
- Modify: `crates/linker-core/src/ops.rs:1-221`
- Modify: `crates/linker-core/src/paths.rs:1-96`
- Modify: `crates/linker-core/src/rules.rs:1-65`
- Modify: `crates/linker-core/src/state.rs:1-441`
- Modify: `crates/linker-core/src/sync.rs:1-621`
- Modify: `crates/linker-daemon/Cargo.toml:1-19`
- Modify: `crates/linker-daemon/src/main.rs:1-230`
- Test: `crates/linker-daemon/tests/daemon_once.rs`

**Interfaces:**

- Consumes: existing CLI subcommands and `qsync-core` public module boundaries.
- Produces: Cargo packages `linker-cli`, `linker-core`, `linker-daemon`; binaries `linker`, `linkerd`; public Rust type `LinkerError`; runtime override `LINKER_APP_SUPPORT_DIR`.

- [ ] **Step 1: Change integration expectations first**

In the CLI sandbox, rename `qs()` to `linker()`, look up the new binary, isolate HOME as a second safety boundary, and expect new help text:

```rust
fn linker(&self) -> Command {
    let mut cmd = Command::cargo_bin("linker").expect("linker bin");
    cmd.env("LINKER_APP_SUPPORT_DIR", &self.app_support);
    cmd.env("HOME", self._tmp.path());
    cmd
}

#[test]
fn reports_linker_name_and_version() {
    let sandbox = Sandbox::new();
    sandbox
        .linker()
        .arg("--version")
        .assert()
        .success()
        .stdout(pred_contains("linker 0.2.0"));
}
```

Replace every CLI test call from `sandbox.qs()` to `sandbox.linker()`. Replace expected command snippets with `linker`, expected product text with `Linker`, and the reserved-name expectation with `.linker`.

In the daemon sandbox, rename `qsd()` to `linkerd()` and use the new binary and environment variable:

```rust
fn linkerd(&self) -> Command {
    let mut cmd = Command::cargo_bin("linkerd").expect("linkerd bin");
    cmd.env("LINKER_APP_SUPPORT_DIR", &self.app_support);
    cmd.env("HOME", self._tmp.path());
    cmd
}

#[test]
fn daemon_help_uses_linkerd_name() {
    let sandbox = Sandbox::new();
    sandbox
        .linkerd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("linkerd"))
        .stdout(predicates::str::contains("Linker background daemon"));
}
```

Before the in-process `ops::add_item` call, set both test isolation variables:

```rust
std::env::set_var("HOME", sandbox._tmp.path());
std::env::set_var("LINKER_APP_SUPPORT_DIR", &sandbox.app_support);
```

Add a focused path validation test in `paths.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::validate_item_name;

    #[test]
    fn reserves_linker_control_directory_name() {
        assert!(validate_item_name(".linker").is_err());
    }
}
```

- [ ] **Step 2: Run the changed tests and verify the red state**

Run:

```bash
cargo test -p qsync-cli --test cli_e2e reports_linker_name_and_version
cargo test -p qsync-daemon --test daemon_once daemon_help_uses_linkerd_name
cargo test -p qsync-core paths::tests::reserves_linker_control_directory_name
```

Expected: the first two commands fail because Cargo does not yet expose `linker` or `linkerd` binaries; the core test fails because the old implementation does not reserve `.linker`.

- [ ] **Step 3: Rename the crate directories**

Run:

```bash
mv crates/qsync-cli crates/linker-cli
mv crates/qsync-core crates/linker-core
mv crates/qsync-daemon crates/linker-daemon
```

- [ ] **Step 4: Update Cargo package identities and version**

Set the root workspace members and version to:

```toml
[workspace]
members = [
    "crates/linker-cli",
    "crates/linker-core",
    "crates/linker-daemon",
]
resolver = "2"

[workspace.package]
edition = "2021"
license = "MIT"
version = "0.2.0"
```

Set the CLI package and target to:

```toml
[package]
name = "linker-cli"
edition.workspace = true
license.workspace = true
version.workspace = true

[[bin]]
name = "linker"
path = "src/main.rs"

[dependencies]
clap.workspace = true
linker-core = { path = "../linker-core" }
```

Set the core package name to `linker-core`. Set the daemon package, target, and dependency to:

```toml
[package]
name = "linker-daemon"
edition.workspace = true
license.workspace = true
version.workspace = true

[[bin]]
name = "linkerd"
path = "src/main.rs"

[dependencies]
clap.workspace = true
fs2.workspace = true
notify.workspace = true
linker-core = { path = "../linker-core" }
```

- [ ] **Step 5: Rename Rust identifiers and runtime names**

Apply these exact code mappings throughout the renamed crates:

```text
qsync_core       -> linker_core
QsyncError       -> LinkerError
QUICKSYNC_APP_SUPPORT_DIR -> LINKER_APP_SUPPORT_DIR
Library/Application Support/QuickSync -> Library/Application Support/Linker
.quicksync       -> .linker
qsync-tmp        -> linker-tmp
qsd.lock         -> linkerd.lock
qsd              -> linkerd in daemon discovery, Clap names, and log text
QuickSync        -> Linker in user-facing text
qs               -> linker only when it denotes the CLI command or test helper
```

The error export must be:

```rust
pub use error::{LinkerError, Result};
```

The path constants must be:

```rust
const APP_SUPPORT_ENV: &str = "LINKER_APP_SUPPORT_DIR";

pub fn app_support_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os(APP_SUPPORT_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join("Library/Application Support/Linker"))
}
```

The CLI and daemon Clap declarations must use:

```rust
#[command(name = "linker")]
```

and:

```rust
#[command(name = "linkerd")]
#[command(about = "Linker background daemon")]
```

Remove test-only references to the unused historical cloud-directory environment variable instead of introducing a new unused variable.

- [ ] **Step 6: Regenerate the lockfile and verify the green state**

Run:

```bash
cargo fmt --all
cargo test --workspace --all-targets
cargo check --workspace --all-targets
cargo metadata --no-deps --format-version 1
```

Expected: all Rust tests pass; metadata contains only the three `linker-*` workspace packages at version `0.2.0`, with `linker` and `linkerd` binary targets.

- [ ] **Step 7: Commit the Rust rename**

```bash
git add Cargo.toml Cargo.lock crates
git commit -m "refactor: rename Rust workspace to Linker"
```

---

### Task 2: Implement guarded legacy-install cleanup

**Files:**

- Create: `scripts/lib/cleanup-legacy.sh`
- Create: `scripts/tests/legacy-cleanup.sh`

**Interfaces:**

- Consumes: an explicit user home directory, command-link directory, and numeric user id.
- Produces: `link_points_into_dir(link_path, root)`, `remove_link_if_points_into(link_path, root)`, `ensure_link_available(link_path, managed_target)`, and `cleanup_legacy_install(user_home, link_dir, user_uid)`.

- [ ] **Step 1: Write the failing Shell test**

Create `scripts/tests/legacy-cleanup.sh` with an isolated temporary HOME. The test must create the previous Application Support tree, previous LaunchAgent plist, one managed command link, one unrelated same-name link, and source/target sentinel directories:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

assert_exists() {
  [[ -e "$1" || -L "$1" ]] || fail "expected path to exist: $1"
}

assert_missing() {
  [[ ! -e "$1" && ! -L "$1" ]] || fail "expected path to be absent: $1"
}

TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT

TEST_HOME="${TEST_ROOT}/home"
LINK_DIR="${TEST_ROOT}/links"
LEGACY_ROOT="${TEST_HOME}/Library/Application Support/QuickSync"
LEGACY_BIN="${LEGACY_ROOT}/bin"
LEGACY_PLIST="${TEST_HOME}/Library/LaunchAgents/com.quicksync.qsd.plist"
SOURCE_SENTINEL="${TEST_ROOT}/source/project/file.txt"
TARGET_SENTINEL="${TEST_ROOT}/target/project/file.txt"
EXTERNAL_BIN="${TEST_ROOT}/external/qsd"

mkdir -p "${LEGACY_BIN}" "$(dirname "${LEGACY_PLIST}")" "${LINK_DIR}"
mkdir -p "$(dirname "${SOURCE_SENTINEL}")" "$(dirname "${TARGET_SENTINEL}")"
mkdir -p "$(dirname "${EXTERNAL_BIN}")"
touch "${LEGACY_BIN}/qs" "${LEGACY_BIN}/qsd" "${LEGACY_PLIST}"
touch "${SOURCE_SENTINEL}" "${TARGET_SENTINEL}" "${EXTERNAL_BIN}"
ln -s "${LEGACY_BIN}/qs" "${LINK_DIR}/qs"
ln -s "${EXTERNAL_BIN}" "${LINK_DIR}/qsd"

LINKER_TEST_LAUNCHCTL_LOG="${TEST_ROOT}/launchctl.log"
launchctl() {
  printf '%s\n' "$*" >> "${LINKER_TEST_LAUNCHCTL_LOG}"
}

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" 501

assert_missing "${LEGACY_ROOT}"
assert_missing "${LEGACY_PLIST}"
assert_missing "${LINK_DIR}/qs"
assert_exists "${LINK_DIR}/qsd"
assert_exists "${SOURCE_SENTINEL}"
assert_exists "${TARGET_SENTINEL}"
grep -F "bootout gui/501 ${LEGACY_PLIST}" "${LINKER_TEST_LAUNCHCTL_LOG" >/dev/null

ensure_link_available "${LINK_DIR}/qsd" "${TEST_ROOT}/managed/qsd" \
  && fail "unrelated command link must block installation"

cleanup_legacy_install "${TEST_HOME}" "${LINK_DIR}" 501

if cleanup_legacy_install "/" "${LINK_DIR}" 501; then
  fail "root HOME must be rejected"
fi

echo "legacy cleanup tests passed"
```

Make the test executable:

```bash
chmod +x scripts/tests/legacy-cleanup.sh
```

- [ ] **Step 2: Run the test and verify it fails**

Run:

```bash
bash scripts/tests/legacy-cleanup.sh
```

Expected: FAIL because `scripts/lib/cleanup-legacy.sh` does not exist.

- [ ] **Step 3: Write the minimal guarded cleanup implementation**

Create `scripts/lib/cleanup-legacy.sh`:

```bash
#!/usr/bin/env bash

link_points_into_dir() {
  local link_path="$1"
  local root="$2"
  local target

  [[ -L "${link_path}" ]] || return 1
  target="$(readlink "${link_path}")" || return 1
  case "${target}" in
    "${root}"/*) return 0 ;;
    *) return 1 ;;
  esac
}

remove_link_if_points_into() {
  local link_path="$1"
  local root="$2"
  local link_dir

  link_points_into_dir "${link_path}" "${root}" || return 0
  link_dir="$(dirname "${link_path}")"
  if [[ -w "${link_dir}" ]]; then
    rm -f "${link_path}"
  else
    sudo rm -f "${link_path}"
  fi
}

ensure_link_available() {
  local link_path="$1"
  local managed_target="$2"

  if [[ ! -e "${link_path}" && ! -L "${link_path}" ]]; then
    return 0
  fi
  if [[ -L "${link_path}" && "$(readlink "${link_path}")" == "${managed_target}" ]]; then
    return 0
  fi
  echo "error: refusing to replace existing command path: ${link_path}" >&2
  return 1
}

cleanup_legacy_install() {
  local user_home="$1"
  local link_dir="$2"
  local user_uid="$3"
  local support_root
  local legacy_root
  local legacy_plist

  if [[ -z "${user_home}" || "${user_home}" == "/" ]]; then
    echo "error: refusing legacy cleanup for unsafe HOME: ${user_home}" >&2
    return 1
  fi

  support_root="${user_home}/Library/Application Support"
  legacy_root="${support_root}/QuickSync"
  legacy_plist="${user_home}/Library/LaunchAgents/com.quicksync.qsd.plist"

  [[ "${legacy_root}" == "${support_root}/QuickSync" ]] || return 1

  if [[ -e "${legacy_plist}" ]] && command -v launchctl >/dev/null 2>&1; then
    launchctl bootout "gui/${user_uid}" "${legacy_plist}" >/dev/null 2>&1 || true
  fi
  rm -f "${legacy_plist}"

  remove_link_if_points_into "${link_dir}/qs" "${legacy_root}/bin"
  remove_link_if_points_into "${link_dir}/qsd" "${legacy_root}/bin"

  if [[ -e "${legacy_root}" || -L "${legacy_root}" ]]; then
    rm -rf "${legacy_root}"
  fi
}
```

Make the helper executable:

```bash
chmod +x scripts/lib/cleanup-legacy.sh
```

- [ ] **Step 4: Run cleanup and syntax tests**

Run:

```bash
bash -n scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh
bash scripts/tests/legacy-cleanup.sh
```

Expected: syntax checks succeed and the test prints `legacy cleanup tests passed`. The unrelated `qsd` link and both sentinel files remain.

- [ ] **Step 5: Commit the cleanup unit**

```bash
git add scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh
git commit -m "feat: add guarded legacy install cleanup"
```

---

### Task 3: Rename installers, LaunchAgent, and Homebrew packaging

**Files:**

- Modify: `scripts/install.sh:1-121`
- Modify: `scripts/uninstall.sh:1-85`
- Modify: `scripts/install-remote.sh:1-43`
- Modify: `scripts/lib/cleanup-legacy.sh`
- Test: `scripts/tests/legacy-cleanup.sh`
- Rename: `packaging/launchagent/com.quicksync.qsd.plist.in` → `packaging/launchagent/com.linker.linkerd.plist.in`
- Rename: `packaging/homebrew/quicksync.rb` → `packaging/homebrew/linker.rb`

**Interfaces:**

- Consumes: Task 1 binaries and Task 2 cleanup/link-safety functions.
- Produces: Linker installer, uninstaller, remote installer, LaunchAgent template, and HEAD-only Homebrew Formula.

- [ ] **Step 1: Extend the Shell test for managed new links**

Before changing installers, add these assertions to `scripts/tests/legacy-cleanup.sh`:

```bash
MANAGED_ROOT="${TEST_ROOT}/managed/bin"
mkdir -p "${MANAGED_ROOT}"
touch "${MANAGED_ROOT}/linker"

ensure_link_available "${LINK_DIR}/linker" "${MANAGED_ROOT}/linker"
ln -s "${MANAGED_ROOT}/linker" "${LINK_DIR}/linker"
ensure_link_available "${LINK_DIR}/linker" "${MANAGED_ROOT}/linker"
remove_link_if_points_into "${LINK_DIR}/linker" "${MANAGED_ROOT}"
assert_missing "${LINK_DIR}/linker"
```

Run `bash scripts/tests/legacy-cleanup.sh`; expected result is PASS because the generic safety functions already support the new binary.

- [ ] **Step 2: Rename packaging files**

Run:

```bash
mv packaging/launchagent/com.quicksync.qsd.plist.in packaging/launchagent/com.linker.linkerd.plist.in
mv packaging/homebrew/quicksync.rb packaging/homebrew/linker.rb
```

- [ ] **Step 3: Update the local installer**

At the top of `scripts/install.sh`, source the cleanup helper and set exact Linker paths:

```bash
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh"

APP_SUPPORT_DIR="${HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
LOG_DIR="${APP_SUPPORT_DIR}/logs"
LAUNCH_AGENTS_DIR="${HOME}/Library/LaunchAgents"
PLIST_LABEL="com.linker.linkerd"
PLIST_PATH="${LAUNCH_AGENTS_DIR}/${PLIST_LABEL}.plist"
PLIST_TEMPLATE="${ROOT_DIR}/packaging/launchagent/${PLIST_LABEL}.plist.in"
```

Build and preflight the new command paths before destructive cleanup, then install the new binaries:

```bash
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
```

Create links to `linker` and `linkerd`, using the existing writable-directory versus `sudo` branches. Render `__LINKERD_PATH__`, `__LOG_DIR__`, and `__APP_SUPPORT_DIR__`. Update all output to Linker names and `linkerd.out.log` / `linkerd.err.log`.

- [ ] **Step 4: Update the uninstaller**

Source the cleanup helper, use the Linker paths and LaunchAgent label, and call legacy cleanup before removing the new installation:

```bash
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "${ROOT_DIR}/scripts/lib/cleanup-legacy.sh"

APP_SUPPORT_DIR="${HOME}/Library/Application Support/Linker"
BIN_DIR="${APP_SUPPORT_DIR}/bin"
PLIST_LABEL="com.linker.linkerd"
PLIST_PATH="${HOME}/Library/LaunchAgents/${PLIST_LABEL}.plist"

cleanup_legacy_install "${HOME}" "${LINK_DIR}" "$(id -u)"
```

Remove only Linker-managed command links:

```bash
remove_link_if_points_into "${LINK_DIR}/linker" "${BIN_DIR}"
remove_link_if_points_into "${LINK_DIR}/linkerd" "${BIN_DIR}"
```

Keep `~/Library/Application Support/Linker` state and logs, matching the existing uninstall semantics.

- [ ] **Step 5: Update the remote installer**

Use:

```bash
REPO_URL="${LINKER_REPO_URL:-https://github.com/SleeplessCatty/Linker.git}"
REF="${LINKER_REF:-main}"
```

Clone into `${tmp_dir}/Linker` and execute `${tmp_dir}/Linker/scripts/install.sh`. Update usage text to the new raw GitHub URL and environment variables.

- [ ] **Step 6: Replace the LaunchAgent template**

The renamed plist must contain:

```xml
<key>Label</key>
<string>com.linker.linkerd</string>
<key>ProgramArguments</key>
<array>
  <string>__LINKERD_PATH__</string>
</array>
<key>StandardOutPath</key>
<string>__LOG_DIR__/linkerd.out.log</string>
<key>StandardErrorPath</key>
<string>__LOG_DIR__/linkerd.err.log</string>
<key>WorkingDirectory</key>
<string>__APP_SUPPORT_DIR__</string>
```

Keep `RunAtLoad` and `KeepAlive` enabled.

- [ ] **Step 7: Make the Formula HEAD-only**

Replace `packaging/homebrew/linker.rb` with:

```ruby
class Linker < Formula
  desc "Lightweight iCloud-backed selective sync for macOS"
  homepage "https://github.com/SleeplessCatty/Linker"
  head "https://github.com/SleeplessCatty/Linker.git", branch: "main"
  license "MIT"

  depends_on "rust" => :build

  def install
    system "cargo", "install",
           "--locked",
           "--path", "crates/linker-cli",
           "--root", prefix
    system "cargo", "install",
           "--locked",
           "--path", "crates/linker-daemon",
           "--root", prefix

    (prefix/"packaging/launchagent").install "packaging/launchagent/com.linker.linkerd.plist.in"
  end

  def caveats
    <<~EOS
      To install the background daemon, create a LaunchAgent from:
        #{prefix}/packaging/launchagent/com.linker.linkerd.plist.in

      The daemon should run:
        #{bin}/linkerd

      Logs and state are stored under:
        ~/Library/Application Support/Linker
    EOS
  end

  test do
    system "#{bin}/linker", "--help"
    system "#{bin}/linkerd", "--help"
  end
end
```

- [ ] **Step 8: Verify packaging syntax and cleanup behavior**

Run:

```bash
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh
bash scripts/tests/legacy-cleanup.sh
ruby -c packaging/homebrew/linker.rb
plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
```

Expected: all commands succeed; Ruby reports `Syntax OK`; plist reports `OK`.

- [ ] **Step 9: Commit installation and packaging**

```bash
git add scripts packaging
git commit -m "refactor: rename Linker installation and packaging"
```

---

### Task 4: Rename documentation and enforce branding residue rules

**Files:**

- Create: `scripts/tests/branding-residue.sh`
- Modify: `README.md`
- Modify: `PRODUCT.md`
- Modify: `SPEC.md`
- Modify: `TASKS.md`
- Modify: `USAGE.md`
- Modify: `INSTALL.md`
- Modify: `LICENSE`
- Modify: `docs/GITHUB_RELEASE.md`
- Modify: `docs/PROJECT_AUDIT.md`
- Modify: `docs/REPOSITORY_FILES.md`

**Interfaces:**

- Consumes: final runtime, binary, package, LaunchAgent, Formula, and repository names from Tasks 1-3.
- Produces: current user/developer documentation and an executable residue gate.

- [ ] **Step 1: Write the failing residue test**

Create `scripts/tests/branding-residue.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
LEGACY_PATTERN='QuickSync|qsync|(^|[^[:alnum:]_])qs([^[:alnum:]_]|$)|(^|[^[:alnum:]_])qsd([^[:alnum:]_]|$)|com\.quicksync|QUICKSYNC_'

MATCHES="$(
  git -C "${ROOT_DIR}" grep -n -I -i -E "${LEGACY_PATTERN}" -- \
    . \
    ':(exclude)scripts/lib/cleanup-legacy.sh' \
    ':(exclude)scripts/tests/legacy-cleanup.sh' \
    ':(exclude)scripts/tests/branding-residue.sh' \
    ':(exclude)docs/superpowers/plans/2026-08-21-linker-rename.md' \
    || true
)"

if [[ -n "${MATCHES}" ]]; then
  echo "legacy identifiers remain outside the allowlist:" >&2
  echo "${MATCHES}" >&2
  exit 1
fi

echo "branding residue test passed"
```

Make it executable and run it:

```bash
chmod +x scripts/tests/branding-residue.sh
bash scripts/tests/branding-residue.sh
```

Expected: FAIL and print the documentation files that still contain legacy identifiers.

- [ ] **Step 2: Update top-level product and usage documentation**

Update `README.md`, `PRODUCT.md`, `SPEC.md`, `TASKS.md`, `USAGE.md`, and `INSTALL.md` with these exact final values:

```text
Product: Linker
CLI: linker
Daemon: linkerd
Crates: linker-cli, linker-core, linker-daemon
Runtime root: ~/Library/Application Support/Linker
Ignore-file example: .linkerignore
LaunchAgent: com.linker.linkerd
Repository: https://github.com/SleeplessCatty/Linker
Remote variables: LINKER_REPO_URL, LINKER_REF
Version examples: v0.2.0
```

Replace development commands with:

```bash
cargo run -p linker-cli -- --help
cargo run -p linker-daemon -- --help
```

Document only `brew install --HEAD linker` until a real `v0.2.0` archive and SHA exist. Add an upgrade warning explaining that installation removes old local application state but leaves source and target directories untouched, after which associations must be added again with `linker add`.

- [ ] **Step 3: Update audit and release documentation**

Update:

```text
docs/GITHUB_RELEASE.md
docs/PROJECT_AUDIT.md
docs/REPOSITORY_FILES.md
```

The release guide must use `SleeplessCatty/Linker`, `homebrew-linker`, `linker.rb`, `v0.2.0`, and HEAD-only installation until the release SHA is calculated. The audit must list the three renamed crates and new runtime paths. The repository file guide must list Linker binaries and state paths.

Change the license attribution to:

```text
Copyright (c) 2026 Linker contributors
```

- [ ] **Step 4: Run the residue test and documentation spot checks**

Run:

```bash
bash scripts/tests/branding-residue.sh
rg -n 'linker (add|rule|sync|remove|delete|status|doctor)' README.md PRODUCT.md SPEC.md USAGE.md
rg -n 'SleeplessCatty/Linker|Application Support/Linker|com.linker.linkerd' README.md INSTALL.md docs packaging scripts
```

Expected: the residue test passes and spot checks print the new names in every relevant documentation group.

- [ ] **Step 5: Commit documentation and residue test**

```bash
git add README.md PRODUCT.md SPEC.md TASKS.md USAGE.md INSTALL.md LICENSE docs scripts/tests/branding-residue.sh
git commit -m "docs: rename project to Linker"
```

---

### Task 5: Add CI coverage for Shell safety and branding

**Files:**

- Modify: `.github/workflows/ci.yml:1-22`

**Interfaces:**

- Consumes: Rust workspace tests and Shell tests from Tasks 1-4.
- Produces: one macOS CI job that blocks broken Rust, Shell, cleanup, packaging syntax, and brand residue.

- [ ] **Step 1: Extend the CI job**

Keep the existing format, test, and check steps, then add:

```yaml
      - name: Shell syntax
        run: >-
          bash -n
          scripts/install.sh
          scripts/uninstall.sh
          scripts/install-remote.sh
          scripts/lib/cleanup-legacy.sh
          scripts/tests/legacy-cleanup.sh
          scripts/tests/branding-residue.sh
      - name: Legacy cleanup tests
        run: bash scripts/tests/legacy-cleanup.sh
      - name: Branding residue
        run: bash scripts/tests/branding-residue.sh
      - name: Homebrew formula syntax
        run: ruby -c packaging/homebrew/linker.rb
      - name: LaunchAgent syntax
        run: plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
```

- [ ] **Step 2: Run the complete local CI equivalent**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo check --workspace --all-targets
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh scripts/tests/branding-residue.sh
bash scripts/tests/legacy-cleanup.sh
bash scripts/tests/branding-residue.sh
ruby -c packaging/homebrew/linker.rb
plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
```

Expected: every command exits zero; Rust reports 0 failed tests; both Shell tests print their success messages.

- [ ] **Step 3: Commit CI coverage**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: verify Linker packaging and branding"
```

---

### Task 6: Review, remove temporary plan residue, and perform clean verification

**Files:**

- Review: every changed file from Tasks 1-5
- Delete after review: `docs/superpowers/plans/2026-08-21-linker-rename.md`
- Modify after verification: `docs/superpowers/specs/2026-08-21-linker-rename-design.md`

**Interfaces:**

- Consumes: completed implementation and all automated checks.
- Produces: reviewed, cleanly rebuilt Linker workspace with no temporary plan in the current tree.

- [ ] **Step 1: Request mandatory code review**

Invoke `superpowers:requesting-code-review` and the applicable Rust/code reviewer. Review specifically for destructive path safety, symlink ownership checks, installer ordering, missed identifiers, Cargo target names, and documentation/runtime consistency.

If review feedback is returned, invoke `superpowers:receiving-code-review`, validate each finding against the code, implement accepted fixes with focused tests, and rerun the Task 5 local CI equivalent.

- [ ] **Step 2: Remove the temporary implementation plan**

The plan contains exact legacy cleanup strings and is intentionally temporary. Remove it after implementation and review:

```bash
git rm docs/superpowers/plans/2026-08-21-linker-rename.md
git commit -m "chore: remove completed rename plan"
```

- [ ] **Step 3: Remove generated legacy build artifacts and rebuild**

Run:

```bash
cargo clean
cargo build --workspace --all-targets
cargo test --workspace --all-targets
cargo check --workspace --all-targets
```

Expected: clean rebuild succeeds and all tests report 0 failures.

- [ ] **Step 4: Run final identity and safety verification**

Run:

```bash
cargo fmt --all -- --check
cargo metadata --no-deps --format-version 1
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh scripts/tests/branding-residue.sh
bash scripts/tests/legacy-cleanup.sh
bash scripts/tests/branding-residue.sh
ruby -c packaging/homebrew/linker.rb
plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
test -x target/debug/linker
test -x target/debug/linkerd
test ! -e target/debug/qs
test ! -e target/debug/qsd
test ! -d crates/qsync-cli
test ! -d crates/qsync-core
test ! -d crates/qsync-daemon
target/debug/linker --version
target/debug/linkerd --help
git diff --check
git status --short --branch
```

Expected:

- version output is `linker 0.2.0`;
- daemon help identifies `linkerd` and Linker;
- only new crate directories and binaries exist;
- branding residue test passes with legacy strings limited to the approved cleanup/test allowlist;
- the worktree is clean and all local commits remain unpushed.

- [ ] **Step 5: Mark the approved design as implemented**

Change the design document status line from `状态：已确认，待实施` to:

```text
状态：已实施
```

Then run and commit the documentation-only status update:

```bash
bash scripts/tests/branding-residue.sh
git diff --check
git add docs/superpowers/specs/2026-08-21-linker-rename-design.md
git commit -m "docs: mark Linker rename design implemented"
git status --short --branch
```

Expected: the residue test passes, the commit succeeds, and Git reports a clean branch ahead of its unchanged remote.
