# QuickSync Product Design

## Product Positioning

QuickSync is a lightweight personal sync tool for macOS.

It lets a user add an existing local file or folder, mirror it into iCloud Drive with a readable name, and keep future changes synced automatically. The first version should feel close to native iCloud behavior: after setup, the user should not need to manage sync manually.

## What Matters Most

MVP priorities:

1. Add a file or folder to QuickSync.
2. Keep it automatically synced in the background.
3. Let the user define exclude rules.
4. Use latest-modified-wins to resolve competing changes automatically.

Everything else is secondary unless required to make those flows reliable.

## Core Model

The user's original path remains the real working path:

```text
~/code/demo
~/Documents/todo.md
```

QuickSync mirrors eligible content into a readable iCloud workspace:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/QuickSync/
├── demo/
├── todo.md
└── .quicksync/
    ├── manifests/demo.json
    ├── manifests/todo.md.json
    ├── rules/demo.ignore
    └── rules/todo.md.ignore
```

There are two real copies:

```text
Local Path  <->  iCloud Visible Copy
```

QuickSync does not move the user's path into iCloud and does not use symlinks or filesystem mounts.

## MVP User Flows

### Add Folder

```bash
qsync add ~/code/demo
qsync add ~/code/demo --name WorkDemo
```

Result:

- creates a QuickSync item
- creates `QuickSync/<name>/`
- creates hidden manifest and rule files under `.quicksync/`
- performs initial sync
- daemon keeps future changes synced

### Add File

```bash
qsync add ~/Documents/todo.md
```

Result:

- creates `QuickSync/todo.md`
- syncs the file directly, without a UUID folder

### Customize Exclude Rules

Default rule behavior is empty: QuickSync excludes nothing unless the user explicitly provides rules.

```bash
qsync add ~/code/demo --ignore-file ~/code/demo/.qsyncignore
qsync add ~/code/demo --exclude node_modules/ --exclude dist/
qsync rule demo exclude tmp/
qsync rule demo list
qsync rule demo include tmp/
```

When a new exclude rule is added, matching cloud-visible files are removed from iCloud, while local originals are kept.

Rules use the same matching semantics as Git ignore files. A directory can have multiple rules, either imported at add time with `--ignore-file` or passed inline with repeated `--exclude` arguments. The stored rule file is plain text: one rule per line.

### Automatic Sync

After `qsync add`, the daemon watches both:

```text
local file/folder
iCloud visible file/folder
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

This is intentionally simple and iCloud-like. MVP does not show a conflict review UI and does not attempt text merge.

### Remove and Delete

```bash
qsync remove demo
```

Stops syncing the item and removes local QuickSync state. It keeps the user's local files, the visible iCloud copy, and hidden cloud metadata.

```bash
qsync delete demo
```

Stops syncing the item and deletes the visible iCloud copy plus hidden manifest/rule files. It never deletes the user's original local file or folder.

## MVP Command Set

```text
qsync add <path> [--name <name>] [--ignore-file <path>] [--exclude <pattern>]...
qsync rule <name> list
qsync rule <name> exclude <pattern>
qsync rule <name> include <pattern>
qsync remove <name>
qsync delete <name>
qsync list
qsync status [name]
qsync sync [name]
qsync doctor
```

`qsync sync` is included mainly for testing and recovery. Normal use should rely on automatic sync.

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
qsync add ~/code/demo
```

Then:

- keep working in `~/code/demo`
- see files directly under `iCloud Drive/QuickSync/demo/`
- edit cloud files from iPhone or iPad and have changes sync back
- customize rules with `qsync rule`
- rely on latest-modified-wins without managing conflicts manually
