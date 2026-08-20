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

## Exclude Rules During Add

By default, Linker creates an empty ignore file and excludes nothing. It does not read `.gitignore` automatically.

Import rules from a file:

```bash
linker add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --ignore-file ~/code/demo/.linkerignore
```

Add multiple rules inline:

```bash
linker add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --exclude node_modules/ --exclude dist/
```

Combine both:

```bash
linker add ~/code/demo ~/Library/Mobile\ Documents/com~apple~CloudDocs --ignore-file ~/code/demo/.linkerignore --exclude .env
```

Rule syntax follows the same matching semantics as Git ignore files. The saved rule file is plain text: one rule per line.

Rule and manifest files are stored locally:

```text
~/Library/Application Support/Linker/rules/<name>.ignore
~/Library/Application Support/Linker/manifests/<name>.json
```

## Manage Exclude Rules

List all rules:

```bash
linker rule demo list
```

Add one exclude rule:

```bash
linker rule demo exclude tmp/
```

This immediately removes matching files from the target directory. It does not delete source files.

Delete one exclude rule and allow a path to sync again:

```bash
linker rule demo include tmp/
linker sync demo
```

If `include` does not find a matching rule, it succeeds without changing the rule list.

## Sync

The daemon normally syncs automatically after `linker add`.

Current automatic sync timing:

- file change events are debounced for 2 seconds before syncing
- a periodic reconciliation pass runs every 5 minutes

Manual sync is mainly for testing, recovery, or immediate verification:

```bash
linker sync
linker sync demo
```

Conflict behavior is latest-modified-wins. If source and target copies differ, the side with the newer modification time overwrites the older side.

## Inspect Items

```bash
linker list
linker status
linker doctor
```

`list` shows configured sync associations, item status, source path, target path, rule count, last sync time, and last error.

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
- the matching local ignore file

## Mobile Editing

Use an iCloud Drive folder as the target parent if you want mobile access:

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Then open `Notes` in iCloud Drive on iPhone or iPad. Edits made there are synced back to the source directory by the daemon.

## Common Issues

If a name is rejected, another association with the same source directory name already exists. Rename the source directory or remove the old association.

If a target path is rejected, make sure the target parent is outside the source directory.

If a file does not come back after removing a rule, run:

```bash
linker sync <name>
linker list
```
