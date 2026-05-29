# QuickSync Command Usage

QuickSync keeps selected local files or folders mirrored through iCloud Drive.
The cloud workspace is intentionally readable on iPhone and iPad:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/QuickSync/
├── <your synced file or folder>
└── .quicksync/
    ├── manifests/
    └── rules/
```

The `.quicksync` directory is internal metadata. Normal viewing and editing should happen in the visible files and folders directly under `QuickSync/`.

## Install

```bash
./scripts/install.sh
```

Add the installed binaries to your shell path if needed:

```bash
export PATH="$PATH:$HOME/Library/Application Support/QuickSync/bin"
```

Verify:

```bash
qsync doctor
qsync status
```

The install script creates `/usr/local/bin/qsync` and `/usr/local/bin/qsyncd` by default. This may ask for your administrator password.

To install without command links:

```bash
./scripts/install.sh --no-link
```

Homebrew packaging notes are in [INSTALL.md](INSTALL.md).

After the GitHub repository is published, remote install will be:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

## Add a Folder

```bash
qsync add ~/Documents/Notes
```

This creates:

```text
iCloud Drive/QuickSync/Notes/
iCloud Drive/QuickSync/.quicksync/manifests/Notes.json
iCloud Drive/QuickSync/.quicksync/rules/Notes.ignore
```

Use a different visible iCloud name:

```bash
qsync add ~/Documents/Notes --name WorkNotes
```

Names must be unique. QuickSync will reject a second file or folder with the same cloud name.

## Add a Single File

```bash
qsync add ~/Documents/todo.md
```

This syncs directly to:

```text
iCloud Drive/QuickSync/todo.md
```

## Exclude Rules During Add

By default, QuickSync creates an empty ignore file and excludes nothing. It does not read `.gitignore` automatically.

Import rules from a file:

```bash
qsync add ~/code/demo --ignore-file ~/code/demo/.qsyncignore
```

Add multiple rules inline:

```bash
qsync add ~/code/demo --exclude node_modules/ --exclude dist/
```

Combine both:

```bash
qsync add ~/code/demo --ignore-file ~/code/demo/.qsyncignore --exclude .env
```

Rule syntax follows the same matching semantics as Git ignore files. The saved rule file is plain text: one rule per line.

## Manage Exclude Rules

List all rules, one pattern per line:

```bash
qsync rule demo list
```

Add one exclude rule:

```bash
qsync rule demo exclude tmp/
```

This immediately removes matching files from the visible iCloud mirror. It does not delete the original local files.

Delete one exclude rule and allow a path to sync again:

```bash
qsync rule demo include tmp/
qsync sync demo
```

The next sync uploads the matching local content back to iCloud.

If `include` does not find a matching rule, it succeeds without changing the rule list.

## Sync

The daemon normally syncs automatically after `qsync add`.

Current automatic sync timing:

- file change events are debounced for 2 seconds before syncing
- a periodic reconciliation pass runs every 5 minutes

Manual sync is mainly for testing, recovery, or immediate verification:

```bash
qsync sync
qsync sync demo
```

Conflict behavior is latest-modified-wins. If local and cloud copies differ, the side with the newer modification time overwrites the older side.

## Inspect Items

```bash
qsync list
qsync status
qsync status demo
qsync doctor
```

`status` shows daemon health, item paths, item type, rule count, last sync time, and last error.

## Remove vs Delete

Remove only the local QuickSync association:

```bash
qsync remove demo
```

This keeps:

- the original local file or folder
- the visible iCloud copy
- hidden `.quicksync` metadata

Delete the QuickSync association and cloud data:

```bash
qsync delete demo
```

This keeps the original local file or folder, but deletes:

- the visible iCloud file or folder
- the matching manifest
- the matching ignore file

## Mobile Editing

Open iCloud Drive, then `QuickSync`, then the synced file or folder name. Edits made there are synced back to the original Mac path by the daemon.

Avoid editing files inside `.quicksync`; that directory is only for QuickSync metadata.

## Common Issues

If iCloud Drive is not found:

```bash
qsync doctor
```

If a name is rejected, choose a simple file or folder name with `--name`. Path separators are not allowed, and `.quicksync` is reserved.

If a file does not come back after removing a rule, run:

```bash
qsync sync <name>
qsync status <name>
```

If a previous experiment created `QuickSync/Items/`, remove it manually after confirming you no longer need it. This version does not migrate the old UUID-based structure.
