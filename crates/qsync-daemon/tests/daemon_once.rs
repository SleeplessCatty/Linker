use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use qsync_core::ops::{self, AddOptions};
use tempfile::TempDir;

struct Sandbox {
    _tmp: TempDir,
    app_support: PathBuf,
    target_parent: PathBuf,
    source: PathBuf,
}

impl Sandbox {
    fn new() -> Self {
        let tmp = tempfile::tempdir().expect("tempdir");
        let app_support = tmp.path().join("app-support");
        let target_parent = tmp.path().join("target-parent");
        let source = tmp.path().join("source").join("demo");

        fs::create_dir_all(&app_support).expect("app support");
        fs::create_dir_all(&target_parent).expect("target parent");
        fs::create_dir_all(&source).expect("source");

        Self {
            _tmp: tmp,
            app_support,
            target_parent,
            source,
        }
    }

    fn qsd(&self) -> Command {
        let mut cmd = Command::cargo_bin("qsd").expect("qsd bin");
        cmd.env("QUICKSYNC_APP_SUPPORT_DIR", &self.app_support);
        cmd.env_remove("QUICKSYNC_ICLOUD_DIR");
        cmd
    }

    fn item_dir(&self) -> PathBuf {
        self.target_parent.join("demo")
    }
}

#[test]
fn daemon_once_without_items_succeeds() {
    let sandbox = Sandbox::new();

    sandbox
        .qsd()
        .arg("--once")
        .assert()
        .success()
        .stderr(predicates::str::contains("qsd watching 0 item(s)"));
}

#[test]
fn daemon_once_syncs_existing_item() {
    let sandbox = Sandbox::new();
    std::env::set_var("QUICKSYNC_APP_SUPPORT_DIR", &sandbox.app_support);
    std::env::remove_var("QUICKSYNC_ICLOUD_DIR");

    write_file(&sandbox.source.join("initial.txt"), "initial");
    ops::add_item(AddOptions {
        source_path: sandbox.source.to_string_lossy().to_string(),
        target_parent_path: sandbox.target_parent.to_string_lossy().to_string(),
        ignore_file: None,
        excludes: Vec::new(),
    })
    .expect("add item");

    write_file(&sandbox.source.join("daemon.txt"), "daemon");

    sandbox
        .qsd()
        .arg("--once")
        .assert()
        .success()
        .stderr(predicates::str::contains("qsd startup synced demo"));

    assert_eq!(read_file(&sandbox.item_dir().join("daemon.txt")), "daemon");
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
