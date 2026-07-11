# QuickSync Project Audit

## Current Architecture

QuickSync is a Rust workspace with three crates:

- `qsync-core`: shared paths, state database, rule handling, manifests, health checks, and sync engine.
- `qsync-cli`: user-facing `qs` command.
- `qsync-daemon`: background watcher and periodic sync daemon.

Runtime state is stored outside the repository:

- local state: `~/Library/Application Support/QuickSync/`
- sync targets: selected per association with `qs add <source-directory> <target-parent-directory>`

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
- `scripts/uninstall.sh`: LaunchAgent and binary cleanup.
- `packaging/launchagent/com.quicksync.qsd.plist.in`: LaunchAgent template.
- `packaging/homebrew/quicksync.rb`: starter Homebrew formula.
- `.github/workflows/ci.yml`: macOS CI for format, tests, and check.

## Known Defects and Risks

- There is no GUI yet; all workflows are CLI based.
- The sync policy is latest-modified-wins without version history or merge UI.
- Older experimental workspace layouts are not migrated.
- Homebrew formula requires a real GitHub release tarball SHA before stable `brew install quicksync` works.
- LaunchAgent install is macOS-user specific and may require manual review if Homebrew is used.
- `/usr/local/bin` linking may require `sudo` on machines where the directory is owned by `root`.
- The daemon watches configured source/target roots and also performs a 5-minute reconciliation pass; very large trees may need future scan optimization.

## Verification Status

Last local verification:

```bash
cargo test --workspace --all-targets
cargo check --workspace --all-targets
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh
```
