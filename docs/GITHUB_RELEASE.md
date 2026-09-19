# GitHub Release and Install Setup

Version 0.3.0 is a local/code upgrade in this change; no GitHub Release is published. The following is a future publishing checklist, not a release-status claim.

## Initialize and Push

```bash
git init
git branch -M main
git add .
git commit -m "Initial Linker release"
git remote add origin git@github.com:SleeplessCatty/Linker.git
git push -u origin main
```

If the repository name or owner changes, update:

- `scripts/install-remote.sh`
- `packaging/homebrew/linker.rb`
- install snippets in `README.md` and `INSTALL.md`

## Remote Script Install

After pushing to GitHub:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash
```

Forward install options with `bash -s --`:

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | bash -s -- --no-link
```

Install a published branch/tag/commit (verify it exists first):

```bash
curl -fsSL https://raw.githubusercontent.com/SleeplessCatty/Linker/main/scripts/install-remote.sh | LINKER_REF=main bash
```

## Homebrew Tap

Create a tap repository:

```text
github.com/SleeplessCatty/homebrew-linker
```

Copy:

```text
packaging/homebrew/linker.rb -> Formula/linker.rb
```

The checked-in formula is intentionally `HEAD`-only until a stable release exists. Users can install it before that release with:

```bash
brew install --HEAD linker
```

The formula is for fresh installs. It does not perform the incompatible pre-0.2 user-state cleanup or install a user LaunchAgent; existing users must complete the guarded script upgrade first.

For a stable formula, publish a GitHub release tag and add `url` plus `sha256`:

```bash
curl -L "https://github.com/SleeplessCatty/Linker/archive/refs/tags/<published-tag>.tar.gz" | shasum -a 256
```

Then users can install:

```bash
brew tap SleeplessCatty/linker
brew install linker
```

Do not publish or document the plain install command until the release URL and SHA are present in the formula. Before then, use:

```bash
brew install --HEAD linker
```
