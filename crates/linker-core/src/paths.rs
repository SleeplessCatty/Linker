use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

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
    Ok(())
}

pub fn app_manifests_dir() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("manifests"))
}

/// Optional Linker-level ignore file, applied to every association.
pub fn app_global_ignore_path() -> Result<PathBuf> {
    Ok(app_support_dir()?.join("global.gitignore"))
}

/// Resolve existing ancestors before creating anything. This also handles ..
/// after a symlink correctly, without treating a missing suffix as a real path.
pub fn resolve_target_dir(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        return Err(LinkerError::InvalidAssociation(
            "target directory is empty".into(),
        ));
    }
    let path = expand_tilde(path)?;
    let absolute = if path.is_absolute() {
        path
    } else {
        env::current_dir()?.join(path)
    };
    let components: Vec<_> = absolute.components().collect();
    let mut resolved = PathBuf::new();
    for (index, component) in components.iter().enumerate() {
        match component {
            Component::RootDir => resolved.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            Component::Normal(name) => {
                resolved.push(name);
                match fs::symlink_metadata(&resolved) {
                    Ok(metadata) => {
                        if metadata.is_symlink() && index + 1 == components.len() {
                            return Err(LinkerError::TargetSymlink(resolved));
                        }
                        resolved = resolved.canonicalize()?;
                        if !resolved.is_dir() {
                            return Err(LinkerError::NotDirectory(resolved));
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Component::Prefix(_) => {
                return Err(LinkerError::InvalidAssociation(
                    "unsupported target path".into(),
                ))
            }
        }
    }
    Ok(resolved)
}

pub fn require_empty_target(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_symlink() => Err(LinkerError::TargetSymlink(path.into())),
        Ok(metadata) if !metadata.is_dir() => Err(LinkerError::NotDirectory(path.into())),
        Ok(_) => match fs::read_dir(path)?.next() {
            Some(entry) => {
                entry?;
                Err(LinkerError::TargetNotEmpty(path.into()))
            }
            None => Ok(()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
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

/// Stricter rules apply only to new registrations, not legacy migration.
pub fn validate_new_item_name(name: &str) -> Result<()> {
    validate_item_name(name)?;
    if name.trim() != name
        || name.chars().any(char::is_control)
        || name.len() > 250
        || name.eq_ignore_ascii_case(".linker")
    {
        return Err(LinkerError::InvalidName(name.to_owned()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_item_name, validate_new_item_name};

    #[test]
    fn reserves_linker_control_directory_name() {
        assert!(validate_item_name(".linker").is_err());
        for legacy_name in [" padded ", "line\nfeed", ".LINKER"] {
            assert!(validate_item_name(legacy_name).is_ok());
            assert!(validate_new_item_name(legacy_name).is_err());
        }
        for unsafe_name in ["../escape", "a/b", "a\\b"] {
            assert!(validate_item_name(unsafe_name).is_err());
            assert!(validate_new_item_name(unsafe_name).is_err());
        }
    }
}
