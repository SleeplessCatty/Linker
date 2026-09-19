use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::health::{self, DaemonHealth, DoctorReport};
use crate::manifest::{write_manifest, Manifest};
use crate::paths;
use crate::state::{Item, NewItem, StateDb};
use crate::sync::SyncSummary;
use crate::Result;

#[derive(Debug, Clone)]
pub struct AddOptions {
    pub source_path: String,
    pub target_path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AddOutcome {
    pub item: Item,
    pub manifest_path: String,
    pub sync_summary: SyncSummary,
}

pub fn add_item(options: AddOptions) -> Result<AddOutcome> {
    let local_path = paths::canonical_existing_dir(&options.source_path)?;
    let name = match options.name {
        Some(name) => name,
        None => paths::default_item_name(&local_path)?,
    };
    paths::validate_new_item_name(&name)?;
    let cloud_path = paths::resolve_target_dir(&options.target_path)?;
    let local_path_string = local_path
        .to_str()
        .ok_or_else(|| std::io::Error::other("source path must be UTF-8"))?
        .to_owned();
    let cloud_path_string = cloud_path
        .to_str()
        .ok_or_else(|| std::io::Error::other("target path must be UTF-8"))?
        .to_owned();
    validate_association_paths(&local_path, &cloud_path)?;
    paths::require_empty_target(&cloud_path)?;
    let support = paths::resolve_target_dir(&paths::app_support_dir()?.to_string_lossy())?;
    for path in [&local_path, &cloud_path] {
        if overlaps(path, &support) {
            return Err(crate::LinkerError::InvalidAssociation(
                "sync paths must not overlap Linker's application state directory".into(),
            ));
        }
    }

    paths::ensure_base_dirs()?;
    let mut db = StateDb::open(&paths::state_db_path()?)?;
    // Serialize registration checks and publication across concurrent add commands.
    let _registration_lock = db.lock_add()?;
    let items = db.list_items()?;
    if items
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(&name) || item.id == name)
    {
        return Err(crate::LinkerError::ItemExists(name));
    }
    for item in &items {
        for (is_source, existing) in [(true, &item.local_path), (false, &item.cloud_path)] {
            let existing = match Path::new(existing).canonicalize() {
                Ok(path) => path,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    paths::resolve_target_dir(existing)?
                }
                Err(error) => return Err(error.into()),
            };
            let shared_source =
                is_source && item.item_type == "directory" && local_path == existing;
            if (!shared_source && overlaps(&local_path, &existing))
                || overlaps(&cloud_path, &existing)
            {
                return Err(crate::LinkerError::InvalidAssociation(format!(
                    "path overlaps existing association {:?}: {}",
                    item.name,
                    existing.display()
                )));
            }
        }
    }
    let manifest_path = paths::app_manifests_dir()?.join(format!("{name}.json"));
    match fs::symlink_metadata(&manifest_path) {
        Ok(_) => {
            return Err(crate::LinkerError::InvalidAssociation(format!(
                "manifest already exists; refusing to overwrite: {}",
                manifest_path.display()
            )))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    paths::require_empty_target(&cloud_path)?;
    crate::tree::Tree::create_directory_path(&cloud_path)?;
    // Reject a changed ancestor/root before publishing or copying any contents.
    if cloud_path.canonicalize()? != cloud_path || local_path.canonicalize()? != local_path {
        return Err(crate::LinkerError::InvalidAssociation(
            "directory changed during add; retry after checking both paths".into(),
        ));
    }
    paths::require_empty_target(&cloud_path)?;
    let mut id = Uuid::new_v4().to_string();
    while items.iter().any(|item| item.name == id || item.id == id) {
        id = Uuid::new_v4().to_string();
    }
    let _item_lock = db.lock_item(&id)?;
    let _source_lock = db.lock_source(&local_path_string)?;
    let manifest = Manifest::new(
        id.clone(),
        name.clone(),
        "directory".into(),
        local_path_string.clone(),
        cloud_path_string.clone(),
    );
    write_manifest(&manifest_path, &manifest)?;

    let result = (|| {
        db.insert_item(NewItem {
            id: &id,
            name: &name,
            item_type: "directory",
            local_path: &local_path_string,
            cloud_path: &cloud_path_string,
        })?;
        let item = db.get_item(&id)?;
        let sync_summary = crate::sync::sync_initial_item(&db, &item)?;
        Ok(AddOutcome {
            item,
            manifest_path: manifest_path.to_string_lossy().to_string(),
            sync_summary,
        })
    })();
    if let Err(error) = result {
        // Do not recursively delete partially copied data on rollback. The item
        // lock prevents the daemon from observing a failed initial sync as live.
        db.rollback_add(&id).map_err(|cleanup| {
            std::io::Error::other(format!(
                "add failed: {error}; registration rollback also failed: {cleanup}"
            ))
        })?;
        remove_file_if_exists(&manifest_path).map_err(|cleanup| {
            std::io::Error::other(format!(
                "add failed: {error}; association removed but manifest cleanup failed: {cleanup}"
            ))
        })?;
        return Err(std::io::Error::other(format!(
            "add failed; new association removed, source kept; target may contain partial copies: {error}"
        )).into());
    }
    result
}

fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

pub fn list_items() -> Result<Vec<Item>> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    db.list_items()
}

pub fn daemon_status() -> DaemonHealth {
    health::daemon_health()
}

pub fn doctor() -> DoctorReport {
    health::doctor_report()
}

pub fn remove_item(name: &str) -> Result<Item> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    let found = db.get_item(name)?;
    let _lock = db.lock_item(&found.id)?;
    let _source_lock = db.lock_source(&found.local_path)?;
    let item = db.remove_item(name)?;
    remove_file_if_exists(&paths::app_manifests_dir()?.join(format!("{}.json", item.name)))?;
    Ok(item)
}

pub fn delete_item(name: &str) -> Result<Item> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    let found = db.get_item(name)?;
    let _lock = db.lock_item(&found.id)?;
    let _source_lock = db.lock_source(&found.local_path)?;
    let item = db.get_item(name)?;
    remove_path_if_exists(Path::new(&item.cloud_path))?;
    remove_file_if_exists(&paths::app_manifests_dir()?.join(format!("{}.json", item.name)))?;
    db.remove_item(name)
}

pub fn sync_item(name: Option<&str>) -> Result<Vec<SyncSummary>> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    let items = if let Some(name) = name {
        vec![db.get_item(name)?]
    } else {
        db.list_items()?
    };

    let mut summaries = Vec::new();
    for item in items {
        summaries.push(crate::sync::sync_item(&db, &item)?);
    }
    Ok(summaries)
}

pub fn preview_sync(name: Option<&str>) -> Result<Vec<crate::sync::SyncPreview>> {
    let path = paths::state_db_path()?;
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return match name {
                Some(name) => Err(crate::LinkerError::ItemNotFound(name.into())),
                None => Ok(Vec::new()),
            };
        }
        Err(error) => return Err(error.into()),
        Ok(_) => {}
    }
    let db = StateDb::open_read_only(&path)?;
    let items = match name {
        Some(name) => vec![db.get_item(name)?],
        None => db.list_items()?,
    };
    items
        .iter()
        .map(|item| crate::sync::preview_item(&db, item))
        .collect()
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            fs::remove_dir_all(path)?;
            Ok(())
        }
        Ok(_) => remove_file_if_exists(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn validate_association_paths(source_path: &Path, target_path: &Path) -> Result<()> {
    let target_compare = if target_path.exists() {
        target_path.canonicalize()?
    } else {
        target_path.to_path_buf()
    };

    if source_path == target_compare
        || source_path.starts_with(&target_compare)
        || target_compare.starts_with(source_path)
    {
        return Err(crate::LinkerError::InvalidAssociation(
            "source directory and target directory must be separate; one cannot contain the other"
                .to_string(),
        ));
    }

    Ok(())
}
