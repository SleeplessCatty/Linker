use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use assert_cmd::prelude::*;
use predicates::str::contains;
use rusqlite::Connection;

struct Fixture {
    tmp: tempfile::TempDir,
    state: PathBuf,
}

#[test]
fn failed_delete_unregisters_before_target_cleanup_and_never_propagates_to_source() {
    use std::os::unix::fs::PermissionsExt;
    if Command::new("id").arg("-u").output().unwrap().stdout == b"0\n" {
        return;
    }
    let f = Fixture::new();
    let source = f.source("source");
    let target = f.path("target");
    f.add(&source, &target, Some("one")).assert().success();
    f.add(&source, &f.path("other"), Some("other"))
        .assert()
        .success();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o500)).unwrap();
    let output = f.cli().args(["delete", "one"]).output().unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("association removed"));
    assert_eq!(f.count(), 1);
    let conn = Connection::open(f.state.join("state.sqlite")).unwrap();
    let dangling: i64 = conn
        .query_row(
            "SELECT count(*) FROM file_states WHERE item_id NOT IN (SELECT id FROM items)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(dangling, 0);
    assert!(!f.state.join("manifests/one.json").exists());
    // Even if failed cleanup has removed only part of the target, later sync
    // must not apply that disappearance to the source or another association.
    fs::remove_file(target.join("keep.txt")).unwrap();
    f.cli().arg("sync").assert().success();
    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "source"
    );
    assert_eq!(
        fs::read_to_string(f.path("other/keep.txt")).unwrap(),
        "source"
    );
    f.cli()
        .args(["sync", "one"])
        .assert()
        .failure()
        .stderr(contains("not found"));
}

#[test]
fn unregister_database_failure_keeps_target_and_all_baselines_unchanged() {
    for command in ["remove", "delete"] {
        let f = Fixture::new();
        let source = f.source("source");
        let target = f.path("target");
        f.add(&source, &target, Some("one")).assert().success();
        let conn = Connection::open(f.state.join("state.sqlite")).unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_unregister BEFORE DELETE ON items
            BEGIN SELECT RAISE(FAIL, 'injected unregister failure'); END;",
        )
        .unwrap();
        let before: String = conn
            .query_row("SELECT group_concat(id) FROM file_states", [], |r| r.get(0))
            .unwrap();
        f.cli().args([command, "one"]).assert().failure();
        assert_eq!(f.count(), 1);
        let after: String = conn
            .query_row("SELECT group_concat(id) FROM file_states", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert_eq!(
            fs::read_to_string(source.join("keep.txt")).unwrap(),
            "source"
        );
        assert_eq!(
            fs::read_to_string(target.join("keep.txt")).unwrap(),
            "source"
        );
        assert!(f.state.join("manifests/one.json").exists());
    }
}

#[test]
fn delete_never_follows_target_root_or_ancestor_symlink_to_source() {
    for ancestor in [false, true] {
        let f = Fixture::new();
        let source = f.source("source");
        let target = f.path("parent/target");
        f.add(&source, &target, Some("one")).assert().success();
        if ancestor {
            fs::create_dir(source.join("target")).unwrap();
            fs::write(source.join("target/precious.txt"), "keep").unwrap();
            fs::rename(f.path("parent"), f.path("moved")).unwrap();
            std::os::unix::fs::symlink(&source, f.path("parent")).unwrap();
            f.cli()
                .args(["delete", "one"])
                .assert()
                .failure()
                .stderr(contains("association removed"));
            assert_eq!(
                fs::read_to_string(source.join("target/precious.txt")).unwrap(),
                "keep"
            );
        } else {
            fs::rename(&target, f.path("moved")).unwrap();
            std::os::unix::fs::symlink(&source, &target).unwrap();
            f.cli().args(["delete", "one"]).assert().success();
            assert!(fs::symlink_metadata(&target).is_err());
        }
        assert_eq!(f.count(), 0);
        assert_eq!(
            fs::read_to_string(source.join("keep.txt")).unwrap(),
            "source"
        );
    }
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
    fn count(&self) -> i64 {
        let path = self.state.join("state.sqlite");
        if !path.exists() {
            return 0;
        }
        Connection::open(path)
            .unwrap()
            .query_row("SELECT count(*) FROM items", [], |r| r.get(0))
            .unwrap()
    }
}

#[test]
fn uses_exact_destination_and_name_is_independent_of_both_basenames() {
    let f = Fixture::new();
    let source = f.source("source/notes");
    let target = f.path("new/nested/MyNotes");
    f.add(&source, &target, Some("工作笔记"))
        .assert()
        .success()
        .stdout(contains("added: 工作笔记"));
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        "source/notes"
    );
    assert!(!target.join("notes").exists());
    assert!(!target.join("工作笔记").exists());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(f.state.join("manifests/工作笔记.json")).unwrap())
            .unwrap();
    assert_eq!(manifest["name"], "工作笔记");
    assert_eq!(
        Path::new(manifest["target_path"].as_str().unwrap()),
        target.canonicalize().unwrap()
    );
    f.cli()
        .args(["sync", "工作笔记", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("no changes"));
    f.cli().args(["remove", "工作笔记"]).assert().success();
    assert!(source.join("keep.txt").exists());
    assert!(target.join("keep.txt").exists());
}

#[test]
fn accepts_empty_existing_directory_and_defaults_to_source_name() {
    let f = Fixture::new();
    let source = f.source("notes");
    let target = f.path("OtherName");
    fs::create_dir(&target).unwrap();
    f.add(&source, &target, None)
        .assert()
        .success()
        .stdout(contains("added: notes"));
    assert!(target.join("keep.txt").exists());
    assert!(f.state.join("manifests/notes.json").exists());
}

#[test]
fn every_target_entry_counts_as_nonempty_and_rejection_preserves_everything() {
    for entry in [
        "ordinary.txt",
        ".DS_Store",
        ".gitignore",
        "empty-child",
        "dangling-link",
    ] {
        let f = Fixture::new();
        let source = f.source("source");
        let target = f.path("target");
        fs::create_dir(&target).unwrap();
        let child = target.join(entry);
        match entry {
            "empty-child" => fs::create_dir(&child).unwrap(),
            "dangling-link" => std::os::unix::fs::symlink(f.path("absent"), &child).unwrap(),
            _ => fs::write(&child, "preserve").unwrap(),
        }
        f.add(&source, &target, None)
            .assert()
            .failure()
            .stderr(contains("target directory is not empty"))
            .stderr(contains(target.to_str().unwrap()));
        assert!(fs::symlink_metadata(&child).is_ok());
        if matches!(entry, "ordinary.txt" | ".DS_Store" | ".gitignore") {
            assert_eq!(fs::read_to_string(&child).unwrap(), "preserve");
        }
        assert!(!target.join("keep.txt").exists());
        assert_eq!(
            fs::read_to_string(source.join("keep.txt")).unwrap(),
            "source"
        );
        assert!(!f.state.exists());
    }
}

#[test]
fn same_source_basenames_work_with_distinct_record_names() {
    let f = Fixture::new();
    let first = f.source("a/notes");
    let second = f.source("b/notes");
    f.add(&first, &f.path("one"), None).assert().success();
    let before = fs::read(f.state.join("state.sqlite")).unwrap();
    f.add(&second, &f.path("two"), None)
        .assert()
        .failure()
        .stderr(contains("unique --name"));
    assert!(!f.path("two").exists());
    assert_eq!(fs::read(f.state.join("state.sqlite")).unwrap(), before);
    f.add(&second, &f.path("two"), Some("second-notes"))
        .assert()
        .success();
    assert_eq!(f.count(), 2);
    assert_eq!(
        fs::read_to_string(f.path("one/keep.txt")).unwrap(),
        "a/notes"
    );
    assert_eq!(
        fs::read_to_string(f.path("two/keep.txt")).unwrap(),
        "b/notes"
    );
}

#[test]
fn invalid_custom_names_fail_before_creating_destination_or_state() {
    for name in [
        "",
        " ",
        ".",
        "..",
        ".linker",
        "a/b",
        "a\\b",
        " padded",
        "padded ",
        "line\nfeed",
        "tab\tname",
    ] {
        let f = Fixture::new();
        let source = f.source("source");
        f.add(&source, &f.path("target"), Some(name))
            .assert()
            .failure()
            .stderr(contains("invalid item name"));
        assert!(!f.path("target").exists());
        assert!(!f.state.exists());
    }
}

#[test]
fn rejects_case_collisions_and_names_that_alias_existing_ids() {
    let f = Fixture::new();
    let one = f.source("one");
    let two = f.source("two");
    f.add(&one, &f.path("out-one"), Some("Notes"))
        .assert()
        .success();
    let conn = Connection::open(f.state.join("state.sqlite")).unwrap();
    let id: String = conn
        .query_row("SELECT id FROM items", [], |r| r.get(0))
        .unwrap();
    for name in ["notes", id.as_str()] {
        f.add(&two, &f.path("out-two"), Some(name))
            .assert()
            .failure()
            .stderr(contains("item already exists"));
        assert!(!f.path("out-two").exists());
    }
    assert_eq!(f.count(), 1);
}

#[test]
fn rejects_files_and_symlink_destinations_without_touching_referents() {
    for kind in ["file", "symlink", "dangling"] {
        let f = Fixture::new();
        let source = f.source("source");
        let target = f.path("target");
        let outside = f.path("outside");
        match kind {
            "file" => fs::write(&target, "keep").unwrap(),
            "symlink" => {
                fs::create_dir(&outside).unwrap();
                std::os::unix::fs::symlink(&outside, &target).unwrap();
            }
            _ => std::os::unix::fs::symlink(&outside, &target).unwrap(),
        }
        f.add(&source, &target, None).assert().failure();
        assert!(fs::symlink_metadata(&target).is_ok());
        assert!(!outside.join("keep.txt").exists());
        assert!(!f.state.exists());
    }
}

#[test]
fn canonical_paths_reject_same_tree_and_symlink_ancestor_overlap() {
    let f = Fixture::new();
    let source = f.source("source");
    std::os::unix::fs::symlink(&source, f.path("alias")).unwrap();
    for target in [
        source.clone(),
        source.join("child"),
        f.tmp.path().to_owned(),
        f.path("alias/new"),
        f.path("unused/../source/new"),
    ] {
        f.add(&source, &target, Some("name"))
            .assert()
            .failure()
            .stderr(contains("invalid sync association"));
    }
    assert!(!source.join("child").exists());
    assert!(!source.join("new").exists());
    assert!(!f.path("unused").exists());
    assert!(!f.state.exists());
}

#[test]
fn relative_and_tilde_paths_resolve_without_appending_any_name() {
    let f = Fixture::new();
    let source = f.source("source");
    f.cli()
        .current_dir(f.tmp.path())
        .args(["add", "source", "./missing/../out", "--name", "relative"])
        .assert()
        .success();
    assert!(f.path("out/keep.txt").exists());
    assert!(!f.path("missing").exists());
    let other = f.source("second");
    f.cli()
        .args(["add", "~/second", "~/tilde-output", "--name", "tilde"])
        .assert()
        .success();
    assert!(f.path("tilde-output/keep.txt").exists());
    assert!(source.join("keep.txt").exists());
    assert!(other.join("keep.txt").exists());
}

#[test]
fn rejects_overlap_with_any_existing_association_or_application_state() {
    let f = Fixture::new();
    let first = f.source("source");
    let target = f.path("target");
    f.add(&first, &target, Some("first")).assert().success();
    let second = f.source("second");
    fs::create_dir(first.join("child")).unwrap();
    for (source, destination) in [
        (first.join("child"), f.path("another")),
        (target.clone(), f.path("another")),
        (second.clone(), first.join("nested")),
        (second.clone(), target.join("nested")),
        (second.clone(), f.state.join("nested")),
        (f.state.clone(), f.path("another")),
    ] {
        f.add(&source, &destination, Some("new-name"))
            .assert()
            .failure()
            .stderr(contains("invalid sync association"));
        assert_eq!(f.count(), 1);
    }
    assert!(!f.path("another").exists());
    assert!(!first.join("nested").exists());
    assert!(!target.join("nested").exists());
    assert!(!f.state.join("nested").exists());
}

#[test]
fn shared_source_supports_distinct_targets_and_independent_removal_and_delete() {
    let f = Fixture::new();
    let source = f.source("learn");
    let one = f.path("local-learn");
    let two = f.path("cloud-learn");
    f.add(&source, &one, None).assert().success();
    let alias = f.path("learn-alias");
    std::os::unix::fs::symlink(&source, &alias).unwrap();
    f.add(&alias, &two, Some("learn-ob")).assert().success();
    assert_eq!(f.count(), 2);
    for path in [&one, &two] {
        assert_eq!(fs::read_to_string(path.join("keep.txt")).unwrap(), "learn");
    }
    f.cli().args(["remove", "learn-ob"]).assert().success();
    assert_eq!(f.count(), 1);
    fs::write(source.join("new.txt"), "new").unwrap();
    f.cli().args(["sync", "learn"]).assert().success();
    assert!(one.join("new.txt").exists());
    assert!(!two.join("new.txt").exists());
    assert!(two.join("keep.txt").exists());
    let three = f.path("third");
    f.add(&source, &three, Some("third")).assert().success();
    f.cli().args(["delete", "learn"]).assert().success();
    assert_eq!(f.count(), 1);
    assert!(!one.exists());
    assert!(source.join("keep.txt").exists());
    assert!(three.join("keep.txt").exists());
    f.cli()
        .args(["sync", "third", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("no changes"));
}

#[test]
fn target_changes_and_deletions_propagate_via_shared_source() {
    let f = Fixture::new();
    let source = f.source("learn");
    let one = f.path("one");
    let two = f.path("two");
    f.add(&source, &one, Some("a")).assert().success();
    f.add(&source, &two, Some("b")).assert().success();
    fs::write(one.join("from-a.txt"), "from target a").unwrap();
    f.cli().args(["sync", "a"]).assert().success();
    f.cli().args(["sync", "b"]).assert().success();
    assert_eq!(
        fs::read_to_string(two.join("from-a.txt")).unwrap(),
        "from target a"
    );
    fs::remove_file(two.join("keep.txt")).unwrap();
    f.cli().args(["sync", "b"]).assert().success();
    assert!(!source.join("keep.txt").exists());
    f.cli().args(["sync", "a"]).assert().success();
    assert!(!one.join("keep.txt").exists());
    f.cli()
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("no changes"));
}

#[test]
fn shared_source_ignore_cleanup_and_reinclusion_preserve_source() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::write(source.join("keep.log"), "never delete source").unwrap();
    let targets = [f.path("one"), f.path("two")];
    for (target, name) in targets.iter().zip(["one", "two"]) {
        f.add(&source, target, Some(name)).assert().success();
    }
    fs::write(source.join(".gitignore"), "*.log\n").unwrap();
    f.cli().arg("sync").assert().success();
    assert!(targets
        .iter()
        .all(|target| !target.join("keep.log").exists()));
    assert_eq!(
        fs::read_to_string(source.join("keep.log")).unwrap(),
        "never delete source"
    );
    fs::remove_file(source.join(".gitignore")).unwrap();
    f.cli().arg("sync").assert().success();
    for target in &targets {
        assert_eq!(
            fs::read_to_string(target.join("keep.log")).unwrap(),
            "never delete source"
        );
    }
    f.cli()
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("no changes"));
}

#[test]
fn shared_source_conflicts_converge_by_mtime_over_successive_pair_syncs() {
    let f = Fixture::new();
    let source = f.source("source");
    filetime::set_file_mtime(
        source.join("keep.txt"),
        filetime::FileTime::from_unix_time(100, 0),
    )
    .unwrap();
    let one = f.path("one");
    let two = f.path("two");
    f.add(&source, &one, Some("one")).assert().success();
    f.add(&source, &two, Some("two")).assert().success();
    fs::write(one.join("keep.txt"), "older target edit").unwrap();
    filetime::set_file_mtime(
        one.join("keep.txt"),
        filetime::FileTime::from_unix_time(200, 0),
    )
    .unwrap();
    fs::write(two.join("keep.txt"), "newer target edit").unwrap();
    filetime::set_file_mtime(
        two.join("keep.txt"),
        filetime::FileTime::from_unix_time(300, 0),
    )
    .unwrap();
    f.cli().arg("sync").assert().success();
    f.cli().arg("sync").assert().success();
    for path in [&source, &one, &two] {
        assert_eq!(
            fs::read_to_string(path.join("keep.txt")).unwrap(),
            "newer target edit"
        );
    }
}

#[test]
fn shared_source_keeps_name_target_and_cross_role_overlap_protection() {
    let f = Fixture::new();
    let source = f.path("source");
    let target = f.path("target");
    fs::create_dir(&source).unwrap();
    f.add(&source, &target, Some("first")).assert().success();
    f.add(&source, &f.path("other"), Some("first"))
        .assert()
        .failure()
        .stderr(contains("item already exists"));
    for (src, dest) in [
        (source.clone(), target.clone()),
        (source.clone(), target.join("nested")),
        (target.clone(), f.path("other")),
        (source.clone(), source.join("nested")),
    ] {
        f.add(&src, &dest, Some("second"))
            .assert()
            .failure()
            .stderr(contains("invalid sync association"));
    }
    assert_eq!(f.count(), 1);
    assert!(!f.path("other").exists());
}

#[test]
fn concurrent_adds_of_shared_source_to_separate_targets_both_succeed() {
    let f = Fixture::new();
    let source = f.source("source");
    let mut children = Vec::new();
    for name in ["one", "two"] {
        children.push(
            f.add(&source, &f.path(name), Some(name))
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    for child in children {
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert_eq!(f.count(), 2);
    for name in ["one", "two"] {
        assert_eq!(
            fs::read_to_string(f.path(name).join("keep.txt")).unwrap(),
            "source"
        );
    }
}

#[test]
fn failed_shared_source_add_preserves_existing_item_and_baselines() {
    let f = Fixture::new();
    let source = f.source("source");
    f.add(&source, &f.path("one"), Some("one"))
        .assert()
        .success();
    let conn = Connection::open(f.state.join("state.sqlite")).unwrap();
    let before: String = conn
        .query_row("SELECT group_concat(id) FROM file_states", [], |r| r.get(0))
        .unwrap();
    conn.execute_batch(
        "CREATE TRIGGER reject_second BEFORE INSERT ON file_states
        WHEN NEW.item_id != (SELECT id FROM items WHERE name='one')
        BEGIN SELECT RAISE(FAIL, 'injected second-target failure'); END;",
    )
    .unwrap();
    f.add(&source, &f.path("two"), Some("two"))
        .assert()
        .failure()
        .stderr(contains("new association removed"));
    assert_eq!(f.count(), 1);
    let after: String = conn
        .query_row("SELECT group_concat(id) FROM file_states", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    assert!(source.join("keep.txt").exists());
    assert!(f.path("one/keep.txt").exists());
    assert!(f.state.join("manifests/one.json").exists());
    assert!(!f.state.join("manifests/two.json").exists());
    f.cli()
        .args(["sync", "one", "--dry-run"])
        .assert()
        .success()
        .stdout(contains("no changes"));
}

#[test]
fn refuses_orphan_manifest_instead_of_overwriting_it() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::create_dir_all(f.state.join("manifests")).unwrap();
    let path = f.state.join("manifests/custom.json");
    fs::write(&path, "preserve unknown manifest").unwrap();
    f.add(&source, &f.path("target"), Some("custom"))
        .assert()
        .failure()
        .stderr(contains("manifest already exists"));
    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "preserve unknown manifest"
    );
    assert!(!f.path("target").exists());
    assert_eq!(f.count(), 0);
}

#[test]
fn failed_initial_sync_rolls_back_registration_and_never_deletes_source() {
    let f = Fixture::new();
    let source = f.source("source");
    fs::create_dir(source.join(".gitignore")).unwrap();
    let target = f.path("target");
    f.add(&source, &target, Some("custom"))
        .assert()
        .failure()
        .stderr(contains("new association removed"))
        .stderr(contains("cannot resolve control"));
    assert_eq!(f.count(), 0);
    assert!(!f.state.join("manifests/custom.json").exists());
    assert_eq!(
        fs::read_to_string(source.join("keep.txt")).unwrap(),
        "source"
    );
    assert!(target.read_dir().unwrap().next().is_none());
    fs::remove_dir(source.join(".gitignore")).unwrap();
    f.add(&source, &target, Some("custom")).assert().success();
}

#[test]
fn concurrent_adds_to_one_target_cannot_publish_two_associations() {
    let f = Fixture::new();
    let first = f.source("first");
    let second = f.source("second");
    let target = f.path("shared");
    let one = f
        .add(&first, &target, Some("one"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let two = f
        .add(&second, &target, Some("two"))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let results = [
        one.wait_with_output().unwrap().status.success(),
        two.wait_with_output().unwrap().status.success(),
    ];
    assert_eq!(results.iter().filter(|&&success| success).count(), 1);
    assert_eq!(f.count(), 1);
    assert_eq!(fs::read_to_string(first.join("keep.txt")).unwrap(), "first");
    assert_eq!(
        fs::read_to_string(second.join("keep.txt")).unwrap(),
        "second"
    );
    assert_eq!(
        fs::read_to_string(target.join("keep.txt")).unwrap(),
        if results[0] { "first" } else { "second" }
    );
}

#[test]
fn partially_copied_initial_sync_rolls_back_all_baselines_but_keeps_files() {
    let f = Fixture::new();
    f.cli().arg("list").assert().success();
    let conn = Connection::open(f.state.join("state.sqlite")).unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_initial_state BEFORE INSERT ON file_states
        WHEN NEW.relative_path = 'z.txt'
        BEGIN SELECT RAISE(FAIL, 'injected state write failure'); END;",
    )
    .unwrap();
    let source = f.source("source");
    fs::write(source.join("z.txt"), "last file").unwrap();
    let target = f.path("target");
    f.add(&source, &target, Some("custom"))
        .assert()
        .failure()
        .stderr(contains("new association removed"))
        .stderr(contains("injected state write failure"));
    assert_eq!(f.count(), 0);
    let states: i64 = conn
        .query_row("SELECT count(*) FROM file_states", [], |r| r.get(0))
        .unwrap();
    assert_eq!(states, 0);
    assert!(!f.state.join("manifests/custom.json").exists());
    for (name, contents) in [("keep.txt", "source"), ("z.txt", "last file")] {
        assert_eq!(fs::read_to_string(source.join(name)).unwrap(), contents);
        assert_eq!(fs::read_to_string(target.join(name)).unwrap(), contents);
    }
    f.add(&source, &target, Some("custom"))
        .assert()
        .failure()
        .stderr(contains("target directory is not empty"));
}

#[test]
fn empty_source_still_reserves_target_and_name_under_concurrent_adds() {
    for same_name in [false, true] {
        let f = Fixture::new();
        f.cli().arg("list").assert().success();
        let first = f.path("first");
        let second = f.path("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let target = f.path("target");
        let other = if same_name {
            f.path("other")
        } else {
            target.clone()
        };
        let one = f
            .add(&first, &target, Some("one"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let two = f
            .add(&second, &other, Some(if same_name { "one" } else { "two" }))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let successes = [one, two]
            .into_iter()
            .map(|mut child| child.wait().unwrap().success())
            .filter(|success| *success)
            .count();
        assert_eq!(successes, 1);
        assert_eq!(f.count(), 1);
    }
}

#[test]
fn permission_errors_fail_closed_and_unreadable_control_rolls_back() {
    use std::os::unix::fs::PermissionsExt;
    if Command::new("id").arg("-u").output().unwrap().stdout == b"0\n" {
        return;
    }
    for control in [false, true] {
        let f = Fixture::new();
        let source = f.source("source");
        let target = f.path("target");
        fs::create_dir(&target).unwrap();
        let protected = if control {
            let path = source.join(".gitignore");
            fs::write(&path, "*.log\n").unwrap();
            path
        } else {
            target.clone()
        };
        let permissions = fs::metadata(&protected).unwrap().permissions();
        fs::set_permissions(&protected, fs::Permissions::from_mode(0o0)).unwrap();
        let output = f.add(&source, &target, Some("custom")).output().unwrap();
        fs::set_permissions(&protected, permissions).unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Permission denied"));
        assert_eq!(f.count(), 0);
        assert!(!f.state.join("manifests/custom.json").exists());
        assert!(target.read_dir().unwrap().next().is_none());
        assert_eq!(
            fs::read_to_string(source.join("keep.txt")).unwrap(),
            "source"
        );
    }
}

#[test]
fn resolves_valid_symlink_ancestors_and_preserves_spaces_in_names() {
    let f = Fixture::new();
    let source = f.source("source notes");
    fs::create_dir_all(f.path("real/child")).unwrap();
    std::os::unix::fs::symlink(f.path("real/child"), f.path("alias")).unwrap();
    f.add(&source, &f.path("alias/../My Notes"), Some("work notes"))
        .assert()
        .success();
    assert!(f.path("real/My Notes/keep.txt").exists());
    assert!(!f.path("My Notes").exists());
    f.cli()
        .args(["sync", "work notes", "--dry-run"])
        .assert()
        .success();
}

#[test]
fn rejects_missing_arguments_long_names_and_invalid_path_types() {
    let f = Fixture::new();
    let source = f.source("source");
    f.add(&source, &f.path("target"), Some(&"n".repeat(251)))
        .assert()
        .failure()
        .stderr(contains("invalid item name"));
    f.add(&source, &f.path("target"), None)
        .arg("--name")
        .assert()
        .code(2);
    f.cli().arg("add").arg(&source).assert().code(2);
    fs::write(f.path("file"), "preserve").unwrap();
    for (src, target) in [
        (f.path("missing"), f.path("target")),
        (f.path("file"), f.path("target")),
        (source.clone(), f.path("file/child")),
    ] {
        f.add(&src, &target, None).assert().failure();
    }
    f.cli().arg("add").arg(&source).arg("").assert().failure();
    assert!(!f.path("target").exists());
    assert!(!f.state.exists());
    assert_eq!(fs::read_to_string(f.path("file")).unwrap(), "preserve");
}

#[test]
fn maximum_length_record_name_supports_manifest_and_normal_commands() {
    let f = Fixture::new();
    let source = f.source("source");
    let name = "n".repeat(250);
    f.add(&source, &f.path("target"), Some(&name))
        .assert()
        .success();
    assert!(f.state.join(format!("manifests/{name}.json")).exists());
    f.cli()
        .args(["sync", &name, "--dry-run"])
        .assert()
        .success();
    f.cli().args(["remove", &name]).assert().success();
    assert_eq!(f.count(), 0);
    assert!(source.join("keep.txt").exists());
    assert!(f.path("target/keep.txt").exists());
}
