use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use clap::Parser;
use fs2::FileExt;
use linker_core::ops;
use linker_core::paths;
use linker_core::state::Item;
use linker_core::Result;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

const DEBOUNCE: Duration = Duration::from_secs(2);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Parser)]
#[command(name = "linkerd")]
#[command(about = "Linker background daemon")]
struct Args {
    #[arg(long)]
    once: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("linkerd error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = Args::parse();
    paths::ensure_base_dirs()?;
    let _lock = DaemonLock::acquire()?;

    let (event_tx, event_rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(move |event| {
        let _ = event_tx.send(event);
    })
    .map_err(to_io_error)?;

    let mut daemon = Daemon::new(event_rx);
    daemon.reload_items(&mut watcher)?;
    daemon.sync_all("startup");
    if args.once {
        return Ok(());
    }
    daemon.run(&mut watcher)
}

struct Daemon {
    events: Receiver<notify::Result<Event>>,
    items: Vec<Item>,
    watched_paths: HashMap<PathBuf, BTreeSet<String>>,
    registered_paths: BTreeSet<PathBuf>,
    dirty_items: BTreeSet<String>,
    last_event_at: Option<Instant>,
    last_reconcile_at: Instant,
    warning_fingerprints: HashMap<String, String>,
}

impl Daemon {
    fn new(events: Receiver<notify::Result<Event>>) -> Self {
        Self {
            events,
            items: Vec::new(),
            watched_paths: HashMap::new(),
            registered_paths: BTreeSet::new(),
            dirty_items: BTreeSet::new(),
            last_event_at: None,
            last_reconcile_at: Instant::now(),
            warning_fingerprints: HashMap::new(),
        }
    }

    fn run(&mut self, watcher: &mut RecommendedWatcher) -> Result<()> {
        loop {
            match self.events.recv_timeout(Duration::from_millis(500)) {
                Ok(Ok(event)) => self.handle_event(event),
                Ok(Err(error)) => eprintln!("watch error: {error}"),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if self.should_flush_dirty() {
                self.sync_dirty();
            }

            if self.last_reconcile_at.elapsed() >= RECONCILE_INTERVAL {
                self.reload_items(watcher)?;
                self.sync_all("periodic");
                self.last_reconcile_at = Instant::now();
            }
        }

        Ok(())
    }

    fn reload_items(&mut self, watcher: &mut RecommendedWatcher) -> Result<()> {
        let items = ops::list_items()?;
        self.items = items.clone();
        self.watched_paths.clear();

        for item in &items {
            self.watch_item_path(watcher, &item.id, Path::new(&item.local_path))?;
            self.watch_item_path(watcher, &item.id, Path::new(&item.cloud_path))?;
        }

        eprintln!("linkerd watching {} item(s)", self.items.len());
        Ok(())
    }

    fn watch_item_path(
        &mut self,
        watcher: &mut RecommendedWatcher,
        item_id: &str,
        path: &Path,
    ) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        if !self.registered_paths.contains(path) {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(to_io_error)?;
            self.registered_paths.insert(path.to_path_buf());
        }

        self.watched_paths
            .entry(path.to_path_buf())
            .or_default()
            .insert(item_id.to_string());
        Ok(())
    }

    fn handle_event(&mut self, event: Event) {
        for path in event.paths {
            let items = self.items_for_path(&path);
            if !items.is_empty() {
                self.dirty_items.extend(items);
                self.last_event_at = Some(Instant::now());
            }
        }
    }

    fn items_for_path(&self, path: &Path) -> BTreeSet<String> {
        self.watched_paths
            .iter()
            .filter(|(root, _)| path.starts_with(root))
            .flat_map(|(_, item_ids)| item_ids.iter().cloned())
            .collect()
    }

    fn should_flush_dirty(&self) -> bool {
        !self.dirty_items.is_empty()
            && self
                .last_event_at
                .is_some_and(|last_event| last_event.elapsed() >= DEBOUNCE)
    }

    fn sync_dirty(&mut self) {
        let dirty = std::mem::take(&mut self.dirty_items);
        self.last_event_at = None;

        for item_id in dirty {
            self.sync_one(&item_id, "event");
        }
    }

    fn sync_all(&mut self, reason: &str) {
        let ids: Vec<_> = self.items.iter().map(|item| item.id.clone()).collect();
        for id in ids {
            self.sync_one(&id, reason);
        }
    }

    fn sync_one(&mut self, item_id: &str, reason: &str) {
        let Some(item) = self.items.iter().find(|item| item.id == item_id).cloned() else {
            return;
        };

        match ops::sync_item(Some(&item.id)) {
            Ok(summaries) => {
                for summary in summaries {
                    if self.warning_fingerprints.get(item_id) != Some(&summary.rules_fingerprint) {
                        for warning in &summary.warnings {
                            eprintln!("warning: {warning}");
                        }
                        self.warning_fingerprints
                            .insert(item_id.to_owned(), summary.rules_fingerprint.clone());
                    }
                    eprintln!(
                        "linkerd {reason} synced {}: source->target={}, target->source={}, deleted_source={}, deleted_target={}, pruned_target_directories={}, unchanged={}, target_root_recovered={}",
                        summary.item_name,
                        summary.copied_local_to_cloud,
                        summary.copied_cloud_to_local,
                        summary.deleted_local,
                        summary.deleted_cloud,
                        summary.pruned_cloud_directories,
                        summary.unchanged,
                        summary.target_root_recovered
                    );
                }
            }
            Err(error) => eprintln!("linkerd failed to sync {}: {error}", item.name),
        }
    }
}

struct DaemonLock {
    file: File,
    path: PathBuf,
}

impl DaemonLock {
    fn acquire() -> Result<Self> {
        let path = paths::app_support_dir()?.join("linkerd.lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        file.try_lock_exclusive()?;

        Ok(Self { file, path })
    }
}

impl Drop for DaemonLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        let _ = fs::remove_file(&self.path);
    }
}

fn to_io_error(error: notify::Error) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_events_dirty_every_association_while_target_events_are_specific() {
        let (_, rx) = mpsc::channel();
        let mut daemon = Daemon::new(rx);
        daemon
            .watched_paths
            .insert("/source".into(), BTreeSet::from(["a".into(), "b".into()]));
        daemon
            .watched_paths
            .insert("/target-a".into(), BTreeSet::from(["a".into()]));
        daemon.handle_event(Event::new(notify::EventKind::Any).add_path("/source/new.txt".into()));
        assert_eq!(daemon.dirty_items, BTreeSet::from(["a".into(), "b".into()]));
        daemon.dirty_items.clear();
        daemon
            .handle_event(Event::new(notify::EventKind::Any).add_path("/target-a/new.txt".into()));
        assert_eq!(daemon.dirty_items, BTreeSet::from(["a".into()]));
        assert!(daemon
            .items_for_path(Path::new("/source-other/new.txt"))
            .is_empty());
    }
}
