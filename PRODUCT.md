# Linker Product Design

## Product Positioning

Linker is a lightweight personal directory sync tool for macOS.

It lets a user link an existing source directory to a target parent directory, then keeps the resulting target directory synced automatically. If the target parent is inside iCloud Drive, the synced directory is easy to view and edit on iPhone or iPad.

## What Matters Most

MVP priorities:

1. Add a source directory and target parent directory.
2. Keep the source and target directories automatically synced in the background.
3. Let the user manage ignores through `.gitignore`.
4. Use latest-modified-wins to resolve competing changes automatically.

Everything else is secondary unless required to make those flows reliable.

## Core Model

The user adds an association:

```bash
linker add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Linker derives:

```text
source directory: ~/code/demo
target directory: ~/Library/Mobile Documents/com~apple~CloudDocs/demo
item name: demo
```

There are two real directories:

```text
Source Directory  <->  Target Directory
```

Linker does not move the user's source directory and does not use symlinks or filesystem mounts.

## MVP User Flows

### Add Directory

```bash
linker add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Result:

- creates a Linker item named `demo`
- creates or reuses the target directory
- creates a local schema-2 manifest under Application Support
- performs initial sync
- daemon keeps future changes synced

### Manage Ignore Files

Edit `.gitignore` in the source or target tree. Linker supports names, directory patterns, relative paths, and single-star wildcards only; advanced syntax is warned and skipped, not interpreted as Git-compatible matching. Nested rules accumulate without negation. See [USAGE.md](USAGE.md#ignore-files) for the exact subset.

Matching source files remain unread and untouched; matching target copies, including target-only files, are deleted. Active `.gitignore` controls sync before effective rules are applied. Removing a rule restores normal sync without reverse-deleting the source. There are no manual rule commands, rule snapshots, or rule counts in current state.

### Automatic Sync

`linker add` performs initial sync immediately. The running daemon loads new associations on its next 5-minute reconciliation, then watches both:

```text
source directory
target directory
```

When a change is detected:

```text
change detected
  -> short debounce
  -> scan changed item
  -> resolve .gitignore controls and apply cumulative rules
  -> copy latest modified side
  -> update state
```

The user should normally not need to run manual sync.

### Inspect and Preview

`linker list` presents one table with complete source/target paths, status, last successful sync time in UTC, and errors. `linker sync [name] --dry-run` previews the same control and data operations without applying them or changing sync state. It does not pause the daemon or reserve the plan for later execution.

### Latest Modified Wins

If both sides differ, Linker chooses the file with the newest modification time and copies it over the older side.

This is intentionally simple. MVP does not show a conflict review UI and does not attempt text merge.

### Remove and Delete

```bash
linker remove demo
```

Stops syncing the item and removes local Linker association metadata. It keeps the source directory and target directory.

```bash
linker delete demo
```

Stops syncing the item, removes local Linker association metadata, and deletes the target directory. It never deletes the source directory.

## MVP Command Set

```text
linker add <source-directory> <target-parent-directory>
linker remove <name>
linker delete <name>
linker list
linker status
linker sync [name]
linker doctor
```

`linker sync` is included mainly for testing and recovery. Normal use should rely on automatic sync.

## Non-Goals for MVP

Linker is not trying to be:

- a Git replacement
- a team collaboration tool
- a backup/version-history product
- a custom cloud drive
- a file system mount
- a merge/conflict review tool

## MVP Success Criteria

A user can add a folder:

```bash
linker add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Then:

- keep working in `~/code/demo`
- see files directly in the target directory
- edit target files from iPhone or iPad if the target parent is iCloud Drive
- manage the supported basic patterns through `.gitignore`
- rely on latest-modified-wins without managing conflicts manually
