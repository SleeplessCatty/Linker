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
    /// The target root was absent at the start of this pass while the baselines
    /// still recorded target content. Its content was restored from the source
    /// instead of being deleted as a per-file removal.
    pub target_root_recovered: bool,
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

/// A target root that disappeared is an environment event, not a per-file
/// deletion, so a deletion inferred from a baseline is restored instead.
fn restore_guard(decision: Decision, restore_only: bool) -> Decision {
    if restore_only && decision == Decision::DeleteLocal {
        Decision::CopyLocalToCloud
    } else {
        decision
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
    /// Populated only by lenient scans: a path whose source and target entry
    /// types disagree, which ordinary sync refuses to reconcile.
    conflicts: Vec<(PathBuf, String)>,
    /// Entries that are neither regular files nor directories. They are never
    /// followed or synchronized, so they need their own inventory for auditing.
    unsupported: Vec<(PathBuf, String)>,
    /// Entries that must not be descended into: type conflicts and unsupported
    /// entries. Baseline paths inside them are left untouched instead of being
    /// read through the wrong entry kind.
    opaque: Vec<PathBuf>,
    /// How this scan resolves content, controls and conflicts.
    options: PlanOptions,
}

/// Scan behaviour. Ordinary sync is strict and resolves content by modification
/// time; audits collect conflicts and may resolve controls by a fixed side.
#[derive(Debug, Clone, Copy)]
struct PlanOptions {
    /// Abort on a file/directory type conflict instead of collecting it.
    strict: bool,
    /// Side that resolves control files (`.gitignore`) when both differ.
    preference: Preference,
    /// The target root was absent when the pass started. A vanished root is an
    /// environment event, not a per-file deletion, so a source deletion that a
    /// baseline would otherwise infer is restored instead.
    restore_only: bool,
}

impl Default for PlanOptions {
    /// Fail closed: strict conflict handling and modification-time resolution.
    fn default() -> Self {
        Self::sync(false)
    }
}

impl PlanOptions {
    fn sync(restore_only: bool) -> Self {
        Self {
            strict: true,
            preference: Preference::Newest,
            restore_only,
        }
    }

    fn audit(preference: Preference) -> Self {
        Self {
            strict: false,
            preference,
            restore_only: false,
        }
    }
}

fn describe_kind(kind: Option<Kind>) -> &'static str {
    match kind {
        Some(Kind::File) => "a regular file",
        Some(Kind::Directory) => "a directory",
        Some(Kind::Symlink) => "a symbolic link",
        Some(Kind::Other) => "a special file",
        None => "absent",
    }
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
    options: PlanOptions,
) -> Result<Operation> {
    let local = read_meta(&roots.local, roots.local_rel(relative))?;
    let cloud = read_meta(&roots.cloud, roots.cloud_rel(relative))?;
    let decision = restore_guard(
        decide(
            local.as_ref(),
            cloud.as_ref(),
            previous.get(&key(relative)?),
        ),
        options.restore_only,
    );
    Ok(Operation {
        relative: relative.to_path_buf(),
        local,
        cloud,
        decision,
    })
}

impl Plan {
    fn build(
        roots: &Roots,
        previous: &BTreeMap<String, StoredFileState>,
        options: PlanOptions,
        global_ignore: &Path,
    ) -> Result<Self> {
        let mut plan = Self {
            options,
            ..Self::default()
        };
        if roots.local_file.is_some() {
            plan.files
                .push(operation(roots, Path::new(""), previous, options)?);
            return Ok(plan);
        }
        plan.load_global_rules(global_ignore)?;
        plan.visit(roots, Path::new(""), previous)?;
        for relative in previous.keys() {
            if plan.forget.contains(relative) {
                continue;
            }
            if plan.rules.ignored(Path::new(relative), false) {
                plan.forget.insert(relative.clone());
            } else if !plan.observed.contains(relative) && !plan.opaque_path(Path::new(relative)) {
                // Do not infer deletion through a symlink or an unexpected file type.
                let rel = Path::new(relative);
                plan.files.push(operation(roots, rel, previous, options)?);
            }
        }
        Ok(plan)
    }

    /// Optional Linker-level rules from the directory holding the state
    /// database. They apply at the association root, before any in-tree
    /// control, are additive with it, and are never synchronized.
    fn load_global_rules(&mut self, path: &Path) -> Result<()> {
        let contents = match fs::read_to_string(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(std::io::Error::other(format!(
                    "cannot read the global ignore file {}: {error}",
                    path.display()
                ))
                .into())
            }
        };
        self.warnings
            .extend(self.rules.add(Path::new(""), path, &contents));
        self.documents.push((
            path.to_path_buf(),
            format!("{:x}", Sha256::digest(contents.as_bytes())),
        ));
        Ok(())
    }

    fn visit(
        &mut self,
        roots: &Roots,
        directory: &Path,
        previous: &BTreeMap<String, StoredFileState>,
    ) -> Result<()> {
        let control = directory.join(".gitignore");
        let stored = key(&control)?;
        let op = operation(roots, &control, previous, self.options).map_err(|error| {
            std::io::Error::other(format!(
                "cannot resolve control {}: {error}",
                control.display()
            ))
        })?;
        self.observed.insert(stored.clone());
        let chosen = match resolve(self.options.preference, &op, previous.get(&stored)) {
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
                if self.options.strict {
                    return Err(std::io::Error::other(format!(
                        "source/target type conflict: {}",
                        rel.display()
                    ))
                    .into());
                }
                // A directory cannot be descended into where the other side is a
                // file, so record the path and leave both entries untouched.
                self.observed.insert(key(&rel)?);
                self.opaque.push(rel.clone());
                self.conflicts.push((
                    rel.clone(),
                    format!(
                        "source is {}, target is {}",
                        describe_kind(local),
                        describe_kind(cloud)
                    ),
                ));
            } else if is_dir {
                self.visit(roots, &rel, previous)?;
            } else if local == Some(Kind::File) || cloud == Some(Kind::File) {
                self.observed.insert(key(&rel)?);
                self.files
                    .push(operation(roots, &rel, previous, self.options)?);
            } else {
                // Unsupported filesystem entries are never followed or synchronized.
                self.observed.insert(key(&rel)?);
                self.forget.insert(key(&rel)?);
                self.opaque.push(rel.clone());
                self.unsupported.push((
                    rel.clone(),
                    format!(
                        "source is {}, target is {}",
                        describe_kind(local),
                        describe_kind(cloud)
                    ),
                ));
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

    /// True when the path lies inside an entry that must not be descended into.
    fn opaque_path(&self, relative: &Path) -> bool {
        self.opaque
            .iter()
            .any(|prefix| relative.starts_with(prefix))
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
    // A missing target root is recreated for an existing association.
    let target_was_missing = !initial && !cloud.exists();
    if !initial && target_was_missing {
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
    // A target root that is entirely gone while the baselines still record
    // target content is an environment event, not the per-file deletion this
    // model propagates: every target file is missing at once, and treating that
    // as a removal would delete the whole source tree. Restore from the source
    // and report it. An existing root still uses ordinary per-file semantics;
    // `linker repair` refills a root that was emptied in place.
    let previous = previous(db, item)?;
    let target_root_recovered =
        !initial && target_was_missing && previous.values().any(|state| state.cloud_hash.is_some());
    let plan = Plan::build(
        &roots,
        &previous,
        PlanOptions::sync(target_root_recovered),
        &db.global_ignore_path(),
    )?;
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
        target_root_recovered,
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
    let plan = previous(db, &item)?;
    let roots = Roots::open(&item)?;
    let global_ignore = db.global_ignore_path();
    let plan = Plan::build(&roots, &plan, PlanOptions::sync(false), &global_ignore)?;
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

// ---------------------------------------------------------------------------
// Manual consistency audit and repair.
//
// Ordinary sync converges what its baselines can explain and resolves conflicts
// by modification time. A manual audit and repair exists for everything else:
// divergent content, one-sided paths, entries whose types disagree, and ignored
// target content, with the authoritative side stated explicitly by the caller
// instead of inferred.
// ---------------------------------------------------------------------------

/// Which side is authoritative when both sides hold different content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preference {
    Source,
    Target,
    Newest,
}

impl Preference {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "source" => Some(Self::Source),
            "target" => Some(Self::Target),
            "newest" => Some(Self::Newest),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Target => "target",
            Self::Newest => "newest",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckEntry {
    /// content_differs, source_only, target_only, type_conflict,
    /// unsupported_entry or ignored_target_content.
    pub class: String,
    /// Affected side: source, target or both.
    pub side: String,
    pub path: PathBuf,
    pub detail: String,
    /// True when the association is not in the requested consistent state.
    pub blocks: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CheckReport {
    pub item_name: String,
    pub source_path: String,
    pub target_path: String,
    pub identical: usize,
    pub entries: Vec<CheckEntry>,
    pub warnings: Vec<RuleWarning>,
}

impl CheckReport {
    pub fn divergent(&self) -> usize {
        self.entries.iter().filter(|entry| entry.blocks).count()
    }

    pub fn advisory(&self) -> usize {
        self.entries.len() - self.divergent()
    }

    pub fn clean(&self) -> bool {
        self.divergent() == 0
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RepairAction {
    /// write_target, write_source, delete_target, delete_source,
    /// prune_target_file, prune_target_directory, replace_target,
    /// replace_source, skip_type_conflict or skip_unsupported.
    pub action: String,
    pub side: String,
    pub path: PathBuf,
    pub note: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RepairReport {
    pub item_name: String,
    pub source_path: String,
    pub target_path: String,
    pub preference: String,
    pub prune: bool,
    pub dry_run: bool,
    pub identical: usize,
    /// Applied actions, or the actions a dry run would apply.
    pub planned: Vec<RepairAction>,
    /// Actions that were not applied, with the reason.
    pub skipped: Vec<RepairAction>,
    pub warnings: Vec<RuleWarning>,
}

struct Analysis {
    roots: Roots,
    plan: Plan,
    previous: BTreeMap<String, StoredFileState>,
}

fn analyze(db: &StateDb, item: &Item, preference: Preference) -> Result<Analysis> {
    let roots = Roots::open(item)?;
    let previous = previous(db, item)?;
    // Audits collect type conflicts instead of aborting, and resolve control
    // files by the requested side so rules and content decisions agree.
    let plan = Plan::build(
        &roots,
        &previous,
        PlanOptions::audit(preference),
        &db.global_ignore_path(),
    )?;
    plan.verify_controls(&roots)?;
    Ok(Analysis {
        roots,
        plan,
        previous,
    })
}

/// Both roots must exist for an audit or a manual repair. An ordinary sync
/// recreates a missing target root; a retained single-file association keeps
/// its target file optional because an absent file is ordinary content.
fn require_existing_roots(item: &Item) -> Result<()> {
    if !Path::new(&item.local_path).exists() {
        return Err(LinkerError::PathMissing(item.local_path.clone().into()));
    }
    let target = Path::new(&item.cloud_path);
    let required = if item.item_type == "file" {
        target.parent().unwrap_or(target)
    } else {
        target
    };
    if !required.exists() {
        return Err(LinkerError::PathMissing(required.to_path_buf()));
    }
    Ok(())
}

fn newer_side(local: &FileMeta, cloud: &FileMeta) -> String {
    if local.mtime > cloud.mtime {
        format!("source is newer by {}s", local.mtime - cloud.mtime)
    } else if cloud.mtime > local.mtime {
        format!("target is newer by {}s", cloud.mtime - local.mtime)
    } else {
        "both sides carry the same modification time".into()
    }
}

/// Which side the recorded baseline still matches, if either.
fn relationship(prev: Option<&StoredFileState>, local: &FileMeta, cloud: &FileMeta) -> String {
    let local_synced = prev.is_some_and(|state| {
        state.local_hash.as_deref() == Some(&local.hash) && state.local_mtime == Some(local.mtime)
    });
    let cloud_synced = prev.is_some_and(|state| {
        state.cloud_hash.as_deref() == Some(&cloud.hash) && state.cloud_mtime == Some(cloud.mtime)
    });
    match (local_synced, cloud_synced) {
        (true, false) => "the target changed after the last sync".into(),
        (false, true) => "the source changed after the last sync".into(),
        (false, false) if prev.is_some() => "both sides changed after the last sync".into(),
        (false, false) => "no baseline is recorded for this path".into(),
        (true, true) => "both sides still match the baseline".into(),
    }
}

/// Read-only audit. Creates only the synchronization lock files an ordinary
/// preview also creates; never changes synced content or baselines.
pub fn check_item(db: &StateDb, item: &Item) -> Result<CheckReport> {
    let _lock = db.lock_item(&item.id)?;
    let item = db.get_item(&item.id)?;
    let _source_lock = db.lock_source(&item.local_path)?;
    require_existing_roots(&item)?;
    let analysis = analyze(db, &item, Preference::Newest)?;
    let mut report = CheckReport {
        item_name: item.name.clone(),
        source_path: item.local_path.clone(),
        target_path: item.cloud_path.clone(),
        identical: 0,
        entries: Vec::new(),
        warnings: analysis.plan.warnings.clone(),
    };
    for op in analysis.plan.controls.iter().chain(&analysis.plan.files) {
        let stored = key(&op.relative)?;
        let prev = analysis.previous.get(&stored);
        let source_path = analysis
            .roots
            .local
            .path
            .join(analysis.roots.local_rel(&op.relative));
        let target_path = analysis
            .roots
            .cloud
            .path
            .join(analysis.roots.cloud_rel(&op.relative));
        match (op.local.as_ref(), op.cloud.as_ref()) {
            (None, None) => {}
            (Some(local), Some(cloud)) if local.hash == cloud.hash => report.identical += 1,
            (Some(local), Some(cloud)) => report.entries.push(CheckEntry {
                class: "content_differs".into(),
                side: "both".into(),
                path: source_path,
                detail: format!("{}; {}", newer_side(local, cloud), relationship(prev, local, cloud)),
                blocks: true,
            }),
            (Some(_), None) => report.entries.push(CheckEntry {
                class: "source_only".into(),
                side: "source".into(),
                path: source_path,
                detail: if op.decision == Decision::DeleteLocal {
                    "the target copy was removed after the last sync, so a normal sync deletes this source file; repair with --prefer source copies it back to the target"
                        .into()
                } else {
                    "the source content is not in the target; a normal sync copies it to the target".into()
                },
                blocks: true,
            }),
            (None, Some(_)) => report.entries.push(CheckEntry {
                class: "target_only".into(),
                side: "target".into(),
                path: target_path,
                detail: if op.decision == Decision::DeleteCloud {
                    "the source copy was removed after the last sync, so a normal sync deletes this target file".into()
                } else {
                    "the target content is not in the source; a normal sync copies it to the source".into()
                },
                blocks: true,
            }),
        }
    }
    for (relative, detail) in &analysis.plan.conflicts {
        report.entries.push(CheckEntry {
            class: "type_conflict".into(),
            side: "both".into(),
            path: analysis.roots.local.path.join(relative),
            detail: format!(
                "{detail}; a normal sync fails this association until one side is changed"
            ),
            blocks: true,
        });
    }
    for (relative, directory) in &analysis.plan.prune {
        report.entries.push(CheckEntry {
            class: "ignored_target_content".into(),
            side: "target".into(),
            path: analysis.roots.cloud.path.join(relative),
            detail: format!(
                "target {} matches .gitignore; the source copy is kept and target cleanup removes this one",
                if *directory { "directory" } else { "file" }
            ),
            blocks: false,
        });
    }
    for (relative, detail) in &analysis.plan.unsupported {
        report.entries.push(CheckEntry {
            class: "unsupported_entry".into(),
            side: "both".into(),
            path: analysis.roots.local.path.join(relative),
            detail: format!("{detail}; symbolic links and special files are never synchronized"),
            blocks: false,
        });
    }
    report
        .entries
        .sort_by(|first, second| first.path.cmp(&second.path));
    Ok(report)
}

/// Which decision the requested side applies to one path. `Newest` reproduces
/// ordinary sync exactly; the fixed sides override every content choice.
fn resolve(preference: Preference, op: &Operation, prev: Option<&StoredFileState>) -> Decision {
    let (local, cloud) = (op.local.as_ref(), op.cloud.as_ref());
    match (local, cloud) {
        (Some(local), Some(cloud)) => {
            if local.hash == cloud.hash {
                Decision::Noop
            } else {
                match preference {
                    Preference::Source => Decision::CopyLocalToCloud,
                    Preference::Target => Decision::CopyCloudToLocal,
                    Preference::Newest => {
                        if local.mtime >= cloud.mtime {
                            Decision::CopyLocalToCloud
                        } else {
                            Decision::CopyCloudToLocal
                        }
                    }
                }
            }
        }
        (Some(_), None) => match preference {
            Preference::Source => Decision::CopyLocalToCloud,
            Preference::Target => Decision::DeleteLocal,
            Preference::Newest => decide(local, cloud, prev),
        },
        (None, Some(_)) => match preference {
            Preference::Source => Decision::DeleteCloud,
            Preference::Target => Decision::CopyCloudToLocal,
            Preference::Newest => decide(local, cloud, prev),
        },
        (None, None) => Decision::Deleted,
    }
}

fn resolved_operation(op: &Operation, decision: Decision) -> Operation {
    Operation {
        relative: op.relative.clone(),
        local: op.local.clone(),
        cloud: op.cloud.clone(),
        decision,
    }
}

fn collect_files(tree: &Tree, relative: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    match tree.kind(relative)? {
        Some(Kind::File) => out.push(relative.to_path_buf()),
        Some(Kind::Directory) => {
            for name in tree.entries(relative)? {
                collect_files(tree, &relative.join(name), out)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Borrowed scan of one association, shared by every repair decision.
struct RepairScope<'a> {
    roots: &'a Roots,
    plan: &'a Plan,
    previous: &'a BTreeMap<String, StoredFileState>,
}

struct RepairPass<'a> {
    db: &'a StateDb,
    item: &'a Item,
    scope: RepairScope<'a>,
    preference: Preference,
    prune: bool,
    dry_run: bool,
    summary: SyncSummary,
    report: RepairReport,
}

impl<'a> RepairPass<'a> {
    fn new(
        db: &'a StateDb,
        item: &'a Item,
        scope: RepairScope<'a>,
        preference: Preference,
        prune: bool,
        dry_run: bool,
    ) -> Self {
        let plan = scope.plan;
        let report = RepairReport {
            item_name: item.name.clone(),
            source_path: item.local_path.clone(),
            target_path: item.cloud_path.clone(),
            preference: preference.label().into(),
            prune,
            dry_run,
            identical: 0,
            planned: Vec::new(),
            skipped: Vec::new(),
            warnings: plan.warnings.clone(),
        };
        let summary = SyncSummary {
            item_name: item.name.clone(),
            copied_local_to_cloud: 0,
            copied_cloud_to_local: 0,
            deleted_local: 0,
            deleted_cloud: 0,
            pruned_cloud_directories: 0,
            unchanged: 0,
            warnings: Vec::new(),
            rules_fingerprint: plan.fingerprint(),
            target_root_recovered: false,
        };
        Self {
            db,
            item,
            scope,
            preference,
            prune,
            dry_run,
            summary,
            report,
        }
    }

    fn run(mut self) -> Result<RepairReport> {
        let plan = self.scope.plan;
        let roots = self.scope.roots;
        // Retire ignored baselines before any cleanup, exactly as sync does: a
        // partial failure must never turn cleanup into a source deletion later.
        if self.prune && !self.dry_run {
            for relative in &plan.forget {
                self.db.forget_file_state(&self.item.id, relative)?;
            }
        }
        let operations: Vec<&Operation> = plan.controls.iter().chain(&plan.files).collect();
        for op in operations {
            self.resolve(op)?;
        }
        for (relative, detail) in &plan.conflicts {
            self.conflict(relative, detail)?;
        }
        self.ignored_target_content()?;
        for (relative, detail) in &plan.unsupported {
            self.report.skipped.push(RepairAction {
                action: "skip_unsupported".into(),
                side: "both".into(),
                path: roots.local.path.join(relative),
                note: format!("{detail}; symbolic links and special files are never synchronized"),
            });
        }
        // Deterministic output regardless of scan order.
        self.report
            .planned
            .sort_by(|first, second| first.path.cmp(&second.path));
        self.report
            .skipped
            .sort_by(|first, second| first.path.cmp(&second.path));
        if !self.dry_run {
            self.db.mark_item_synced(&self.item.id)?;
        }
        Ok(self.report)
    }

    fn resolve(&mut self, op: &Operation) -> Result<()> {
        let db = self.db;
        let item = self.item;
        let roots = self.scope.roots;
        let stored = key(&op.relative)?;
        let prev = self.scope.previous.get(&stored);
        let decision = resolve(self.preference, op, prev);
        // Whether ordinary sync reaches the same decision decides how a removal
        // is explained: a recorded deletion versus authority alone.
        let baseline_backed = decision == decide(op.local.as_ref(), op.cloud.as_ref(), prev);
        let source_path = roots.local.path.join(roots.local_rel(&op.relative));
        let target_path = roots.cloud.path.join(roots.cloud_rel(&op.relative));
        match decision {
            Decision::Noop => self.report.identical += 1,
            Decision::Deleted => {}
            Decision::CopyLocalToCloud | Decision::CopyCloudToLocal => {
                let source_wins = decision == Decision::CopyLocalToCloud;
                self.report.planned.push(RepairAction {
                    action: if source_wins {
                        "write_target"
                    } else {
                        "write_source"
                    }
                    .into(),
                    side: if source_wins { "target" } else { "source" }.into(),
                    path: if source_wins {
                        target_path
                    } else {
                        source_path
                    },
                    note: if source_wins {
                        format!(
                            "the source copy is authoritative ({})",
                            self.preference.label()
                        )
                    } else {
                        format!(
                            "the target copy is authoritative ({})",
                            self.preference.label()
                        )
                    },
                });
                if !self.dry_run {
                    let resolved = resolved_operation(op, decision);
                    apply(db, item, roots, &resolved, &mut self.summary)?;
                }
            }
            Decision::DeleteLocal | Decision::DeleteCloud => {
                let delete_source = decision == Decision::DeleteLocal;
                let action = if delete_source {
                    "delete_source"
                } else {
                    "delete_target"
                };
                let path = if delete_source {
                    source_path
                } else {
                    target_path
                };
                let evidence = if baseline_backed {
                    "the other side deleted its copy after the last sync"
                } else {
                    "the authoritative side never recorded this path"
                };
                if !self.prune {
                    self.report.skipped.push(RepairAction {
                        action: action.into(),
                        side: if delete_source { "source" } else { "target" }.into(),
                        path,
                        note: format!("{evidence}; rerun with --prune to remove it"),
                    });
                    return Ok(());
                }
                self.report.planned.push(RepairAction {
                    action: action.into(),
                    side: if delete_source { "source" } else { "target" }.into(),
                    path,
                    note: format!("removed: {evidence}"),
                });
                if !self.dry_run {
                    let resolved = resolved_operation(op, decision);
                    apply(db, item, roots, &resolved, &mut self.summary)?;
                }
            }
        }
        Ok(())
    }

    fn conflict(&mut self, relative: &Path, detail: &str) -> Result<()> {
        if self.preference == Preference::Newest {
            self.report.skipped.push(RepairAction {
                action: "skip_type_conflict".into(),
                side: "both".into(),
                path: self.scope.roots.local.path.join(relative),
                note: format!("{detail}; a type conflict has no newest side, rerun with --prefer source or --prefer target"),
            });
            return Ok(());
        }
        let (db, item, roots, previous) =
            (self.db, self.item, self.scope.roots, self.scope.previous);
        let options = self.scope.plan.options;
        let source_wins = self.preference == Preference::Source;
        let winner_side = if source_wins { "source" } else { "target" };
        let loser_side = if source_wins { "target" } else { "source" };
        let winner = if source_wins {
            &roots.local
        } else {
            &roots.cloud
        };
        let loser = if source_wins {
            &roots.cloud
        } else {
            &roots.local
        };
        let action = if source_wins {
            "replace_target"
        } else {
            "replace_source"
        };
        let mut files = Vec::new();
        collect_files(winner, relative, &mut files)?;
        let mirror = if files.is_empty() {
            "the winner holds an empty directory, which Linker does not mirror".to_string()
        } else {
            format!(
                "and {} file(s) from the {winner_side} are copied",
                files.len()
            )
        };
        if !self.prune {
            self.report.skipped.push(RepairAction {
                action: action.into(),
                side: loser_side.into(),
                path: loser.path.join(relative),
                note: format!(
                    "{detail}; replacing the {loser_side} entry requires --prune ({mirror})"
                ),
            });
            return Ok(());
        }
        self.report.planned.push(RepairAction {
            action: action.into(),
            side: loser_side.into(),
            path: loser.path.join(relative),
            note: format!("{detail}; the {winner_side} entry replaces it, {mirror}"),
        });
        if self.dry_run {
            return Ok(());
        }
        loser.remove_recursive(relative)?;
        for file in files {
            let op = operation(roots, &file, previous, options)?;
            let decision = if source_wins {
                Decision::CopyLocalToCloud
            } else {
                Decision::CopyCloudToLocal
            };
            let resolved = Operation {
                relative: file,
                local: op.local,
                cloud: op.cloud,
                decision,
            };
            apply(db, item, roots, &resolved, &mut self.summary)?;
        }
        Ok(())
    }

    fn ignored_target_content(&mut self) -> Result<()> {
        let plan = self.scope.plan;
        let roots = self.scope.roots;
        for (relative, directory) in &plan.prune {
            let action = if *directory {
                "prune_target_directory"
            } else {
                "prune_target_file"
            };
            let path = roots.cloud.path.join(relative);
            if !self.prune {
                self.report.skipped.push(RepairAction {
                    action: action.into(),
                    side: "target".into(),
                    path,
                    note: "matches .gitignore; the source copy is kept and the target copy is removed only with --prune".into(),
                });
                continue;
            }
            self.report.planned.push(RepairAction {
                action: action.into(),
                side: "target".into(),
                path,
                note: "ignored target content removed, matching an ordinary sync".into(),
            });
            if !self.dry_run {
                let removed = roots.cloud.remove(relative, *directory).map_err(|error| {
                    std::io::Error::other(format!(
                        "target cleanup failed at {}: {error}",
                        roots.cloud.path.join(relative).display()
                    ))
                })?;
                if removed {
                    if *directory {
                        self.summary.pruned_cloud_directories += 1;
                    } else {
                        self.summary.deleted_cloud += 1;
                    }
                }
            }
        }
        Ok(())
    }
}

/// Make every diverging non-ignored path match the requested side. Deletions
/// happen only with `prune`; without it one-sided paths are reported instead.
pub fn repair_item(
    db: &StateDb,
    item: &Item,
    preference: Preference,
    prune: bool,
    dry_run: bool,
) -> Result<RepairReport> {
    let _lock = db.lock_item(&item.id)?;
    let item = db.get_item(&item.id)?;
    let _source_lock = db.lock_source(&item.local_path)?;
    let result = run_repair(db, &item, preference, prune, dry_run);
    if let Err(error) = &result {
        if !dry_run {
            db.mark_item_error(&item.id, &error.to_string())?;
        }
    }
    result
}

fn run_repair(
    db: &StateDb,
    item: &Item,
    preference: Preference,
    prune: bool,
    dry_run: bool,
) -> Result<RepairReport> {
    require_existing_roots(item)?;
    let analysis = analyze(db, item, preference)?;
    RepairPass::new(
        db,
        item,
        RepairScope {
            roots: &analysis.roots,
            plan: &analysis.plan,
            previous: &analysis.previous,
        },
        preference,
        prune,
        dry_run,
    )
    .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(hash: &str, mtime: i64) -> FileMeta {
        FileMeta {
            hash: hash.into(),
            size: 1,
            mtime,
        }
    }

    fn op(local: Option<FileMeta>, cloud: Option<FileMeta>) -> Operation {
        Operation {
            relative: PathBuf::from("file.txt"),
            local,
            cloud,
            decision: Decision::Noop,
        }
    }

    /// A baseline that still matches a local copy of `local`, with the cloud
    /// copy recorded and gone.
    fn recorded_deletion(local: &FileMeta) -> StoredFileState {
        StoredFileState {
            relative_path: "file.txt".into(),
            local_hash: Some(local.hash.clone()),
            local_mtime: Some(local.mtime),
            local_size: Some(local.size),
            cloud_hash: Some("cloud".into()),
            cloud_mtime: Some(local.mtime),
            cloud_size: Some(local.size),
            last_synced_hash: Some(local.hash.clone()),
            deleted: false,
        }
    }

    #[test]
    fn a_fixed_side_overrides_the_modification_time_rule() {
        let local = meta("local", 10);
        let cloud = meta("cloud", 20);
        let differing = op(Some(local.clone()), Some(cloud.clone()));
        assert_eq!(
            resolve(Preference::Source, &differing, None),
            Decision::CopyLocalToCloud
        );
        assert_eq!(
            resolve(Preference::Target, &differing, None),
            Decision::CopyCloudToLocal
        );
        assert_eq!(
            resolve(Preference::Newest, &differing, None),
            Decision::CopyCloudToLocal
        );
        assert_eq!(
            resolve(
                Preference::Newest,
                &op(Some(meta("local", 30)), Some(cloud)),
                None
            ),
            Decision::CopyLocalToCloud
        );
        assert_eq!(
            resolve(
                Preference::Source,
                &op(Some(local.clone()), Some(local.clone())),
                None
            ),
            Decision::Noop
        );
    }

    #[test]
    fn a_fixed_side_decides_one_sided_paths_without_a_baseline() {
        let local = meta("local", 10);
        let cloud = meta("cloud", 10);
        let source_only = op(Some(local.clone()), None);
        assert_eq!(
            resolve(Preference::Source, &source_only, None),
            Decision::CopyLocalToCloud
        );
        assert_eq!(
            resolve(Preference::Target, &source_only, None),
            Decision::DeleteLocal
        );
        assert_eq!(
            resolve(Preference::Newest, &source_only, None),
            Decision::CopyLocalToCloud
        );
        let target_only = op(None, Some(cloud.clone()));
        assert_eq!(
            resolve(Preference::Source, &target_only, None),
            Decision::DeleteCloud
        );
        assert_eq!(
            resolve(Preference::Target, &target_only, None),
            Decision::CopyCloudToLocal
        );
    }

    #[test]
    fn a_recorded_deletion_still_propagates_under_the_newest_rule() {
        let local = meta("local", 10);
        let previous = recorded_deletion(&local);
        assert_eq!(
            resolve(
                Preference::Newest,
                &op(Some(local.clone()), None),
                Some(&previous)
            ),
            Decision::DeleteLocal
        );
        // The source wins before any baseline is consulted.
        assert_eq!(
            resolve(
                Preference::Source,
                &op(Some(local.clone()), None),
                Some(&previous)
            ),
            Decision::CopyLocalToCloud
        );
        // A changed surviving copy is restored, not deleted.
        assert_eq!(
            resolve(
                Preference::Newest,
                &op(Some(meta("edited", 99)), None),
                Some(&previous)
            ),
            Decision::CopyLocalToCloud
        );
    }

    #[test]
    fn only_a_vanished_target_root_turns_a_deletion_into_a_restore() {
        assert_eq!(
            restore_guard(Decision::DeleteLocal, true),
            Decision::CopyLocalToCloud
        );
        assert_eq!(
            restore_guard(Decision::DeleteLocal, false),
            Decision::DeleteLocal
        );
        assert_eq!(
            restore_guard(Decision::DeleteCloud, true),
            Decision::DeleteCloud
        );
        assert_eq!(
            restore_guard(Decision::CopyCloudToLocal, true),
            Decision::CopyCloudToLocal
        );
        assert_eq!(restore_guard(Decision::Noop, true), Decision::Noop);
    }
}
