# QuickSync Product Design

## Product Positioning

QuickSync is a lightweight personal directory sync tool for macOS.

It lets a user link an existing source directory to a target parent directory, then keeps the resulting target directory synced automatically. If the target parent is inside iCloud Drive, the synced directory is easy to view and edit on iPhone or iPad.

## What Matters Most

MVP priorities:

1. Add a source directory and target parent directory.
2. Keep the source and target directories automatically synced in the background.
3. Let the user define exclude rules.
4. Use latest-modified-wins to resolve competing changes automatically.

Everything else is secondary unless required to make those flows reliable.

## Core Model

The user adds an association:

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

QuickSync derives:

```text
source directory: ~/code/demo
target directory: ~/Library/Mobile Documents/com~apple~CloudDocs/demo
item name: demo
```

There are two real directories:

```text
Source Directory  <->  Target Directory
```

QuickSync does not move the user's source directory and does not use symlinks or filesystem mounts.

## MVP User Flows

### Add Directory

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Result:

- creates a QuickSync item named `demo`
- creates or reuses the target directory
- creates local manifest and rule files under Application Support
- performs initial sync
- daemon keeps future changes synced

### Customize Exclude Rules

Default rule behavior is empty: QuickSync excludes nothing unless the user explicitly provides rules.

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --ignore-file ~/code/demo/.qsyncignore
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --exclude node_modules/ --exclude dist/
qs rule demo exclude tmp/
qs rule demo list
qs rule demo include tmp/
```

When a new exclude rule is added, matching target files are removed, while source files are kept.

Rules use the same matching semantics as Git ignore files. A directory can have multiple rules, either imported at add time with `--ignore-file` or passed inline with repeated `--exclude` arguments. The stored rule file is plain text: one rule per line.

### Automatic Sync

After `qs add`, the daemon watches both:

```text
source directory
target directory
```

When a change is detected:

```text
change detected
  -> short debounce
  -> scan changed item
  -> apply exclude rules
  -> copy latest modified side
  -> update state
```

The user should normally not need to run manual sync.

### Latest Modified Wins

If both sides differ, QuickSync chooses the file with the newest modification time and copies it over the older side.

This is intentionally simple. MVP does not show a conflict review UI and does not attempt text merge.

### Remove and Delete

```bash
qs remove demo
```

Stops syncing the item and removes local QuickSync association metadata. It keeps the source directory and target directory.

```bash
qs delete demo
```

Stops syncing the item, removes local QuickSync association metadata, and deletes the target directory. It never deletes the source directory.

## MVP Command Set

```text
qs add <source-directory> <target-parent-directory> [--ignore-file <path>] [--exclude <pattern>]...
qs rule <name> list
qs rule <name> exclude <pattern>
qs rule <name> include <pattern>
qs remove <name>
qs delete <name>
qs list
qs status
qs sync [name]
qs doctor
```

`qs sync` is included mainly for testing and recovery. Normal use should rely on automatic sync.

## Non-Goals for MVP

QuickSync is not trying to be:

- a Git replacement
- a team collaboration tool
- a backup/version-history product
- a custom cloud drive
- a file system mount
- a merge/conflict review tool

## MVP Success Criteria

A user can add a folder:

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Then:

- keep working in `~/code/demo`
- see files directly in the target directory
- edit target files from iPhone or iPad if the target parent is iCloud Drive
- customize rules with `qs rule`
- rely on latest-modified-wins without managing conflicts manually
