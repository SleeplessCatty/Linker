# Linker Command Usage

Linker links a source directory to a target parent directory.

```bash
linker add <source-directory> <target-parent-directory>
```

If the source directory is `~/Documents/Notes`, the item name is `Notes`, and the target directory becomes:

```text
<target-parent-directory>/Notes/
```

There is no `linker init` step and no global Linker workspace.

## Install

```bash
./scripts/install.sh
```

Verify:

```bash
linker doctor
linker status
```

The install script creates `/usr/local/bin/linker` and `/usr/local/bin/linkerd` by default. This may ask for your administrator password.

To install without command links:

```bash
./scripts/install.sh --no-link
```

Remote install:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash
```

## Add a Directory

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

This creates or reuses:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/Notes/
```

The item name is always the source directory name. Names must be unique; Linker rejects a second association named `Notes`.

The source and target directories must be separate. Linker rejects associations where one side contains the other.

## Ignore Files

Only `.gitignore` files inside the association control ignores. Edit them on either side, then let the daemon sync or run `linker sync <name>`. No Git repository is required. With no applicable rules, nothing is excluded.

This is **Linker's basic subset**, not full Git compatibility:

| Pattern | Meaning |
| --- | --- |
| `.env`, `.DS_Store` | Same name at this level or any descendant level; file or directory |
| `node_modules/` | Same-named directories and all their contents |
| `/build`, `src/cache/` | Path relative to the directory containing this `.gitignore` |
| `*.log`, `temp*`, `labs/*/.env` | A single `*` matches zero or more characters, never `/` |

A slash at the beginning or inside a pattern anchors it to the rule directory; a trailing slash restricts it to directories. Matching is case-sensitive, includes hidden names, trims leading/trailing whitespace, and preserves internal spaces. Blank lines and whole-line `#` comments are ignored.

Negation (`!`), double stars (`**`), `?`, character groups (`[]`), backslash escapes, empty paths/segments, and `.` or `..` segments are unsupported. Linker reports the file, line number, and reason, skips the **entire line**, and continues with valid lines. In particular, a skipped `!keep.log` does not protect a file matched by `*.log`.

Rules in active root and child directories accumulate: any match excludes the path; there is no override or reinclusion. Ignored directories are not searched for nested rules. Rules above/outside the associated tree are not loaded.

### Sync and safety

- Ignored source files remain in place and their contents are not read. Matching target copies are deleted, including target-only files and complete ignored directories.
- Active `.gitignore` files are control files and sync even when a pattern matches their name. Controls inside ignored directories are not exempt.
- Each pass resolves control files first using the newer modification time, preferring the source on ties, then applies those effective rules in the same pass. Normal baseline-based deletion also applies to control files.
- Editing or deleting controls causes reevaluation next pass. Removing an ignore restores normal sync; prior target cleanup is not propagated as a source deletion.
- Unreadable controls and control-file type conflicts abort that association's pass before changes. Unsupported syntax only warns. Each invalid line warns once per pass; the daemon does not repeat unchanged rule-content warnings until content changes or it restarts.
- Target cleanup never follows symbolic links. Cleanup failures are reported. Output gives actual deleted target file counts and separately pruned target directories.

For example, `*.py[cod]` and `!labs/*/output/.gitkeep` are skipped. With `labs/*/output/*` also present, `.gitkeep` is ignored and its target copy is removed. Installation never rewrites user rule text.

### Upgrade from 0.2

Version 0.3.0 removes `linker rule`, `add --ignore-file`, and `add --exclude`; old invocations return argument errors. Associations and file state are preserved. Old manual rules and known rule snapshots are archived, not applied. See [INSTALL.md](INSTALL.md) for backup and migration details.

## Sync

The daemon normally syncs automatically after `linker add`.

Current automatic sync timing:

- file change events are debounced for 2 seconds before syncing
- a periodic reconciliation pass runs every 5 minutes
- newly added associations are loaded into the running daemon on the next reconciliation (up to 5 minutes); `add` itself performs the initial sync immediately, and `linker sync <name>` remains available in the meantime

Manual sync is mainly for testing, recovery, or immediate verification:

```bash
linker sync
linker sync demo
```

Conflict behavior is latest-modified-wins. If source and target copies differ, the side with the newer modification time overwrites the older side. Equal modification times prefer the source.

### Preview Before Syncing

```bash
linker sync --dry-run
linker sync demo --dry-run
```

Preview uses the same effective `.gitignore` rules and conflict decisions as real sync, including control-file changes. It prints an `ACTION | PATH` table per association and sends invalid-rule warnings to stderr.

| Action | Meaning |
| --- | --- |
| `write_target` | Copy source to target, creating or replacing the displayed target file |
| `write_source` | Copy target to source, creating or replacing the displayed source file |
| `delete_target` | Propagate an ordinary source-side deletion to the displayed target file |
| `delete_source` | Propagate an ordinary target-side deletion to the displayed source file |
| `prune_target_file` | Remove an ignored target file/link; source is retained |
| `prune_target_directory` | Remove an ignored target directory after its children |

No-change associations print `no changes`; an unconfigured installation prints `no items`. Unknown names, unavailable roots, and invalid/unreadable controls return an error.

Dry-run does not change sync files, manifests, baselines, timestamps, or stored errors, and never initializes/migrates the database. It requires existing source and target roots and already-upgraded metadata; to preview a legacy upgrade, use the copied-database procedure in [INSTALL.md](INSTALL.md). Per-association lock files may be created. Scanning reads non-ignored content and can cause iCloud to download placeholders.

**Preview is not a pause or an approval gate.** A running daemon can sync independently after the preview releases its item lock. Stop the daemon first if you need to review a cleanup plan before anything changes. Results are a snapshot, not a saved executable plan; a later sync recomputes them.

## Inspect Items

```bash
linker list
linker status
linker doctor
```

`list` prints one name-sorted table with `NAME`, `TYPE`, `STATUS`, `SOURCE`, `TARGET`, `LAST SYNC (UTC)`, and `LAST ERROR` columns. Example with shortened demonstration paths:

```text
+------+-----------+--------+-----------+-----------+---------------------+------------+
| NAME | TYPE      | STATUS | SOURCE    | TARGET    | LAST SYNC (UTC)     | LAST ERROR |
+------+-----------+--------+-----------+-----------+---------------------+------------+
| demo | directory | active | /src/demo | /dst/demo | 2026-09-19 08:30:00 | -          |
+------+-----------+--------+-----------+-----------+---------------------+------------+
```

Paths and errors are not truncated. Chinese names and other Unicode text are padded by display width. Newlines, tabs, control characters, backslashes and vertical bars embedded in cells are visibly escaped so they cannot create extra table rows or terminal escape sequences.

`LAST SYNC (UTC)` is the last **successful** sync, formatted as `YYYY-MM-DD HH:MM:SS` in UTC, not local time. `-` means never synced/no recorded error; an invalid stored timestamp is shown as `invalid (<seconds>)`. An empty installation prints `no items`.

For long paths on a narrow terminal, scroll horizontally:

```bash
linker list | less -S
```

`status` only shows background daemon health.

## Remove vs Delete

Remove only the Linker association:

```bash
linker remove demo
```

This keeps:

- the source directory
- the target directory

It deletes local Linker metadata for that association.

Delete the Linker association and target directory:

```bash
linker delete demo
```

This keeps the source directory, but deletes:

- the target directory
- the matching local manifest

## Mobile Editing

Use an iCloud Drive folder as the target parent if you want mobile access:

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Then open `Notes` in iCloud Drive on iPhone or iPad. Edits made there are synced back to the source directory by the daemon.

## Filesystem and Failure Boundaries

Sync tracks regular file contents, not empty-directory history. Parent directories are created as needed for copied files; an ordinary file deletion can leave an empty parent directory. Symlink data entries are not copied or followed, while an ignored target symlink itself may be removed. New single-file associations are not supported by `add`; existing stored single-file associations are retained for compatibility.

`linker sync` applies associations in name order and stops at the first error; successful earlier changes are not rolled back. The daemon logs errors per association and continues with other associations, retrying on events/reconciliation. Control preflight errors stop that association before file operations; later I/O errors can leave a partially applied pass. Inspect `LAST ERROR`, preserve backups, and retry after correcting the cause.

## Common Issues

If a name is rejected, another association with the same source directory name already exists. Rename the source directory or remove the old association.

If a target path is rejected, make sure the target parent is outside the source directory.

If a file does not come back after removing a rule, run:

```bash
linker sync <name>
linker list
```
