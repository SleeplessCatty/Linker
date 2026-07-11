# QuickSync Command Usage

QuickSync links a source directory to a target parent directory.

```bash
qs add <source-directory> <target-parent-directory>
```

If the source directory is `~/Documents/Notes`, the item name is `Notes`, and the target directory becomes:

```text
<target-parent-directory>/Notes/
```

There is no `qs init` step and no global QuickSync workspace.

## Install

```bash
./scripts/install.sh
```

Verify:

```bash
qs doctor
qs status
```

The install script creates `/usr/local/bin/qs` and `/usr/local/bin/qsd` by default. This may ask for your administrator password.

To install without command links:

```bash
./scripts/install.sh --no-link
```

Remote install:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

## Add a Directory

```bash
qs add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

This creates or reuses:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/Notes/
```

The item name is always the source directory name. Names must be unique; QuickSync rejects a second association named `Notes`.

The source and target directories must be separate. QuickSync rejects associations where one side contains the other.

## Exclude Rules During Add

By default, QuickSync creates an empty ignore file and excludes nothing. It does not read `.gitignore` automatically.

Import rules from a file:

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --ignore-file ~/code/demo/.qsyncignore
```

Add multiple rules inline:

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --exclude node_modules/ --exclude dist/
```

Combine both:

```bash
qs add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --ignore-file ~/code/demo/.qsyncignore --exclude .env
```

Rule syntax follows the same matching semantics as Git ignore files. The saved rule file is plain text: one rule per line.

Rule and manifest files are stored locally:

```text
~/Library/Application Support/QuickSync/rules/<name>.ignore
~/Library/Application Support/QuickSync/manifests/<name>.json
```

## Manage Exclude Rules

List all rules:

```bash
qs rule demo list
```

Add one exclude rule:

```bash
qs rule demo exclude tmp/
```

This immediately removes matching files from the target directory. It does not delete source files.

Delete one exclude rule and allow a path to sync again:

```bash
qs rule demo include tmp/
qs sync demo
```

If `include` does not find a matching rule, it succeeds without changing the rule list.

## Sync

The daemon normally syncs automatically after `qs add`.

Current automatic sync timing:

- file change events are debounced for 2 seconds before syncing
- a periodic reconciliation pass runs every 5 minutes

Manual sync is mainly for testing, recovery, or immediate verification:

```bash
qs sync
qs sync demo
```

Conflict behavior is latest-modified-wins. If source and target copies differ, the side with the newer modification time overwrites the older side.

## Inspect Items

```bash
qs list
qs status
qs doctor
```

`list` shows configured sync associations, item status, source path, target path, rule count, last sync time, and last error.

`status` only shows background daemon health.

## Remove vs Delete

Remove only the QuickSync association:

```bash
qs remove demo
```

This keeps:

- the source directory
- the target directory

It deletes local QuickSync metadata for that association.

Delete the QuickSync association and target directory:

```bash
qs delete demo
```

This keeps the source directory, but deletes:

- the target directory
- the matching local manifest
- the matching local ignore file

## Mobile Editing

Use an iCloud Drive folder as the target parent if you want mobile access:

```bash
qs add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Then open `Notes` in iCloud Drive on iPhone or iPad. Edits made there are synced back to the source directory by the daemon.

## Common Issues

If a name is rejected, another association with the same source directory name already exists. Rename the source directory or remove the old association.

If a target path is rejected, make sure the target parent is outside the source directory.

If a file does not come back after removing a rule, run:

```bash
qs sync <name>
qs list
```
