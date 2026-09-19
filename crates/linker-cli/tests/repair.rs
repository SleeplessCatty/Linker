use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::UNIX_EPOCH;

use assert_cmd::prelude::*;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;

struct Fixture {
    tmp: tempfile::TempDir,
    state: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let state = tmp.path().join("app");
        Self { tmp, state }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn source(&self, name: &str) -> PathBuf {
        let path = self.path(name);
        fs::create_dir_all(&path).unwrap();
        fs::write(path.join("keep.txt"), name).unwrap();
        path
    }

    fn cli(&self) -> Command {
        let mut cmd = Command::cargo_bin("linker").unwrap();
        cmd.env("LINKER_APP_SUPPORT_DIR", &self.state)
            .env("HOME", self.tmp.path());
        cmd
    }

    fn add(&self, source: &Path, target: &Path, name: Option<&str>) -> Command {
        let mut cmd = self.cli();
        cmd.arg("add").arg(source).arg(target);
        if let Some(name) = name {
            cmd.arg("--name").arg(name);
        }
        cmd
    }

    fn set_mtime(&self, path: &Path, seconds: i64) {
        filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(seconds, 0)).unwrap();
    }

    fn sqlite_bytes(&self) -> Vec<u8> {
        fs::read(self.state.join("state.sqlite")).unwrap()
    }
}

/// Relative paths, sizes and modification times of every entry below `root`.
fn snapshot(root: &Path) -> Vec<(PathBuf, u64, i64)> {
    let mut entries = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let metadata = fs::symlink_metadata(entry.path()).unwrap();
            if metadata.is_dir() {
                stack.push(entry.path());
            }
            entries.push((
                entry.path().strip_prefix(root).unwrap().to_path_buf(),
                metadata.len(),
                metadata
                    .modified()
                    .unwrap()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64,
            ));
        }
    }
    entries.sort();
    entries
}

#[test]
fn check_reports_every_divergence_class_and_exits_nonzero() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::write(source.join("differ.txt"), "source version").unwrap();
    fs::write(target.join("differ.txt"), "target version").unwrap();
    fs::write(source.join("source-only.txt"), "only here").unwrap();
    fs::write(target.join("target-only.txt"), "only there").unwrap();
    fs::write(source.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(target.join("ignored.txt"), "ignored").unwrap();
    std::os::unix::fs::symlink(f.path("absent"), source.join("link")).unwrap();

    f.cli()
        .args(["check", "source"])
        .assert()
        .failure()
        .stdout(contains("check: source"))
        .stdout(contains(source.display().to_string()))
        .stdout(contains("identical:"))
        .stdout(contains("content_differs"))
        .stdout(contains("source_only"))
        .stdout(contains("target_only"))
        .stdout(contains("ignored_target_content"))
        .stdout(contains("unsupported_entry"))
        .stdout(contains("divergent: 4"))
        .stdout(contains("advisory: 2"));
}

#[test]
fn check_of_a_converged_pair_is_consistent_and_changes_nothing() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    let before_state = f.sqlite_bytes();
    let before_source = snapshot(&source);
    let before_target = snapshot(&target);

    f.cli()
        .arg("check")
        .assert()
        .success()
        .stdout(contains("consistent"))
        .stdout(contains("divergent").not());

    assert_eq!(before_state, f.sqlite_bytes());
    assert_eq!(before_source, snapshot(&source));
    assert_eq!(before_target, snapshot(&target));
}

#[test]
fn repair_defaults_to_the_source_and_keeps_target_only_paths_without_prune() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::write(source.join("differ.txt"), "source version").unwrap();
    fs::write(target.join("differ.txt"), "target version").unwrap();
    fs::write(source.join("copied.txt"), "from source").unwrap();
    fs::write(target.join("target-only.txt"), "kept").unwrap();

    f.cli()
        .arg("repair")
        .assert()
        .success()
        .stdout(contains("authoritative side: source"))
        .stdout(contains("write_target"))
        .stdout(contains("skipped: 1"));

    assert_eq!(
        fs::read_to_string(target.join("differ.txt")).unwrap(),
        "source version"
    );
    assert_eq!(
        fs::read_to_string(target.join("copied.txt")).unwrap(),
        "from source"
    );
    assert!(target.join("target-only.txt").exists());

    f.cli()
        .args(["repair", "--prune"])
        .assert()
        .success()
        .stdout(contains("delete_target"));

    assert!(!target.join("target-only.txt").exists());
    f.cli().args(["check", "source"]).assert().success();
}

#[test]
fn repair_dry_run_reports_operations_without_changing_files_or_state() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::write(source.join("differ.txt"), "source version").unwrap();
    fs::write(target.join("differ.txt"), "target version").unwrap();
    fs::write(source.join("copied.txt"), "from source").unwrap();

    let before_state = f.sqlite_bytes();
    let before_source = snapshot(&source);
    let before_target = snapshot(&target);

    f.cli()
        .args(["repair", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("planned: 2"));

    assert!(!target.join("copied.txt").exists());
    assert_eq!(
        fs::read_to_string(target.join("differ.txt")).unwrap(),
        "target version"
    );
    assert_eq!(before_state, f.sqlite_bytes());
    assert_eq!(before_source, snapshot(&source));
    assert_eq!(before_target, snapshot(&target));
}

#[test]
fn repair_prefers_the_target_only_when_asked_and_needs_prune_to_drop_source_paths() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::write(source.join("differ.txt"), "source version").unwrap();
    fs::write(target.join("differ.txt"), "target version").unwrap();
    fs::write(source.join("source-only.txt"), "kept by default").unwrap();

    f.cli()
        .args(["repair", "--prefer", "target"])
        .assert()
        .success()
        .stdout(contains("authoritative side: target"))
        .stdout(contains("write_source"))
        .stdout(contains("skipped: 1"));

    assert_eq!(
        fs::read_to_string(source.join("differ.txt")).unwrap(),
        "target version"
    );
    assert!(source.join("source-only.txt").exists());

    f.cli()
        .args(["repair", "--prefer", "target", "--prune"])
        .assert()
        .success();

    assert!(!source.join("source-only.txt").exists());
    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "source"
    );
}

#[test]
fn repair_newest_uses_the_newer_modification_time() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::write(source.join("newest.txt"), "source version").unwrap();
    fs::write(target.join("newest.txt"), "target version").unwrap();
    f.set_mtime(&source.join("newest.txt"), 1_700_000_000);
    f.set_mtime(&target.join("newest.txt"), 1_700_000_500);

    f.cli()
        .args(["repair", "--prefer", "newest"])
        .assert()
        .success()
        .stdout(contains("authoritative side: newest"));

    assert_eq!(
        fs::read_to_string(source.join("newest.txt")).unwrap(),
        "target version"
    );

    fs::write(source.join("newest.txt"), "newer source version").unwrap();
    f.set_mtime(&source.join("newest.txt"), 1_700_001_000);
    f.cli()
        .args(["repair", "--prefer", "newest"])
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(target.join("newest.txt")).unwrap(),
        "newer source version"
    );
}

#[test]
fn repaired_paths_leave_the_next_sync_with_nothing_to_do() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::write(source.join("differ.txt"), "source version").unwrap();
    fs::write(target.join("differ.txt"), "target version").unwrap();
    fs::write(source.join("copied.txt"), "from source").unwrap();

    f.cli().arg("repair").assert().success();
    f.cli()
        .args(["sync", "source", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("no changes"));
    f.cli().args(["check", "source"]).assert().success();
    f.cli()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("deleted source: 0"))
        .stdout(contains("deleted target: 0"));
}

#[test]
fn repair_resolves_a_type_conflict_only_with_an_explicit_side_and_prune() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::create_dir(source.join("conflict")).unwrap();
    fs::write(source.join("conflict/file.txt"), "inside").unwrap();
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::remove_dir_all(target.join("conflict")).unwrap();
    fs::write(target.join("conflict"), "a plain file").unwrap();

    f.cli()
        .args(["sync", "source"])
        .assert()
        .failure()
        .stderr(contains("source/target type conflict"));

    f.cli()
        .args(["check", "source"])
        .assert()
        .failure()
        .stdout(contains("type_conflict"));

    f.cli()
        .args(["repair", "--prefer", "newest"])
        .assert()
        .success()
        .stdout(contains("skip_type_conflict"));

    f.cli()
        .arg("repair")
        .assert()
        .success()
        .stdout(contains("replace_target"))
        .stdout(contains("requires --prune"));

    assert!(fs::symlink_metadata(target.join("conflict"))
        .unwrap()
        .is_file());

    f.cli()
        .args(["repair", "--prune"])
        .assert()
        .success()
        .stdout(contains("replace_target"));

    assert!(fs::symlink_metadata(target.join("conflict"))
        .unwrap()
        .is_dir());
    assert_eq!(
        fs::read_to_string(target.join("conflict/file.txt")).unwrap(),
        "inside"
    );
    f.cli().args(["check", "source"]).assert().success();
}

#[test]
fn check_and_repair_require_existing_roots_and_known_items() {
    let f = Fixture::new();
    f.cli()
        .arg("check")
        .assert()
        .success()
        .stdout(contains("no items"));

    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();
    fs::remove_dir_all(&target).unwrap();

    f.cli()
        .args(["check", "source"])
        .assert()
        .failure()
        .stderr(contains("path does not exist"));
    f.cli()
        .args(["repair", "source"])
        .assert()
        .failure()
        .stderr(contains("path does not exist"));
    f.cli()
        .args(["check", "missing"])
        .assert()
        .failure()
        .stderr(contains("item was not found"));
    f.cli()
        .args(["repair", "missing"])
        .assert()
        .failure()
        .stderr(contains("item was not found"));
}
