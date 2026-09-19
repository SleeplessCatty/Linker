# Installing Linker on macOS

Linker currently uses a simple user-level install script.

## Upgrade from Linker 0.2 to 0.3.0

The SQLite/manifest schema migrates to version 2 while retaining associations and per-file state. Before any schema change, Linker backs up metadata to `~/Library/Application Support/Linker/backups/gitignore-v2/`. Migration can resume after interruption. Old manual rules are archived only and stop applying; only confirmed Linker-owned rule snapshots are removed. New installations do not create `rules/`.

This metadata backup does **not** back up user files or old binaries. Before installing:

1. Stop the old LaunchAgent (`launchctl bootout gui/$(id -u)/com.linker.linkerd`).
2. Back up the entire Linker Application Support directory and LaunchAgent plist outside all sync trees.
3. Inventory effective `.gitignore` changes and ignored target files; back up controls to be overwritten and target files to be removed. For a read-only sync inventory, copy the database to a staging directory and use `cargo run -p linker-core --example preview -- /path/to/staging/state.sqlite`. Opening the staging DB migrates that copy; do not point the preview helper at unbacked-up live metadata.
   After schema 2 is already installed, `linker sync --dry-run` provides a direct CLI preview without migration. Keep the daemon stopped while reviewing if you need to prevent independent background changes.
4. Install from this checkout, verify all associations, logs and `linker doctor`, then run a second sync to confirm stability.
5. On upgrade failure, stop the new daemon before restoring metadata, binaries, plist, and affected user files from backup, then restart the old service.

User `.gitignore` text is never automatically rewritten. Unsupported patterns warn and are skipped; valid patterns can remove target copies that an unsupported negation previously protected. Review [USAGE.md](USAGE.md#ignore-files) before enabling the new daemon. Version 0.3.0 does not publish a GitHub Release in this change.

## Shared-source Database Upgrade

This checkout upgrades the **database to schema 3** to allow the same source directory in multiple associations. Manifest files remain schema 2. Existing names, IDs, source/target paths, errors, timestamps and file baselines are preserved. A transaction rebuilds the association table; the original database is backed up first under `~/Library/Application Support/Linker/backups/shared-source-v3/state.sqlite`. Reopening completed state does not repeat or overwrite that backup. Failed database transactions can be retried.

Before enabling this change, stop the old daemon, back up all application state and both binaries outside sync trees, then install **both** `linker` and `linkerd` using the update procedure below. Do not run a new CLI against an old daemon: older daemons do not take shared-source locks. Do not downgrade only the binaries once multiple targets exist; stop the new service and restore the complete prior state/binary/plist backup if rollback is required. The automatic database backup does not contain manifests, binaries or user data.

After installation, verify existing records and `doctor`; add each additional target with a unique `--name` without removing the first record. New targets still must be empty/missing. On schema-2 state, new-code `sync --dry-run` remains read-only and does not trigger this migration; ordinary state-opening commands perform it. Build/tests alone do not update the installed programs or migrate live state.

## Exact-target and optional-name CLI update

New adds use `linker add <source-directory> <target-directory> [--name <name>]`. Update scripts that passed a parent directory: include the final desired directory name. Nonempty destinations now fail; an empty old parent would become the exact destination. Existing schema-2 associations keep their stored names, paths and baselines; do not remove/re-add them for this update. That original CLI-only change required no schema migration; the subsequent shared-source feature requires the database upgrade above. No GitHub Release is published. Installation is a separate explicit operation from building/testing the checkout.

## Historical Pre-0.2 Clean Cutover

Installing Linker 0.2 performs a clean cutover from any pre-0.2 installation:

- stops and removes the legacy daemon registration
- removes legacy command links only when they point into the legacy managed binary directory
- deletes the legacy Application Support state, including associations, manifests, rules, logs, and cached binaries
- never deletes configured source directories or target directories

Legacy associations are not migrated. New `add` accepts only missing or empty exact target directories; a retained nonempty target cannot be reattached through it. Preserve and reconcile existing data before choosing a new empty destination. The installer also refuses to overwrite unrelated files or links already named `linker` or `linkerd`.

Cleanup is fail-closed. It validates physical paths, rejects symlink ancestry and unrecognized legacy content, and uses the legacy database only to veto deletion when a recorded source or target is inside the legacy state root. It never deletes paths obtained from that database. The old state is removed only after the new daemon starts successfully; otherwise it remains available for recovery.

## Install With Script

From a local checkout:

```bash
./scripts/install.sh
```

The script:

- builds release binaries
- installs `linker` and `linkerd` under `~/Library/Application Support/Linker/bin/`
- creates `/usr/local/bin/linker` and `/usr/local/bin/linkerd` command links by default
- creates `~/Library/LaunchAgents/com.linker.linkerd.plist`
- starts the daemon with `launchctl`

Linker does not need a global workspace initialization step. Add each directory by passing a source directory and a exact target directory.

Creating command links under `/usr/local/bin` may ask for your administrator password because that directory is usually owned by `root`.

`sudo` is used only for the fixed `/usr/local/bin` location. A custom `--link-dir` must already exist and be writable by the current user.

If you do not want command links:

```bash
./scripts/install.sh --no-link
```

If you skip command links, make `linker` available in every terminal by adding this to your shell profile:

```bash
export PATH="$PATH:$HOME/Library/Application Support/Linker/bin"
```

Add that line to your shell profile if you want it to persist.

## Update an Existing Script Installation

The script installation consists of two real binaries under `~/Library/Application Support/Linker/bin/`, command symlinks under `/usr/local/bin/`, and the user LaunchAgent `~/Library/LaunchAgents/com.linker.linkerd.plist`. It is not a Homebrew service. LaunchAgent label `com.linker.linkerd` starts the daemon at login and keeps it running; logs and SQLite state remain under Application Support.

For an installation with working command links, update from the intended local checkout:

1. Run the tests and `cargo build --release` before stopping the working service.
2. Stop the loaded service with `launchctl bootout gui/$(id -u)/com.linker.linkerd`. Confirm it stopped; investigate failures rather than ignoring them.
3. Back up the Application Support directory and LaunchAgent plist outside the sync trees. For already-upgraded state, run `linker sync --dry-run` while stopped and back up any files that the next pass could overwrite/delete.
4. Run `./scripts/install.sh --no-link`. This rebuilds and replaces **both** `linker` and `linkerd`, writes the plist and restarts the service. Existing `/usr/local/bin` symlinks continue to work; `--no-link` does not remove them and no new PATH entry is needed.
5. Verify `linker --version`, `linker add --help` (exact target and `--name`), `linker list`, `linker doctor`, `linker sync --dry-run`, and `launchctl print gui/$(id -u)/com.linker.linkerd`. The version remains 0.3.0 for this checkout update, so the version string alone does not identify the new CLI behavior.

This preserves existing associations and file state, migrating the database as described above. Keep the backup until verification is complete. If installation fails, stop any newly started daemon before restoring the saved binaries/state/plist and restarting the previous service. Updating the binaries is separate from committing or pushing repository changes; it does not create a GitHub Release.

## Remote Script Install

After the GitHub repository is published, install directly from the remote script:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash
```

Forward installer options with `bash -s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash -s -- --no-link
```

Install a specific branch, tag, or commit:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | LINKER_REF=main bash
```

The remote installer requires `git` and Rust/Cargo on the target Mac. It fetches the selected branch, tag, or commit into a temporary directory, then runs `scripts/install.sh`. A legacy database cleanup also requires the macOS `sqlite3` command so the installer can fail closed around recorded source and target paths.

## Link Into /usr/local/bin

`./scripts/install.sh` now creates these links by default:

```text
/usr/local/bin/linker  -> ~/Library/Application Support/Linker/bin/linker
/usr/local/bin/linkerd -> ~/Library/Application Support/Linker/bin/linkerd
```

You can choose another link directory:

```bash
mkdir -p "$HOME/.local/bin"
./scripts/install.sh --link-dir "$HOME/.local/bin"
```

If you installed without links, rerun the guarded installer when you want them:

```bash
./scripts/install.sh
```

Verify:

```bash
which linker
linker --help
```

To remove those links:

```bash
./scripts/uninstall.sh
```

The script removes a command link only when its exact target is Linker's managed binary. It leaves unrelated files and links untouched.

## Homebrew Formula

A starter formula is available at:

```text
packaging/homebrew/linker.rb
```

To publish it through Homebrew, create a tap repository such as:

```text
github.com/<user>/homebrew-linker
```

Put the formula at:

```text
Formula/linker.rb
```

Then users can install it with:

```bash
brew tap SleeplessCatty/linker
brew install --HEAD linker
```

The checked-in formula is intentionally `HEAD`-only until a stable GitHub release is published.

After publishing a stable GitHub release, add `url` and `sha256` to the formula. Calculate the release tarball SHA256 with:

```bash
curl -L "https://github.com/<user>/Linker/archive/refs/tags/<published-tag>.tar.gz" | shasum -a 256
```

The formula installs binaries and a LaunchAgent template only. It does not manage user LaunchAgents or perform the incompatible pre-0.2 cleanup, so it is a fresh-install path, not an upgrade path. Existing pre-0.2 users must run the guarded script installer first.

After the release exists, add its `url` and `sha256` before documenting plain `brew install linker` as supported.

## Verify

```bash
linker list
linker status
linker doctor
launchctl print gui/$(id -u)/com.linker.linkerd
```

Daemon logs:

```text
~/Library/Application Support/Linker/logs/linkerd.out.log
~/Library/Application Support/Linker/logs/linkerd.err.log
```

## Uninstall

```bash
./scripts/uninstall.sh
```

The uninstall script stops the LaunchAgent and removes installed binaries. It keeps local Linker state and logs by default.
