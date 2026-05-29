# QuickSync Development Tasks

## MVP Decisions

- Binary name: `qsync`
- MVP target: macOS file and directory sync
- Sync model: bidirectional mirror copy between local path and visible iCloud path
- Conflict behavior: latest modified file wins
- First implementation order: manual sync before daemon auto sync
- Local files are never deleted when removing an item from QuickSync

## Phase 1: CLI, State, Manifest

Status: completed

- [x] Create Rust workspace
- [x] Create `qsync` CLI binary
- [x] Create core library for paths, manifest, and state
- [x] Initialize SQLite state database
- [x] Implement `qsync add <path>`
- [x] Implement `qsync list`
- [x] Implement basic `qsync status [name]`
- [x] Create iCloud workspace folders
- [x] Write manifest under `QuickSync/.quicksync/manifests/<name>.json`
- [x] Write rule snapshot under `QuickSync/.quicksync/rules/<name>.ignore`
- [x] Verify with `cargo test` once Rust toolchain is available

## Phase 2: Exclude Rules

- [x] Default to empty excludes
- [x] Support explicit `--ignore-file`
- [x] Support explicit `--exclude`
- [x] Implement `qsync rule <name> exclude <pattern>`
- [x] Implement `qsync rule <name> include <pattern>`
- [x] Implement `qsync rule <name> list`
- [x] Show exclude count in status
- [ ] Dry scan report during add

## Phase 3: Manual Sync Engine

- [x] Scan local folder
- [x] Scan iCloud mirror folder
- [x] Apply exclude rules
- [x] Compare mtime, size, and hash when needed
- [x] Implement latest-modified-wins copy plan
- [x] Copy local changes to cloud
- [x] Copy cloud changes to local
- [x] Handle inner-file deletions
- [x] Implement `qsync sync [name]`
- [x] Run initial sync during `qsync add`
- [x] Add core sync unit tests
- [x] Add end-to-end CLI integration tests for manual sync

## Phase 4: Daemon Auto Sync

- [x] Create `qsyncd`
- [x] Watch local folders
- [x] Watch cloud mirror folders
- [x] Debounce file events
- [x] Run item sync jobs
- [x] Add periodic reconciliation
- [x] Prevent concurrent sync jobs for the same item
- [x] Add `qsyncd --once` for one-shot daemon verification
- [x] Add daemon integration test

## Phase 4.5: iCloud Layout and Command Semantics

- [x] Remove public `Items/`, `Manifests/`, and `Rules/` layout
- [x] Store metadata under hidden `.quicksync/`
- [x] Use visible item names for cloud files and folders
- [x] Reject duplicate visible names
- [x] Add single-file sync support
- [x] Implement `qsync delete <name>`
- [x] Define `remove` as association-only
- [x] Add detailed `USAGE.md`

## Phase 5: macOS Install and Polish

- [x] Add LaunchAgent plist
- [x] Add install/uninstall script or Homebrew formula
- [x] Add log directory and basic log output
- [x] Add isolated automated app tests with temporary app/iCloud paths
- [x] Add simplified `qsync doctor`
- [x] Show daemon installed/running state in `qsync status`
- [x] Improve user-facing errors
- [ ] Add integration tests with temporary folders

## Deferred

- Multi-device pull command
- Advanced iCloud placeholder APIs
- Manual conflict review
- Version history
- GUI
