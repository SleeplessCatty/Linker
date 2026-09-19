# Linker

Linker is a lightweight macOS directory sync tool.

Version 0.3.0 focuses on three practical needs:

1. Link a source directory to a target parent directory and keep it synced.
2. Manage ignores through in-tree `.gitignore` files.
3. Resolve changes automatically by using the latest modified file.

There is no global Linker workspace. Add a directory by passing both sides:

```bash
linker add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

Linker creates or reuses:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/Notes/
```

The source directory name, `Notes`, is the item name used by `linker sync`, `linker remove`, and `linker delete`.

Linker metadata is stored locally under:

```text
~/Library/Application Support/Linker/
├── state.sqlite
├── manifests/
├── locks/
└── backups/
```

No Linker control directory is written into the target parent directory, so iCloud Drive stays readable on mobile devices.

## Inspect and Preview

```bash
linker list                    # table: paths, status, UTC sync time and errors
linker list | less -S          # horizontal scrolling for wide tables
linker sync Notes --dry-run    # preview copies, deletions and ignore cleanup
```

Preview does not apply changes or update sync state, but a running daemon can still sync independently. See [USAGE.md](USAGE.md#preview-before-syncing) for prerequisites and safety boundaries.

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

Deferred:

- GUI
- single-file associations
- team collaboration
- complex conflict UI
- version history
- manual merge
- advanced multi-device management
- Windows/Linux support
