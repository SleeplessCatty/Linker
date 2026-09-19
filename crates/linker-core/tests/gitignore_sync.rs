use filetime::{set_file_mtime, FileTime};
use linker_core::state::{Item, NewItem, StateDb};
use linker_core::sync::{preview_item, sync_item, SyncSummary};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

struct Fixture {
    _tmp: tempfile::TempDir,
    local: PathBuf,
    cloud: PathBuf,
    db_path: PathBuf,
    db: StateDb,
    item: Item,
}
impl Fixture {
    fn new() -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let local = tmp.path().join("source");
        let cloud = tmp.path().join("target");
        fs::create_dir(&local).unwrap();
        fs::create_dir(&cloud).unwrap();
        let db_path = tmp.path().join("support/state.sqlite");
        let mut db = StateDb::open(&db_path).unwrap();
        db.insert_item(NewItem {
            id: "demo-id",
            name: "demo",
            item_type: "directory",
            local_path: local.to_str().unwrap(),
            cloud_path: cloud.to_str().unwrap(),
        })
        .unwrap();
        let item = db.get_item("demo").unwrap();
        Self {
            _tmp: tmp,
            local,
            cloud,
            db_path,
            db,
            item,
        }
    }
    fn sync(&self) -> SyncSummary {
        sync_item(&self.db, &self.item).unwrap()
    }
    fn global_ignore(&self) -> PathBuf {
        self.db.global_ignore_path()
    }
}
fn write(path: &Path, contents: &str, time: i64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    set_file_mtime(path, FileTime::from_unix_time(time, 0)).unwrap();
}

#[test]
fn copies_preserve_permissions_in_both_directions() {
    let f = Fixture::new();
    write(&f.local.join("run"), "source", 100);
    fs::set_permissions(f.local.join("run"), fs::Permissions::from_mode(0o751)).unwrap();
    f.sync();
    assert_eq!(
        fs::metadata(f.cloud.join("run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o751
    );
    write(&f.cloud.join("run"), "new target", 200);
    fs::set_permissions(f.cloud.join("run"), fs::Permissions::from_mode(0o750)).unwrap();
    f.sync();
    assert_eq!(
        fs::metadata(f.local.join("run"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o750
    );
}

#[test]
fn ignored_paths_bypass_normal_type_conflicts() {
    for directory_rule in ["cache\n", "cache/\n"] {
        for target_link in [false, true] {
            let f = Fixture::new();
            write(&f.local.join(".gitignore"), directory_rule, 100);
            write(&f.local.join("cache/keep"), "source", 100);
            write(&f.local.join("normal"), "normal", 100);
            let outside = f._tmp.path().join("outside");
            write(&outside, "outside", 100);
            if target_link {
                symlink(&outside, f.cloud.join("cache")).unwrap();
            } else {
                write(&f.cloud.join("cache"), "different type", 100);
            }
            assert_eq!(f.sync().deleted_cloud, 1);
            assert_eq!(
                fs::read_to_string(f.local.join("cache/keep")).unwrap(),
                "source"
            );
            assert!(!f.cloud.join("cache").exists());
            assert!(f.cloud.join("normal").exists());
            assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");
        }
    }
}

#[test]
fn ignored_directory_replacement_retires_an_old_file_baseline() {
    let f = Fixture::new();
    write(&f.local.join("cache"), "old file", 100);
    f.sync();
    fs::remove_file(f.local.join("cache")).unwrap();
    write(&f.local.join("cache/keep"), "new directory", 200);
    write(&f.local.join(".gitignore"), "cache/\n", 200);
    assert_eq!(f.sync().deleted_cloud, 1);
    assert!(f.local.join("cache/keep").exists());
    assert!(!f
        .db
        .list_file_states(&f.item.id)
        .unwrap()
        .iter()
        .any(|s| s.relative_path == "cache"));
    write(&f.local.join(".gitignore"), "", 300);
    f.sync();
    assert!(f.cloud.join("cache/keep").exists());
}

#[test]
fn ordinary_bidirectional_sync_and_user_deletion_still_work() {
    let f = Fixture::new();
    write(&f.local.join("a"), "local", 100);
    write(&f.cloud.join("b"), "cloud", 100);
    let summary = f.sync();
    assert_eq!(summary.copied_local_to_cloud, 1);
    assert_eq!(summary.copied_cloud_to_local, 1);
    write(&f.cloud.join("a"), "new cloud", 200);
    f.sync();
    assert_eq!(fs::read_to_string(f.local.join("a")).unwrap(), "new cloud");
    fs::remove_file(f.cloud.join("a")).unwrap();
    assert_eq!(f.sync().deleted_local, 1);
    assert!(!f.local.join("a").exists());
    assert_eq!(f.sync().unchanged, 1);
}

#[test]
fn initial_prune_never_reads_ignored_source_content() {
    let f = Fixture::new();
    write(&f.local.join(".gitignore"), "secret/\n*.log\n", 100);
    write(&f.local.join("secret/inside"), "source", 100);
    fs::set_permissions(f.local.join("secret"), fs::Permissions::from_mode(0o000)).unwrap();
    write(&f.cloud.join("secret/inside"), "old cloud", 100);
    write(&f.cloud.join("only.log"), "cloud only", 100);
    write(&f.local.join("keep"), "normal", 100);
    let result = sync_item(&f.db, &f.item);
    fs::set_permissions(f.local.join("secret"), fs::Permissions::from_mode(0o700)).unwrap();
    let s = result.unwrap();
    assert_eq!(s.deleted_cloud, 2);
    assert_eq!(s.pruned_cloud_directories, 1);
    assert_eq!(
        fs::read_to_string(f.local.join("secret/inside")).unwrap(),
        "source"
    );
    assert!(!f.cloud.join("secret").exists());
    assert!(!f.local.join("only.log").exists());
    assert!(f.cloud.join("keep").exists());
}

#[test]
fn root_and_nested_rules_accumulate_without_git_repository() {
    let f = Fixture::new();
    write(&f.local.join(".gitignore"), "*.log\n", 100);
    write(&f.local.join("sub/.gitignore"), "/cache/\n*.tmp\n", 100);
    for p in [
        "a.log",
        "sub/a.log",
        "sub/cache/file",
        "sub/a.tmp",
        "other/a.tmp",
        "sub/deep/cache/keep",
    ] {
        write(&f.local.join(p), p, 100);
    }
    f.sync();
    for p in ["a.log", "sub/a.log", "sub/cache/file", "sub/a.tmp"] {
        assert!(!f.cloud.join(p).exists(), "{p}");
    }
    for p in ["other/a.tmp", "sub/deep/cache/keep", "sub/.gitignore"] {
        assert!(f.cloud.join(p).exists(), "{p}");
    }
}

#[test]
fn ignores_external_rules_and_keeps_unmatched_hidden_files() {
    let f = Fixture::new();
    write(&f._tmp.path().join(".gitignore"), "*\n", 100);
    write(&f.local.join(".ignore"), "*\n", 100);
    write(&f.local.join(".git/info/exclude"), "*\n", 100);
    write(&f.local.join(".hidden"), "keep", 100);
    f.sync();
    assert!(f.cloud.join(".hidden").exists());
    assert!(f.cloud.join(".git/info/exclude").exists());
}

#[test]
fn newest_control_rules_apply_before_same_pass_data_sync() {
    let f = Fixture::new();
    write(&f.local.join(".gitignore"), "*.old\n", 100);
    write(&f.cloud.join(".gitignore"), "*.log\n", 200);
    write(&f.local.join("a.log"), "local keep", 300);
    write(&f.cloud.join("a.log"), "delete even newer", 400);
    write(&f.local.join("a.old"), "now included", 100);
    f.sync();
    assert_eq!(
        fs::read_to_string(f.local.join(".gitignore")).unwrap(),
        "*.log\n"
    );
    assert!(!f.cloud.join("a.log").exists());
    assert!(f.local.join("a.log").exists());
    assert!(f.cloud.join("a.old").exists());
    // Equal timestamps favor the source.
    write(&f.local.join(".gitignore"), "*.old\n", 500);
    write(&f.cloud.join(".gitignore"), "*.log\n", 500);
    f.sync();
    assert_eq!(
        fs::read_to_string(f.cloud.join(".gitignore")).unwrap(),
        "*.old\n"
    );
    assert!(f.cloud.join("a.log").exists());
    assert!(!f.cloud.join("a.old").exists());
}

#[test]
fn removing_rules_restores_files_without_propagating_prune_deletions() {
    let f = Fixture::new();
    write(&f.local.join("cache/a"), "keep", 100);
    f.sync();
    write(&f.local.join(".gitignore"), "cache/\n", 200);
    f.sync();
    assert!(!f.cloud.join("cache").exists());
    assert!(!f
        .db
        .list_file_states(&f.item.id)
        .unwrap()
        .iter()
        .any(|s| s.relative_path == "cache/a"));
    f.sync();
    assert!(f.local.join("cache/a").exists());
    fs::remove_file(f.cloud.join(".gitignore")).unwrap();
    f.sync();
    assert!(!f.local.join(".gitignore").exists());
    assert_eq!(fs::read_to_string(f.cloud.join("cache/a")).unwrap(), "keep");
}

#[test]
fn control_files_are_exempt_but_controls_inside_ignored_dirs_are_not_read() {
    let f = Fixture::new();
    write(&f.local.join(".gitignore"), "*\n.gitignore\n", 100);
    write(&f.local.join("nested/.gitignore"), "**\n", 100);
    write(&f.cloud.join("nested/.gitignore"), "**\n", 100);
    let s = f.sync();
    assert!(s.warnings.is_empty());
    assert!(f.cloud.join(".gitignore").exists());
    assert!(f.local.join("nested/.gitignore").exists());
    assert!(!f.cloud.join("nested").exists());
}

#[test]
fn invalid_lines_warn_once_and_never_gain_special_meaning() {
    let f = Fixture::new();
    let contents = "*.log\n!keep.log\n*.py[cod]\n**/x\n";
    write(&f.local.join(".gitignore"), contents, 100);
    write(&f.cloud.join(".gitignore"), contents, 100);
    write(&f.local.join("keep.log"), "excluded", 100);
    write(&f.local.join("code.pyc"), "included", 100);
    let s = f.sync();
    assert_eq!(s.warnings.len(), 3);
    assert_eq!(s.warnings[0].line, 2);
    assert!(!f.cloud.join("keep.log").exists());
    assert!(f.cloud.join("code.pyc").exists());
    assert_eq!(f.sync().rules_fingerprint, s.rules_fingerprint);
}

#[test]
fn unreadable_or_nonfile_control_aborts_before_copy_and_prune() {
    let f = Fixture::new();
    write(&f.local.join(".gitignore"), "old\n", 100);
    write(&f.cloud.join("old"), "must survive failed preflight", 100);
    write(&f.local.join("sub/.gitignore"), "rule", 100);
    fs::set_permissions(
        f.local.join("sub/.gitignore"),
        fs::Permissions::from_mode(0o000),
    )
    .unwrap();
    let result = sync_item(&f.db, &f.item);
    fs::set_permissions(
        f.local.join("sub/.gitignore"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert!(result.is_err());
    assert!(f.cloud.join("old").exists());
    assert!(!f.cloud.join(".gitignore").exists());
    fs::remove_file(f.local.join("sub/.gitignore")).unwrap();
    fs::create_dir(f.local.join("sub/.gitignore")).unwrap();
    assert!(sync_item(&f.db, &f.item).is_err());
    assert!(f.cloud.join("old").exists());
    assert_eq!(f.db.get_item("demo").unwrap().status, "error");
}

#[test]
fn symlink_control_fails_closed_and_ignored_target_link_does_not_delete_outside() {
    let f = Fixture::new();
    let outside = f._tmp.path().join("outside");
    write(&outside.join("sentinel"), "safe", 100);
    write(&outside.join("rules"), "*\n", 100);
    symlink(outside.join("rules"), f.local.join(".gitignore")).unwrap();
    assert!(sync_item(&f.db, &f.item).is_err());
    assert!(f.cloud.read_dir().unwrap().next().is_none());
    fs::remove_file(f.local.join(".gitignore")).unwrap();
    write(&f.local.join(".gitignore"), "cache\n", 100);
    symlink(&outside, f.cloud.join("cache")).unwrap();
    assert_eq!(f.sync().deleted_cloud, 1);
    assert_eq!(
        fs::read_to_string(outside.join("sentinel")).unwrap(),
        "safe"
    );
    assert!(!f.cloud.join("cache").exists());
}

#[test]
fn preview_is_read_only_and_matches_actual_changes() {
    let f = Fixture::new();
    write(&f.local.join(".gitignore"), "*.log\n", 100);
    write(&f.cloud.join("old.log"), "old", 100);
    let p = preview_item(&f.db, &f.item).unwrap();
    assert_eq!(p.operations.len(), 2);
    assert!(f.cloud.join("old.log").exists());
    assert!(!f.cloud.join(".gitignore").exists());
    assert!(f.db.list_file_states(&f.item.id).unwrap().is_empty());
    let s = f.sync();
    assert_eq!(s.deleted_cloud, 1);
    assert_eq!(s.copied_local_to_cloud, 1);
    assert!(preview_item(&f.db, &f.item).unwrap().operations.is_empty());
}

#[test]
fn item_lock_serializes_independent_database_connections() {
    use std::sync::mpsc;
    use std::time::Duration;
    let f = Fixture::new();
    write(&f.local.join("a"), "one", 100);
    let guard = f.db.lock_item(&f.item.id).unwrap();
    let path = f.db_path.clone();
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        let db = StateDb::open(&path).unwrap();
        let item = db.get_item("demo").unwrap();
        tx.send("ready").unwrap();
        sync_item(&db, &item).unwrap();
        tx.send("done").unwrap();
    });
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "ready");
    assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
    assert!(!f.cloud.join("a").exists());
    drop(guard);
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "done");
    handle.join().unwrap();
    assert!(f.cloud.join("a").exists());
}

#[test]
fn shared_source_lock_blocks_other_item_sync_and_preview_but_not_unrelated_sources() {
    use std::sync::mpsc;
    use std::time::Duration;
    for preview in [false, true] {
        let mut f = Fixture::new();
        let second_target = f._tmp.path().join("second");
        fs::create_dir(&second_target).unwrap();
        f.db.insert_item(NewItem {
            id: "second",
            name: "second",
            item_type: "directory",
            local_path: f.local.to_str().unwrap(),
            cloud_path: second_target.to_str().unwrap(),
        })
        .unwrap();
        write(&f.local.join("a"), "one", 100);
        let guard = f.db.lock_source(&f.item.local_path).unwrap();
        let path = f.db_path.clone();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let db = StateDb::open(&path).unwrap();
            let item = db.get_item("second").unwrap();
            tx.send("ready").unwrap();
            if preview {
                preview_item(&db, &item).unwrap();
            } else {
                sync_item(&db, &item).unwrap();
            }
            tx.send("done").unwrap();
        });
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "ready");
        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
        assert!(!second_target.join("a").exists());
        let unrelated_local = f._tmp.path().join("unrelated-source");
        let unrelated_cloud = f._tmp.path().join("unrelated-target");
        write(&unrelated_local.join("b"), "independent", 100);
        fs::create_dir(&unrelated_cloud).unwrap();
        f.db.insert_item(NewItem {
            id: "unrelated",
            name: "unrelated",
            item_type: "directory",
            local_path: unrelated_local.to_str().unwrap(),
            cloud_path: unrelated_cloud.to_str().unwrap(),
        })
        .unwrap();
        sync_item(&f.db, &f.db.get_item("unrelated").unwrap()).unwrap();
        assert!(unrelated_cloud.join("b").exists());
        drop(guard);
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "done");
        handle.join().unwrap();
        assert_eq!(second_target.join("a").exists(), !preview);
    }
}

#[test]
fn existing_single_file_associations_continue_syncing() {
    let tmp = tempfile::tempdir().unwrap();
    let source = tmp.path().join("source/note");
    let target = tmp.path().join("target/note");
    write(&source, "hello", 100);
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let mut db = StateDb::open(&tmp.path().join("support/state.sqlite")).unwrap();
    db.insert_item(NewItem {
        id: "file",
        name: "note",
        item_type: "file",
        local_path: source.to_str().unwrap(),
        cloud_path: target.to_str().unwrap(),
    })
    .unwrap();
    let item = db.get_item("note").unwrap();
    sync_item(&db, &item).unwrap();
    assert_eq!(fs::read_to_string(target).unwrap(), "hello");
}

#[test]
fn global_rules_apply_to_the_whole_association_and_stay_additive() {
    let f = Fixture::new();
    write(&f.local.join("keep.txt"), "keep", 100);
    write(&f.local.join("notes.md"), "notes", 100);
    write(&f.local.join(".DS_Store"), "source finder", 100);
    write(&f.local.join("cache/a"), "cached", 100);
    // A pre-existing target copy of ignored content is cleaned up.
    write(&f.cloud.join(".DS_Store"), "target finder", 100);
    write(&f.cloud.join("cache/old"), "old", 100);
    write(&f.local.join(".gitignore"), "notes.md\n", 100);
    write(
        &f.global_ignore(),
        "# Linker-wide rules\n.DS_Store\ncache/\n",
        100,
    );

    f.sync();

    // Global and in-tree rules are both effective.
    assert!(!f.cloud.join(".DS_Store").exists());
    assert!(!f.cloud.join("cache").exists());
    assert!(!f.cloud.join("notes.md").exists());
    assert_eq!(
        fs::read_to_string(f.cloud.join("keep.txt")).unwrap(),
        "keep"
    );
    assert_eq!(
        fs::read_to_string(f.cloud.join(".gitignore")).unwrap(),
        "notes.md\n"
    );
    // Ignored source content is retained, including inside ignored directories.
    assert_eq!(
        fs::read_to_string(f.local.join(".DS_Store")).unwrap(),
        "source finder"
    );
    assert_eq!(
        fs::read_to_string(f.local.join("cache/a")).unwrap(),
        "cached"
    );
    assert_eq!(
        fs::read_to_string(f.local.join("notes.md")).unwrap(),
        "notes"
    );
}

#[test]
fn unsupported_global_rule_lines_warn_with_the_global_file_path() {
    let f = Fixture::new();
    write(&f.local.join("a.pyc"), "byte", 100);
    write(&f.global_ignore(), "*.py[cod]\n", 100);

    let summary = f.sync();

    assert_eq!(summary.warnings.len(), 1);
    assert_eq!(summary.warnings[0].file, f.global_ignore());
    assert_eq!(summary.warnings[0].line, 1);
    // The unsupported line is skipped, so the file keeps synchronizing.
    assert_eq!(fs::read_to_string(f.cloud.join("a.pyc")).unwrap(), "byte");
}

#[test]
fn removing_a_global_rule_restores_normal_sync_after_target_cleanup() {
    let f = Fixture::new();
    write(&f.local.join(".DS_Store"), "finder", 100);
    write(&f.global_ignore(), ".DS_Store\n", 100);

    f.sync();
    assert!(!f.cloud.join(".DS_Store").exists());

    fs::remove_file(f.global_ignore()).unwrap();
    f.sync();
    assert_eq!(
        fs::read_to_string(f.cloud.join(".DS_Store")).unwrap(),
        "finder"
    );
    assert!(f.local.join(".DS_Store").exists());
}

#[test]
fn the_global_ignore_file_itself_is_never_synchronized() {
    let f = Fixture::new();
    write(&f.local.join("keep.txt"), "keep", 100);
    write(&f.global_ignore(), "orphan.txt\n", 100);

    f.sync();

    assert!(!f.local.join("global.gitignore").exists());
    assert!(!f.cloud.join("global.gitignore").exists());
    assert_eq!(
        fs::read_to_string(f.cloud.join("keep.txt")).unwrap(),
        "keep"
    );
}
