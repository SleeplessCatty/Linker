# Installing Linker on macOS

Linker currently uses a simple user-level install script.

## Incompatible Upgrade to 0.2

Installing Linker 0.2 performs a clean cutover from any pre-0.2 installation:

- stops and removes the legacy daemon registration
- removes legacy command links only when they point into the legacy managed binary directory
- deletes the legacy Application Support state, including associations, manifests, rules, logs, and cached binaries
- never deletes configured source directories or target directories

Legacy associations are not migrated. Add them again with `linker add` after installation. The installer also refuses to overwrite unrelated files or links already named `linker` or `linkerd`.

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

Linker does not need a global workspace initialization step. Add each directory by passing a source directory and a target parent directory.

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
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | LINKER_REF=v0.2.0 bash
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

The checked-in formula is intentionally `HEAD`-only until the `v0.2.0` GitHub release exists.

After publishing a stable GitHub release, add `url` and `sha256` to the formula. Calculate the release tarball SHA256 with:

```bash
curl -L https://github.com/<user>/Linker/archive/refs/tags/v0.2.0.tar.gz | shasum -a 256
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
