use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use linker_core::state::{NewItem, StateDb};
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

        let app_support = app_support.canonicalize().unwrap();
        let target_parent = target_parent.canonicalize().unwrap();
        let source = source.canonicalize().unwrap();
        Self {
            _tmp: tmp,
            app_support,
            target_parent,
            source,
        }
    }

    fn linkerd(&self) -> Command {
        let mut cmd = Command::cargo_bin("linkerd").expect("linkerd bin");
        cmd.env("LINKER_APP_SUPPORT_DIR", &self.app_support);
        cmd.env("HOME", self._tmp.path());
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
        .linkerd()
        .arg("--once")
        .assert()
        .success()
        .stderr(predicates::str::contains("linkerd watching 0 item(s)"));
}

#[test]
fn daemon_help_uses_linkerd_name() {
    let sandbox = Sandbox::new();

    sandbox
        .linkerd()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicates::str::contains("linkerd"))
        .stdout(predicates::str::contains("Linker background daemon"));
}

#[test]
fn daemon_once_syncs_existing_item() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.source.join("initial.txt"), "initial");
    seed(&sandbox);

    write_file(&sandbox.source.join("daemon.txt"), "daemon");

    sandbox
        .linkerd()
        .arg("--once")
        .assert()
        .success()
        .stderr(predicates::str::contains("linkerd startup synced demo"));

    assert_eq!(read_file(&sandbox.item_dir().join("daemon.txt")), "daemon");
}

#[test]
fn daemon_once_syncs_each_target_of_shared_source() {
    let sandbox = Sandbox::new();
    write_file(&sandbox.source.join("initial.txt"), "initial");
    seed(&sandbox);
    let second = sandbox.target_parent.join("other-name");
    fs::create_dir(&second).unwrap();
    let mut db = StateDb::open(&sandbox.app_support.join("state.sqlite")).unwrap();
    db.insert_item(NewItem {
        id: "second",
        name: "second",
        item_type: "directory",
        local_path: sandbox.source.to_str().unwrap(),
        cloud_path: second.to_str().unwrap(),
    })
    .unwrap();
    write_file(&sandbox.source.join("new.txt"), "both targets");
    sandbox
        .linkerd()
        .arg("--once")
        .assert()
        .success()
        .stderr(predicates::str::contains("startup synced demo"))
        .stderr(predicates::str::contains("startup synced second"));
    assert_eq!(
        read_file(&sandbox.item_dir().join("new.txt")),
        "both targets"
    );
    assert_eq!(read_file(&second.join("new.txt")), "both targets");
}

#[test]
fn running_daemon_fans_source_events_out_to_all_targets() {
    use std::process::{Child, Stdio};
    use std::time::{Duration, Instant};
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn wait_for(mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !check() {
            assert!(
                Instant::now() < deadline,
                "multi-target daemon event did not complete"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let sandbox = Sandbox::new();
    seed(&sandbox);
    let second = sandbox.target_parent.join("other-name");
    fs::create_dir(&second).unwrap();
    let mut db = StateDb::open(&sandbox.app_support.join("state.sqlite")).unwrap();
    db.insert_item(NewItem {
        id: "second",
        name: "second",
        item_type: "directory",
        local_path: sandbox.source.to_str().unwrap(),
        cloud_path: second.to_str().unwrap(),
    })
    .unwrap();
    let log = sandbox._tmp.path().join("multi-target-daemon.log");
    let child = sandbox
        .linkerd()
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let _running = Running(child);
    wait_for(|| {
        fs::read_to_string(&log)
            .unwrap()
            .contains("startup synced second")
    });
    write_file(&sandbox.source.join("from-source.txt"), "fan out");
    wait_for(|| {
        [&sandbox.item_dir(), &second].iter().all(|target| {
            fs::read_to_string(target.join("from-source.txt")).is_ok_and(|value| value == "fan out")
        })
    });
    write_file(&second.join("from-target.txt"), "back through source");
    wait_for(|| {
        [&sandbox.source, &sandbox.item_dir()].iter().all(|target| {
            fs::read_to_string(target.join("from-target.txt"))
                .is_ok_and(|value| value == "back through source")
        })
    });
    let output = fs::read_to_string(log).unwrap();
    assert!(output.contains("event synced demo"));
    assert!(output.contains("event synced second"));
    assert!(!output.contains("failed to sync"));
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

fn seed(sandbox: &Sandbox) {
    fs::create_dir_all(sandbox.item_dir()).unwrap();
    let mut db = StateDb::open(&sandbox.app_support.join("state.sqlite")).unwrap();
    db.insert_item(NewItem {
        id: "daemon-demo",
        name: "demo",
        item_type: "directory",
        local_path: sandbox.source.to_str().unwrap(),
        cloud_path: sandbox.item_dir().to_str().unwrap(),
    })
    .unwrap();
    linker_core::sync::sync_item(&db, &db.get_item("demo").unwrap()).unwrap();
}

#[test]
fn running_daemon_reloads_gitignore_and_deduplicates_warnings() {
    use std::process::{Child, Stdio};
    use std::time::{Duration, Instant};
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    fn wait_for(mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !check() {
            assert!(Instant::now() < deadline, "daemon event did not complete");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let sandbox = Sandbox::new();
    write_file(&sandbox.source.join(".gitignore"), "*.bad\n!keep.bad\n");
    write_file(&sandbox.source.join("cache.log"), "source stays");
    seed(&sandbox);
    let log = sandbox._tmp.path().join("daemon.log");
    let child = sandbox
        .linkerd()
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log).unwrap())
        .spawn()
        .unwrap();
    let _running = Running(child);
    wait_for(|| {
        fs::read_to_string(&log)
            .unwrap()
            .contains("startup synced demo")
    });
    assert_eq!(
        fs::read_to_string(&log).unwrap().matches("skipped").count(),
        1
    );

    write_file(&sandbox.source.join("new.txt"), "event");
    wait_for(|| sandbox.item_dir().join("new.txt").exists());
    wait_for(|| {
        fs::read_to_string(&log)
            .unwrap()
            .contains("event synced demo")
    });
    assert_eq!(
        fs::read_to_string(&log).unwrap().matches("skipped").count(),
        1
    );

    write_file(
        &sandbox.source.join(".gitignore"),
        "*.bad\n!keep.bad\n*.log\n",
    );
    wait_for(|| !sandbox.item_dir().join("cache.log").exists());
    wait_for(|| fs::read_to_string(&log).unwrap().matches("skipped").count() == 2);
    assert_eq!(read_file(&sandbox.source.join("cache.log")), "source stays");

    fs::remove_file(sandbox.source.join(".gitignore")).unwrap();
    wait_for(|| sandbox.item_dir().join("cache.log").exists());
    assert!(!sandbox.item_dir().join(".gitignore").exists());
}
