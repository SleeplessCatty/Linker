# Installing QuickSync on macOS

QuickSync currently uses a simple user-level install script.

## Install With Script

From a local checkout:

```bash
./scripts/install.sh
```

The script:

- builds release binaries
- installs `qsync` and `qsyncd` under `~/Library/Application Support/QuickSync/bin/`
- creates `/usr/local/bin/qsync` and `/usr/local/bin/qsyncd` command links by default
- creates `~/Library/LaunchAgents/com.quicksync.qsyncd.plist`
- starts the daemon with `launchctl`

Creating command links under `/usr/local/bin` may ask for your administrator password because that directory is usually owned by `root`.

If you do not want command links:

```bash
./scripts/install.sh --no-link
```

If you skip command links, make `qsync` available in every terminal by adding this to your shell profile:

```bash
export PATH="$PATH:$HOME/Library/Application Support/QuickSync/bin"
```

Add that line to your shell profile if you want it to persist.

## Remote Script Install

After the GitHub repository is published, install directly from the remote script:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

Forward installer options with `bash -s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash -s -- --no-link
```

Install a specific branch, tag, or commit:

```bash
QUICKSYNC_REF=v0.1.0 curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

The remote installer requires `git` and Rust/Cargo on the target Mac. It clones the repository into a temporary directory, then runs `scripts/install.sh`.

## Link Into /usr/local/bin

`./scripts/install.sh` now creates these links by default:

```text
/usr/local/bin/qsync  -> ~/Library/Application Support/QuickSync/bin/qsync
/usr/local/bin/qsyncd -> ~/Library/Application Support/QuickSync/bin/qsyncd
```

You can choose another link directory:

```bash
./scripts/install.sh --link-dir /usr/local/bin
```

If you installed without links, create them manually:

```bash
sudo ln -sf "$HOME/Library/Application Support/QuickSync/bin/qsync" /usr/local/bin/qsync
sudo ln -sf "$HOME/Library/Application Support/QuickSync/bin/qsyncd" /usr/local/bin/qsyncd
```

Verify:

```bash
which qsync
qsync --help
```

To remove those links:

```bash
./scripts/uninstall.sh
```

or manually:

```bash
sudo rm -f /usr/local/bin/qsync /usr/local/bin/qsyncd
```

## Homebrew Formula

A starter formula is available at:

```text
packaging/homebrew/quicksync.rb
```

To publish it through Homebrew, create a tap repository such as:

```text
github.com/<user>/homebrew-quicksync
```

Put the formula at:

```text
Formula/quicksync.rb
```

Then users can install it with:

```bash
brew tap SleeplessCatty/quicksync
brew install quicksync
```

The current formula is a `HEAD` formula, so install with:

```bash
brew install --HEAD quicksync
```

After publishing a stable GitHub release, add `url` and `sha256` to the formula. Calculate the release tarball SHA256 with:

```bash
curl -L https://github.com/<user>/QuickSync/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
```

The formula installs the binaries. The LaunchAgent still needs a small post-install setup because it contains user-specific paths under `~/Library/Application Support/QuickSync`.

Before the first stable release, source install uses:

```bash
brew install --HEAD quicksync
```

## Verify

```bash
qsync list
qsync status
launchctl print gui/$(id -u)/com.quicksync.qsyncd
```

Daemon logs:

```text
~/Library/Application Support/QuickSync/logs/qsyncd.out.log
~/Library/Application Support/QuickSync/logs/qsyncd.err.log
```

## Uninstall

```bash
./scripts/uninstall.sh
```

The uninstall script stops the LaunchAgent and removes installed binaries. It keeps local QuickSync state and logs by default.
