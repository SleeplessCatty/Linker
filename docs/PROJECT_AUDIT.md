# Linker Project Audit

## Current Architecture

Linker is a Rust workspace with three crates:

- `linker-core`: shared paths, state database, rule handling, manifests, health checks, and sync engine.
- `linker-cli`: user-facing `linker` command.
- `linker-daemon`: background watcher and periodic sync daemon.

Runtime state is stored outside the repository:

- local state: `~/Library/Application Support/Linker/`
- sync targets: selected per association with `linker add <source-directory> <target-directory> [--name <name>]`

## File Description

- `Cargo.toml`, `Cargo.lock`: Rust workspace and locked dependencies. Commit both because this is an application workspace.
- `README.md`: project overview and install entrypoints.
- `USAGE.md`: command-level usage and examples.
- `INSTALL.md`: script install, `/usr/local/bin`, remote install, and Homebrew packaging notes.
- `PRODUCT.md`: product scope and user behavior.
- `SPEC.md`: implementation design and sync behavior.
- `TASKS.md`: development progress and remaining roadmap.
- `crates/linker-cli/src/output.rs`: Unicode-width tables, safe cell escaping and readable UTC timestamps.
- `crates/linker-core/src/rules.rs`: basic `.gitignore` path-segment matcher and warning diagnostics.
- `crates/linker-core/src/sync.rs`: control-first planning, baseline retirement and counted target cleanup.
- `crates/linker-core/src/tree.rs`: descriptor-relative no-follow reads/copies/deletes.
- `crates/linker-core/src/migration.rs`: backed-up, restartable metadata migrations to database schema 3; manifests remain schema 2.
- `crates/linker-core/src/lock.rs`: persistent per-association process lock files.
- `crates/linker-core/examples/preview.rs`: upgrade inventory helper; use a copied database.
- `scripts/install.sh`: local checkout installer.
- `scripts/install-remote.sh`: curl/bash installer that clones the GitHub repo then runs the local installer.
- `scripts/uninstall.sh`: LaunchAgent and binary cleanup while preserving Linker state.
- `scripts/lib/cleanup-legacy.sh`: guarded pre-0.2 daemon, command-link, and Application Support cleanup.
- `scripts/tests/legacy-cleanup.sh`: isolated safety and idempotence tests for cleanup and managed links.
- `scripts/tests/install-safety.sh`: failed-upgrade and atomic LaunchAgent-write regression tests.
- `scripts/tests/uninstall-safety.sh`: daemon-stop failure regression test.
- `scripts/tests/remote-ref.sh`: branch, tag, and commit fetch tests for remote installation.
- `scripts/tests/branding-residue.sh`: repository-wide legacy-branding gate.
- `packaging/launchagent/com.linker.linkerd.plist.in`: LaunchAgent template.
- `packaging/homebrew/linker.rb`: starter Homebrew formula.
- `.github/workflows/ci.yml`: macOS CI for format, tests, strict Clippy, compile checks and Shell/packaging regressions.

## Agreed Scope Audit (2026-09-19)

Compared PRODUCT.md, SPEC.md, TASKS.md and the approved basic-.gitignore scope against current commands, core modules and regression tests. The previously unchecked public dry-run is now implemented as `sync --dry-run`; `add` now treats its second positional argument as an exact missing/empty destination and accepts optional `--name`. The newly requested list table is implemented, not just documented.

| Agreed capability | Implementation / evidence |
| --- | --- |
| Shared source, multiple targets | Pairwise baselines and source locks; CLI fan-out/propagation/rollback tests, daemon startup/live multi-target event tests, schema-3 migration tests |
| Add exact target and custom record name | `add_safety.rs`: destination contents/types, names, overlap, permissions, concurrent processes and rollback after partial copies |
| Add/list/status/doctor/remove/delete | CLI integration tests; table covers multiple/empty items and Unicode/control characters |
| Basic in-tree ignore subset and control-first sync | `rules.rs`, `sync.rs`; matching, warnings, nested rules, target-only cleanup and reinclusion tests |
| State-preserving schema-2 upgrade | Migration tests include six associations, retries, partial backups and unknown snapshot preservation |
| Bidirectional file sync and deletion | Core and CLI tests cover timestamps, permission preservation, deletion and source retention |
| Cross-process item locks and no-follow cleanup | Lock serialization and parent-symlink replacement regression tests |
| Daemon automatic sync and warning suppression | Daemon integration test edits/removes rules and checks unchanged warnings are not repeated |
| User-facing preview | CLI tests verify planned copies/deletions/cleanup, no file or DB-byte changes, no initialization, no legacy migration, and missing-root/control failures |
| Installation and packaging safety | Five isolated Shell regression suites plus plist/Ruby syntax checks |
| Documentation | Table examples, preview action semantics, daemon timing, failure boundaries, migration/rollback and deferred scope |

The requested CLI additions have implementation and test coverage. A separately identified partial-delete failure defect remains outstanding, as documented below. This is not a claim that every possible failure/race is eliminated. GUI, version history, merge UI, multi-device management, new single-file associations and GitHub/stable-Homebrew publishing remain outside this delivery.

## Known Defects and Risks

- Shared targets are bidirectionally connected through their source. Edits and ordinary deletions can spread to other targets; convergence is pairwise, not atomic. Old daemons lack shared-source locking and must be stopped before upgrading both binaries.
- `delete` removes the target before unregistering. Partial target-removal failure retains registration/baselines; later daemon sync can propagate missing target files to the source. Stop the daemon and inspect both trees before recovery. The new `add` rollback does not fix this separate deletion path.
- The exact-target `add` interface is incompatible with old parent-directory arguments. Existing records are unchanged; retained nonempty targets cannot be re-added. `add` rollback handles caught failures, not process termination or all external-writer races.

- `.gitignore` is a documented basic subset, not full Git compatibility; unsupported lines are skipped. Existing negation patterns must not be assumed to protect target files.
- Schema 2 migration preserves Linker 0.2 associations/state and archives old manual rules. Metadata backups do not protect user data; back up cleanup candidates before first 0.3 sync.
- There is no GUI yet; all workflows are CLI based.
- The sync policy is latest-modified-wins without version history or merge UI.
- Pre-0.2 application state is intentionally deleted during installation and is not migrated; source and target directories remain untouched.
- Homebrew formula requires a real GitHub release tarball SHA before stable `brew install linker` works and is intentionally not an upgrade path.
- LaunchAgent installation and incompatible cleanup are handled only by the guarded script installer, not Homebrew.
- `/usr/local/bin` linking may require `sudo` on machines where the directory is owned by `root`.
- Dry-run is a snapshot and does not suspend the daemon; stop the service before reviewing a plan that must not execute independently.
- Ordinary file sync does not mirror empty-directory history. Failed sync passes can be partially applied; this is not a transactional backup system.
- `linkerd --once` reports individual association failures through stderr and item state; its process exit status alone is not a per-item success check.
- The daemon reloads new associations only at startup or the next reconciliation (up to 5 minutes after add). It watches configured source/target roots and also performs a 5-minute reconciliation pass; very large trees may need future scan optimization.

## Verification Status

Local verification on 2026-09-19: 88 Rust tests passed (3 output-unit, 20 CLI integration, 26 add-safety integration, 6 core-unit, 17 sync integration, 9 migration, 1 daemon-unit, 6 daemon integration). Format, strict Clippy, compile/release build, Shell regressions and packaging syntax checks passed. This is local evidence; it does not claim that a remote CI run or GitHub release occurred. Add tests use isolated temporary state and directories; live associations are not modified. Permission checks require a non-root test user.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo check --workspace --all-targets
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh scripts/tests/install-safety.sh scripts/tests/uninstall-safety.sh scripts/tests/remote-ref.sh scripts/tests/branding-residue.sh
bash scripts/tests/legacy-cleanup.sh
bash scripts/tests/install-safety.sh
bash scripts/tests/uninstall-safety.sh
bash scripts/tests/remote-ref.sh
bash scripts/tests/branding-residue.sh
ruby -c packaging/homebrew/linker.rb
plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
```
