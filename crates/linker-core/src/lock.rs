use std::fs::{self, File, OpenOptions};
use std::path::Path;

use fs2::FileExt;
use sha2::{Digest, Sha256};

use crate::Result;

/// Never unlink a lock: doing so can split waiters across different inodes.
pub(crate) fn acquire(directory: &Path, key: &str) -> Result<File> {
    fs::create_dir_all(directory)?;
    let path = directory.join(format!("{:x}.lock", Sha256::digest(key.as_bytes())));
    if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file()) {
        return Err(std::io::Error::other("unsafe lock file").into());
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.lock_exclusive()?;
    Ok(file)
}
