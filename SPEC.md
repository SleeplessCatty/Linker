# Linker MVP Technical Design

## Design Goal

Build the smallest reliable version of Linker:

1. `linker add <source-directory> <target-parent-directory>` creates a directory association.
2. A background daemon automatically keeps the source directory and target directory in sync.
3. Users manage ignores only through in-tree `.gitignore` files.
4. If both sides differ, the newest modified file wins automatically.

There is no global Linker workspace and no `linker init` step.

## Architecture

```text
Source Directory
    ^
    | scan/watch/copy
    v
linkerd
    ^
    | scan/watch/copy
    v
Target Parent Directory / <source-directory-name>
```

The sync model is bidirectional mirror copy.

Linker does not:

- move the source directory
- create symlinks
- mount a filesystem
- implement its own cloud backend

## Paths

For this command:

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Linker stores:

```text
source directory: ~/Documents/Notes
target directory: ~/Library/Mobile Documents/com~apple~CloudDocs/Notes
item name: Notes
```

Local app state:

```text
~/Library/Application Support/Linker/
├── state.sqlite
├── manifests/
│   └── <name>.json
├── locks/
├── backups/
│   └── gitignore-v2/
├── logs/
└── tmp/
```

No Linker-specific control directory is created in sync trees. User `.gitignore` files are synchronized controls.

## MVP Commands

```text
linker add <source-directory> <target-parent-directory>
linker list
linker status
linker sync [name] [--dry-run]
linker remove <name>
linker delete <name>
linker doctor
```

Command responsibilities:

- `add`: create association, create or reuse target directory, create local metadata, perform initial sync.
- `list`: render one table, ordered by name, with name/type/status/full paths/UTC last-successful-sync/error columns; use display-width padding, escape control characters and never truncate paths.
- `status`: show daemon health only.
- `sync`: run one sync pass manually; `--dry-run` uses a read-only database connection and the same planner, returning operations without applying them or updating sync state.
- `remove`: stop syncing an item without deleting source or target directories.
- `delete`: stop syncing and remove the target directory.

## Manifest

Each item has one local manifest:

```json
{
  "schema_version": 2,
  "id": "uuid",
  "name": "demo",
  "type": "directory",
  "source_path": "/Users/jason/code/demo",
  "target_path": "/Users/jason/Library/Mobile Documents/com~apple~CloudDocs/demo",
  "created_at": "2026-07-11T00:00:00Z",
  "updated_at": "2026-07-11T00:00:00Z"
}
```

Notes:

- `id` is the internal stable identity.
- `name` is the source directory name and must be unique.
- `type` is currently `directory`.

## State Database

Use one SQLite database:

```text
~/Library/Application Support/Linker/state.sqlite
```

Minimum schema:

```sql
CREATE TABLE items (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    item_type TEXT NOT NULL DEFAULT 'directory',
    local_path TEXT NOT NULL UNIQUE,
    cloud_path TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_sync_at INTEGER,
    last_error TEXT
);
```

The internal column names still use `local_path` and `cloud_path` for compatibility. User-facing behavior treats them as source and target paths.

The database also stores per-file sync state. Schema 2 removes `exclude_rules` and `items.rule_path`. Opening schema 1 takes a migration lock, backs up SQLite plus owned manifests/rule snapshots to `backups/gitignore-v2`, then migrates transactionally. Filesystem completion is restartable; only known snapshots whose content equals the backup are removed. Unknown files are left alone. Associations and file states remain intact, and old rules are not applied.

## Rule Engine

`rules.rs` implements a path-segment matcher for names, directory-only suffixes, relative/anchored paths, and single `*`. It does not use the full Git matcher. Exact grammar, invalid-line warnings, nested accumulation, and control-file exemptions are documented in [USAGE.md](USAGE.md#ignore-files).

Each pass builds the rule set from effective source/target controls, top-down, without entering ignored source directories. Control read/type errors fail closed before changes. Daemon warning fingerprints suppress repeats until effective rule content changes or restart.

## Sync Algorithm

For each item:

```text
1. take per-association cross-process lock and reload item
2. resolve controls top-down; parse effective rules; build complete plan
3. inspect normal files and target-only ignored paths; do not read ignored source contents
4. verify root identities and control snapshots before mutation
5. retire ignored-path baselines so target cleanup cannot delete source later
6. apply control operations, prune ignored targets, apply normal file operations
7. update file_states and sync summary; release lock
```

Relative paths are normal paths inside the associated directory. `tree.rs` pins root/parent directory descriptors, uses no-follow relative opens and unlink operations, and atomically renames copied files. Symlink data files are not synchronized; an ignored target symlink itself can be removed without following it. Directory/control type conflicts and I/O errors are surfaced, not treated as missing rules. Cleanup summaries count successful file/link removals separately from directory removals.

## Dry-Run Contract

`StateDb::open_read_only` refuses uninitialized/legacy schemas instead of migrating them. `preview_item` takes the same item lock as sync, reloads the association after acquiring it, builds and verifies the control plan, and returns an operation inventory. It does not retire baselines, apply controls, delete files, update item status, or create missing roots. Lock files may be created. Both roots must already exist.

CLI renders absolute action/path pairs, warnings on stderr, and `no changes` for an empty plan. A missing database returns `no items` without creating storage (or item-not-found for an explicit name). The plan is only a snapshot; it is neither persisted nor used as a gate for subsequent daemon passes.

## Latest Modified Wins

If source and target both exist and differ:

- source newer -> copy source to target
- target newer -> copy target to source
- same content -> no-op
- same mtime but different content -> prefer source by default

This matches the desired automatic behavior. It is not a version-control system.

## Delete Behavior

Inside an item:

- source file deleted -> delete target file
- target file deleted -> delete source file
- an unchanged surviving copy follows a recorded deletion; a changed surviving copy is restored instead
- ignored paths bypass ordinary deletion propagation, keep source contents, and lose old baselines; cancellation of ignores restores normal sync

Item commands:

- `remove` deletes local Linker association state and local metadata, but keeps source and target directories.
- `delete` deletes local Linker association state, local metadata, and target directory.
- neither command deletes the source directory.

Ordinary synchronization tracks files; empty directories are not independently mirrored or removed. Manual all-item sync stops on the first error, without rollback of completed operations; the daemon handles errors per association and continues.

Root source path missing pauses the item by marking an error; Linker should not automatically delete a root path.

## Watcher and Scheduling

The daemon watches:

- source item paths
- target item paths

Scheduling:

```text
file event -> debounce 2 seconds -> sync item
manual linker sync -> sync immediately
periodic reconciliation -> every 5 minutes
```

Current daemon constants:

- event debounce: 2 seconds
- periodic reconciliation: 300 seconds
- the association registry is reloaded at startup and reconciliation, not immediately on `add`; the add command performs its own initial sync
