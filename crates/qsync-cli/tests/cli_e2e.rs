use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

struct Sandbox {
    _tmp: TempDir,
    app_support: PathBuf,
    icloud: PathBuf,
    local: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app_support = tmp.path().join("app-support");
        let icloud = tmp.path().join("icloud");
        let local = tmp.path().join("local");

        fs::create_dir_all(&app_support).expect("app support");
        fs::create_dir_all(&icloud).expect("icloud");
        fs::create_dir_all(&local).expect("local");

        Self {
            _tmp: tmp,
            app_support,
            icloud,
            local,
        }
    }

    fn qsync(&self) -> Command {
        let mut cmd = Command::cargo_bin("qsync").expect("qsync bin");
        cmd.env("QUICKSYNC_APP_SUPPORT_DIR", &self.app_support)
            .env("QUICKSYNC_ICLOUD_DIR", &self.icloud);
        cmd
    }

    fn quicksync_dir(&self) -> PathBuf {
        self.icloud.join("QuickSync")
    }

    fn item_path(&self, name: &str) -> PathBuf {
        self.quicksync_dir().join(name)
    }

    fn manifest_path(&self, name: &str) -> PathBuf {
        self.quicksync_dir()
            .join(".quicksync/manifests")
            .join(format!("{name}.json"))
    }

    fn rule_path(&self, name: &str) -> PathBuf {
        self.quicksync_dir()
            .join(".quicksync/rules")
            .join(format!("{name}.ignore"))
    }
}

#[test]
fn add_list_status_and_remove_item_keep_cloud_content_and_metadata() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join("README.md"), "hello");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("added: demo"))
        .stdout(pred_contains("type: directory"))
        .stdout(pred_contains("initial sync local -> cloud: 1"));

    let cloud_readme = sandbox.item_path("demo").join("README.md");
    assert_eq!(read_file(&cloud_readme), "hello");
    assert!(sandbox.manifest_path("demo").exists());
    assert!(sandbox.rule_path("demo").exists());
    assert!(!sandbox.quicksync_dir().join("Items").exists());

    sandbox
        .qsync()
        .arg("list")
        .assert()
        .success()
        .stdout(pred_contains("demo"))
        .stdout(pred_contains("directory"))
        .stdout(pred_contains(sandbox.local.to_str().unwrap()));

    sandbox
        .qsync()
        .args(["status", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("daemon installed:"))
        .stdout(pred_contains("daemon running:"))
        .stdout(pred_contains("type: directory"))
        .stdout(pred_contains("status: active"))
        .stdout(pred_contains("rules: 0"));

    sandbox
        .qsync()
        .args(["remove", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("local files kept"))
        .stdout(pred_contains("cloud mirror kept"))
        .stdout(pred_contains("hidden metadata kept"));

    assert!(sandbox.local.join("README.md").exists());
    assert!(cloud_readme.exists());
    assert!(sandbox.manifest_path("demo").exists());
    assert!(sandbox.rule_path("demo").exists());
}

#[test]
fn delete_removes_cloud_content_and_metadata_but_keeps_local_files() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join("README.md"), "hello");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success();

    sandbox
        .qsync()
        .args(["delete", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("local files kept"))
        .stdout(pred_contains("cloud files deleted"))
        .stdout(pred_contains("hidden metadata deleted"));

    assert!(sandbox.local.join("README.md").exists());
    assert!(!sandbox.item_path("demo").exists());
    assert!(!sandbox.manifest_path("demo").exists());
    assert!(!sandbox.rule_path("demo").exists());
}

#[test]
fn add_defaults_to_empty_ignore_and_can_use_ignore_file_or_inline_excludes() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join(".env"), "secret");
    write_file(&sandbox.local.join("node_modules/pkg/index.js"), "pkg");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success();

    assert_eq!(read_file(&sandbox.item_path("demo").join(".env")), "secret");
    assert_eq!(
        read_file(&sandbox.item_path("demo").join("node_modules/pkg/index.js")),
        "pkg"
    );

    let second = sandbox.local.with_file_name("second");
    fs::create_dir_all(&second).expect("second dir");
    write_file(&second.join("src/app.js"), "app");
    write_file(&second.join("dist/bundle.js"), "bundle");
    let ignore_file = sandbox.app_support.join("rules.ignore");
    write_file(&ignore_file, "dist/\ntmp/\n");

    sandbox
        .qsync()
        .args([
            "add",
            second.to_str().unwrap(),
            "--name",
            "second",
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
        .qsync()
        .args(["rule", "second", "list"])
        .assert()
        .success()
        .stdout(pred_contains("PATTERN"))
        .stdout(pred_contains("dist/"))
        .stdout(pred_contains("tmp/"));

    sandbox
        .qsync()
        .args(["rule", "second", "include", "dist/"])
        .assert()
        .success()
        .stdout(pred_contains("rule included: dist/"))
        .stdout(pred_contains("rules: 1"));

    sandbox.qsync().args(["sync", "second"]).assert().success();
    assert_eq!(
        read_file(&sandbox.item_path("second").join("dist/bundle.js")),
        "bundle"
    );
}

#[test]
fn duplicate_visible_name_is_rejected() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join("README.md"), "hello");
    let other = sandbox.local.with_file_name("other");
    fs::create_dir_all(&other).expect("other dir");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success();

    sandbox
        .qsync()
        .args(["add", other.to_str().unwrap(), "--name", "demo"])
        .assert()
        .failure()
        .stderr(pred_contains("item already exists"));
}

#[test]
fn rule_exclude_prunes_cloud_and_rule_include_restores_after_sync() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join("src/app.js"), "app");
    write_file(&sandbox.local.join("tmp/cache.txt"), "cache");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success();

    let item_dir = sandbox.item_path("demo");
    assert!(item_dir.join("tmp/cache.txt").exists());

    sandbox
        .qsync()
        .args(["rule", "demo", "exclude", "tmp/"])
        .assert()
        .success()
        .stdout(pred_contains("rule excluded: tmp/"));

    assert!(sandbox.local.join("tmp/cache.txt").exists());
    assert!(!item_dir.join("tmp/cache.txt").exists());
    assert_eq!(read_file(&sandbox.rule_path("demo")), "tmp/\n");

    sandbox
        .qsync()
        .args(["rule", "demo", "exclude", "tmp/"])
        .assert()
        .success()
        .stdout(pred_contains("rules: 1"));
    assert_eq!(read_file(&sandbox.rule_path("demo")), "tmp/\n");

    sandbox
        .qsync()
        .args(["rule", "demo", "list"])
        .assert()
        .success()
        .stdout(pred_contains("tmp/"));

    sandbox
        .qsync()
        .args(["rule", "demo", "include", "tmp/"])
        .assert()
        .success()
        .stdout(pred_contains("rule included: tmp/"));

    sandbox.qsync().args(["sync", "demo"]).assert().success();
    assert_eq!(read_file(&item_dir.join("tmp/cache.txt")), "cache");

    sandbox
        .qsync()
        .args(["rule", "demo", "include", "missing/"])
        .assert()
        .success()
        .stdout(pred_contains("rule included: missing/"))
        .stdout(pred_contains("rules: 0"));
}

#[test]
fn single_file_item_syncs_directly_under_quicksync() {
    let sandbox = Sandbox::new();
    let local_file = sandbox.local.join("file.md");
    write_file(&local_file, "local file");

    sandbox
        .qsync()
        .args(["add", local_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(pred_contains("type: file"));

    let cloud_file = sandbox.item_path("file.md");
    assert_eq!(read_file(&cloud_file), "local file");
    assert!(sandbox.manifest_path("file.md").exists());
    assert!(sandbox.rule_path("file.md").exists());

    write_file(&cloud_file, "cloud edit");
    set_mtime(&cloud_file, 300);
    set_mtime(&local_file, 100);

    sandbox
        .qsync()
        .args(["sync", "file.md"])
        .assert()
        .success()
        .stdout(pred_contains("cloud -> local: 1"));
    assert_eq!(read_file(&local_file), "cloud edit");
}

#[test]
fn sync_copies_both_directions_and_latest_modified_wins() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join("local.txt"), "local");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success();

    let item_dir = sandbox.item_path("demo");

    write_file(&sandbox.local.join("second.txt"), "from local");
    sandbox
        .qsync()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("local -> cloud: 1"));
    assert_eq!(read_file(&item_dir.join("second.txt")), "from local");

    write_file(&item_dir.join("cloud.txt"), "from cloud");
    sandbox
        .qsync()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("cloud -> local: 1"));
    assert_eq!(read_file(&sandbox.local.join("cloud.txt")), "from cloud");

    write_file(&sandbox.local.join("winner.txt"), "old local");
    sandbox.qsync().args(["sync", "demo"]).assert().success();

    write_file(&sandbox.local.join("winner.txt"), "older local edit");
    write_file(&item_dir.join("winner.txt"), "newer cloud edit");
    set_mtime(&sandbox.local.join("winner.txt"), 100);
    set_mtime(&item_dir.join("winner.txt"), 200);

    sandbox.qsync().args(["sync", "demo"]).assert().success();
    assert_eq!(
        read_file(&sandbox.local.join("winner.txt")),
        "newer cloud edit"
    );
}

#[test]
fn sync_deletes_inner_file_from_other_side() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.local.join("src/a.txt"), "a");

    sandbox
        .qsync()
        .args(["add", sandbox.local.to_str().unwrap(), "--name", "demo"])
        .assert()
        .success();

    let item_dir = sandbox.item_path("demo");
    assert!(item_dir.join("src/a.txt").exists());

    fs::remove_file(sandbox.local.join("src/a.txt")).expect("remove local");
    sandbox
        .qsync()
        .args(["sync", "demo"])
        .assert()
        .success()
        .stdout(pred_contains("deleted cloud: 1"));
    assert!(!item_dir.join("src/a.txt").exists());
}

#[test]
fn doctor_reports_isolated_environment_health() {
    let sandbox = Sandbox::new();

    sandbox
        .qsync()
        .arg("doctor")
        .assert()
        .success()
        .stdout(pred_contains("[ok] iCloud Drive"))
        .stdout(pred_contains("[ok] Application Support"))
        .stdout(pred_contains("[ok] QuickSync Workspace"))
        .stdout(pred_contains("[ok] State Database"));
}

#[test]
fn errors_include_actionable_hints() {
    let sandbox = Sandbox::new();

    sandbox
        .qsync()
        .args(["status", "missing"])
        .assert()
        .failure()
        .stderr(pred_contains("run `qsync list`"));

    sandbox
        .qsync()
        .args(["add", sandbox.local.join("nope").to_str().unwrap()])
        .assert()
        .failure()
        .stderr(pred_contains("check the path and try again"));

    sandbox
        .qsync()
        .args([
            "add",
            sandbox.local.to_str().unwrap(),
            "--name",
            ".quicksync",
        ])
        .assert()
        .failure()
        .stderr(pred_contains("invalid item name"));

    sandbox
        .qsync()
        .args(["rule", "missing", "exclude", ""])
        .assert()
        .failure()
        .stderr(pred_contains("invalid rule pattern"));
}

#[test]
fn help_describes_core_commands_and_rule_behavior() {
    let sandbox = Sandbox::new();

    let no_args = sandbox.qsync().output().expect("qsync without args");
    let with_help = sandbox
        .qsync()
        .arg("--help")
        .output()
        .expect("qsync --help");
    assert!(no_args.status.success());
    assert!(with_help.status.success());
    assert_eq!(no_args.stdout, with_help.stdout);
    assert!(no_args.stderr.is_empty());
    assert!(with_help.stderr.is_empty());

    sandbox
        .qsync()
        .assert()
        .success()
        .stdout(pred_contains("Usage: qsync <COMMAND>"))
        .stdout(pred_contains("Common workflow:"));

    sandbox
        .qsync()
        .arg("--help")
        .assert()
        .success()
        .stdout(pred_contains("QuickSync/.quicksync"))
        .stdout(pred_contains("remove keeps cloud files"))
        .stdout(pred_contains("delete removes cloud files"));

    sandbox
        .qsync()
        .args(["add", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("Local file or folder to sync"))
        .stdout(pred_contains("does not import .gitignore"))
        .stdout(pred_contains("--ignore-file"));

    sandbox
        .qsync()
        .args(["rule", "exclude", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("prune matching cloud files"))
        .stdout(pred_contains("local originals are kept"));

    sandbox
        .qsync()
        .args(["rule", "include", "--help"])
        .assert()
        .success()
        .stdout(pred_contains("Include a path again"))
        .stdout(pred_contains("If no existing rule matches"))
        .stdout(pred_contains("next qsync sync"));
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
