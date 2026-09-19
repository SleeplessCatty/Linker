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
    for (source, destination) in [
        (first.clone(), f.path("another")),
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
