# QuickSync

QuickSync is a lightweight macOS sync tool built on top of iCloud Drive.

The first version focuses on three practical needs:

1. Add a local file or folder to QuickSync and keep it automatically synced.
2. Customize exclude rules for each synced item.
3. Resolve changes automatically by using the latest modified file, similar to the normal iCloud experience.

The iCloud layout is designed to be readable on mobile devices:

```text
iCloud Drive/QuickSync/
├── <synced file or folder>
└── .quicksync/
    ├── manifests/
    └── rules/
```

## Documents

- [PRODUCT.md](PRODUCT.md): simplified product scope and user flows.
- [SPEC.md](SPEC.md): MVP technical design for the CLI, daemon, rule engine, and sync engine.
- [INSTALL.md](INSTALL.md): macOS user-level install, LaunchAgent setup, and uninstall.
- [USAGE.md](USAGE.md): detailed command examples and remove/delete behavior.
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

This installs `qsync` and `qsyncd` under:

```text
~/Library/Application Support/QuickSync/bin/
```

It also creates `/usr/local/bin/qsync`, `/usr/local/bin/qsyncd`, and starts a user LaunchAgent for `qsyncd`.

Use `./scripts/install.sh --no-link` if you do not want `/usr/local/bin` command links.

Homebrew packaging notes and a starter formula are in [INSTALL.md](INSTALL.md).

After the repository is published, remote install will be:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

## MVP Scope

Must have:

- macOS only
- CLI plus LaunchAgent daemon
- add local file or folder
- automatic two-way mirror sync through iCloud Drive
- custom exclude rules
- empty default ignore; only explicit rules are applied
- latest-modified-wins conflict behavior
- basic status/list commands

Deferred:

- GUI
- team collaboration
- complex conflict UI
- version history
- manual merge
- advanced multi-device management
- Windows/Linux support
