# QuickSync

QuickSync is a lightweight macOS directory sync tool.

The first version focuses on three practical needs:

1. Link a source directory to a target parent directory and keep it synced.
2. Customize exclude rules for each linked directory.
3. Resolve changes automatically by using the latest modified file.

There is no global QuickSync workspace. Add a directory by passing both sides:

```bash
qs add ~/Documents/Notes ~/Library/Mobile\ Documents/com~apple~CloudDocs
```

QuickSync creates or reuses:

```text
~/Library/Mobile Documents/com~apple~CloudDocs/Notes/
```

The source directory name, `Notes`, is the item name used by `qs sync`, `qs rule`, `qs remove`, and `qs delete`.

QuickSync metadata is stored locally under:

```text
~/Library/Application Support/QuickSync/
├── state.sqlite
├── manifests/
└── rules/
```

No QuickSync control directory is written into the target parent directory, so iCloud Drive stays readable on mobile devices.

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
cargo run -p qsync-cli -- --help
cargo run -p qsync-daemon -- --help
```

## Install

From a local checkout:

```bash
./scripts/install.sh
```

This installs `qs` and `qsd` under:

```text
~/Library/Application Support/QuickSync/bin/
```

It also creates `/usr/local/bin/qs`, `/usr/local/bin/qsd`, and starts a user LaunchAgent for `qsd`.

Use `./scripts/install.sh --no-link` if you do not want `/usr/local/bin` command links.

Remote install:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

## MVP Scope

Must have:

- macOS only
- CLI plus LaunchAgent daemon
- source directory to target directory association
- automatic two-way sync
- custom exclude rules
- empty default ignore; only explicit rules are applied
- latest-modified-wins conflict behavior
- basic status/list commands

Deferred:

- GUI
- single-file associations
- team collaboration
- complex conflict UI
- version history
- manual merge
- advanced multi-device management
- Windows/Linux support
