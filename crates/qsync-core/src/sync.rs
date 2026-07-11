use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use filetime::{set_file_mtime, FileTime};
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

use crate::rules::Rule;
use crate::state::{FileStateUpdate, Item, StateDb, StoredFileState};
use crate::{QsyncError, Result};

#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub item_name: String,
    pub copied_local_to_cloud: usize,
    pub copied_cloud_to_local: usize,
    pub deleted_local: usize,
    pub deleted_cloud: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone)]
struct FileMeta {
    abs_path: PathBuf,
    rel_path: String,
    hash: String,
    size: i64,
    mtime: i64,
}

pub fn sync_item(db: &StateDb, item: &Item, rules: &[Rule]) -> Result<SyncSummary> {
    let local_root = PathBuf::from(&item.local_path);
    let cloud_root = PathBuf::from(&item.cloud_path);
    let is_file = item.item_type == "file";

    if !local_root.exists() {
        db.mark_item_error(&item.id, "local root is missing")?;
        return Err(QsyncError::PathMissing(local_root));
    }
    if !cloud_root.exists() {
        if is_file {
            if let Some(parent) = cloud_root.parent() {
                fs::create_dir_all(parent)?;
            }
        } else {
            fs::create_dir_all(&cloud_root)?;
        }
    }

    let local_matcher_root = matcher_root(&local_root, is_file);
    let cloud_matcher_root = matcher_root(&cloud_root, is_file);
    let local_matcher = build_matcher(&local_matcher_root, rules)?;
    let cloud_matcher = build_matcher(&cloud_matcher_root, rules)?;
    let local_files = scan_item_side(&local_root, is_file, &local_matcher, &local_matcher_root)?;
    let cloud_files = scan_item_side(&cloud_root, is_file, &cloud_matcher, &cloud_matcher_root)?;
    let previous = db
        .list_file_states(&item.id)?
        .into_iter()
        .map(|state| (state.relative_path.clone(), state))
        .collect::<HashMap<_, _>>();

    let mut paths = BTreeSet::new();
    paths.extend(local_files.keys().cloned());
    paths.extend(cloud_files.keys().cloned());
    paths.extend(previous.keys().cloned());

    let mut summary = SyncSummary {
        item_name: item.name.clone(),
        copied_local_to_cloud: 0,
        copied_cloud_to_local: 0,
        deleted_local: 0,
        deleted_cloud: 0,
        unchanged: 0,
    };

    for rel_path in paths {
        let local = local_files.get(&rel_path);
        let cloud = cloud_files.get(&rel_path);
        let prev = previous.get(&rel_path);

        match decide(local, cloud, prev) {
            Decision::Noop => {
                summary.unchanged += 1;
                write_state(db, &item.id, &rel_path, local, cloud, false)?;
            }
            Decision::CopyLocalToCloud => {
                let local = local.expect("local file exists for local-to-cloud copy");
                let target = target_path(&cloud_root, &rel_path);
                copy_file(local, &target)?;
                let copied = scan_after_copy(&target, &cloud_root, is_file)?;
                summary.copied_local_to_cloud += 1;
                write_state(db, &item.id, &rel_path, Some(local), Some(&copied), false)?;
            }
            Decision::CopyCloudToLocal => {
                let cloud = cloud.expect("cloud file exists for cloud-to-local copy");
                let target = target_path(&local_root, &rel_path);
                copy_file(cloud, &target)?;
                let copied = scan_after_copy(&target, &local_root, is_file)?;
                summary.copied_cloud_to_local += 1;
                write_state(db, &item.id, &rel_path, Some(&copied), Some(cloud), false)?;
            }
            Decision::DeleteLocal => {
                if let Some(local) = local {
                    remove_file_if_exists(&local.abs_path)?;
                    summary.deleted_local += 1;
                }
                write_state(db, &item.id, &rel_path, None, cloud, cloud.is_none())?;
            }
            Decision::DeleteCloud => {
                if let Some(cloud) = cloud {
                    remove_file_if_exists(&cloud.abs_path)?;
                    summary.deleted_cloud += 1;
                }
                write_state(db, &item.id, &rel_path, local, None, local.is_none())?;
            }
            Decision::Deleted => {
                write_state(db, &item.id, &rel_path, None, None, true)?;
            }
        }
    }

    db.mark_item_synced(&item.id)?;
    Ok(summary)
}

pub fn prune_cloud_excluded(item: &Item, rules: &[Rule]) -> Result<usize> {
    let cloud_root = PathBuf::from(&item.cloud_path);
    if !cloud_root.exists() {
        return Ok(0);
    }

    let is_file = item.item_type == "file";
    let matcher_root = matcher_root(&cloud_root, is_file);
    let matcher = build_matcher(&matcher_root, rules)?;

    if is_file {
        let Some(file_name) = cloud_root.file_name() else {
            return Ok(0);
        };
        let ignored = matcher
            .matched_path_or_any_parents(Path::new(file_name), false)
            .is_ignore();
        if ignored {
            remove_file_if_exists(&cloud_root)?;
            return Ok(1);
        }
        return Ok(0);
    }

    let mut targets = Vec::new();
    for entry in WalkDir::new(&cloud_root)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
    {
        let entry = entry?;
        if entry.depth() == 0 {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(&cloud_root) else {
            continue;
        };
        if matcher
            .matched_path_or_any_parents(rel, entry.file_type().is_dir())
            .is_ignore()
        {
            targets.push((entry.path().to_path_buf(), entry.file_type().is_dir()));
        }
    }

    let mut removed = 0;
    for (path, is_dir) in targets {
        if is_dir {
            if fs::remove_dir_all(&path).is_ok() {
                removed += 1;
            }
        } else if remove_file_if_exists(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[derive(Debug, Clone, Copy)]
enum Decision {
    Noop,
    CopyLocalToCloud,
    CopyCloudToLocal,
    DeleteLocal,
    DeleteCloud,
    Deleted,
}

fn decide(
    local: Option<&FileMeta>,
    cloud: Option<&FileMeta>,
    prev: Option<&StoredFileState>,
) -> Decision {
    match (local, cloud) {
        (Some(local), Some(cloud)) => {
            if local.hash == cloud.hash {
                return Decision::Noop;
            }
            if local.mtime >= cloud.mtime {
                Decision::CopyLocalToCloud
            } else {
                Decision::CopyCloudToLocal
            }
        }
        (Some(local), None) => {
            if let Some(prev) = prev {
                if prev.cloud_hash.is_some()
                    && prev.local_hash.as_deref() == Some(local.hash.as_str())
                    && prev.local_mtime == Some(local.mtime)
                {
                    return Decision::DeleteLocal;
                }
            }
            Decision::CopyLocalToCloud
        }
        (None, Some(cloud)) => {
            if let Some(prev) = prev {
                if prev.local_hash.is_some()
                    && prev.cloud_hash.as_deref() == Some(cloud.hash.as_str())
                    && prev.cloud_mtime == Some(cloud.mtime)
                {
                    return Decision::DeleteCloud;
                }
            }
            Decision::CopyCloudToLocal
        }
        (None, None) => Decision::Deleted,
    }
}

fn build_matcher(root: &Path, rules: &[Rule]) -> Result<Gitignore> {
    let mut builder = GitignoreBuilder::new(root);
    for rule in rules {
        builder
            .add_line(None, &rule.pattern)
            .map_err(|err| QsyncError::Rule(err.to_string()))?;
    }
    builder
        .build()
        .map_err(|err| QsyncError::Rule(err.to_string()))
}

fn matcher_root(path: &Path, is_file: bool) -> PathBuf {
    if is_file {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        path.to_path_buf()
    }
}

fn scan_item_side(
    root: &Path,
    is_file: bool,
    matcher: &Gitignore,
    matcher_root: &Path,
) -> Result<HashMap<String, FileMeta>> {
    if is_file {
        return scan_file_item(root, matcher, matcher_root);
    }
    scan_tree(root, matcher)
}

fn scan_file_item(
    path: &Path,
    matcher: &Gitignore,
    matcher_root: &Path,
) -> Result<HashMap<String, FileMeta>> {
    let mut files = HashMap::new();
    if !path.exists() {
        return Ok(files);
    }
    if !path.is_file() {
        return Err(QsyncError::NotFile(path.to_path_buf()));
    }

    let rel_for_match = path
        .strip_prefix(matcher_root)
        .map_err(|err| QsyncError::StripPrefix(err.to_string()))?;
    if matcher
        .matched_path_or_any_parents(rel_for_match, false)
        .is_ignore()
    {
        return Ok(files);
    }

    let mut meta = scan_one(path, path)?;
    meta.rel_path = String::new();
    files.insert(String::new(), meta);
    Ok(files)
}

fn scan_tree(root: &Path, matcher: &Gitignore) -> Result<HashMap<String, FileMeta>> {
    let mut files = HashMap::new();
    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| should_walk(root, matcher, entry))
    {
        let entry = entry?;
        if !entry.file_type().is_file() {
            continue;
        }

        let meta = scan_one(entry.path(), root)?;
        files.insert(meta.rel_path.clone(), meta);
    }
    Ok(files)
}

fn target_path(root: &Path, rel_path: &str) -> PathBuf {
    if rel_path.is_empty() {
        root.to_path_buf()
    } else {
        root.join(rel_path)
    }
}

fn scan_after_copy(path: &Path, root: &Path, is_file: bool) -> Result<FileMeta> {
    if is_file {
        let mut meta = scan_one(path, path)?;
        meta.rel_path = String::new();
        Ok(meta)
    } else {
        scan_one(path, root)
    }
}

fn should_walk(root: &Path, matcher: &Gitignore, entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Ok(rel) = entry.path().strip_prefix(root) else {
        return false;
    };
    !matcher
        .matched_path_or_any_parents(rel, entry.file_type().is_dir())
        .is_ignore()
}

fn scan_one(path: &Path, root: &Path) -> Result<FileMeta> {
    let metadata = fs::metadata(path)?;
    let rel_path = path
        .strip_prefix(root)
        .map_err(|err| QsyncError::StripPrefix(err.to_string()))?
        .to_string_lossy()
        .replace('\\', "/");
    let mtime = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|err| QsyncError::Timestamp(err.to_string()))?
        .as_secs() as i64;
    let size = metadata.len() as i64;
    let hash = hash_file(path)?;

    Ok(FileMeta {
        abs_path: path.to_path_buf(),
        rel_path,
        hash,
        size,
        mtime,
    })
}

fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_file(source: &FileMeta, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }

    let tmp = target.with_extension(format!(
        "{}qsync-tmp-{}",
        target
            .extension()
            .map(|ext| format!("{}.", ext.to_string_lossy()))
            .unwrap_or_default(),
        uuid::Uuid::new_v4()
    ));

    fs::copy(&source.abs_path, &tmp)?;
    set_file_mtime(&tmp, FileTime::from_unix_time(source.mtime, 0))?;
    fs::rename(&tmp, target)?;
    Ok(())
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

fn write_state(
    db: &StateDb,
    item_id: &str,
    rel_path: &str,
    local: Option<&FileMeta>,
    cloud: Option<&FileMeta>,
    deleted: bool,
) -> Result<()> {
    db.upsert_file_state(
        item_id,
        &FileStateUpdate {
            relative_path: rel_path.to_string(),
            local_hash: local.map(|meta| meta.hash.clone()),
            local_mtime: local.map(|meta| meta.mtime),
            local_size: local.map(|meta| meta.size),
            cloud_hash: cloud.map(|meta| meta.hash.clone()),
            cloud_mtime: cloud.map(|meta| meta.mtime),
            cloud_size: cloud.map(|meta| meta.size),
            last_synced_hash: local
                .map(|meta| meta.hash.clone())
                .or_else(|| cloud.map(|meta| meta.hash.clone())),
            deleted,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use filetime::{set_file_mtime, FileTime};
    use tempfile::TempDir;
    use uuid::Uuid;

    use super::sync_item;
    use crate::rules::Rule;
    use crate::state::{NewItem, StateDb};

    struct Fixture {
        _tmp: TempDir,
        local: std::path::PathBuf,
        cloud: std::path::PathBuf,
        db: StateDb,
        item: crate::state::Item,
        rules: Vec<Rule>,
    }

    impl Fixture {
        fn new(rules: Vec<Rule>) -> Self {
            let tmp = tempfile::tempdir().expect("tempdir");
            let local = tmp.path().join("local");
            let cloud = tmp.path().join("cloud");
            let rule_path = tmp.path().join("rules.ignore");
            fs::create_dir_all(&local).expect("local dir");
            fs::create_dir_all(&cloud).expect("cloud dir");

            let db_path = tmp.path().join("state.sqlite");
            let mut db = StateDb::open(&db_path).expect("db");
            let id = Uuid::new_v4().to_string();
            let local_path = local.to_string_lossy();
            let cloud_path = cloud.to_string_lossy();
            let rule_path = rule_path.to_string_lossy();
            db.insert_item(NewItem {
                id: &id,
                name: "demo",
                item_type: "directory",
                local_path: &local_path,
                cloud_path: &cloud_path,
                rule_path: &rule_path,
                rules: &rules,
            })
            .expect("insert item");
            let item = db.get_item("demo").expect("item");

            Self {
                _tmp: tmp,
                local,
                cloud,
                db,
                item,
                rules,
            }
        }
    }

    #[test]
    fn copies_local_file_to_cloud() {
        let fixture = Fixture::new(vec![]);
        write_file(&fixture.local.join("README.md"), "local", 100);

        let summary = sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("sync");

        assert_eq!(summary.copied_local_to_cloud, 1);
        assert_eq!(read_file(&fixture.cloud.join("README.md")), "local");

        let summary = sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("sync again");
        assert_eq!(summary.unchanged, 1);
    }

    #[test]
    fn copies_cloud_file_to_local() {
        let fixture = Fixture::new(vec![]);
        write_file(&fixture.cloud.join("notes.md"), "cloud", 100);

        let summary = sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("sync");

        assert_eq!(summary.copied_cloud_to_local, 1);
        assert_eq!(read_file(&fixture.local.join("notes.md")), "cloud");
    }

    #[test]
    fn latest_modified_file_wins() {
        let fixture = Fixture::new(vec![]);
        write_file(&fixture.local.join("same.txt"), "old local", 100);
        write_file(&fixture.cloud.join("same.txt"), "new cloud", 200);

        let summary = sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("sync");

        assert_eq!(summary.copied_cloud_to_local, 1);
        assert_eq!(read_file(&fixture.local.join("same.txt")), "new cloud");
    }

    #[test]
    fn deletes_cloud_file_after_local_inner_delete() {
        let fixture = Fixture::new(vec![]);
        let local_file = fixture.local.join("src/a.txt");
        let cloud_file = fixture.cloud.join("src/a.txt");
        write_file(&local_file, "a", 100);

        sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("initial sync");
        assert!(cloud_file.exists());

        fs::remove_file(&local_file).expect("remove local");
        let summary = sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("delete sync");

        assert_eq!(summary.deleted_cloud, 1);
        assert!(!cloud_file.exists());
    }

    #[test]
    fn excludes_matching_paths() {
        let fixture = Fixture::new(vec![Rule {
            pattern: "dist/".to_string(),
        }]);
        write_file(&fixture.local.join("dist/bundle.js"), "ignored", 100);
        write_file(&fixture.local.join("src/app.js"), "included", 100);

        let summary = sync_item(&fixture.db, &fixture.item, &fixture.rules).expect("sync");

        assert_eq!(summary.copied_local_to_cloud, 1);
        assert!(!fixture.cloud.join("dist/bundle.js").exists());
        assert_eq!(read_file(&fixture.cloud.join("src/app.js")), "included");
    }

    #[test]
    fn syncs_single_file_item() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let local = tmp.path().join("note.md");
        let cloud = tmp.path().join("QuickSync/note.md");
        let rule_path = tmp.path().join(".quicksync/rules/note.md.ignore");
        write_file(&local, "local", 100);

        let db_path = tmp.path().join("state.sqlite");
        let mut db = StateDb::open(&db_path).expect("db");
        let id = Uuid::new_v4().to_string();
        let local_path = local.to_string_lossy();
        let cloud_path = cloud.to_string_lossy();
        let rule_path = rule_path.to_string_lossy();
        db.insert_item(NewItem {
            id: &id,
            name: "note.md",
            item_type: "file",
            local_path: &local_path,
            cloud_path: &cloud_path,
            rule_path: &rule_path,
            rules: &[],
        })
        .expect("insert item");
        let item = db.get_item("note.md").expect("item");

        let summary = sync_item(&db, &item, &[]).expect("sync");
        assert_eq!(summary.copied_local_to_cloud, 1);
        assert_eq!(read_file(&cloud), "local");

        write_file(&cloud, "cloud", 200);
        let summary = sync_item(&db, &item, &[]).expect("sync cloud edit");
        assert_eq!(summary.copied_cloud_to_local, 1);
        assert_eq!(read_file(&local), "cloud");
    }

    fn write_file(path: &Path, contents: &str, mtime: i64) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(path, contents).expect("write");
        set_file_mtime(path, FileTime::from_unix_time(mtime, 0)).expect("mtime");
    }

    fn read_file(path: &Path) -> String {
        fs::read_to_string(path).expect("read")
    }
}
