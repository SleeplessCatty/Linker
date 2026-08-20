# Linker MVP Technical Design

## Design Goal

Build the smallest reliable version of Linker:

1. `linker add <source-directory> <target-parent-directory>` creates a directory association.
2. A background daemon automatically keeps the source directory and target directory in sync.
3. Users can customize exclude rules.
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
├── rules/
│   └── <name>.ignore
├── logs/
└── tmp/
```

No Linker control files are written into the source directory or target parent directory.

## MVP Commands

```text
linker add <source-directory> <target-parent-directory> [--ignore-file <path>] [--exclude <pattern>]...
linker rule <name> list
linker rule <name> exclude <pattern>
linker rule <name> include <pattern>
linker list
linker status
linker sync [name]
linker remove <name>
linker delete <name>
linker doctor
```

Command responsibilities:

- `add`: create association, create or reuse target directory, create local metadata, perform initial sync.
- `rule list`: list all exclude rules for an item.
- `rule exclude`: add one exclude rule and prune matching target files.
- `rule include`: delete one matching exclude rule; if no rule matches, succeed without changing rules.
- `list`: show configured associations and item sync state.
- `status`: show daemon health only.
- `sync`: run one sync pass manually.
- `remove`: stop syncing an item without deleting source or target directories.
- `delete`: stop syncing and remove the target directory.

## Manifest

Each item has one local manifest:

```json
{
  "schema_version": 1,
  "id": "uuid",
  "name": "demo",
  "type": "directory",
  "source_path": "/Users/jason/code/demo",
  "target_path": "/Users/jason/Library/Mobile Documents/com~apple~CloudDocs/demo",
  "rule_path": "/Users/jason/Library/Application Support/Linker/rules/demo.ignore",
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
    rule_path TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_sync_at INTEGER,
    last_error TEXT
);
```

The internal column names still use `local_path` and `cloud_path` for compatibility. User-facing behavior treats them as source and target paths.

The database also stores exclude rules and per-file sync state.

## Rule Engine

Default rules are empty. Linker does not automatically import `.gitignore` and does not apply forced template excludes.

Each item has one plain-text rule file:

```text
~/Library/Application Support/Linker/rules/<name>.ignore
```

The file stores one rule per line. `--ignore-file`, repeated `--exclude`, and later `linker rule exclude` all produce the same kind of rule.

Implementation uses Rust's `ignore` crate and follows the same matching semantics as Git ignore files.

When rules change:

- future scans use the new rules
- newly excluded source files are ignored
- newly excluded target files are removed from the target directory
- source files are not deleted only because they became excluded

## Sync Algorithm

For each item:

```text
1. scan source side
2. scan target side
3. apply exclude rules
4. compare source and target file metadata
5. decide operation
6. copy/delete
7. update file_states
```

Relative paths are normal paths inside the associated directory.

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

Item commands:

- `remove` deletes local Linker association state and local metadata, but keeps source and target directories.
- `delete` deletes local Linker association state, local metadata, and target directory.
- neither command deletes the source directory.

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
