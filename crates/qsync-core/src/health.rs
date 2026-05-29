use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use fs2::FileExt;

use crate::paths;
use crate::state::StateDb;

#[derive(Debug, Clone)]
pub enum CheckStatus {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct HealthCheck {
    pub name: String,
    pub status: CheckStatus,
    pub message: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DoctorReport {
    pub checks: Vec<HealthCheck>,
}

impl DoctorReport {
    pub fn has_errors(&self) -> bool {
        self.checks
            .iter()
            .any(|check| matches!(check.status, CheckStatus::Error))
    }
}

#[derive(Debug, Clone)]
pub struct DaemonHealth {
    pub installed: bool,
    pub running: bool,
    pub binary_path: Option<PathBuf>,
}

pub fn doctor_report() -> DoctorReport {
    let mut checks = Vec::new();

    let icloud_base = paths::icloud_base_dir();
    match &icloud_base {
        Ok(path) if path.exists() => checks.push(ok(
            "iCloud Drive",
            format!("found iCloud Drive folder at {}", path.display()),
        )),
        Ok(path) => checks.push(error(
            "iCloud Drive",
            format!("missing iCloud Drive folder at {}", path.display()),
            "Enable iCloud Drive, or set QUICKSYNC_ICLOUD_DIR for testing.",
        )),
        Err(err) => checks.push(error(
            "iCloud Drive",
            err.to_string(),
            "Check HOME or set QUICKSYNC_ICLOUD_DIR for testing.",
        )),
    }

    match paths::app_support_dir() {
        Ok(path) => match fs::create_dir_all(&path) {
            Ok(()) => checks.push(ok(
                "Application Support",
                format!("writable at {}", path.display()),
            )),
            Err(err) => checks.push(error(
                "Application Support",
                format!("not writable at {}: {err}", path.display()),
                "Check directory permissions.",
            )),
        },
        Err(err) => checks.push(error(
            "Application Support",
            err.to_string(),
            "Check HOME or set QUICKSYNC_APP_SUPPORT_DIR for testing.",
        )),
    }

    if let Ok(path) = paths::cloud_workspace_dir() {
        match ensure_writable_dir(&path) {
            Ok(()) => checks.push(ok(
                "QuickSync Workspace",
                format!("writable at {}", path.display()),
            )),
            Err(err) => checks.push(error(
                "QuickSync Workspace",
                format!("not writable at {}: {err}", path.display()),
                "Check iCloud Drive permissions and available storage.",
            )),
        }
    }

    match paths::state_db_path().and_then(|path| StateDb::open(&path).map(|_| path)) {
        Ok(path) => checks.push(ok("State Database", format!("opened {}", path.display()))),
        Err(err) => checks.push(error(
            "State Database",
            err.to_string(),
            "Check QuickSync Application Support permissions.",
        )),
    }

    let daemon = daemon_health();
    match (&daemon.binary_path, daemon.installed, daemon.running) {
        (Some(path), true, true) => checks.push(ok(
            "Daemon",
            format!("qsyncd appears to be running at {}", path.display()),
        )),
        (Some(path), true, false) => checks.push(warn(
            "Daemon",
            format!(
                "qsyncd binary found at {}, but it does not appear to be running",
                path.display()
            ),
            "Run ./scripts/install.sh, or start qsyncd manually for testing.",
        )),
        _ => checks.push(warn(
            "Daemon",
            "qsyncd binary was not found next to qsync".to_string(),
            "Build the project or run ./scripts/install.sh.",
        )),
    }

    DoctorReport { checks }
}

pub fn daemon_health() -> DaemonHealth {
    let binary_path = qsyncd_binary_path();
    let installed = binary_path.as_ref().is_some_and(|path| path.exists());
    let running = is_daemon_lock_held();

    DaemonHealth {
        installed,
        running,
        binary_path,
    }
}

fn qsyncd_binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join("qsyncd");
    Some(candidate)
}

fn is_daemon_lock_held() -> bool {
    let Ok(path) = paths::app_support_dir().map(|dir| dir.join("qsyncd.lock")) else {
        return false;
    };
    if !path.exists() {
        return false;
    }

    let Ok(file) = OpenOptions::new().read(true).write(true).open(path) else {
        return false;
    };

    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = file.unlock();
            false
        }
        Err(_) => true,
    }
}

fn ensure_writable_dir(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    let test_path = path.join(".qsync-write-test");
    File::create(&test_path)?;
    fs::remove_file(test_path)?;
    Ok(())
}

fn ok(name: &str, message: String) -> HealthCheck {
    HealthCheck {
        name: name.to_string(),
        status: CheckStatus::Ok,
        message,
        hint: None,
    }
}

fn warn(name: &str, message: String, hint: &str) -> HealthCheck {
    HealthCheck {
        name: name.to_string(),
        status: CheckStatus::Warn,
        message,
        hint: Some(hint.to_string()),
    }
}

fn error(name: &str, message: String, hint: &str) -> HealthCheck {
    HealthCheck {
        name: name.to_string(),
        status: CheckStatus::Error,
        message,
        hint: Some(hint.to_string()),
    }
}
