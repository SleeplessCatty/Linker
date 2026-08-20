use clap::{CommandFactory, Parser, Subcommand};
use linker_core::health::CheckStatus;
use linker_core::ops::{self, AddOptions};
use linker_core::{LinkerError, Result};

#[derive(Debug, Parser)]
#[command(name = "linker")]
#[command(version)]
#[command(about = "Lightweight iCloud-backed selective sync for macOS")]
#[command(
    long_about = "Linker links one source directory to one target parent directory. The target directory is created as <target-parent>/<source-directory-name>, and the source directory name is used as the item name for sync, rules, remove, and delete."
)]
#[command(after_help = "Common workflow:
  linker add ~/Documents/Notes ~/Library/Mobile\\ Documents/com~apple~CloudDocs
  linker rule Notes exclude tmp/
  linker sync Notes
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
        long_about = "Add a source directory to Linker, create or reuse the target directory at <target-parent-directory>/<source-directory-name>, and run the initial bidirectional sync.\n\nBy default no files are excluded. Linker does not import .gitignore unless you explicitly pass it through --ignore-file."
    )]
    #[command(after_help = "Examples:
  linker add ~/Documents/Notes ~/Library/Mobile\\ Documents/com~apple~CloudDocs
  linker add ~/code/demo ~/Library/Mobile\\ Documents/com~apple~CloudDocs --ignore-file ~/code/demo/.linkerignore
  linker add ~/code/demo ~/Library/Mobile\\ Documents/com~apple~CloudDocs --exclude node_modules/ --exclude dist/")]
    Add {
        #[arg(help = "Source directory to sync")]
        source_directory: String,
        #[arg(help = "Parent directory where the target directory will be created")]
        target_parent_directory: String,
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
        long_about = "Manage gitignore-style exclude rules for a synced item. Rules are stored under Linker Application Support and use the same matching semantics as Git ignore files. The rule file is plain text: one rule per line.\n\nrule list shows all rules. rule exclude adds one rule and removes matching files from the target directory immediately, but never deletes source files. rule include removes one matching rule; if no rule matches, it is ignored. The next sync can restore matching source content to the target directory."
    )]
    #[command(after_help = "Examples:
  linker rule demo list
  linker rule demo exclude tmp/
  linker rule demo include tmp/")]
    Rule {
        #[arg(help = "Configured item name or internal item id")]
        name: String,
        #[command(subcommand)]
        command: RuleCommand,
    },
    #[command(about = "List configured Linker items")]
    #[command(
        long_about = "List configured directory associations with their item name, status, source directory, and target directory."
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
        long_about = "Run one manual sync pass for all items or one named item. Normal usage should rely on linkerd automatic syncing; this command is mainly for immediate verification and recovery."
    )]
    Sync {
        #[arg(help = "Optional item name or internal item id")]
        name: Option<String>,
    },
    #[command(about = "Stop syncing an item but keep both directories")]
    #[command(
        long_about = "Remove the local Linker association for an item. This keeps the source directory and target directory, but removes Linker's local rule and manifest files for the association."
    )]
    Remove {
        #[arg(help = "Item name or internal item id")]
        name: String,
    },
    #[command(about = "Stop syncing an item and delete the target directory")]
    #[command(
        long_about = "Delete the local Linker association, local rule and manifest files, and the target directory. The source directory is never deleted."
    )]
    Delete {
        #[arg(help = "Item name or internal item id")]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum RuleCommand {
    #[command(about = "Exclude a path and prune matching target files")]
    #[command(
        long_about = "Add one gitignore-style exclude rule and prune matching target files. Matching follows the same semantics as Git ignore files. Matching files are removed from the target directory immediately, while source files are kept."
    )]
    Exclude {
        #[arg(help = "Gitignore-style pattern, such as tmp/ or *.log")]
        pattern: String,
    },
    #[command(about = "Include a path again by removing an exclude rule")]
    #[command(
        long_about = "Include a path again by removing one matching exclude rule. If no existing rule matches, the command succeeds without changing rules. Matching source files can be copied to the target directory again on the next linker sync or daemon sync pass."
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
            source_directory,
            target_parent_directory,
            excludes,
            ignore_file,
        } => {
            let outcome = ops::add_item(AddOptions {
                source_path: source_directory,
                target_parent_path: target_parent_directory,
                ignore_file,
                excludes,
            })?;
            println!("added: {}", outcome.item.name);
            println!("type: {}", outcome.item.item_type);
            println!("source: {}", outcome.item.local_path);
            println!("target: {}", outcome.item.cloud_path);
            println!("manifest: {}", outcome.manifest_path);
            println!("rules: {}", outcome.item.exclude_count);
            println!(
                "initial sync source -> target: {}",
                outcome.sync_summary.copied_local_to_cloud
            );
            println!(
                "initial sync target -> source: {}",
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
                for item in items {
                    println!("----------------------------------------");
                    println!("name: {}", item.name);
                    println!("type: {}", item.item_type);
                    println!("status: {}", item.status);
                    println!("source: {}", item.local_path);
                    println!("target: {}", item.cloud_path);
                    println!("rules: {}", item.exclude_count);
                    if let Some(last_sync_at) = item.last_sync_at {
                        println!("last sync: {last_sync_at}");
                    }
                    if let Some(last_error) = &item.last_error {
                        println!("last error: {last_error}");
                    }
                }
            }
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
        Command::Sync { name } => {
            let summaries = ops::sync_item(name.as_deref())?;
            if summaries.is_empty() {
                println!("no items");
            }
            for summary in summaries {
                println!("synced: {}", summary.item_name);
                println!("source -> target: {}", summary.copied_local_to_cloud);
                println!("target -> source: {}", summary.copied_cloud_to_local);
                println!("deleted source: {}", summary.deleted_local);
                println!("deleted target: {}", summary.deleted_cloud);
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
            "error: item already exists: {name}\nhelp: item names come from source directory names; rename the source directory or run `linker list` to see existing items."
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
        LinkerError::NotFile(path) => format!(
            "error: path is not a file: {}.",
            path.display()
        ),
        LinkerError::InvalidName(name) => format!(
            "error: invalid item name: {name}\nhelp: use a simple file or folder name without path separators; `.linker` is reserved."
        ),
        LinkerError::InvalidRulePattern(message) => format!(
            "error: invalid rule pattern: {message}\nhelp: pass one non-empty gitignore-style pattern, for example `tmp/` or `*.log`."
        ),
        LinkerError::InvalidAssociation(message) => format!(
            "error: invalid sync association: {message}\nhelp: choose a target parent outside the source directory."
        ),
        _ => format!("error: {error}"),
    }
}
