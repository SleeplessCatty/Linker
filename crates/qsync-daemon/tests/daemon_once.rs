use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use qsync_core::ops::{self, AddOptions};
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

    fn qsyncd(&self) -> Command {
        let mut cmd = Command::cargo_bin("qsyncd").expect("qsyncd bin");
        cmd.env("QUICKSYNC_APP_SUPPORT_DIR", &self.app_support)
            .env("QUICKSYNC_ICLOUD_DIR", &self.icloud);
        cmd
    }

    fn item_dir(&self) -> PathBuf {
        self.icloud.join("QuickSync/demo")
    }
}

#[test]
fn daemon_once_syncs_existing_item() {
    let sandbox = Sandbox::new();
    std::env::set_var("QUICKSYNC_APP_SUPPORT_DIR", &sandbox.app_support);
    std::env::set_var("QUICKSYNC_ICLOUD_DIR", &sandbox.icloud);

    write_file(&sandbox.local.join("initial.txt"), "initial");
    ops::add_item(AddOptions {
        path: sandbox.local.to_string_lossy().to_string(),
        name: Some("demo".to_string()),
        ignore_file: None,
        excludes: Vec::new(),
    })
    .expect("add item");

    write_file(&sandbox.local.join("daemon.txt"), "daemon");

    sandbox
        .qsyncd()
        .arg("--once")
        .assert()
        .success()
        .stderr(predicates::str::contains("qsyncd startup synced demo"));

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
