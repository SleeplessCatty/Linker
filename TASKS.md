# Linker Development Tasks

## MVP Decisions

- CLI binary name: `linker`
- Daemon binary name: `linkerd`
- MVP target: macOS directory sync
- Sync model: bidirectional mirror copy between source directory and target directory
- Conflict behavior: latest modified file wins
- First implementation order: manual sync before daemon auto sync
- `remove` preserves both trees; `delete` must preserve source data (partial-failure protection remains outstanding below)

## Phase 1: CLI, State, Manifest

Status: completed

- [x] Create Rust workspace
- [x] Create `linker` CLI binary
- [x] Create core library for paths, manifest, and state
- [x] Initialize SQLite state database
- [x] Implement `linker add <source-directory> <target-directory> [--name <name>]`
- [x] Implement `linker list` as a Unicode-aligned table with full paths, UTC times and errors
- [x] Implement `linker status` daemon health
- [x] Use exact target path; reject nonempty targets including hidden entries
- [x] Write manifest under Application Support
- [x] Write schema-2 manifests; archive retired rule snapshots
- [x] Verify with workspace tests, format, strict Clippy and compilation checks

## Phase 2: Basic `.gitignore` (0.3.0)

- [x] Load only in-tree `.gitignore` controls; nested additive rules
- [x] Match names, directories, relative paths, single `*`
- [x] Warn and skip unsupported advanced syntax
- [x] Resolve controls before data; preserve ignored source contents
- [x] Clean target-only ignored paths without following symlinks
- [x] Retire ignored baselines and restore normal sync after rule removal
- [x] Remove manual rule APIs, CLI flags, counters, snapshots and full matcher dependency
- [x] Back up and migrate schema 1 to schema 2, preserving associations/state
- [x] Add read-only core preview helper for upgrade inventories
- [x] Public CLI preview: `linker sync [name] --dry-run`, read-only sync state and explicit legacy/missing-root errors

## Phase 3: Manual Sync Engine

- [x] Scan source directory
- [x] Scan target directory
- [x] Apply simplified `.gitignore` rules
- [x] Compare mtime, size, and hash when needed
- [x] Implement latest-modified-wins copy plan
- [x] Copy source changes to target
- [x] Copy target changes to source
- [x] Handle inner-file deletions
- [x] Implement `linker sync [name]`
- [x] Run initial sync during `linker add`
- [x] Add core sync unit tests
- [x] Add end-to-end CLI integration tests for manual sync

## Phase 4: Daemon Auto Sync

- [x] Create `linkerd`
- [x] Watch source directories
- [x] Watch target directories
- [x] Debounce file events
- [x] Run item sync jobs
- [x] Add periodic reconciliation
- [x] Prevent concurrent sync jobs for the same item
- [x] Add `linkerd --once` for one-shot daemon verification
- [x] Add daemon integration test

## Phase 4.5: Directory Association and Command Semantics

- [x] Remove global workspace initialization
- [x] Store metadata under Application Support
- [x] Default to source basename, with independent optional `--name`
- [x] Reject duplicate item names
- [x] Defer single-file sync support
- [x] Implement `linker delete <name>`
- [x] Define `remove` as association-only
- [x] Add detailed `USAGE.md`

## Phase 5: macOS Install and Polish

- [x] Add LaunchAgent plist
- [x] Add install/uninstall script or Homebrew formula
- [x] Add log directory and basic log output
- [x] Add isolated automated app tests with temporary app/iCloud paths
- [x] Add simplified `linker doctor`
- [x] Show daemon installed/running state in `linker status`
- [x] Improve user-facing errors
- [x] Add integration tests with temporary folders

## Documentation and Acceptance (0.3.0)

- [x] Document table fields, UTC timestamps, empty values and wide-terminal usage
- [x] Document dry-run actions, prerequisites and independent daemon activity
- [x] Clarify new-association watcher timing and file/empty-directory behavior
- [x] Record committed functionality, tests and deferred scope in `docs/PROJECT_AUDIT.md`
- [x] Include strict Clippy in CI and the pre-push checklist
- [x] Keep GitHub Release and stable Homebrew publishing outside this change

The current CLI/ignore/sync additions have implementation and test coverage, subject to the failure boundaries in the audit. No `add --dry-run` interface is introduced: `add` accepts two directory arguments plus optional `--name` and performs initial sync.

## Add Safety Acceptance

- [x] Exact target with a different basename and custom/default names
- [x] Reject hidden files, empty child directories, links, overlaps and invalid/duplicate names
- [x] Serialize concurrent registration; protect initial sync with the item lock
- [x] Roll back failed registration/baselines without deleting source or partial target copies
- [x] Exercise permissions, control failures, injected database errors and concurrent CLI processes
- [x] Document incompatible positional semantics and unchanged existing records

## Outstanding Defect

- [ ] Make partial `delete` failures safe before later daemon sync can propagate target deletions to the source. Not changed by the add safety work; see USAGE.md.

## Deferred

- Multi-device pull command
- Advanced iCloud placeholder APIs
- Manual conflict review
- Version history
- GUI
