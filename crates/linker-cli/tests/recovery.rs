use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
}

#[test]
fn a_vanished_target_root_is_restored_from_the_source() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::create_dir_all(source.join("nested")).unwrap();
    fs::write(source.join("nested/deep.txt"), "deep").unwrap();
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::remove_dir_all(&target).unwrap();
    f.cli()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("source -> target: 2"))
        .stdout(contains("target -> source: 0"))
        .stdout(contains("deleted source: 0"))
        .stderr(contains("the target root was missing"));

    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "source"
    );
    assert_eq!(
        fs::read_to_string(source.join("nested/deep.txt")).unwrap(),
        "deep"
    );
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "source"
    );
    assert_eq!(
        fs::read_to_string(target.join("nested/deep.txt")).unwrap(),
        "deep"
    );
    f.cli().args(["check", "source"]).assert().success();
}

#[test]
fn a_single_deleted_target_file_still_propagates_to_the_source() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::write(source.join("other.txt"), "other").unwrap();
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    // The target root still holds other.txt, so this is an ordinary per-file
    // removal and keeps propagating to the source.
    fs::remove_file(target.join("keep.txt")).unwrap();
    f.cli()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("deleted source: 1"));

    assert!(!source.join("keep.txt").exists());
    assert_eq!(
        fs::read_to_string(source.join("other.txt")).unwrap(),
        "other"
    );
    assert_eq!(
        fs::read_to_string(target.join("other.txt")).unwrap(),
        "other"
    );
}

#[test]
fn deleting_the_last_target_file_still_follows_per_file_deletion_semantics() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    // An existing target root keeps ordinary per-file semantics: removing its
    // last file propagates to the source. `linker check` reports such a removal
    // before it happens and `linker repair` refills a target that was emptied
    // by accident without deleting source content.
    fs::remove_file(target.join("keep.txt")).unwrap();
    f.cli()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("deleted source: 1"));

    assert!(!source.join("keep.txt").exists());
}

#[test]
fn an_emptied_target_root_is_reported_and_repairable_without_losing_source_content() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::write(source.join("other.txt"), "other").unwrap();
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    // The root still exists, so a sync would propagate these removals. The audit
    // says so explicitly, which is what makes the state safe to act on.
    fs::remove_file(target.join("keep.txt")).unwrap();
    fs::remove_file(target.join("other.txt")).unwrap();
    f.cli()
        .args(["check", "source"])
        .assert()
        .failure()
        .stdout(contains("deletes this source file"));

    f.cli()
        .arg("repair")
        .assert()
        .success()
        .stdout(contains("write_target"));

    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "source"
    );
    assert_eq!(
        fs::read_to_string(source.join("other.txt")).unwrap(),
        "other"
    );
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "source"
    );
    assert_eq!(
        fs::read_to_string(target.join("other.txt")).unwrap(),
        "other"
    );
    f.cli().args(["check", "source"]).assert().success();
}

#[test]
fn a_changed_source_file_is_restored_instead_of_deleted() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::remove_file(target.join("keep.txt")).unwrap();
    fs::write(source.join("keep.txt"), "edited after the last sync").unwrap();
    f.cli()
        .arg("sync")
        .assert()
        .success()
        .stdout(contains("deleted source: 0"));

    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "edited after the last sync"
    );
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "edited after the last sync"
    );
}

#[test]
fn a_vanished_source_root_pauses_the_item_without_touching_the_target() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    fs::remove_dir_all(&source).unwrap();
    f.cli()
        .arg("sync")
        .assert()
        .failure()
        .stderr(contains("path does not exist"));

    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "source"
    );
    f.cli()
        .arg("check")
        .assert()
        .failure()
        .stderr(contains("path does not exist"));
}

#[test]
fn check_predicts_what_a_real_sync_does() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    // A recorded target deletion is announced as a source deletion...
    fs::write(source.join("doomed.txt"), "doomed").unwrap();
    f.cli().arg("sync").assert().success();
    fs::remove_file(target.join("doomed.txt")).unwrap();
    // ...while a source-only file is announced as a copy to the target.
    fs::write(source.join("fresh.txt"), "fresh").unwrap();

    f.cli()
        .args(["check", "source"])
        .assert()
        .failure()
        .stdout(contains("deletes this source file"))
        .stdout(contains("copies it to the target"));

    f.cli().arg("sync").assert().success();

    assert!(!source.join("doomed.txt").exists());
    assert_eq!(
        fs::read_to_string(target.join("fresh.txt")).unwrap(),
        "fresh"
    );
}

#[test]
fn a_target_root_replaced_by_a_symlink_is_refused_instead_of_followed() {
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, None).assert().success();

    let elsewhere = f.path("elsewhere");
    fs::create_dir(&elsewhere).unwrap();
    fs::write(elsewhere.join("keep.txt"), "elsewhere content").unwrap();
    fs::write(elsewhere.join("only-here.txt"), "elsewhere only").unwrap();
    fs::rename(&target, f.path("moved-target")).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &target).unwrap();

    f.cli().arg("sync").assert().failure();

    // Nothing is written through the link and no deletion is propagated.
    assert_eq!(
        fs::read_to_string(elsewhere.join("keep.txt")).unwrap(),
        "elsewhere content"
    );
    assert!(elsewhere.join("only-here.txt").exists());
    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "source"
    );
    assert!(fs::symlink_metadata(&target).unwrap().is_symlink());
}

#[test]
fn an_emptied_target_root_without_baselines_reports_no_recovery() {
    let f = Fixture::new();
    let source = f.path("empty-source");
    fs::create_dir(&source).unwrap();
    let target = f.path("empty-target");
    f.add(&source, &target, None).assert().success();

    // Nothing was ever recorded on the target, so an empty root is normal and
    // must not be announced as a recovery.
    f.cli()
        .arg("sync")
        .assert()
        .success()
        .stderr(contains("the target root was missing").not());
}
