use clap::{CommandFactory, Parser, Subcommand};
use linker_core::health::CheckStatus;
use linker_core::ops::{self, AddOptions};
use linker_core::{LinkerError, Result};

mod output;

#[derive(Debug, Parser)]
#[command(name = "linker")]
#[command(version)]
#[command(about = "Lightweight iCloud-backed selective sync for macOS")]
#[command(
    long_about = "Linker links a source directory directly to the specified target directory, which must be absent or empty. Use --name during add to choose a unique association name; otherwise the source directory basename is used."
)]
#[command(after_help = "Common workflow:
  linker add ~/Documents/Notes ~/Library/Mobile\\ Documents/com~apple~CloudDocs/MyNotes --name notes
  linker sync notes
  linker status

Important:
  remove stops tracking but keeps both source and target directories.
  delete stops tracking and deletes the target directory.
  The source directory is never deleted by remove or delete.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Add a source directory to Linker")]
    #[command(
        long_about = "Use the exact target directory; no source name is appended. Create it if absent, or use it only if completely empty (including hidden entries). Nonempty targets, target symlinks and overlapping sync paths are rejected. --name changes only the association name, not either directory. Initial sync copies from source; later sync is bidirectional.\n\nOnly .gitignore files inside the association control exclusions. Supported: names, directories, relative paths and single-star wildcards. Unsupported patterns warn and are skipped. Ignored source files are kept; matching target files are deleted. Active .gitignore control files remain synchronized."
    )]
    Add {
        #[arg(help = "Source directory to sync")]
        source_directory: String,
        #[arg(help = "Exact target directory; must be absent or empty")]
        target_directory: String,
        #[arg(
            long,
            help = "Unique association name (default: source directory basename)"
        )]
        name: Option<String>,
    },
    #[command(about = "List configured Linker items")]
    #[command(
        long_about = "List configured associations in a table: name, type, status, full source and target paths, last successful sync time (UTC), and last error. A dash means no recorded value. Wide tables can be viewed with `linker list | less -S`."
    )]
    List,
    #[command(about = "Show background daemon status")]
    #[command(
        long_about = "Show only the background daemon installation and running state. Use `linker list` to inspect configured directory associations and sync item state."
    )]
    Status,
    #[command(about = "Run environment health checks")]
    #[command(
        long_about = "Check Linker application support storage, state database access, configured directory associations, and daemon availability."
    )]
    Doctor,
    #[command(about = "Run one manual sync pass")]
    #[command(
        long_about = "Run one manual sync pass for all items or one named item. Use --dry-run to preview effective control-file, copy, deletion and ignore-cleanup operations without applying them or updating sync state. Preview requires upgraded metadata and existing roots; it may create synchronization lock files. A running daemon can still sync independently. Normal usage should rely on linkerd automatic syncing."
    )]
    Sync {
        #[arg(help = "Optional item name or internal item id")]
        name: Option<String>,
        #[arg(long, help = "Preview operations without changing sync files or state")]
        dry_run: bool,
    },
    #[command(about = "Stop syncing an item but keep both directories")]
    #[command(
        long_about = "Remove the local Linker association for an item. This keeps the source directory and target directory, but removes Linker's local manifest files for the association."
    )]
    Remove {
        #[arg(help = "Item name or internal item id")]
        name: String,
    },
    #[command(about = "Stop syncing an item and delete the target directory")]
    #[command(
        long_about = "Delete the local Linker association, local manifest files, and the target directory. The source directory is never deleted."
    )]
    Delete {
        #[arg(help = "Item name or internal item id")]
        name: String,
    },
}

fn main() {
    if std::env::args_os().len() == 1 {
        if let Err(error) = print_root_help() {
            eprintln!("error: {error}");
            std::process::exit(1);
        }
        return;
    }

    if let Err(error) = run() {
        eprintln!("{}", format_error(&error));
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Add {
            source_directory,
            target_directory,
            name,
        } => {
            let outcome = ops::add_item(AddOptions {
                source_path: source_directory,
                target_path: target_directory,
                name,
            })?;
            for warning in &outcome.sync_summary.warnings {
                eprintln!("warning: {warning}");
            }
            println!("added: {}", outcome.item.name);
            println!("type: {}", outcome.item.item_type);
            println!("source: {}", outcome.item.local_path);
            println!("target: {}", outcome.item.cloud_path);
            println!("manifest: {}", outcome.manifest_path);
            println!(
                "initial sync source -> target: {}",
                outcome.sync_summary.copied_local_to_cloud
            );
            println!(
                "initial sync target -> source: {}",
                outcome.sync_summary.copied_cloud_to_local
            );
            println!(
                "initial sync deleted target: {}",
                outcome.sync_summary.deleted_cloud
            );
            println!(
                "initial sync pruned target directories: {}",
                outcome.sync_summary.pruned_cloud_directories
            );
        }
        Command::List => {
            let items = ops::list_items()?;
            print!("{}", output::items(&items));
        }
        Command::Status => {
            let daemon = ops::daemon_status();
            println!("daemon installed: {}", yes_no(daemon.installed));
            println!("daemon running: {}", yes_no(daemon.running));
            if let Some(path) = daemon.binary_path {
                println!("daemon path: {}", path.display());
            }
        }
        Command::Doctor => {
            let report = ops::doctor();
            for check in &report.checks {
                let status = match check.status {
                    CheckStatus::Ok => "ok",
                    CheckStatus::Warn => "warn",
                    CheckStatus::Error => "error",
                };
                println!("[{status}] {}: {}", check.name, check.message);
                if let Some(hint) = &check.hint {
                    println!("  hint: {hint}");
                }
            }

            if report.has_errors() {
                std::process::exit(1);
            }
        }
        Command::Sync {
            name,
            dry_run: true,
        } => {
            let previews = ops::preview_sync(name.as_deref())?;
            if previews.is_empty() {
                println!("no items");
            }
            for preview in previews {
                for warning in &preview.warnings {
                    eprintln!("warning: {warning}");
                }
                // The same escaping used for table cells keeps item names safe.
                println!("dry run: {}", output::label(&preview.item_name));
                if preview.operations.is_empty() {
                    println!("no changes");
                } else {
                    let rows = preview
                        .operations
                        .into_iter()
                        .map(|op| [op.action, op.path.display().to_string()])
                        .collect();
                    print!("{}", output::table(["ACTION", "PATH"], rows));
                }
            }
        }
        Command::Sync {
            name,
            dry_run: false,
        } => {
            let summaries = ops::sync_item(name.as_deref())?;
            if summaries.is_empty() {
                println!("no items");
            }
            for summary in summaries {
                for warning in &summary.warnings {
                    eprintln!("warning: {warning}");
                }
                println!("synced: {}", summary.item_name);
                println!("source -> target: {}", summary.copied_local_to_cloud);
                println!("target -> source: {}", summary.copied_cloud_to_local);
                println!("deleted source: {}", summary.deleted_local);
                println!("deleted target: {}", summary.deleted_cloud);
                println!(
                    "pruned target directories: {}",
                    summary.pruned_cloud_directories
                );
                println!("unchanged: {}", summary.unchanged);
            }
        }
        Command::Remove { name } => {
            let item = ops::remove_item(&name)?;
            println!("removed from Linker: {}", item.name);
            println!("source kept: {}", item.local_path);
            println!("target kept: {}", item.cloud_path);
            println!("local metadata deleted");
        }
        Command::Delete { name } => {
            let item = ops::delete_item(&name)?;
            println!("deleted from Linker: {}", item.name);
            println!("source kept: {}", item.local_path);
            println!("target deleted: {}", item.cloud_path);
            println!("local metadata deleted");
        }
    }

    Ok(())
}

fn print_root_help() -> Result<()> {
    let mut buffer = Vec::new();
    Cli::command().write_long_help(&mut buffer)?;
    print!("{}", String::from_utf8_lossy(&buffer));
    Ok(())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn format_error(error: &LinkerError) -> String {
    match error {
        LinkerError::ItemExists(name) => format!(
            "error: item already exists: {name}\nhelp: choose a unique --name; run `linker list` to see existing associations."
        ),
        LinkerError::ItemNotFound(name) => format!(
            "error: item was not found: {name}\nhelp: run `linker list` to see configured items."
        ),
        LinkerError::PathMissing(path) => format!(
            "error: path does not exist: {}\nhelp: check the path and try again.",
            path.display()
        ),
        LinkerError::NotDirectory(path) => format!(
            "error: path is not a directory: {}.",
            path.display()
        ),
        LinkerError::TargetNotEmpty(path) => format!(
            "error: target directory is not empty: {}\nhelp: specify the exact new or empty target directory, not its parent; hidden files and subdirectories also count. Existing contents were not removed.", path.display()
        ),
        LinkerError::TargetSymlink(path) => format!(
            "error: target directory must not be a symbolic link: {}\nhelp: choose an explicit new or empty physical directory.", path.display()
        ),
        LinkerError::NotFile(path) => format!(
            "error: path is not a file: {}.",
            path.display()
        ),
        LinkerError::InvalidName(name) => format!(
            "error: invalid item name: {name:?}\nhelp: use --name with 1-250 UTF-8 bytes, no path separators, control characters or surrounding whitespace; `.`, `..` and `.linker` are reserved."
        ),
        LinkerError::InvalidAssociation(message) => format!(
            "error: invalid sync association: {message}\nhelp: choose separate source and target directories, outside existing associations and Linker's application state."
        ),
        _ => format!("error: {error}"),
    }
}
