# GitHub Release and Install Setup

## Initialize and Push

```bash
git init
git branch -M main
git add .
git commit -m "Initial QuickSync release"
git remote add origin git@github.com:SleeplessCatty/QuickSync.git
git push -u origin main
```

If the repository name or owner changes, update:

- `scripts/install-remote.sh`
- `packaging/homebrew/quicksync.rb`
- install snippets in `README.md` and `INSTALL.md`

## Remote Script Install

After pushing to GitHub:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

Forward install options with `bash -s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash -s -- --no-link
```

Install a specific tag:

```bash
QUICKSYNC_REF=v0.1.0 curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/QuickSync/main/scripts/install-remote.sh | bash
```

## Homebrew Tap

Create a tap repository:

```text
github.com/SleeplessCatty/homebrew-quicksync
```

Copy:

```text
packaging/homebrew/quicksync.rb -> Formula/quicksync.rb
```

The checked-in formula is a `HEAD` formula first. Users can install it before a release with:

```bash
brew install --HEAD quicksync
```

For a stable formula, publish a GitHub release tag and add `url` plus `sha256`:

```bash
curl -L https://github.com/SleeplessCatty/QuickSync/archive/refs/tags/v0.1.0.tar.gz | shasum -a 256
```

Then users can install:

```bash
brew tap SleeplessCatty/quicksync
brew install quicksync
```

Before a release SHA is available, the formula can still support source install with:

```bash
brew install --HEAD quicksync
```
