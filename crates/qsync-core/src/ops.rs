use std::fs;
use std::path::Path;

use uuid::Uuid;

use crate::health::{self, DaemonHealth, DoctorReport};
use crate::manifest::{write_manifest, Manifest};
use crate::paths;
use crate::rules::{initial_rules, normalize_rule_pattern, write_rule_snapshot, Rule};
use crate::state::{Item, StateDb};
use crate::sync::SyncSummary;
use crate::Result;

#[derive(Debug, Clone)]
pub struct AddOptions {
    pub path: String,
    pub name: Option<String>,
    pub ignore_file: Option<String>,
    pub excludes: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct AddOutcome {
    pub item: Item,
    pub manifest_path: String,
    pub sync_summary: SyncSummary,
}

pub fn add_item(options: AddOptions) -> Result<AddOutcome> {
    let local_path = paths::canonical_existing_path(&options.path)?;
    let name = options
        .name
        .unwrap_or(paths::default_item_name(&local_path)?);
    paths::validate_item_name(&name)?;

    let metadata = fs::metadata(&local_path)?;
    let item_type = if metadata.is_file() {
        "file"
    } else if metadata.is_dir() {
        "directory"
    } else {
        return Err(crate::QsyncError::PathMissing(local_path));
    };

    paths::ensure_base_dirs()?;

    let mut db = StateDb::open(&paths::state_db_path()?)?;
    if db.name_exists(&name)? {
        return Err(crate::QsyncError::ItemExists(name));
    }

    let id = Uuid::new_v4().to_string();
    let cloud_path = paths::cloud_item_path(&name)?;
    if cloud_path.exists() {
        return Err(crate::QsyncError::ItemExists(name));
    }
    let manifest_path = paths::cloud_manifests_dir()?.join(format!("{name}.json"));
    let rule_path = paths::cloud_rules_dir()?.join(format!("{name}.ignore"));

    if item_type == "directory" {
        fs::create_dir_all(&cloud_path)?;
    } else if let Some(parent) = cloud_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let ignore_file = options
        .ignore_file
        .as_deref()
        .map(paths::expand_tilde)
        .transpose()?
        .map(|path| path.canonicalize())
        .transpose()?;
    let rules = initial_rules(ignore_file.as_deref(), &options.excludes)?;
    write_rule_snapshot(&rule_path, &rules)?;

    let manifest = Manifest::new(
        id.clone(),
        name.clone(),
        item_type.to_string(),
        local_path.to_string_lossy().to_string(),
    );
    write_manifest(&manifest_path, &manifest)?;

    let local_path_string = local_path.to_string_lossy().to_string();
    let cloud_path_string = cloud_path.to_string_lossy().to_string();
    let rule_path_string = rule_path.to_string_lossy().to_string();

    db.insert_item(
        &id,
        &name,
        item_type,
        &local_path_string,
        &cloud_path_string,
        &rule_path_string,
        &rules,
    )?;

    let item = db.get_item(&name)?;
    let sync_summary = crate::sync::sync_item(&db, &item, &rules)?;
    Ok(AddOutcome {
        item,
        manifest_path: manifest_path.to_string_lossy().to_string(),
        sync_summary,
    })
}

pub fn list_items() -> Result<Vec<Item>> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    db.list_items()
}

pub fn status(name: Option<&str>) -> Result<Vec<Item>> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    if let Some(name) = name {
        Ok(vec![db.get_item(name)?])
    } else {
        db.list_items()
    }
}

pub fn daemon_status() -> DaemonHealth {
    health::daemon_health()
}

pub fn doctor() -> DoctorReport {
    health::doctor_report()
}

pub fn remove_item(name: &str) -> Result<Item> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    db.remove_item(name)
}

pub fn delete_item(name: &str) -> Result<Item> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    let item = db.get_item(name)?;
    remove_path_if_exists(Path::new(&item.cloud_path))?;
    remove_file_if_exists(&paths::cloud_manifests_dir()?.join(format!("{}.json", item.name)))?;
    remove_file_if_exists(Path::new(&item.rule_path))?;
    db.remove_item(name)
}

pub fn add_exclude(name: &str, pattern: &str) -> Result<Vec<Rule>> {
    let pattern = normalize_rule_pattern(pattern)?;
    let db = StateDb::open(&paths::state_db_path()?)?;
    let item = db.add_user_exclude(name, &pattern)?;
    let rules = rewrite_rule_snapshot(&db, &item)?;
    crate::sync::prune_cloud_excluded(&item, &rules)?;
    crate::sync::sync_item(&db, &item, &rules)?;
    Ok(rules)
}

pub fn remove_exclude(name: &str, pattern: &str) -> Result<Vec<Rule>> {
    let pattern = normalize_rule_pattern(pattern)?;
    let db = StateDb::open(&paths::state_db_path()?)?;
    let item = db.remove_user_exclude(name, &pattern)?;
    rewrite_rule_snapshot(&db, &item)
}

pub fn list_excludes(name: &str) -> Result<Vec<Rule>> {
    let db = StateDb::open(&paths::state_db_path()?)?;
    let item = db.get_item(name)?;
    rules_for_item(&db, &item)
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
        let rules = rules_for_item(&db, &item)?;
        summaries.push(crate::sync::sync_item(&db, &item, &rules)?);
    }
    Ok(summaries)
}

fn rewrite_rule_snapshot(db: &StateDb, item: &Item) -> Result<Vec<Rule>> {
    let rules = rules_for_item(db, item)?;
    write_rule_snapshot(std::path::Path::new(&item.rule_path), &rules)?;
    Ok(rules)
}

fn rules_for_item(db: &StateDb, item: &Item) -> Result<Vec<Rule>> {
    Ok(db
        .list_excludes_for_item(&item.id)?
        .into_iter()
        .map(|rule| Rule {
            pattern: rule.pattern,
        })
        .collect())
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
