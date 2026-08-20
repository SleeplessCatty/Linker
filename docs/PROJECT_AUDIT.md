# Linker Project Audit

## Current Architecture

Linker is a Rust workspace with three crates:

- `linker-core`: shared paths, state database, rule handling, manifests, health checks, and sync engine.
- `linker-cli`: user-facing `linker` command.
- `linker-daemon`: background watcher and periodic sync daemon.

Runtime state is stored outside the repository:

- local state: `~/Library/Application Support/Linker/`
- sync targets: selected per association with `linker add <source-directory> <target-parent-directory>`

## File Description

- `Cargo.toml`, `Cargo.lock`: Rust workspace and locked dependencies. Commit both because this is an application workspace.
- `README.md`: project overview and install entrypoints.
- `USAGE.md`: command-level usage and examples.
- `INSTALL.md`: script install, `/usr/local/bin`, remote install, and Homebrew packaging notes.
- `PRODUCT.md`: product scope and user behavior.
- `SPEC.md`: implementation design and sync behavior.
- `TASKS.md`: development progress and remaining roadmap.
- `scripts/install.sh`: local checkout installer.
- `scripts/install-remote.sh`: curl/bash installer that clones the GitHub repo then runs the local installer.
- `scripts/uninstall.sh`: LaunchAgent and binary cleanup while preserving Linker state.
- `scripts/lib/cleanup-legacy.sh`: guarded pre-0.2 daemon, command-link, and Application Support cleanup.
- `scripts/tests/legacy-cleanup.sh`: isolated safety and idempotence tests for cleanup and managed links.
- `scripts/tests/branding-residue.sh`: repository-wide legacy-branding gate.
- `packaging/launchagent/com.linker.linkerd.plist.in`: LaunchAgent template.
- `packaging/homebrew/linker.rb`: starter Homebrew formula.
- `.github/workflows/ci.yml`: macOS CI for format, tests, and check.

## Known Defects and Risks

- There is no GUI yet; all workflows are CLI based.
- The sync policy is latest-modified-wins without version history or merge UI.
- Pre-0.2 application state is intentionally deleted during installation and is not migrated; source and target directories remain untouched.
- Homebrew formula requires a real GitHub release tarball SHA before stable `brew install linker` works.
- LaunchAgent install is macOS-user specific and may require manual review if Homebrew is used.
- `/usr/local/bin` linking may require `sudo` on machines where the directory is owned by `root`.
- The daemon watches configured source/target roots and also performs a 5-minute reconciliation pass; very large trees may need future scan optimization.

## Verification Status

Last local verification:

```bash
cargo test --workspace --all-targets
cargo check --workspace --all-targets
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh scripts/tests/branding-residue.sh
bash scripts/tests/legacy-cleanup.sh
bash scripts/tests/branding-residue.sh
ruby -c packaging/homebrew/linker.rb
plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
```
