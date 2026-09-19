use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

struct Sandbox {
    _tmp: TempDir,
    app_support: PathBuf,
    sources: PathBuf,
    target_parent: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app_support = tmp.path().join("app-support");
        let sources = tmp.path().join("sources");
        let target_parent = tmp.path().join("target-parent");

        fs::create_dir_all(&app_support).expect("app support");
        fs::create_dir_all(&sources).expect("sources");
        fs::create_dir_all(&target_parent).expect("target parent");

        Self {
            _tmp: tmp,
            app_support,
            sources,
            target_parent,
        }
    }

    fn linker(&self) -> Command {
        let mut cmd = Command::cargo_bin("linker").expect("linker bin");
        cmd.env("LINKER_APP_SUPPORT_DIR", &self.app_support);
        cmd.env("HOME", self._tmp.path());
        cmd
    }

    fn source_dir(&self, name: &str) -> PathBuf {
        let path = self.sources.join(name);
        fs::create_dir_all(&path).expect("source dir");
        path
    }

    fn item_path(&self, name: &str) -> PathBuf {
        self.target_parent.join(name)
    }

    fn manifest_path(&self, name: &str) -> PathBuf {
        self.app_support
            .join("manifests")
            .join(format!("{name}.json"))
    }
}

#[test]
fn reports_linker_name_and_version() {
    let sandbox = Sandbox::new();

    sandbox
        .linker()
        .arg("--version")
        .assert()
        .success()
        .stdout(pred_contains("linker 0.3.0"));
}

#[test]
fn add_list_status_and_remove_item_keep_source_and_target() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("README.md"), "hello");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(pred_contains("added: demo"))
        .stdout(pred_contains("type: directory"))
        .stdout(pred_contains("source:"))
        .stdout(pred_contains("target:"))
        .stdout(pred_contains("initial sync source -> target: 1"));

    let target_readme = sandbox.item_path("demo").join("README.md");
    assert_eq!(read_file(&target_readme), "hello");
    assert!(sandbox.manifest_path("demo").exists());
    assert!(!sandbox.app_support.join("rules").exists());

    sandbox
        .linker()
        .arg("list")
        .assert()
        .success()
        .stdout(pred_contains("| NAME"))
        .stdout(pred_contains("| demo"))
        .stdout(pred_contains("directory"))
        .stdout(pred_contains("active"))
        .stdout(pred_contains("SOURCE"))
        .stdout(pred_contains(source.to_str().unwrap()))
        .stdout(pred_contains("TARGET"))
        .stdout(pred_contains(sandbox.item_path("demo").to_str().unwrap()))
        .stdout(predicates::str::contains("rules:").not().from_utf8());

    sandbox
        .linker()
        .arg("status")
        .assert()
        .success()
        .stdout(pred_contains("daemon installed:"))
        .stdout(pred_contains("daemon running:"))
        .stdout(predicates::str::contains("source:").not().from_utf8())
        .stdout(predicates::str::contains("target:").not().from_utf8());

    sandbox
        .linker()
        .args(["remove", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("source kept"))
        .stdout(pred_contains("target kept"))
        .stdout(pred_contains("local metadata deleted"));

    assert!(source.join("README.md").exists());
    assert!(target_readme.exists());
    assert!(!sandbox.manifest_path("demo").exists());
    assert!(!sandbox.app_support.join("rules").exists());
}

#[test]
fn list_displays_multiple_items_in_a_single_table() {
    let sandbox = Sandbox::new();
    let alpha = sandbox.source_dir("alpha");
    let beta = sandbox.source_dir("beta");
    write_file(&alpha.join("a.txt"), "a");
    write_file(&beta.join("b.txt"), "b");

    sandbox
        .linker()
        .args([
            "add",
            alpha.to_str().unwrap(),
            sandbox.item_path("alpha").to_str().unwrap(),
        ])
        .assert()
        .success();
    sandbox
        .linker()
        .args([
            "add",
            beta.to_str().unwrap(),
            sandbox.item_path("beta").to_str().unwrap(),
        ])
        .assert()
        .success();

    let output = sandbox.linker().arg("list").output().unwrap();
    assert!(output.status.success());
    let table = String::from_utf8(output.stdout).unwrap();
    assert_eq!(table.matches("| NAME").count(), 1);
    assert!(table.contains("| alpha"));
    assert!(table.contains("| beta"));
    assert!(table.find("| alpha").unwrap() < table.find("| beta").unwrap());
    for heading in [
        "TYPE",
        "STATUS",
        "SOURCE",
        "TARGET",
        "LAST SYNC (UTC)",
        "LAST ERROR",
    ] {
        assert!(table.contains(heading));
    }
}

#[test]
fn list_empty_state_is_explicit() {
    Sandbox::new()
        .linker()
        .arg("list")
        .assert()
        .success()
        .stdout("no items\n");
}

#[test]
fn dry_run_shows_effective_rule_cleanup_without_changing_files_or_state() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("cache.log"), "keep source");
    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();
    write_file(&source.join(".gitignore"), "*.log\n!cache.log\n");
    write_file(&sandbox.item_path("demo").join("target-only.log"), "cloud");
    write_file(&source.join("new.txt"), "new");
    let db_path = sandbox.app_support.join("state.sqlite");
    let before = fs::read(&db_path).unwrap();
    let manifest = fs::read(sandbox.manifest_path("demo")).unwrap();
    for args in [vec!["sync", "--dry-run"], vec!["sync", "demo", "--dry-run"]] {
        sandbox
            .linker()
            .args(args)
            .assert()
            .success()
            .stdout(pred_contains("dry run: demo"))
            .stdout(pred_contains("write_target"))
            .stdout(pred_contains("prune_target_file"))
            .stdout(pred_contains("target-only.log"))
            .stderr(pred_contains(".gitignore:2: skipped"));
        assert!(sandbox.item_path("demo").join("cache.log").exists());
        assert!(sandbox.item_path("demo").join("target-only.log").exists());
        assert!(!sandbox.item_path("demo").join(".gitignore").exists());
        assert!(!sandbox.item_path("demo").join("new.txt").exists());
        assert_eq!(fs::read(&db_path).unwrap(), before);
        assert_eq!(fs::read(sandbox.manifest_path("demo")).unwrap(), manifest);
    }
    sandbox.linker().args(["sync", "demo"]).assert().success();
    assert!(!sandbox.item_path("demo").join("cache.log").exists());
    assert_eq!(read_file(&source.join("cache.log")), "keep source");
    sandbox
        .linker()
        .args(["sync", "demo", "--dry-run"])
        .assert()
        .success()
        .stdout(pred_contains("no changes"));
}

#[test]
fn dry_run_reports_both_deletion_directions_and_target_to_source_copy() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    for name in ["deleted-source.txt", "deleted-target.txt"] {
        write_file(&source.join(name), "original");
    }
    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();
    fs::remove_file(source.join("deleted-source.txt")).unwrap();
    fs::remove_file(sandbox.item_path("demo").join("deleted-target.txt")).unwrap();
    write_file(
        &sandbox.item_path("demo").join("cloud-only.txt"),
        "new target",
    );
    let before = fs::read(sandbox.app_support.join("state.sqlite")).unwrap();
    sandbox
        .linker()
        .args(["sync", "demo", "--dry-run"])
        .assert()
        .success()
        .stdout(pred_contains("delete_source"))
        .stdout(pred_contains("delete_target"))
        .stdout(pred_contains("write_source"));
    assert!(source.join("deleted-target.txt").exists());
    assert!(sandbox
        .item_path("demo")
        .join("deleted-source.txt")
        .exists());
    assert!(!source.join("cloud-only.txt").exists());
    assert_eq!(
        fs::read(sandbox.app_support.join("state.sqlite")).unwrap(),
        before
    );
}

#[test]
fn dry_run_missing_target_fails_without_creating_it() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();
    fs::remove_dir(sandbox.item_path("demo")).unwrap();
    let before = fs::read(sandbox.app_support.join("state.sqlite")).unwrap();
    sandbox
        .linker()
        .args(["sync", "demo", "--dry-run"])
        .assert()
        .failure();
    assert!(!sandbox.item_path("demo").exists());
    assert_eq!(
        fs::read(sandbox.app_support.join("state.sqlite")).unwrap(),
        before
    );
}

#[test]
fn dry_run_without_state_does_not_initialize_application_storage() {
    let sandbox = Sandbox::new();
    fs::remove_dir(&sandbox.app_support).unwrap();
    sandbox
        .linker()
        .args(["sync", "--dry-run"])
        .assert()
        .success()
        .stdout("no items\n");
    sandbox
        .linker()
        .args(["sync", "missing", "--dry-run"])
        .assert()
        .failure()
        .stderr(pred_contains("item was not found"));
    assert!(!sandbox.app_support.exists());
}

#[test]
fn dry_run_control_error_does_not_update_last_error_or_copy_files() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();
    fs::create_dir(source.join(".gitignore")).unwrap();
    write_file(&source.join("new.txt"), "new");
    let before = fs::read(sandbox.app_support.join("state.sqlite")).unwrap();
    sandbox
        .linker()
        .args(["sync", "demo", "--dry-run"])
        .assert()
        .failure()
        .stderr(pred_contains("cannot resolve control"));
    assert_eq!(
        fs::read(sandbox.app_support.join("state.sqlite")).unwrap(),
        before
    );
    assert!(!sandbox.item_path("demo").join("new.txt").exists());
}

#[test]
fn delete_removes_target_but_keeps_source() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("README.md"), "hello");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();

    sandbox
        .linker()
        .args(["delete", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("source kept"))
        .stdout(pred_contains("target deleted"))
        .stdout(pred_contains("local metadata deleted"));

    assert!(source.join("README.md").exists());
    assert!(!sandbox.item_path("demo").exists());
    assert!(!sandbox.manifest_path("demo").exists());
    assert!(!sandbox.app_support.join("rules").exists());
}

#[test]
fn only_gitignore_controls_initial_sync() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join(".gitignore"), "*.log\ncache/\n!keep.log\n");
    write_file(
        &source.join("keep.log"),
        "ignored despite unsupported negation",
    );
    write_file(&source.join("cache/data"), "source cache");
    write_file(&source.join(".env"), "included unless explicitly ignored");
    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success()
        .stderr(pred_contains(".gitignore:3: skipped"))
        .stdout(pred_contains("initial sync deleted target: 0"));
    write_file(&sandbox.item_path("demo").join("target.log"), "target only");
    sandbox
        .linker()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("deleted target: 1"));
    assert!(source.join("keep.log").exists());
    assert!(!sandbox.item_path("demo").join("keep.log").exists());
    assert!(!sandbox.item_path("demo").join("cache").exists());
    assert!(!sandbox.item_path("demo").join("target.log").exists());
    assert!(sandbox.item_path("demo").join(".env").exists());
    assert!(sandbox.item_path("demo").join(".gitignore").exists());
    assert!(!sandbox.app_support.join("rules").exists());
    assert!(
        !sqlite_table_info(&sandbox.app_support.join("state.sqlite"), "items")
            .contains(&"rule_path".into())
    );
    assert!(
        sqlite_table_info(&sandbox.app_support.join("state.sqlite"), "exclude_rules").is_empty()
    );
    let manifest: serde_json::Value =
        serde_json::from_str(&read_file(&sandbox.manifest_path("demo"))).unwrap();
    assert_eq!(manifest["schema_version"], 2);
    assert!(manifest.get("rule_path").is_none());
}

#[test]
fn rejects_retired_rule_interfaces() {
    let sandbox = Sandbox::new();
    sandbox
        .linker()
        .args(["rule", "demo", "list"])
        .assert()
        .failure();
    for flag in ["--exclude", "--ignore-file"] {
        sandbox
            .linker()
            .args(["add", "/source", "/target", flag, "rules"])
            .assert()
            .failure()
            .stderr(pred_contains("unexpected argument"));
    }
}

#[test]
fn duplicate_directory_name_is_rejected() {
    let sandbox = Sandbox::new();
    let first_parent = sandbox.sources.join("first");
    let second_parent = sandbox.sources.join("second");
    let first = first_parent.join("demo");
    let second = second_parent.join("demo");
    fs::create_dir_all(&first).expect("first");
    fs::create_dir_all(&second).expect("second");
    write_file(&first.join("README.md"), "hello");

    sandbox
        .linker()
        .args([
            "add",
            first.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();

    sandbox
        .linker()
        .args([
            "add",
            second.to_str().unwrap(),
            sandbox.item_path("another-demo").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(pred_contains("item already exists"));
}

#[test]
fn nested_or_same_source_and_target_is_rejected() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.sources.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(pred_contains("invalid sync association"));
}

#[test]
fn editing_gitignore_prunes_target_and_removing_it_restores_source_content() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("tmp/cache.txt"), "cache");
    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();
    write_file(&source.join(".gitignore"), "tmp/\n");
    sandbox
        .linker()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("deleted target: 1"));
    assert_eq!(read_file(&source.join("tmp/cache.txt")), "cache");
    assert!(!sandbox.item_path("demo").join("tmp").exists());
    fs::remove_file(source.join(".gitignore")).unwrap();
    sandbox.linker().args(["sync", "demo"]).assert().success();
    assert_eq!(
        read_file(&sandbox.item_path("demo").join("tmp/cache.txt")),
        "cache"
    );
    assert!(!sandbox.item_path("demo").join(".gitignore").exists());
}

#[test]
fn sync_copies_both_directions_and_latest_modified_wins() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("local.txt"), "local");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();

    let item_dir = sandbox.item_path("demo");

    write_file(&source.join("second.txt"), "from source");
    sandbox
        .linker()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("source -> target: 1"));
    assert_eq!(read_file(&item_dir.join("second.txt")), "from source");

    write_file(&item_dir.join("target.txt"), "from target");
    sandbox
        .linker()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("target -> source: 1"));
    assert_eq!(read_file(&source.join("target.txt")), "from target");

    write_file(&source.join("winner.txt"), "old source");
    sandbox.linker().args(["sync", "demo"]).assert().success();

    write_file(&source.join("winner.txt"), "older source edit");
    write_file(&item_dir.join("winner.txt"), "newer target edit");
    set_mtime(&source.join("winner.txt"), 100);
    set_mtime(&item_dir.join("winner.txt"), 200);

    sandbox.linker().args(["sync", "demo"]).assert().success();
    assert_eq!(read_file(&source.join("winner.txt")), "newer target edit");
}

#[test]
fn sync_deletes_inner_file_from_other_side() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("src/a.txt"), "a");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .success();

    let item_dir = sandbox.item_path("demo");
    assert!(item_dir.join("src/a.txt").exists());

    fs::remove_file(source.join("src/a.txt")).expect("remove source");
    sandbox
        .linker()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("deleted target: 1"));
    assert!(!item_dir.join("src/a.txt").exists());
}

#[test]
fn doctor_reports_isolated_environment_health() {
    let sandbox = Sandbox::new();

    sandbox
        .linker()
        .arg("doctor")
        .assert()
        .success()
        .stdout(pred_contains("[ok] Application Support"))
        .stdout(pred_contains("[ok] State Database"))
        .stdout(pred_contains("[ok] Sync Associations"));
}

#[test]
fn errors_include_actionable_hints() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");

    sandbox
        .linker()
        .args(["sync", "missing"])
        .assert()
        .failure()
        .stderr(pred_contains("run `linker list`"));

    sandbox
        .linker()
        .args([
            "add",
            sandbox.sources.join("nope").to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(pred_contains("check the path and try again"));

    let file = sandbox.sources.join("file.md");
    write_file(&file, "file");
    sandbox
        .linker()
        .args([
            "add",
            file.to_str().unwrap(),
            sandbox.item_path("demo").to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(pred_contains("path is not a directory"));

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.sources.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(pred_contains(
            "choose separate source and target directories",
        ));
}

#[test]
fn help_describes_core_commands_and_rule_behavior() {
    let sandbox = Sandbox::new();

    let no_args = sandbox.linker().output().expect("linker without args");
    let with_help = sandbox
        .linker()
        .arg("--help")
        .output()
        .expect("linker --help");
    assert!(no_args.status.success());
    assert!(with_help.status.success());
    assert_eq!(no_args.stdout, with_help.stdout);
    assert!(no_args.stderr.is_empty());
    assert!(with_help.stderr.is_empty());

    sandbox
        .linker()
        .assert()
        .success()
        .stdout(pred_contains("Usage: linker <COMMAND>"))
        .stdout(pred_contains("Common workflow:"))
        .stdout(predicates::str::contains("linker init").not().from_utf8());

    sandbox
        .linker()
        .arg("--help")
        .assert()
        .success()
        .stdout(pred_contains("target directory"))
        .stdout(pred_contains("remove stops tracking"))
        .stdout(pred_contains("delete stops tracking"));

    sandbox
        .linker()
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("Source directory to sync"))
        .stdout(pred_contains(
            "Exact target directory; must be absent or empty",
        ))
        .stdout(pred_contains("--name"))
        .stdout(pred_contains("Only .gitignore"))
        .stdout(pred_contains("matching target files are deleted"))
        .stdout(predicates::str::contains("--ignore-file").not().from_utf8())
        .stdout(predicates::str::contains("--exclude").not().from_utf8());

    sandbox
        .linker()
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("table"))
        .stdout(pred_contains("UTC"));
    sandbox
        .linker()
        .args(["sync", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("--dry-run"))
        .stdout(pred_contains("without applying"));
}

fn pred_contains(text: &str) -> impl Predicate<[u8]> {
    predicates::str::contains(text).from_utf8()
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("parent");
    }
    fs::write(path, contents).expect("write");
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|_| panic!("read file: {}", path.display()))
}

fn set_mtime(path: &Path, mtime: i64) {
    filetime::set_file_mtime(path, filetime::FileTime::from_unix_time(mtime, 0)).expect("mtime");
}

fn sqlite_table_info(path: &Path, table: &str) -> Vec<String> {
    let conn = rusqlite::Connection::open(path).expect("open sqlite");
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .expect("table info");
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .expect("columns");
    rows.collect::<Result<Vec<_>, _>>().expect("column names")
}
