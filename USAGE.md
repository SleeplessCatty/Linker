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
- Source and target must be separate: neither may contain the other. Exactly the same canonical source directory may be reused with a different target and unique name. Nested source trees are still rejected; targets cannot equal, contain, or be inside any source or target tree. Neither side may overlap Linker's Application Support directory.
- New record names must be unique (ASCII case-insensitive) and must not alias an existing record ID. Names allow Unicode and internal spaces, but not leading/trailing whitespace, control characters, `/`, `\`, `.`, `..`, or reserved `.linker`. The maximum is 250 UTF-8 bytes. Existing orphan manifests are not overwritten.
- Registration checks are serialized across processes. Item and shared-source locks prevent conflicting daemon/CLI operations during initial sync or rollback. Initial sync only copies source files to the empty target, applying source `.gitignore` rules; it cannot copy back to or delete the source. Later sync is bidirectional.
- Validation failures occur before creating target directories or registering the item. State setup/migration and lock files can still be created during registry checks. Once directory creation/initial sync begins, I/O failures can leave created directories or copied files. A caught initial-sync failure removes the new association, its baselines and manifest; it **keeps** the source and partial target copies. Rollback failures are explicitly reported. Inspect those copies before choosing an empty target for retry.
- This is not crash-atomic filesystem storage. Process termination, disk failure, or an external writer racing with `add` can require manual inspection; registration locks coordinate Linker processes, not iCloud or other applications.

**Breaking CLI change:** old commands that supplied a target parent must append the desired folder explicitly. A nonempty parent now fails; an empty parent is treated as the exact target, not automatically corrected. Existing stored associations are unchanged. Re-adding a removed association to its retained nonempty target is intentionally rejected; there is no force/merge option.

## One Source, Multiple Targets

Keep an existing association and add a second target with another name; no `remove` is needed:

```bash
linker add /Users/jason/learn /Users/jason/Documents/learn --name learn
linker add /Users/jason/learn "/Users/jason/Library/Mobile Documents/iCloud~md~obsidian/Documents/learn" --name learn-ob
```

If `learn` already exists, run only the second command. Inside double quotes, spaces and these embedded tildes need **no backslashes**. Each target must be missing or empty. Each `add` still takes one target; repeat it for more targets. Duplicate source/target pairs are rejected even with a different name.

- Each association has its own name, target, manifest and file baseline. The canonical source directory can be identical; merely nested source directories are not allowed. Symbolic-link aliases resolving to the same source use the same canonical path and lock.
- Sync remains **bidirectional**, not isolated backups. A target edit or ordinary deletion can reach the shared source and then other targets. `.gitignore` controls likewise propagate through the source; ignored-source retention still applies.
- `sync learn-ob` processes that association only. Other targets catch up when their daemon/manual passes run. `sync` processes associations in name order; conflicting target edits may need another pass to converge. Each pair keeps the newer-mtime/source-on-tie rule; this is not a single atomic multi-target transaction or global conflict election.
- Shared-source operations take an item lock followed by the same source-path lock. Separate CLI/daemon processes cannot simultaneously sync or preview the shared source. Unrelated sources remain independent. External filesystem editors are not locked.
- `remove learn-ob` unregisters only that association and keeps all directories; `learn` continues working. A successful `delete learn-ob` also removes only its target. The known partial-delete failure risk below remains; with shared sources it can affect other targets too.
- Database schema 3 preserves existing associations and baselines while permitting repeated source paths. Manifests remain schema 2. Stop the old daemon and update both programs together; see [INSTALL.md](INSTALL.md#shared-source-database-upgrade).

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

## Check and Repair

`linker sync` converges what its baselines explain and resolves competing content by modification time. Use `check` to see every remaining difference and `repair` to make both sides match one side that you choose.

### Check

```bash
linker check
linker check demo
```

The audit is read-only. It scans both directories with the same rules, control files and no-follow file access as sync, prints the number of identical files plus one `CLASS | SIDE | PATH | DETAIL` row per difference, and exits with status 1 when a blocking difference exists, so it can gate a script.

| Class | Meaning |
| --- | --- |
| `content_differs` | Both sides hold a file with different content. `DETAIL` names the newer side and whether the recorded baseline still matches one side. |
| `source_only` | The file exists only in the source. `DETAIL` states whether a normal sync would copy it to the target or delete it from the source, which happens when the target copy was removed after the last sync. |
| `target_only` | The file exists only in the target; the mirror of `source_only`. |
| `type_conflict` | One side is a file where the other side is a directory. Ordinary sync reports an error for that association until one side is changed. |
| `ignored_target_content` | Target content matched by an effective `.gitignore` rule. The source copy is kept and the next sync removes this one. |
| `unsupported_entry` | A symbolic link or another special file. It is never followed or synchronized. |

The first four classes are counted as `divergent`; `ignored_target_content` and `unsupported_entry` are counted as `advisory` because they follow the documented ignore and file-type contract. `check` never writes files or baselines, never initializes or migrates state, and requires both roots to exist.

### Repair

```bash
linker repair                  # the source is authoritative (default)
linker repair demo --prefer source
linker repair demo --prefer target
linker repair demo --prefer newest
linker repair demo --prune     # also remove paths the source does not have
linker repair demo --dry-run   # list the operations only
```

`repair` makes every diverging path match the authoritative side:

- content that both sides have is overwritten with the authoritative copy;
- a path only the source has is copied to the target, and the reverse with `--prefer target`;
- a path only the non-authoritative side has is reported and kept unless `--prune` is given;
- ignored target content and the losing side of a type conflict are removed only with `--prune`;
- `--prefer newest` reproduces ordinary sync, including propagating a deletion that the baselines recorded; the default `source` restores from the source instead;
- a file-versus-directory conflict needs an explicit `--prefer source` or `--prefer target`, because "newest" cannot compare a file with a directory.

Repaired paths update the stored baselines, so the daemon does not revert the repair on its next pass. `--dry-run` prints the operations as `planned` without touching files or state.

**`--prune` is the only option that can delete data.** It removes target content the source does not have, ignored target files and directories, and the losing side of a type conflict, always through descriptor-relative no-follow removal. Review `linker repair demo --dry-run --prune` first. Entries that are neither regular files nor directories are never touched.

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

Failure behavior: Linker unregisters the association and its baselines before touching the target, so a partial cleanup can never be read back as a deletion to propagate into the source, and the name/pair stay reserved until cleanup finishes. The failure is still reported explicitly:

```text
error: association removed; target cleanup failed at /path/to/target: ...; source kept, target may be partially removed; inspect the remainder manually
```

The command exits with status 1. The source directory is intact, the association is gone from `linker list`, and the target may keep the entries that could not be removed. Inspect and remove the remainder yourself, then run `linker add` again if the association is still wanted. A failed `delete` no longer requires stopping the daemon to protect the source.

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
