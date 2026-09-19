use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use sha2::{Digest, Sha256};

use crate::rules::{RuleWarning, Rules};
use crate::state::{FileStateUpdate, Item, StateDb, StoredFileState};
use crate::tree::{Kind, Tree};
use crate::{LinkerError, Result};

#[derive(Debug, Clone)]
pub struct SyncSummary {
    pub item_name: String,
    pub copied_local_to_cloud: usize,
    pub copied_cloud_to_local: usize,
    pub deleted_local: usize,
    pub deleted_cloud: usize,
    pub pruned_cloud_directories: usize,
    pub unchanged: usize,
    pub warnings: Vec<RuleWarning>,
    pub rules_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileMeta {
    hash: String,
    size: i64,
    mtime: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
                Decision::Noop
            } else if local.mtime >= cloud.mtime {
                Decision::CopyLocalToCloud
            } else {
                Decision::CopyCloudToLocal
            }
        }
        (Some(local), None) => {
            if prev.is_some_and(|p| {
                p.cloud_hash.is_some()
                    && p.local_hash.as_deref() == Some(&local.hash)
                    && p.local_mtime == Some(local.mtime)
            }) {
                Decision::DeleteLocal
            } else {
                Decision::CopyLocalToCloud
            }
        }
        (None, Some(cloud)) => {
            if prev.is_some_and(|p| {
                p.local_hash.is_some()
                    && p.cloud_hash.as_deref() == Some(&cloud.hash)
                    && p.cloud_mtime == Some(cloud.mtime)
            }) {
                Decision::DeleteCloud
            } else {
                Decision::CopyCloudToLocal
            }
        }
        (None, None) => Decision::Deleted,
    }
}

struct Roots {
    local: Tree,
    cloud: Tree,
    local_file: Option<PathBuf>,
    cloud_file: Option<PathBuf>,
}
impl Roots {
    fn open(item: &Item) -> Result<Self> {
        let local = Path::new(&item.local_path);
        let cloud = Path::new(&item.cloud_path);
        let single = item.item_type == "file";
        Ok(Self {
            local: Tree::open(if single {
                local.parent().unwrap()
            } else {
                local
            })?,
            cloud: Tree::open(if single {
                cloud.parent().unwrap()
            } else {
                cloud
            })?,
            local_file: if single {
                local.file_name().map(PathBuf::from)
            } else {
                None
            },
            cloud_file: if single {
                cloud.file_name().map(PathBuf::from)
            } else {
                None
            },
        })
    }
    fn local_rel<'a>(&'a self, key: &'a Path) -> &'a Path {
        self.local_file.as_deref().unwrap_or(key)
    }
    fn cloud_rel<'a>(&'a self, key: &'a Path) -> &'a Path {
        self.cloud_file.as_deref().unwrap_or(key)
    }
}

struct Operation {
    relative: PathBuf,
    local: Option<FileMeta>,
    cloud: Option<FileMeta>,
    decision: Decision,
}

#[derive(Default)]
struct Plan {
    rules: Rules,
    controls: Vec<Operation>,
    files: Vec<Operation>,
    prune: Vec<(PathBuf, bool)>,
    forget: BTreeSet<String>,
    warnings: Vec<RuleWarning>,
    documents: Vec<(PathBuf, String)>,
    observed: BTreeSet<String>,
}

fn key(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| std::io::Error::other("non-UTF-8 sync path").into())
}

fn read_meta(tree: &Tree, relative: &Path) -> Result<Option<FileMeta>> {
    match tree.kind(relative)? {
        None => Ok(None),
        Some(Kind::File) => {
            let mut file = tree.file(relative)?;
            let before = file.metadata()?;
            let mut hasher = Sha256::new();
            let mut buffer = [0; 64 * 1024];
            loop {
                let n = file.read(&mut buffer)?;
                if n == 0 {
                    break;
                }
                hasher.update(&buffer[..n]);
            }
            let after = file.metadata()?;
            if before.len() != after.len() || before.modified()? != after.modified()? {
                return Err(std::io::Error::other(format!(
                    "file changed during scan: {}",
                    tree.path.join(relative).display()
                ))
                .into());
            }
            Ok(Some(FileMeta {
                hash: format!("{:x}", hasher.finalize()),
                size: after.len() as i64,
                mtime: after
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .map_err(|e| LinkerError::Timestamp(e.to_string()))?
                    .as_secs() as i64,
            }))
        }
        _ => Err(std::io::Error::other(format!(
            "expected regular file: {}",
            tree.path.join(relative).display()
        ))
        .into()),
    }
}

fn operation(
    roots: &Roots,
    relative: &Path,
    previous: &BTreeMap<String, StoredFileState>,
) -> Result<Operation> {
    let local = read_meta(&roots.local, roots.local_rel(relative))?;
    let cloud = read_meta(&roots.cloud, roots.cloud_rel(relative))?;
    let decision = decide(
        local.as_ref(),
        cloud.as_ref(),
        previous.get(&key(relative)?),
    );
    Ok(Operation {
        relative: relative.to_path_buf(),
        local,
        cloud,
        decision,
    })
}

impl Plan {
    fn build(roots: &Roots, previous: &BTreeMap<String, StoredFileState>) -> Result<Self> {
        let mut plan = Self::default();
        if roots.local_file.is_some() {
            plan.files.push(operation(roots, Path::new(""), previous)?);
            return Ok(plan);
        }
        plan.visit(roots, Path::new(""), previous)?;
        for relative in previous.keys() {
            if plan.forget.contains(relative) {
                continue;
            }
            if plan.rules.ignored(Path::new(relative), false) {
                plan.forget.insert(relative.clone());
            } else if !plan.observed.contains(relative) {
                // Do not infer deletion through a symlink or an unexpected file type.
                let rel = Path::new(relative);
                plan.files.push(operation(roots, rel, previous)?);
            }
        }
        Ok(plan)
    }

    fn visit(
        &mut self,
        roots: &Roots,
        directory: &Path,
        previous: &BTreeMap<String, StoredFileState>,
    ) -> Result<()> {
        let control = directory.join(".gitignore");
        let op = operation(roots, &control, previous).map_err(|error| {
            std::io::Error::other(format!(
                "cannot resolve control {}: {error}",
                control.display()
            ))
        })?;
        self.observed.insert(key(&control)?);
        let chosen = match op.decision {
            Decision::Noop | Decision::CopyLocalToCloud => {
                op.local.as_ref().map(|m| (&roots.local, m))
            }
            Decision::CopyCloudToLocal => op.cloud.as_ref().map(|m| (&roots.cloud, m)),
            _ => None,
        };
        if let Some((tree, meta)) = chosen {
            let mut contents = String::new();
            tree.file(&control)?.read_to_string(&mut contents)?;
            if format!("{:x}", Sha256::digest(contents.as_bytes())) != meta.hash {
                return Err(
                    std::io::Error::other("gitignore changed during scan; retry sync").into(),
                );
            }
            self.warnings.extend(
                self.rules
                    .add(directory, &tree.path.join(&control), &contents),
            );
            self.documents.push((control.clone(), meta.hash.clone()));
        }
        // Even missing controls are retained as snapshots to catch newly created rules.
        self.controls.push(op);

        let names: BTreeSet<_> = roots
            .local
            .entries(directory)?
            .into_iter()
            .chain(roots.cloud.entries(directory)?)
            .collect();
        for name in names {
            if name == ".gitignore" {
                continue;
            }
            let rel = directory.join(name);
            let local = roots.local.kind(&rel)?;
            let cloud = roots.cloud.kind(&rel)?;
            let is_dir = local == Some(Kind::Directory) || cloud == Some(Kind::Directory);
            if self.rules.ignored(&rel, is_dir) {
                if cloud.is_some() {
                    self.collect_prune(&roots.cloud, &rel)?;
                }
                self.forget.insert(key(&rel)?);
            } else if local.is_some() && cloud.is_some() && local != cloud {
                return Err(std::io::Error::other(format!(
                    "source/target type conflict: {}",
                    rel.display()
                ))
                .into());
            } else if is_dir {
                self.visit(roots, &rel, previous)?;
            } else if local == Some(Kind::File) || cloud == Some(Kind::File) {
                self.observed.insert(key(&rel)?);
                self.files.push(operation(roots, &rel, previous)?);
            } else {
                // Unsupported filesystem entries are never followed or synchronized.
                self.observed.insert(key(&rel)?);
                self.forget.insert(key(&rel)?);
            }
        }
        Ok(())
    }

    fn collect_prune(&mut self, tree: &Tree, relative: &Path) -> Result<()> {
        let kind = tree.kind(relative)?;
        if kind == Some(Kind::Directory) {
            for name in tree.entries(relative)? {
                self.collect_prune(tree, &relative.join(name))?;
            }
        }
        if let Some(kind) = kind {
            self.prune
                .push((relative.to_path_buf(), kind == Kind::Directory));
        }
        Ok(())
    }

    fn fingerprint(&self) -> String {
        let mut hash = Sha256::new();
        for (rel, content_hash) in &self.documents {
            hash.update(rel.as_os_str().as_encoded_bytes());
            hash.update([0]);
            hash.update(content_hash);
        }
        format!("{:x}", hash.finalize())
    }

    fn verify_controls(&self, roots: &Roots) -> Result<()> {
        if !roots.local.is_current()? || !roots.cloud.is_current()? {
            return Err(std::io::Error::other("association root changed during scan").into());
        }
        for op in &self.controls {
            verify_operation(roots, op)?;
        }
        Ok(())
    }
}

fn verify_operation(roots: &Roots, op: &Operation) -> Result<()> {
    if read_meta(&roots.local, roots.local_rel(&op.relative))? != op.local
        || read_meta(&roots.cloud, roots.cloud_rel(&op.relative))? != op.cloud
    {
        return Err(std::io::Error::other(format!(
            "file changed during sync; retry: {}",
            op.relative.display()
        ))
        .into());
    }
    Ok(())
}

fn apply(
    db: &StateDb,
    item: &Item,
    roots: &Roots,
    op: &Operation,
    summary: &mut SyncSummary,
) -> Result<()> {
    let relative = key(&op.relative)?;
    let mut local = op.local.clone();
    let mut cloud = op.cloud.clone();
    if op.decision != Decision::Deleted {
        verify_operation(roots, op)?;
    }
    match op.decision {
        Decision::Noop => summary.unchanged += 1,
        Decision::CopyLocalToCloud => {
            roots.cloud.copy_from(
                roots.cloud_rel(&op.relative),
                &roots.local,
                roots.local_rel(&op.relative),
                local.as_ref().unwrap().mtime,
            )?;
            cloud = read_meta(&roots.cloud, roots.cloud_rel(&op.relative))?;
            summary.copied_local_to_cloud += 1;
        }
        Decision::CopyCloudToLocal => {
            roots.local.copy_from(
                roots.local_rel(&op.relative),
                &roots.cloud,
                roots.cloud_rel(&op.relative),
                cloud.as_ref().unwrap().mtime,
            )?;
            local = read_meta(&roots.local, roots.local_rel(&op.relative))?;
            summary.copied_cloud_to_local += 1;
        }
        Decision::DeleteLocal => {
            if roots.local.remove(roots.local_rel(&op.relative), false)? {
                summary.deleted_local += 1;
            }
            local = None;
        }
        Decision::DeleteCloud => {
            if roots.cloud.remove(roots.cloud_rel(&op.relative), false)? {
                summary.deleted_cloud += 1;
            }
            cloud = None;
        }
        Decision::Deleted => {
            // Absent controls with no history must not grow the database every pass.
            db.forget_file_state(&item.id, &relative)?;
            return Ok(());
        }
    }
    db.upsert_file_state(
        &item.id,
        &FileStateUpdate {
            relative_path: relative,
            local_hash: local.as_ref().map(|m| m.hash.clone()),
            local_mtime: local.as_ref().map(|m| m.mtime),
            local_size: local.as_ref().map(|m| m.size),
            cloud_hash: cloud.as_ref().map(|m| m.hash.clone()),
            cloud_mtime: cloud.as_ref().map(|m| m.mtime),
            cloud_size: cloud.as_ref().map(|m| m.size),
            last_synced_hash: local.as_ref().or(cloud.as_ref()).map(|m| m.hash.clone()),
            deleted: local.is_none() && cloud.is_none(),
        },
    )
}

pub fn sync_item(db: &StateDb, item: &Item) -> Result<SyncSummary> {
    let _lock = db.lock_item(&item.id)?;
    let item = db.get_item(&item.id)?;
    let _source_lock = db.lock_source(&item.local_path)?;
    let result = run_sync(db, &item, false);
    if let Err(error) = &result {
        db.mark_item_error(&item.id, &error.to_string())?;
    }
    result
}

fn previous(db: &StateDb, item: &Item) -> Result<BTreeMap<String, StoredFileState>> {
    Ok(db
        .list_file_states(&item.id)?
        .into_iter()
        .map(|s| (s.relative_path.clone(), s))
        .collect())
}

/// Caller holds both item and source locks through publication and rollback.
pub(crate) fn sync_initial_item(db: &StateDb, item: &Item) -> Result<SyncSummary> {
    run_sync(db, item, true)
}

fn run_sync(db: &StateDb, item: &Item, initial: bool) -> Result<SyncSummary> {
    if !Path::new(&item.local_path).exists() {
        return Err(LinkerError::PathMissing(item.local_path.clone().into()));
    }
    let cloud = Path::new(&item.cloud_path);
    if !initial && !cloud.exists() {
        fs::create_dir_all(if item.item_type == "file" {
            cloud.parent().unwrap()
        } else {
            cloud
        })?;
    }
    let roots = Roots::open(item)?;
    if initial && !roots.cloud.entries(Path::new(""))?.is_empty() {
        return Err(LinkerError::TargetNotEmpty(cloud.into()));
    }
    let plan = Plan::build(&roots, &previous(db, item)?)?;
    plan.verify_controls(&roots)?;
    if initial
        && (!roots.cloud.entries(Path::new(""))?.is_empty()
            || !plan.prune.is_empty()
            || plan.controls.iter().chain(&plan.files).any(|op| {
                op.cloud.is_some()
                    || !matches!(
                        op.decision,
                        Decision::CopyLocalToCloud | Decision::Noop | Decision::Deleted
                    )
            }))
    {
        return Err(LinkerError::TargetNotEmpty(cloud.into()));
    }
    let mut summary = SyncSummary {
        item_name: item.name.clone(),
        copied_local_to_cloud: 0,
        copied_cloud_to_local: 0,
        deleted_local: 0,
        deleted_cloud: 0,
        pruned_cloud_directories: 0,
        unchanged: 0,
        warnings: plan.warnings.clone(),
        rules_fingerprint: plan.fingerprint(),
    };
    // Retire baselines BEFORE pruning. A partial failure must never turn cleanup
    // into a user deletion that propagates back to the source after a rule edit.
    for rel in &plan.forget {
        db.forget_file_state(&item.id, rel)?;
    }
    for op in &plan.controls {
        apply(db, item, &roots, op, &mut summary)?;
    }
    for (rel, directory) in &plan.prune {
        let removed = roots.cloud.remove(rel, *directory).map_err(|error| {
            std::io::Error::other(format!(
                "target cleanup failed at {}: {error}; already removed {} file(s)/link(s) and {} directorie(s)",
                roots.cloud.path.join(rel).display(), summary.deleted_cloud, summary.pruned_cloud_directories
            ))
        })?;
        if removed {
            if *directory {
                summary.pruned_cloud_directories += 1;
            } else {
                summary.deleted_cloud += 1;
            }
        }
    }
    for op in &plan.files {
        apply(db, item, &roots, op, &mut summary)?;
    }
    db.mark_item_synced(&item.id)?;
    Ok(summary)
}

/// Read-only operation inventory for deployment/backup tooling. Both roots must exist.
#[derive(Debug, serde::Serialize)]
pub struct SyncPreview {
    pub item_name: String,
    pub operations: Vec<PreviewOperation>,
    pub warnings: Vec<RuleWarning>,
}
#[derive(Debug, serde::Serialize)]
pub struct PreviewOperation {
    pub action: String,
    pub path: PathBuf,
}
pub fn preview_item(db: &StateDb, item: &Item) -> Result<SyncPreview> {
    let _lock = db.lock_item(&item.id)?;
    let item = db.get_item(&item.id)?;
    let _source_lock = db.lock_source(&item.local_path)?;
    let roots = Roots::open(&item)?;
    let plan = Plan::build(&roots, &previous(db, &item)?)?;
    plan.verify_controls(&roots)?;
    let mut operations = Vec::new();
    for op in plan.controls.iter().chain(&plan.files) {
        let (action, path) = match op.decision {
            Decision::CopyLocalToCloud => (
                "write_target",
                roots.cloud.path.join(roots.cloud_rel(&op.relative)),
            ),
            Decision::CopyCloudToLocal => (
                "write_source",
                roots.local.path.join(roots.local_rel(&op.relative)),
            ),
            Decision::DeleteLocal => (
                "delete_source",
                roots.local.path.join(roots.local_rel(&op.relative)),
            ),
            Decision::DeleteCloud => (
                "delete_target",
                roots.cloud.path.join(roots.cloud_rel(&op.relative)),
            ),
            _ => continue,
        };
        operations.push(PreviewOperation {
            action: action.into(),
            path,
        });
    }
    for (rel, dir) in &plan.prune {
        operations.push(PreviewOperation {
            action: if *dir {
                "prune_target_directory"
            } else {
                "prune_target_file"
            }
            .into(),
            path: roots.cloud.path.join(rel),
        });
    }
    Ok(SyncPreview {
        item_name: item.name.clone(),
        operations,
        warnings: plan.warnings,
    })
}
