use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum LinkerError {
    #[error("home directory could not be resolved")]
    HomeDirMissing,

    #[error("path does not exist: {0}")]
    PathMissing(PathBuf),

    #[error("path is not a directory: {0}")]
    NotDirectory(PathBuf),

    #[error("target directory is not empty: {0}")]
    TargetNotEmpty(PathBuf),

    #[error("target directory must not be a symbolic link: {0}")]
    TargetSymlink(PathBuf),

    #[error("path is not a file: {0}")]
    NotFile(PathBuf),

    #[error("item already exists: {0}")]
    ItemExists(String),

    #[error("item was not found: {0}")]
    ItemNotFound(String),

    #[error("invalid item name for path: {0}")]
    InvalidItemName(PathBuf),

    #[error("invalid item name: {0}")]
    InvalidName(String),

    #[error("invalid sync association: {0}")]
    InvalidAssociation(String),

    #[error("failed to strip path prefix: {0}")]
    StripPrefix(String),

    #[error("unsupported file timestamp: {0}")]
    Timestamp(String),

    #[error(transparent)]
    Walkdir(#[from] walkdir::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Sql(#[from] rusqlite::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, LinkerError>;
