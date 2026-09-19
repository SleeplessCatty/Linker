# Linker MVP Technical Design

## Design Goal

Build the smallest reliable version of Linker:

1. `linker add <source-directory> <target-directory> [--name <name>]` creates a directory association.
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
Exact Target Directory (user-selected name)
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
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs/Notes
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
linker add <source-directory> <target-directory> [--name <name>]
linker list
linker status
linker sync [name] [--dry-run]
linker check [name]
linker repair [name] [--prefer source|target|newest] [--prune] [--dry-run]
linker remove <name>
linker delete <name>
linker doctor
```

Command responsibilities:

- `add`: register a unique optional name, use the exact target path (missing or empty only), create local metadata, perform a guarded source-to-target initial sync.
- `list`: render one table, ordered by name, with name/type/status/full paths/UTC last-successful-sync/error columns; use display-width padding, escape control characters and never truncate paths.
- `status`: show daemon health only.
- `sync`: run one sync pass manually; `--dry-run` uses a read-only database connection and the same planner, returning operations without applying them or updating sync state.
- `check`: report the differences between both sides of every association (or one named association) without changing anything; exit status 1 when a blocking difference exists.
- `repair`: make every diverging path match one explicitly requested authoritative side, updating baselines so a later sync agrees; deletions require `--prune`.
- `remove`: stop syncing an item without deleting source or target directories.
- `delete`: stop syncing and remove the target directory.

## Add Registration Safety

Resolve the source and existing target ancestors before creating directories. Reject nonempty targets (including hidden entries), non-directory targets, final target symlinks, containment between the two sides, overlap with registered source/target trees except exact directory-source reuse, and overlap with app state. Names are checked for invalid path/control characters, outer whitespace, reserved names, length, ASCII case collisions and aliases of existing IDs; orphan manifests are not overwritten.

The persistent add-registry process lock covers duplicate/overlap checks, publication and initial sync. Create missing directories with descriptor-relative no-follow traversal; revalidate roots and target emptiness before publication. The per-item lock followed by a shared canonical-source lock is held before publishing the new item and until success or rollback, so a daemon cannot sync an incomplete registration. Initial planning verifies an empty target and rejects reverse copies or target cleanup; per-operation checks detect changed files.

On a caught registration/initial-sync error, transactionally remove this new ID and its baselines, then remove its owned manifest. Keep source files and any partial target copies; report rollback failures explicitly. This does not promise crash-atomicity or exclusion of external filesystem writers. Existing stored paths/names and schema-2 manifests do not change. Database schema 3 permits repeated source paths. See [USAGE.md](USAGE.md#add-a-directory) for the breaking positional-argument change.

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
- `name` defaults to the canonical source basename; `--name` supplies an independent unique record name. Neither path is derived from this name.
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
    local_path TEXT NOT NULL,
    cloud_path TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    last_sync_at INTEGER,
    last_error TEXT,
    UNIQUE(local_path, cloud_path)
);
CREATE INDEX items_source_path ON items(local_path);
```

The internal column names still use `local_path` and `cloud_path` for compatibility. User-facing behavior treats them as source and target paths.

The database also stores per-file sync state. Schema 2 removes `exclude_rules` and `items.rule_path`. Opening schema 1 takes a migration lock, backs up SQLite plus owned manifests/rule snapshots to `backups/gitignore-v2`, then migrates transactionally. Filesystem completion is restartable; only known snapshots whose content equals the backup are removed. Unknown files are left alone. Associations and file states remain intact, and old rules are not applied.

### Shared-source storage and scheduling

Schema 3 backs up SQLite to `backups/shared-source-v3/state.sqlite` before transactionally rebuilding `items` without source-only uniqueness. Names and source/target pairs remain unique. Preserve IDs, timestamps, errors and all `file_states`; check foreign keys before commit. The migration process lock serializes upgrades. Failed transactions leave schema 2 intact and are retryable; a committed version marker prevents overwriting the original backup on later opens. Manifest schema remains 2.

New registrations allow exact canonical directory-source equality only; source containment, all target overlap and source/target cross-role overlap remain forbidden. Each association owns its baseline, so removing or rolling back one association does not retire another's state. Acquisition order is add-registry (registration only), item, then canonical stored source path. Initial sync, regular sync, preview, remove and delete use the source lock; unrelated sources do not share it.

A shared source connects independent bidirectional pairs. Edits, ordinary deletions and controls can propagate from one target through the source to other targets. Preserve existing per-pair conflict/deletion semantics; all-item sync follows name order and may require a further pass for convergence. No all-target atomic snapshot or immediate fan-out on a single named sync is promised. The daemon maps each watched root to a set of item IDs: a source event marks every associated target dirty, while a target event initially marks its own record; source writes then generate follow-up source events. Old daemons lack source locks and event fan-out and must be stopped before upgrade.

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

A target root that is absent at the start of an existing association's pass is recreated; when the baselines still record target content, that pass restores from the source instead of reading every path as a target-side deletion. The summary reports `target_root_recovered`, the CLI warns, and the daemon logs the flag, so an unmounted, moved or lost target root cannot delete the whole source tree. An existing target root keeps ordinary per-file semantics, including that removing its last file propagates; `check` names those paths before a sync runs and `repair` can refill the target without deleting source content.

## Dry-Run Contract

`StateDb::open_read_only` refuses uninitialized/legacy schemas instead of migrating them. `preview_item` takes the same item lock as sync, reloads the association after acquiring it, builds and verifies the control plan, and returns an operation inventory. It does not retire baselines, apply controls, delete files, update item status, or create missing roots. Lock files may be created. Both roots must already exist.

CLI renders absolute action/path pairs, warnings on stderr, and `no changes` for an empty plan. A missing database returns `no items` without creating storage (or item-not-found for an explicit name). The plan is only a snapshot; it is neither persisted nor used as a gate for subsequent daemon passes.

## Consistency Audit and Repair

`sync` converges what the baselines explain and resolves competing content by modification time. `check` and `repair` cover everything else and let the caller state which side is authoritative instead of inferring it.

`linker check [name]` is read-only. It takes the same item and source locks, scans both roots with the same controls, effective rules and no-follow access that sync uses, and reports the identical file count plus one row per difference:

| Class | Meaning | Blocking |
| --- | --- | --- |
| `content_differs` | Both sides hold a regular file with different content; the row states which side is newer and which side the baseline still matches. | yes |
| `source_only` | A file exists only in the source; a normal sync copies it to the target, or deletes it from the source when the baseline proves the target copy was removed after the last sync. | yes |
| `target_only` | The mirror case on the target side. | yes |
| `type_conflict` | One side is a regular file where the other is a directory; ordinary sync fails this association until one side changes, and baselines inside the conflicting entry are left unread. | yes |
| `ignored_target_content` | Target content matched by an effective `.gitignore` rule; the source copy is kept and target cleanup removes this one. | no |
| `unsupported_entry` | A symbolic link or another special file; never followed or synchronized. | no |

The exit status is 1 when any blocking class is present, so the command can gate scripting. An association whose only differences are `ignored_target_content` or `unsupported_entry` reports `consistent` with advisories and exits 0. The audit itself writes nothing: only synchronization lock files may be created, and `StateDb::open_read_only` refuses uninitialized or legacy state instead of migrating it. A missing root is an error, never a silently created directory.

`linker repair [name] --prefer source|target|newest [--prune] [--dry-run]` makes every diverging non-ignored path match the authoritative side. `source` is the default. `newest` reproduces ordinary sync, including baseline-driven deletion propagation. Applied copies use the same `apply` path as sync, so `file_states` baselines are updated for each repaired path and a later daemon pass sees a converged association instead of reverting the repair.

The chosen side also resolves conflicting `.gitignore` control files, so the rules that drive a pass and the content it writes always come from the same side instead of mixing one side's rules with the other side's content. Removal rows state whether the baselines recorded the other side's copy, separating a deletion to propagate from a path the authoritative side never had. Retained single-file associations keep their target file optional: an absent file is ordinary content to restore, while a missing parent directory is a missing root.

Without `--prune` the command only adds and overwrites. A path the authoritative side does not have, ignored target content and the losing side of a type conflict are reported with their reason and skipped. `--prune` is the only option that can delete data: it removes those paths, and replacing a type conflict uses recursive no-follow removal, so a swapped target root or ancestor symlink cannot redirect the deletion into the source. Ignored baselines are retired before any pruned cleanup, as in ordinary sync.

A file-versus-directory conflict is resolved only with an explicit `source` or `target` preference, because `newest` has no comparable modification time for the pair. Entries that are neither regular files nor directories are never touched. `--dry-run` lists the operations without changing files or state, and requires the same upgraded, existing state as the applied form.

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
- neither command directly deletes the source directory.
- Both item commands hold the add-registry lock, then unregister the association and its baselines in one transaction before any target data is touched. A partial or interrupted target cleanup therefore cannot leave active baselines, and no later sync can propagate that cleanup to the source. The name and pair stay reserved until cleanup finishes, so a concurrent `add` cannot reuse them mid-deletion.
- Target removal runs through pinned parent descriptors and never follows the target root or an ancestor symlink. Cleanup failure is reported as `association removed; target cleanup failed at <path>; source kept, target may be partially removed; inspect the remainder manually` with exit status 1; the source is intact and the remainder is left for manual inspection. See [USAGE.md](USAGE.md#remove-vs-delete).

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
