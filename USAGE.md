# Linker Command Usage

Linker links a source directory to a exact target directory.

```bash
linker add <source-directory> <target-directory> [--name <name>]
```

The second argument is used directly; Linker never appends the source or record name. The record name defaults to the source directory basename, or can be set with `--name`.

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
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs/Notes
```

This creates the following exact directory if missing, or accepts it only if it is empty:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/Notes/
```

To use different folder names and disambiguate same-named sources:

```bash
linker add ~/work/Notes ~/Cloud/WorkNotes --name work-notes
linker add ~/personal/Notes ~/Cloud/PersonalNotes --name personal-notes
linker sync work-notes
```

`--name` changes only the association name used by `list`, `sync`, `remove`, and `delete`; it does not rename either directory. Without it, Linker uses the basename of the canonical source path. It does not rename an existing record.

### Add validation and failure behavior

- The source must be an existing directory. Both arguments are required; paths with spaces must be quoted or shell-escaped. Relative paths and `~` are supported.
- The target must be missing or a completely empty directory. **Every entry counts**, including `.DS_Store`, `.gitignore`, ignored files, empty subdirectories, and broken symlinks. A nonempty target fails with its path and a reason; Linker does not merge, clear, or overwrite existing contents to make it acceptable.
- A target that is a regular file or a symbolic link is rejected. Existing ancestor symlinks are resolved before checking separation; creating missing directories does not follow ancestors replaced by symlinks.
- Source and target must be separate: neither may contain the other. Neither path may overlap either side of an existing association, or Linker's Application Support directory.
- New record names must be unique (ASCII case-insensitive) and must not alias an existing record ID. Names allow Unicode and internal spaces, but not leading/trailing whitespace, control characters, `/`, `\`, `.`, `..`, or reserved `.linker`. The maximum is 250 UTF-8 bytes. Existing orphan manifests are not overwritten.
- Registration checks are serialized across processes. A new item lock prevents the daemon from syncing while initial sync or rollback is in progress. Initial sync only copies source files to the empty target, applying source `.gitignore` rules; it cannot copy back to or delete the source. Later sync is bidirectional.
- Validation failures occur before creating target directories or registering the item. State setup/migration and lock files can still be created during registry checks. Once directory creation/initial sync begins, I/O failures can leave created directories or copied files. A caught initial-sync failure removes the new association, its baselines and manifest; it **keeps** the source and partial target copies. Rollback failures are explicitly reported. Inspect those copies before choosing an empty target for retry.
- This is not crash-atomic filesystem storage. Process termination, disk failure, or an external writer racing with `add` can require manual inspection; registration locks coordinate Linker processes, not iCloud or other applications.

**Breaking CLI change:** old commands that supplied a target parent must append the desired folder explicitly. A nonempty parent now fails; an empty parent is treated as the exact target, not automatically corrected. Existing stored associations are unchanged. Re-adding a removed association to its retained nonempty target is intentionally rejected; there is no force/merge option.

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

Known failure limitation: if target removal fails partway (for example, `Directory not empty` while iCloud modifies the tree), registration and baselines may remain. A later sync can interpret partial target deletions as source deletions. Stop the daemon and inspect both sides before recovery; do not assume a failed `delete` is harmless or that re-adding fixes it. This outstanding delete failure-protection issue is separate from the guarded `add` workflow.

## Mobile Editing

Use a missing or empty directory inside iCloud Drive as the exact target if you want mobile access:

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs/Notes
```

Then open `Notes` in iCloud Drive on iPhone or iPad. Edits made there are synced back to the source directory by the daemon.

## Filesystem and Failure Boundaries

Sync tracks regular file contents, not empty-directory history. Parent directories are created as needed for copied files; an ordinary file deletion can leave an empty parent directory. Symlink data entries are not copied or followed, while an ignored target symlink itself may be removed. New single-file associations are not supported by `add`; existing stored single-file associations are retained for compatibility.

`linker sync` applies associations in name order and stops at the first error; successful earlier changes are not rolled back. The daemon logs errors per association and continues with other associations, retrying on events/reconciliation. Control preflight errors stop that association before file operations; later I/O errors can leave a partially applied pass. Inspect `LAST ERROR`, preserve backups, and retry after correcting the cause.

## Common Issues

If a name is rejected, check its syntax and choose a unique `--name`; you do not need to rename the source directory.

If a target path is rejected, check that it is the exact missing/empty destination, not a nonempty parent. Check hidden entries and path overlaps. Do not delete existing contents just to bypass this protection.

If a file does not come back after removing a rule, run:

```bash
linker sync <name>
linker list
```
