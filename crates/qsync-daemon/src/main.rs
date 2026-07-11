use std::collections::{BTreeSet, HashMap};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use clap::Parser;
use fs2::FileExt;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use qsync_core::ops;
use qsync_core::paths;
use qsync_core::state::Item;
use qsync_core::Result;

const DEBOUNCE: Duration = Duration::from_secs(2);
const RECONCILE_INTERVAL: Duration = Duration::from_secs(300);

#[derive(Debug, Parser)]
#[command(name = "qsd")]
#[command(about = "QuickSync background daemon")]
struct Args {
    #[arg(long)]
    once: bool,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("qsd error: {error}");
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
    watched_paths: HashMap<PathBuf, String>,
    registered_paths: BTreeSet<PathBuf>,
    dirty_items: BTreeSet<String>,
    last_event_at: Option<Instant>,
    last_reconcile_at: Instant,
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

        eprintln!("qsd watching {} item(s)", self.items.len());
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
            .insert(path.to_path_buf(), item_id.to_string());
        Ok(())
    }

    fn handle_event(&mut self, event: Event) {
        for path in event.paths {
            if let Some(item_id) = self.item_for_path(&path) {
                self.dirty_items.insert(item_id);
                self.last_event_at = Some(Instant::now());
            }
        }
    }

    fn item_for_path(&self, path: &Path) -> Option<String> {
        self.watched_paths
            .iter()
            .find(|(root, _)| path.starts_with(root))
            .map(|(_, item_id)| item_id.clone())
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

    fn sync_all(&self, reason: &str) {
        for item in &self.items {
            self.sync_one(&item.id, reason);
        }
    }

    fn sync_one(&self, item_id: &str, reason: &str) {
        let Some(item) = self.items.iter().find(|item| item.id == item_id) else {
            return;
        };

        match ops::sync_item(Some(&item.id)) {
            Ok(summaries) => {
                for summary in summaries {
                    eprintln!(
                        "qsd {reason} synced {}: source->target={}, target->source={}, deleted_source={}, deleted_target={}, unchanged={}",
                        summary.item_name,
                        summary.copied_local_to_cloud,
                        summary.copied_cloud_to_local,
                        summary.deleted_local,
                        summary.deleted_cloud,
                        summary.unchanged
                    );
                }
            }
            Err(error) => eprintln!("qsd failed to sync {}: {error}", item.name),
        }
    }
}

struct DaemonLock {
    file: File,
    path: PathBuf,
}

impl DaemonLock {
    fn acquire() -> Result<Self> {
        let path = paths::app_support_dir()?.join("qsd.lock");
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
