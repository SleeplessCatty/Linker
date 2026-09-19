//! One-time, recoverable retirement of the pre-0.3 manual rule store.
use std::fs;
use std::path::Path;

use crate::{paths, Result};
use rusqlite::{Connection, DatabaseName};

fn regular(path: &Path) -> Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => Ok(true),
        Ok(_) => Err(std::io::Error::other(format!(
            "expected regular metadata file: {}",
            path.display()
        ))
        .into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

fn safe_dir(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => Ok(()),
        Ok(_) => Err(std::io::Error::other(format!(
            "unsafe metadata directory: {}",
            path.display()
        ))
        .into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

// Only publish complete, durable backups. Before `ready`, every attempt rebuilds
// them from the untouched legacy metadata, even if an older partial file exists.
fn atomic_file(path: &Path, write: impl FnOnce(&Path) -> Result<()>) -> Result<()> {
    regular(path)?;
    let parent = path.parent().unwrap();
    let temp = parent.join(format!(".backup-{}", uuid::Uuid::new_v4()));
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    let result = (|| {
        write(&temp)?;
        fs::File::open(&temp)?.sync_all()?;
        fs::rename(&temp, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(crate) fn upgrade(conn: &mut Connection, directory: &Path) -> Result<()> {
    let legacy = conn
        .prepare("PRAGMA table_info(items)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?
        .iter()
        .any(|name| name == "rule_path");
    let backup = directory.join("backups/gitignore-v2");
    let ready = backup.join("ready");
    let done = backup.join("done");
    if !legacy && !ready.exists() {
        return Ok(());
    }
    if !legacy && done.exists() {
        return Ok(());
    }

    let names = conn
        .prepare("SELECT name FROM items ORDER BY name")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for name in &names {
        paths::validate_item_name(name)?;
    }

    safe_dir(&directory.join("backups"))?;
    safe_dir(&backup)?;
    safe_dir(&backup.join("manifests"))?;
    safe_dir(&backup.join("rules"))?;
    for name in ["manifests", "rules"] {
        let path = directory.join(name);
        if path.exists() || fs::symlink_metadata(&path).is_ok() {
            safe_dir(&path)?;
        }
    }

    if !regular(&ready)? {
        if !legacy {
            return Err(std::io::Error::other(
                "incomplete migration backup without legacy database",
            )
            .into());
        }
        let db_backup = backup.join("state.sqlite");
        atomic_file(&db_backup, |temp| {
            conn.backup(DatabaseName::Main, temp, None)?;
            Ok(())
        })?;
        for name in &names {
            for (folder, extension) in [("manifests", "json"), ("rules", "ignore")] {
                let rel = format!("{folder}/{name}.{extension}");
                let from = directory.join(&rel);
                let to = backup.join(&rel);
                if regular(&from)? {
                    atomic_file(&to, |temp| {
                        fs::copy(&from, temp)?;
                        Ok(())
                    })?;
                }
            }
        }
        atomic_file(&ready, |temp| {
            fs::write(temp, b"backup complete\n")?;
            Ok(())
        })?;
    }

    if legacy {
        let count: i64 = conn.query_row("SELECT count(*) FROM exclude_rules", [], |r| r.get(0))?;
        eprintln!("Linker: archived {count} retired manual rule(s) at {}; only .gitignore rules will apply", backup.display());
        let tx = conn.transaction()?;
        tx.execute_batch(
            "DROP TABLE IF EXISTS exclude_rules;
             ALTER TABLE items DROP COLUMN rule_path;
             CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at INTEGER NOT NULL);"
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO schema_migrations VALUES (2, ?1)",
            [chrono::Utc::now().timestamp()],
        )?;
        tx.commit()?;
    }

    // Finish filesystem migration after the DB commit. 'ready' permits retry after a crash.
    for name in names {
        let manifest = directory.join("manifests").join(format!("{name}.json"));
        if regular(&manifest)? {
            let mut value: serde_json::Value = serde_json::from_slice(&fs::read(&manifest)?)?;
            let object = value
                .as_object_mut()
                .ok_or_else(|| std::io::Error::other("invalid manifest"))?;
            if object.get("schema_version").and_then(|v| v.as_u64()) != Some(2)
                || object.contains_key("rule_path")
            {
                object.remove("rule_path");
                object.insert("schema_version".into(), 2.into());
                crate::manifest::write_json_atomic(&manifest, &value)?;
            }
        }
        let rel = format!("rules/{name}.ignore");
        let original = directory.join(&rel);
        let saved = backup.join(&rel);
        if regular(&original)? && regular(&saved)? {
            if fs::read(&original)? == fs::read(&saved)? {
                fs::remove_file(&original)?;
            } else {
                eprintln!(
                    "Linker: retained changed retired rule snapshot: {}",
                    original.display()
                );
            }
        }
    }
    let rules = directory.join("rules");
    if rules.is_dir() && fs::read_dir(&rules)?.next().is_none() {
        fs::remove_dir(&rules)?;
    }
    atomic_file(&done, |temp| {
        fs::write(temp, b"migration complete\n")?;
        Ok(())
    })?;
    Ok(())
}
