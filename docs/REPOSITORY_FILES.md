# Repository File Policy

## Commit to Git

Commit source, docs, packaging, and reproducibility files:

- `Cargo.toml`
- `Cargo.lock`
- `crates/**/Cargo.toml`
- `crates/**/src/**/*.rs`
- `crates/**/tests/**/*.rs`
- `README.md`
- `USAGE.md`
- `INSTALL.md`
- `PRODUCT.md`
- `SPEC.md`
- `TASKS.md`
- `LICENSE`
- `.gitignore`
- `.github/workflows/ci.yml`
- `scripts/**/*.sh`
- `packaging/**`
- `docs/**`

## Keep Local Only

Do not commit generated or machine-local files:

- `target/`
- `.DS_Store`
- `*.sqlite`
- `*.sqlite-shm`
- `*.sqlite-wal`
- local logs
- installed binaries under `~/Library/Application Support/Linker/bin/`
- runtime state under `~/Library/Application Support/Linker/`
- user sync content under source directories and target parent directories

## Pre-Push Checklist

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo check --workspace --all-targets
bash -n scripts/install.sh scripts/uninstall.sh scripts/install-remote.sh scripts/lib/cleanup-legacy.sh scripts/tests/legacy-cleanup.sh scripts/tests/install-safety.sh scripts/tests/uninstall-safety.sh scripts/tests/remote-ref.sh scripts/tests/branding-residue.sh
bash scripts/tests/legacy-cleanup.sh
bash scripts/tests/install-safety.sh
bash scripts/tests/uninstall-safety.sh
bash scripts/tests/remote-ref.sh
bash scripts/tests/branding-residue.sh
ruby -c packaging/homebrew/linker.rb
plutil -lint packaging/launchagent/com.linker.linkerd.plist.in
git status --short --ignored
```

The ignored output should include local build artifacts like `target/` and `.DS_Store`, but tracked source/docs/scripts should be staged.
