# Linker

Linker is a lightweight macOS directory sync tool.

Version 0.3.0 focuses on three practical needs:

1. Link a source directory to a exact target directory and keep it synced.
2. Manage ignores through in-tree `.gitignore` files.
3. Resolve changes automatically by using the latest modified file.

There is no global Linker workspace. Add a directory by passing both sides:

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs/Notes
```

The second argument is the exact destination, not its parent. Linker creates a missing destination or accepts an existing **empty** directory:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/Notes/
```

The record name defaults to the source directory name (`Notes`). Use `--name` for sources sharing a basename; the record name does not change either path:

```bash
linker add ~/work/Notes ~/Cloud/WorkNotes --name work-notes
linker add ~/personal/Notes ~/Cloud/PersonalNotes --name personal-notes
```

The same source can also have multiple targets, each with a unique record name:

```bash
linker add ~/learn ~/Documents/learn --name learn-local
linker add ~/learn "~/Library/Mobile Documents/iCloud~md~obsidian/Documents/learn" --name learn-ob
```

These are bidirectional associations: target edits and deletions can travel through the shared source to other targets. Shared-source operations are serialized; convergence can require another pass. See [multi-target behavior](USAGE.md#one-source-multiple-targets).

Existing nonempty destinations fail without merging or deleting their contents, including hidden files or empty subdirectories. See [add safety and failure behavior](USAGE.md#add-a-directory). Existing associations keep their stored paths and names.

Linker metadata is stored locally under:

```text
~/Library/Application Support/Linker/
├── state.sqlite
├── manifests/
├── locks/
└── backups/
```

No Linker control directory is written into the exact target directory, so iCloud Drive stays readable on mobile devices.

## Inspect and Preview

```bash
linker list                    # table: paths, status, UTC sync time and errors
linker list | less -S          # horizontal scrolling for wide tables
linker sync Notes --dry-run    # preview copies, deletions and ignore cleanup
linker check                   # read-only audit; exit status 1 when divergent
linker repair Notes            # make the target match the source (adds and overwrites)
linker repair Notes --prune    # also remove target-only and ignored target content
```

Preview does not apply changes or update sync state, but a running daemon can still sync independently. See [USAGE.md](USAGE.md#preview-before-syncing) for prerequisites and safety boundaries.

`linker check` is a read-only audit of both sides of every association: it reports content differences, one-sided paths (including whether a normal sync would restore or delete them), file-versus-directory conflicts, ignored target content and unsupported entries. `linker repair` then makes every divergence match one authoritative side, the source by default, and updates the stored baselines so the daemon does not revert it. Deletions stay opt-in behind `--prune`. See [check and repair](USAGE.md#check-and-repair).

## Documents

- [PRODUCT.md](PRODUCT.md): product scope and user flows.
- [SPEC.md](SPEC.md): technical design for the CLI, daemon, rule engine, and sync engine.
- [INSTALL.md](INSTALL.md): macOS user-level install, LaunchAgent setup, Homebrew, and uninstall.
- [USAGE.md](USAGE.md): command examples and remove/delete behavior.
- [docs/PROJECT_AUDIT.md](docs/PROJECT_AUDIT.md): code structure, file descriptions, and known risks.
- [docs/REPOSITORY_FILES.md](docs/REPOSITORY_FILES.md): what belongs in Git and what stays local.
- [docs/GITHUB_RELEASE.md](docs/GITHUB_RELEASE.md): GitHub, remote install, and Homebrew release checklist.

## Development

```bash
cargo test
cargo run -p linker-cli -- --help
cargo run -p linker-daemon -- --help
```

## Install

From a local checkout:

```bash
./scripts/install.sh
```

This installs `linker` and `linkerd` under:

```text
~/Library/Application Support/Linker/bin/
```

It also creates `/usr/local/bin/linker`, `/usr/local/bin/linkerd`, and starts a user LaunchAgent for `linkerd`.

Use `./scripts/install.sh --no-link` if you do not want `/usr/local/bin` command links.

Remote install:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash
```

Linker 0.3.0 migrates existing Linker associations and file state, archives retired manual rules, and uses only `.gitignore`. Stop the old daemon and back up state and affected target files before upgrading; newly ignored targets will be deleted. See [INSTALL.md](INSTALL.md). This update does not publish a GitHub Release.

## MVP Scope

Must have:

- macOS only
- CLI plus LaunchAgent daemon
- source directory to target directory association
- automatic two-way sync
- simplified `.gitignore` rules (names, directories, relative paths, single `*`)
- nested additive rules; ignored source files retained and target copies removed
- invalid advanced patterns warned and skipped; not full Git compatibility
- latest-modified-wins conflict behavior
- table-formatted association list and daemon health checks
- dry-run preview for existing associations
- read-only consistency audit and manual repair with an explicit authoritative side

Deferred:

- GUI
- single-file associations
- team collaboration
- complex conflict UI
- version history
- manual merge
- advanced multi-device management
- Windows/Linux support
