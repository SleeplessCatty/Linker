# QuickSync Development Tasks

## MVP Decisions

- CLI binary name: `qs`
- Daemon binary name: `qsd`
- MVP target: macOS directory sync
- Sync model: bidirectional mirror copy between source directory and target directory
- Conflict behavior: latest modified file wins
- First implementation order: manual sync before daemon auto sync
- Source directories are never deleted by `remove` or `delete`

## Phase 1: CLI, State, Manifest

Status: completed

- [x] Create Rust workspace
- [x] Create `qs` CLI binary
- [x] Create core library for paths, manifest, and state
- [x] Initialize SQLite state database
- [x] Implement `qs add <source-directory> <target-parent-directory>`
- [x] Implement `qs list`
- [x] Implement basic `qs status [name]`
- [x] Create target directory under the target parent
- [x] Write manifest under Application Support
- [x] Write rule snapshot under Application Support
- [x] Verify with `cargo test` once Rust toolchain is available

## Phase 2: Exclude Rules

- [x] Default to empty excludes
- [x] Support explicit `--ignore-file`
- [x] Support explicit `--exclude`
- [x] Implement `qs rule <name> exclude <pattern>`
- [x] Implement `qs rule <name> include <pattern>`
- [x] Implement `qs rule <name> list`
- [x] Show exclude count in status
- [ ] Dry scan report during add

## Phase 3: Manual Sync Engine

- [x] Scan source directory
- [x] Scan target directory
- [x] Apply exclude rules
- [x] Compare mtime, size, and hash when needed
- [x] Implement latest-modified-wins copy plan
- [x] Copy source changes to target
- [x] Copy target changes to source
- [x] Handle inner-file deletions
- [x] Implement `qs sync [name]`
- [x] Run initial sync during `qs add`
- [x] Add core sync unit tests
- [x] Add end-to-end CLI integration tests for manual sync

## Phase 4: Daemon Auto Sync

- [x] Create `qsd`
- [x] Watch source directories
- [x] Watch target directories
- [x] Debounce file events
- [x] Run item sync jobs
- [x] Add periodic reconciliation
- [x] Prevent concurrent sync jobs for the same item
- [x] Add `qsd --once` for one-shot daemon verification
- [x] Add daemon integration test

## Phase 4.5: Directory Association and Command Semantics

- [x] Remove global workspace initialization
- [x] Store metadata under Application Support
- [x] Use source directory names as item names
- [x] Reject duplicate item names
- [x] Defer single-file sync support
- [x] Implement `qs delete <name>`
- [x] Define `remove` as association-only
- [x] Add detailed `USAGE.md`

## Phase 5: macOS Install and Polish

- [x] Add LaunchAgent plist
- [x] Add install/uninstall script or Homebrew formula
- [x] Add log directory and basic log output
- [x] Add isolated automated app tests with temporary app/iCloud paths
- [x] Add simplified `qs doctor`
- [x] Show daemon installed/running state in `qs status`
- [x] Improve user-facing errors
- [ ] Add integration tests with temporary folders

## Deferred

- Multi-device pull command
- Advanced iCloud placeholder APIs
- Manual conflict review
- Version history
- GUI
