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

    fn rule_path(&self, name: &str) -> PathBuf {
        self.app_support
            .join("rules")
            .join(format!("{name}.ignore"))
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
        .stdout(pred_contains("linker 0.2.0"));
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
            sandbox.target_parent.to_str().unwrap(),
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
    assert!(sandbox.rule_path("demo").exists());

    sandbox
        .linker()
        .arg("list")
        .assert()
        .success()
        .stdout(pred_contains("----------------------------------------"))
        .stdout(pred_contains("name: demo"))
        .stdout(pred_contains("directory"))
        .stdout(pred_contains("status: active"))
        .stdout(pred_contains("source:"))
        .stdout(pred_contains(source.to_str().unwrap()))
        .stdout(pred_contains("target:"))
        .stdout(pred_contains(sandbox.item_path("demo").to_str().unwrap()))
        .stdout(pred_contains("rules: 0"));

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
    assert!(!sandbox.rule_path("demo").exists());
}

#[test]
fn list_separates_multiple_items_and_emphasizes_names() {
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
            sandbox.target_parent.to_str().unwrap(),
        ])
        .assert()
        .success();
    sandbox
        .linker()
        .args([
            "add",
            beta.to_str().unwrap(),
            sandbox.target_parent.to_str().unwrap(),
        ])
        .assert()
        .success();

    sandbox
        .linker()
        .arg("list")
        .assert()
        .success()
        .stdout(pred_contains("----------------------------------------"))
        .stdout(pred_contains("name: alpha"))
        .stdout(pred_contains("name: beta"))
        .stdout(pred_contains("target:"));
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
            sandbox.target_parent.to_str().unwrap(),
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
    assert!(!sandbox.rule_path("demo").exists());
}

#[test]
fn add_defaults_to_empty_ignore_and_can_use_ignore_file_or_inline_excludes() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join(".env"), "secret");
    write_file(&source.join("node_modules/pkg/index.js"), "pkg");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.target_parent.to_str().unwrap(),
        ])
        .assert()
        .success();

    assert_eq!(read_file(&sandbox.item_path("demo").join(".env")), "secret");
    assert_eq!(
        read_file(&sandbox.item_path("demo").join("node_modules/pkg/index.js")),
        "pkg"
    );

    let second = sandbox.source_dir("second");
    write_file(&second.join("src/app.js"), "app");
    write_file(&second.join("dist/bundle.js"), "bundle");
    let ignore_file = sandbox.app_support.join("rules.ignore");
    write_file(&ignore_file, "dist/\ntmp/\n");

    sandbox
        .linker()
        .args([
            "add",
            second.to_str().unwrap(),
            sandbox.target_parent.to_str().unwrap(),
            "--ignore-file",
            ignore_file.to_str().unwrap(),
            "--exclude",
            "tmp/",
            "--exclude",
            "dist/",
        ])
        .assert()
        .success()
        .stdout(pred_contains("rules: 2"));

    assert!(sandbox.item_path("second").join("src/app.js").exists());
    assert!(!sandbox.item_path("second").join("dist/bundle.js").exists());
    assert_eq!(read_file(&sandbox.rule_path("second")), "dist/\ntmp/\n");
    assert!(
        !sqlite_table_info(&sandbox.app_support.join("state.sqlite"), "exclude_rules")
            .contains(&"source".to_string())
    );

    sandbox
        .linker()
        .args(["rule", "second", "list"])
        .assert()
        .success()
        .stdout(pred_contains("PATTERN"))
        .stdout(pred_contains("dist/"))
        .stdout(pred_contains("tmp/"));

    sandbox
        .linker()
        .args(["rule", "second", "include", "dist/"])
        .assert()
        .success()
        .stdout(pred_contains("rule included: dist/"))
        .stdout(pred_contains("rules: 1"));

    sandbox.linker().args(["sync", "second"]).assert().success();
    assert_eq!(
        read_file(&sandbox.item_path("second").join("dist/bundle.js")),
        "bundle"
    );
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
            sandbox.target_parent.to_str().unwrap(),
        ])
        .assert()
        .success();

    sandbox
        .linker()
        .args([
            "add",
            second.to_str().unwrap(),
            sandbox.target_parent.to_str().unwrap(),
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
fn rule_exclude_prunes_target_and_rule_include_restores_after_sync() {
    let sandbox = Sandbox::new();
    let source = sandbox.source_dir("demo");
    write_file(&source.join("src/app.js"), "app");
    write_file(&source.join("tmp/cache.txt"), "cache");

    sandbox
        .linker()
        .args([
            "add",
            source.to_str().unwrap(),
            sandbox.target_parent.to_str().unwrap(),
        ])
        .assert()
        .success();

    let item_dir = sandbox.item_path("demo");
    assert!(item_dir.join("tmp/cache.txt").exists());

    sandbox
        .linker()
        .args(["rule", "demo", "exclude", "tmp/"])
        .assert()
        .success()
        .stdout(pred_contains("rule excluded: tmp/"));

    assert!(source.join("tmp/cache.txt").exists());
    assert!(!item_dir.join("tmp/cache.txt").exists());
    assert_eq!(read_file(&sandbox.rule_path("demo")), "tmp/\n");

    sandbox
        .linker()
        .args(["rule", "demo", "exclude", "tmp/"])
        .assert()
        .success()
        .stdout(pred_contains("rules: 1"));
    assert_eq!(read_file(&sandbox.rule_path("demo")), "tmp/\n");

    sandbox
        .linker()
        .args(["rule", "demo", "list"])
        .assert()
        .success()
        .stdout(pred_contains("tmp/"));

    sandbox
        .linker()
        .args(["rule", "demo", "include", "tmp/"])
        .assert()
        .success()
        .stdout(pred_contains("rule included: tmp/"));

    sandbox.linker().args(["sync", "demo"]).assert().success();
    assert_eq!(read_file(&item_dir.join("tmp/cache.txt")), "cache");

    sandbox
        .linker()
        .args(["rule", "demo", "include", "missing/"])
        .assert()
        .success()
        .stdout(pred_contains("rule included: missing/"))
        .stdout(pred_contains("rules: 0"));
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
            sandbox.target_parent.to_str().unwrap(),
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
            sandbox.target_parent.to_str().unwrap(),
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
            sandbox.target_parent.to_str().unwrap(),
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
            sandbox.target_parent.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(pred_contains("path is not a directory"));

    sandbox
        .linker()
        .args(["rule", "missing", "exclude", ""])
        .assert()
        .failure()
        .stderr(pred_contains("invalid rule pattern"));

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
            "choose a target parent outside the source directory",
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
        .stdout(pred_contains("target parent directory"))
        .stdout(pred_contains("remove stops tracking"))
        .stdout(pred_contains("delete stops tracking"));

    sandbox
        .linker()
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("Source directory to sync"))
        .stdout(pred_contains(
            "Parent directory where the target directory will be created",
        ))
        .stdout(pred_contains("does not import .gitignore"))
        .stdout(pred_contains("--ignore-file"));

    sandbox
        .linker()
        .args(["rule", "demo", "exclude", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("prune matching target files"))
        .stdout(pred_contains("source files are kept"));

    sandbox
        .linker()
        .args(["rule", "demo", "include", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("Include a path again"))
        .stdout(pred_contains("If no existing rule matches"))
        .stdout(pred_contains("next linker sync"));
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
