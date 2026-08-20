use std::fs::{self, OpenOptions};
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
            "Check HOME or set LINKER_APP_SUPPORT_DIR for testing.",
        )),
    }

    match paths::state_db_path().and_then(|path| StateDb::open(&path).map(|_| path)) {
        Ok(path) => checks.push(ok("State Database", format!("opened {}", path.display()))),
        Err(err) => checks.push(error(
            "State Database",
            err.to_string(),
            "Check Linker Application Support permissions.",
        )),
    }

    match paths::state_db_path().and_then(|path| StateDb::open(&path)) {
        Ok(db) => match db.list_items() {
            Ok(items) if items.is_empty() => checks.push(ok(
                "Sync Associations",
                "no configured directory associations".to_string(),
            )),
            Ok(items) => {
                let mut missing = Vec::new();
                for item in &items {
                    if !Path::new(&item.local_path).is_dir() {
                        missing.push(format!("source missing for {}", item.name));
                    }
                    if !Path::new(&item.cloud_path).is_dir() {
                        missing.push(format!("target missing for {}", item.name));
                    }
                }
                if missing.is_empty() {
                    checks.push(ok(
                        "Sync Associations",
                        format!("{} configured item(s) are reachable", items.len()),
                    ));
                } else {
                    checks.push(error(
                        "Sync Associations",
                        missing.join("; "),
                        "Run `linker status`, then fix the missing directory or remove the association.",
                    ));
                }
            }
            Err(err) => checks.push(error(
                "Sync Associations",
                err.to_string(),
                "Check the Linker state database.",
            )),
        },
        Err(err) => checks.push(error(
            "Sync Associations",
            err.to_string(),
            "Check Linker Application Support permissions.",
        )),
    }

    let daemon = daemon_health();
    match (&daemon.binary_path, daemon.installed, daemon.running) {
        (Some(path), true, true) => checks.push(ok(
            "Daemon",
            format!("linkerd appears to be running at {}", path.display()),
        )),
        (Some(path), true, false) => checks.push(warn(
            "Daemon",
            format!(
                "linkerd binary found at {}, but it does not appear to be running",
                path.display()
            ),
            "Run ./scripts/install.sh, or start linkerd manually for testing.",
        )),
        _ => checks.push(warn(
            "Daemon",
            "linkerd binary was not found next to linker".to_string(),
            "Build the project or run ./scripts/install.sh.",
        )),
    }

    DoctorReport { checks }
}

pub fn daemon_health() -> DaemonHealth {
    let binary_path = linkerd_binary_path();
    let installed = binary_path.as_ref().is_some_and(|path| path.exists());
    let running = is_daemon_lock_held();

    DaemonHealth {
        installed,
        running,
        binary_path,
    }
}

fn linkerd_binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidate = dir.join("linkerd");
    Some(candidate)
}

fn is_daemon_lock_held() -> bool {
    let Ok(path) = paths::app_support_dir().map(|dir| dir.join("linkerd.lock")) else {
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
