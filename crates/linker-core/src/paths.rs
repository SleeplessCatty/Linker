use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::{LinkerError, Result};

const APP_SUPPORT_ENV: &str = "LINKER_APP_SUPPORT_DIR";

pub fn home_dir() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(LinkerError::HomeDirMissing)
}

pub fn expand_tilde(path: &str) -> Result<PathBuf> {
    if path == "~" {
        return home_dir();
    }

    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(home_dir()?.join(rest));
    }

    Ok(PathBuf::from(path))
}

pub fn app_support_dir() -> Result<PathBuf> {
    if let Some(path) = env::var_os(APP_SUPPORT_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(home_dir()?.join("Library/Application Support/Linker"))
}

pub fn state_db_path() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("state.sqlite"))
}

pub fn ensure_base_dirs() -> Result<()> {
    fs::create_dir_all(app_support_dir()?.join("logs"))?;
    fs::create_dir_all(app_support_dir()?.join("tmp"))?;
    fs::create_dir_all(app_manifests_dir()?)?;
    fs::create_dir_all(app_rules_dir()?)?;
    Ok(())
}

pub fn app_manifests_dir() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("manifests"))
}

pub fn app_rules_dir() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("rules"))
}

pub fn canonical_dir_create(path: &str) -> Result<PathBuf> {
    let path = expand_tilde(path)?;
    fs::create_dir_all(&path)?;
    Ok(path.canonicalize()?)
}

pub fn canonical_existing_dir(path: &str) -> Result<PathBuf> {
    let path = expand_tilde(path)?;
    if !path.exists() {
        return Err(LinkerError::PathMissing(path));
    }
    if !path.is_dir() {
        return Err(LinkerError::NotDirectory(path));
    }
    Ok(path.canonicalize()?)
}

pub fn target_item_path(target_parent: &Path, name: &str) -> Result<PathBuf> {
    validate_item_name(name)?;
    Ok(target_parent.join(name))
}

pub fn default_item_name(path: &Path) -> Result<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| LinkerError::InvalidItemName(path.to_path_buf()))
}

pub fn validate_item_name(name: &str) -> Result<()> {
    if name.trim().is_empty()
        || name == "."
        || name == ".."
        || name == ".linker"
        || name.contains('/')
        || name.contains('\\')
        || name.contains(std::path::MAIN_SEPARATOR)
    {
        return Err(LinkerError::InvalidName(name.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_item_name;

    #[test]
    fn reserves_linker_control_directory_name() {
        assert!(validate_item_name(".linker").is_err());
    }
}
