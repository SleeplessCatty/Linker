use clap::{CommandFactory, Parser, Subcommand};
use qsync_core::health::CheckStatus;
use qsync_core::ops::{self, AddOptions};
use qsync_core::{QsyncError, Result};

#[derive(Debug, Parser)]
#[command(name = "qsync")]
#[command(version)]
#[command(about = "Lightweight iCloud-backed selective sync for macOS")]
#[command(
    long_about = "QuickSync mirrors selected local files or folders into iCloud Drive/QuickSync using readable names. Metadata is stored under QuickSync/.quicksync, while synced content stays directly visible for mobile editing."
)]
#[command(after_help = "Common workflow:
  qsync add ~/Documents/Notes
  qsync rule Notes exclude tmp/
  qsync sync Notes
  qsync status Notes

Important:
  remove keeps cloud files and hidden metadata.
  delete removes cloud files and hidden metadata.
  Local original files are never deleted by remove or delete.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Add a local file or folder to QuickSync")]
    #[command(
        long_about = "Add a local file or folder to QuickSync, create its visible iCloud copy under QuickSync/<name>, create hidden metadata under QuickSync/.quicksync, and run the initial sync.\n\nBy default no files are excluded. QuickSync does not import .gitignore unless you explicitly pass it through --ignore-file."
    )]
    #[command(after_help = "Examples:
  qsync add ~/Documents/Notes
  qsync add ~/Documents/todo.md
  qsync add ~/code/demo --name WorkDemo
  qsync add ~/code/demo --ignore-file ~/code/demo/.qsyncignore
  qsync add ~/code/demo --exclude node_modules/ --exclude dist/")]
    Add {
        #[arg(help = "Local file or folder to sync")]
        path: String,
        #[arg(
            long,
            help = "Visible iCloud name under QuickSync/",
            long_help = "Visible iCloud name under QuickSync/. Defaults to the local file or folder name. Names must be unique and cannot contain path separators. .quicksync is reserved."
        )]
        name: Option<String>,
        #[arg(
            long = "exclude",
            help = "Add an initial gitignore-style exclude pattern",
            long_help = "Add an initial gitignore-style exclude pattern. Can be passed multiple times. Matching paths are handled with the same rule semantics as Git ignore files and are not uploaded during the initial sync."
        )]
        excludes: Vec<String>,
        #[arg(
            long = "ignore-file",
            help = "Import initial exclude patterns from a file",
            long_help = "Import initial exclude patterns from a file. Blank lines and lines starting with # are ignored. This is the explicit replacement for automatic .gitignore import."
        )]
        ignore_file: Option<String>,
    },
    #[command(about = "List, exclude, or include paths for an item")]
    #[command(
        long_about = "Manage gitignore-style exclude rules for a synced item. Rules are stored in QuickSync/.quicksync/rules/<name>.ignore and use the same matching semantics as Git ignore files. The rule file is plain text: one rule per line.\n\nrule list shows all rules. rule exclude adds one rule and removes matching files from the visible iCloud copy immediately, but never deletes the local originals. rule include removes one matching rule; if no rule matches, it is ignored. The next sync can restore matching local content to iCloud."
    )]
    #[command(after_help = "Examples:
  qsync rule demo list
  qsync rule demo exclude tmp/
  qsync rule demo include tmp/")]
    Rule {
        #[arg(help = "Configured item name or internal item id")]
        name: String,
        #[command(subcommand)]
        command: RuleCommand,
    },
    #[command(about = "List configured QuickSync items")]
    #[command(
        long_about = "List configured items with their visible name, type, status, and original local path."
    )]
    List,
    #[command(about = "Show daemon and item sync status")]
    #[command(
        long_about = "Show daemon installation/running state and sync status. Pass an item name to show one item, or omit it to show all configured items."
    )]
    Status {
        #[arg(help = "Optional item name or internal item id")]
        name: Option<String>,
    },
    #[command(about = "Run environment health checks")]
    #[command(
        long_about = "Check iCloud Drive access, QuickSync application support storage, state database access, and daemon availability."
    )]
    Doctor,
    #[command(about = "Run one manual sync pass")]
    #[command(
        long_about = "Run one manual sync pass for all items or one named item. Normal usage should rely on qsyncd automatic syncing; this command is mainly for immediate verification and recovery."
    )]
    Sync {
        #[arg(help = "Optional item name or internal item id")]
        name: Option<String>,
    },
    #[command(about = "Stop syncing an item but keep cloud files")]
    #[command(
        long_about = "Remove the local QuickSync association for an item. This keeps the original local file or folder, the visible iCloud copy, and hidden .quicksync metadata."
    )]
    Remove {
        #[arg(help = "Item name or internal item id")]
        name: String,
    },
    #[command(about = "Stop syncing an item and delete its cloud copy")]
    #[command(
        long_about = "Delete the local QuickSync association, the visible iCloud file or folder, and matching hidden manifest/rule files. The original local file or folder is never deleted."
    )]
    Delete {
        #[arg(help = "Item name or internal item id")]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum RuleCommand {
    #[command(about = "Exclude a path and prune matching cloud files")]
    #[command(
        long_about = "Add one gitignore-style exclude rule and prune matching cloud files. Matching follows the same semantics as Git ignore files. Matching files are removed from the visible iCloud copy immediately, while local originals are kept."
    )]
    Exclude {
        #[arg(help = "Gitignore-style pattern, such as tmp/ or *.log")]
        pattern: String,
    },
    #[command(about = "Include a path again by removing an exclude rule")]
    #[command(
        long_about = "Include a path again by removing one matching exclude rule. If no existing rule matches, the command succeeds without changing rules. Matching local files can be uploaded again on the next qsync sync or daemon sync pass."
    )]
    Include {
        #[arg(help = "Exact rule pattern to include again")]
        pattern: String,
    },
    #[command(about = "List exclude rules")]
    #[command(
        long_about = "List all stored exclude rules for the item. Each output row is one gitignore-style pattern."
    )]
    List,
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
            path,
            name,
            excludes,
            ignore_file,
        } => {
            let outcome = ops::add_item(AddOptions {
                path,
                name,
                ignore_file,
                excludes,
            })?;
            println!("added: {}", outcome.item.name);
            println!("type: {}", outcome.item.item_type);
            println!("local: {}", outcome.item.local_path);
            println!("cloud: {}", outcome.item.cloud_path);
            println!("manifest: {}", outcome.manifest_path);
            println!("rules: {}", outcome.item.exclude_count);
            println!(
                "initial sync local -> cloud: {}",
                outcome.sync_summary.copied_local_to_cloud
            );
            println!(
                "initial sync cloud -> local: {}",
                outcome.sync_summary.copied_cloud_to_local
            );
        }
        Command::Rule { name, command } => match command {
            RuleCommand::Exclude { pattern } => {
                let rules = ops::add_exclude(&name, &pattern)?;
                println!("rule excluded: {pattern}");
                println!("rules: {}", rules.len());
            }
            RuleCommand::Include { pattern } => {
                let rules = ops::remove_exclude(&name, &pattern)?;
                println!("rule included: {pattern}");
                println!("rules: {}", rules.len());
            }
            RuleCommand::List => {
                let rules = ops::list_excludes(&name)?;
                if rules.is_empty() {
                    println!("no rules");
                } else {
                    println!("PATTERN");
                    for rule in rules {
                        println!("{}", rule.pattern);
                    }
                }
            }
        },
        Command::List => {
            let items = ops::list_items()?;
            if items.is_empty() {
                println!("no items");
            } else {
                println!("{:<24} {:<10} {:<10} LOCAL PATH", "NAME", "TYPE", "STATUS");
                for item in items {
                    println!(
                        "{:<24} {:<10} {:<10} {}",
                        item.name, item.item_type, item.status, item.local_path
                    );
                }
            }
        }
        Command::Status { name } => {
            let daemon = ops::daemon_status();
            println!("daemon installed: {}", yes_no(daemon.installed));
            println!("daemon running: {}", yes_no(daemon.running));
            if let Some(path) = daemon.binary_path {
                println!("daemon path: {}", path.display());
            }

            let items = ops::status(name.as_deref())?;
            if items.is_empty() {
                println!("no items");
            }
            for item in items {
                println!("name: {}", item.name);
                println!("type: {}", item.item_type);
                println!("status: {}", item.status);
                println!("local: {}", item.local_path);
                println!("cloud: {}", item.cloud_path);
                println!("rules: {}", item.exclude_count);
                if let Some(last_sync_at) = item.last_sync_at {
                    println!("last sync: {last_sync_at}");
                }
                if let Some(last_error) = item.last_error {
                    println!("last error: {last_error}");
                }
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
        Command::Sync { name } => {
            let summaries = ops::sync_item(name.as_deref())?;
            if summaries.is_empty() {
                println!("no items");
            }
            for summary in summaries {
                println!("synced: {}", summary.item_name);
                println!("local -> cloud: {}", summary.copied_local_to_cloud);
                println!("cloud -> local: {}", summary.copied_cloud_to_local);
                println!("deleted local: {}", summary.deleted_local);
                println!("deleted cloud: {}", summary.deleted_cloud);
                println!("unchanged: {}", summary.unchanged);
            }
        }
        Command::Remove { name } => {
            let item = ops::remove_item(&name)?;
            println!("removed from QuickSync: {}", item.name);
            println!("local files kept: {}", item.local_path);
            println!("cloud mirror kept: {}", item.cloud_path);
            println!("hidden metadata kept");
        }
        Command::Delete { name } => {
            let item = ops::delete_item(&name)?;
            println!("deleted from QuickSync: {}", item.name);
            println!("local files kept: {}", item.local_path);
            println!("cloud files deleted: {}", item.cloud_path);
            println!("hidden metadata deleted");
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

fn format_error(error: &QsyncError) -> String {
    match error {
        QsyncError::IcloudMissing(path) => format!(
            "error: iCloud Drive folder was not found at {}\nhelp: enable iCloud Drive, or set QUICKSYNC_ICLOUD_DIR when testing.",
            path.display()
        ),
        QsyncError::ItemExists(name) => format!(
            "error: item already exists: {name}\nhelp: use a different --name, or run `qsync list` to see existing items."
        ),
        QsyncError::ItemNotFound(name) => format!(
            "error: item was not found: {name}\nhelp: run `qsync list` to see configured items."
        ),
        QsyncError::PathMissing(path) => format!(
            "error: path does not exist: {}\nhelp: check the path and try again.",
            path.display()
        ),
        QsyncError::NotDirectory(path) => format!(
            "error: path is not a directory: {}.",
            path.display()
        ),
        QsyncError::NotFile(path) => format!(
            "error: path is not a file: {}.",
            path.display()
        ),
        QsyncError::InvalidName(name) => format!(
            "error: invalid item name: {name}\nhelp: use a simple file or folder name without path separators; `.quicksync` is reserved."
        ),
        QsyncError::InvalidRulePattern(message) => format!(
            "error: invalid rule pattern: {message}\nhelp: pass one non-empty gitignore-style pattern, for example `tmp/` or `*.log`."
        ),
        _ => format!("error: {error}"),
    }
}
