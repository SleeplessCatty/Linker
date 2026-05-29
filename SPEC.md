# QuickSync MVP Technical Design

## Design Goal

Build the smallest reliable version of QuickSync:

1. `qsync add <path>` adds a local file or folder.
2. A background daemon automatically mirrors eligible content through iCloud Drive.
3. Users can customize exclude rules.
4. If both sides differ, the newest modified file wins automatically.

The MVP deliberately avoids complex conflict management, multi-device administration, and deep recovery workflows.

## Architecture

```text
Local File/Folder
    |
    | scan/watch/copy
    v
qsyncd
    |
    | mirror copy
    v
iCloud Drive / QuickSync / <name>
```

The sync model is bidirectional mirror copy.

QuickSync does not:

- move the original path
- create symlinks
- mount a filesystem
- implement its own cloud backend

## Paths

iCloud base path:

```text
~/Library/Mobile Documents/com~apple~CloudDocs
```

QuickSync cloud workspace:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/QuickSync/
├── <synced file or directory>
└── .quicksync/
    ├── manifests/
    │   └── <name>.json
    └── rules/
        └── <name>.ignore
```

Local app state:

```text
~/Library/Application Support/QuickSync/
├── state.sqlite
├── logs/
└── tmp/
```

No QuickSync control files are written into the user's synced source folder.

## MVP Commands

```text
qsync add <path> [--name <name>] [--ignore-file <path>] [--exclude <pattern>]...
qsync rule <name> list
qsync rule <name> exclude <pattern>
qsync rule <name> include <pattern>
qsync list
qsync status [name]
qsync sync [name]
qsync remove <name>
qsync delete <name>
qsync doctor
```

Command responsibilities:

- `add`: create item, create visible iCloud path, create hidden metadata, perform initial sync.
- `rule list`: list all exclude rules for an item.
- `rule exclude`: add one exclude rule and prune matching cloud-visible files.
- `rule include`: delete one matching exclude rule; if no rule matches, succeed without changing rules.
- `list`: show configured items.
- `status`: show daemon/basic item health.
- `sync`: run one sync pass manually.
- `remove`: stop syncing an item without deleting local or cloud files.
- `delete`: stop syncing and remove cloud-visible files plus hidden metadata.

## Manifest

Each item has one hidden cloud manifest:

```json
{
  "schema_version": 1,
  "id": "uuid",
  "name": "demo",
  "type": "directory",
  "local_path_hint": "/Users/jason/code/demo",
  "item_path": "demo",
  "rule_path": ".quicksync/rules/demo.ignore",
  "created_at": "2026-05-29T00:00:00Z",
  "updated_at": "2026-05-29T00:00:00Z"
}
```

Notes:

- `id` is the internal stable identity.
- `name` is the visible iCloud file/folder name and must be unique.
- `type` is `file` or `directory`.
- `local_path_hint` is only a convenience hint for the original device, not an authority.

## State Database

Use one SQLite database:

```text
~/Library/Application Support/QuickSync/state.sqlite
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

The database also stores exclude rules and per-file sync state.

## Rule Engine

Default rules are empty. QuickSync does not automatically import `.gitignore` and does not apply forced template excludes.

Each item has one plain-text rule file:

```text
QuickSync/.quicksync/rules/<name>.ignore
```

The file stores one rule per line. `--ignore-file`, repeated `--exclude`, and later `qsync rule exclude` all produce the same kind of rule.

Implementation uses Rust's `ignore` crate and follows the same matching semantics as Git ignore files.

When rules change:

- future scans use the new rules
- newly excluded local files are ignored
- newly excluded cloud-visible files are removed from iCloud
- local files are not deleted only because they became excluded

## Sync Algorithm

For each item:

```text
1. scan local side
2. scan cloud side
3. apply exclude rules
4. compare local and cloud file metadata
5. decide operation
6. copy/delete
7. update file_states
```

For a directory item, relative paths are normal paths inside the directory.

For a single-file item, the internal relative path is empty and both sides point directly to the file.

## Latest Modified Wins

If local and cloud both exist and differ:

- local newer -> copy local to cloud
- cloud newer -> copy cloud to local
- same content -> no-op
- same mtime but different content -> prefer local by default

This matches the desired automatic, iCloud-like behavior. It is not a version-control system.

## Delete Behavior

Inside an item:

- local file deleted -> delete cloud mirror file
- cloud mirror file deleted -> delete local file

Item commands:

- `remove` deletes only local QuickSync association state.
- `delete` deletes local QuickSync association state, cloud-visible content, and hidden cloud metadata.
- neither command deletes the user's original local file or folder.

Root local path missing pauses the item by marking an error; QuickSync should not automatically delete a root path.

## Watcher and Scheduling

The daemon watches:

- local item paths
- cloud item paths

Scheduling:

```text
file event -> debounce 2 seconds -> sync item
manual qsync sync -> sync immediately
periodic reconciliation -> every 5 minutes
```

Current daemon constants:

- event debounce: 2 seconds
- periodic reconciliation: 300 seconds
