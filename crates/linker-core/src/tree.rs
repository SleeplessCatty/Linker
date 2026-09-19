//! Directory-FD based access: no symlink traversal below an association root.
use std::ffi::{CStr, CString, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Component, Path, PathBuf};

use crate::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    File,
    Directory,
    Symlink,
    Other,
}

pub(crate) struct Tree {
    dir: File,
    pub path: PathBuf,
}

fn c_name(name: &std::ffi::OsStr) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| io::Error::other("NUL in path"))
}

fn open_at(dir: &File, name: &CStr, flags: i32) -> io::Result<File> {
    // SAFETY: the borrowed directory FD and NUL-terminated name remain live.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            flags | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            0o600,
        )
    };
    if fd < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { File::from_raw_fd(fd) })
    }
}

impl Tree {
    /// Create a previously resolved absolute directory without following any
    /// ancestor that was swapped to a symlink after validation.
    pub(crate) fn create_directory_path(path: &Path) -> Result<()> {
        let relative = path
            .strip_prefix("/")
            .map_err(|_| io::Error::other("expected absolute target path"))?;
        Self::open(Path::new("/"))?.directory(relative, true)?;
        Ok(())
    }

    pub fn open(path: &Path) -> Result<Self> {
        let dir = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        Ok(Self {
            dir,
            path: path.to_path_buf(),
        })
    }

    fn directory(&self, relative: &Path, create: bool) -> io::Result<File> {
        let mut dir = open_at(&self.dir, c".", libc::O_RDONLY | libc::O_DIRECTORY)?;
        for part in relative.components() {
            let Component::Normal(name) = part else {
                return Err(io::Error::other("unsafe relative path"));
            };
            let name = c_name(name)?;
            match open_at(&dir, &name, libc::O_RDONLY | libc::O_DIRECTORY) {
                Ok(next) => dir = next,
                Err(e) if create && e.kind() == io::ErrorKind::NotFound => {
                    let result = unsafe { libc::mkdirat(dir.as_raw_fd(), name.as_ptr(), 0o755) };
                    if result != 0 {
                        let e = io::Error::last_os_error();
                        if e.kind() != io::ErrorKind::AlreadyExists {
                            return Err(e);
                        }
                    }
                    dir = open_at(&dir, &name, libc::O_RDONLY | libc::O_DIRECTORY)?;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(dir)
    }

    fn parent(&self, relative: &Path, create: bool) -> io::Result<(File, CString)> {
        let name = relative
            .file_name()
            .ok_or_else(|| io::Error::other("empty relative path"))?;
        let parent = self.directory(relative.parent().unwrap_or(Path::new("")), create)?;
        Ok((parent, c_name(name)?))
    }

    pub fn kind(&self, relative: &Path) -> Result<Option<Kind>> {
        let (dir, name) = match self.parent(relative, false) {
            Ok(pair) => pair,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
        let rc = unsafe {
            libc::fstatat(
                dir.as_raw_fd(),
                name.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if rc != 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::NotFound {
                return Ok(None);
            }
            return Err(error.into());
        }
        let mode = unsafe { stat.assume_init() }.st_mode;
        Ok(Some(match mode & libc::S_IFMT {
            libc::S_IFREG => Kind::File,
            libc::S_IFDIR => Kind::Directory,
            libc::S_IFLNK => Kind::Symlink,
            _ => Kind::Other,
        }))
    }

    pub fn file(&self, relative: &Path) -> Result<File> {
        let (dir, name) = self.parent(relative, false)?;
        // NONBLOCK ensures a file swapped for a FIFO cannot hang the daemon.
        let file = open_at(&dir, &name, libc::O_RDONLY | libc::O_NONBLOCK)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other(format!(
                "not a regular file: {}",
                self.path.join(relative).display()
            ))
            .into());
        }
        Ok(file)
    }

    pub fn entries(&self, relative: &Path) -> Result<Vec<OsString>> {
        let dir = match self.directory(relative, false) {
            Ok(d) => d,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let fd = dir.into_raw_fd();
        // fdopendir takes ownership only on success.
        let raw = unsafe { libc::fdopendir(fd) };
        if raw.is_null() {
            let error = io::Error::last_os_error();
            unsafe {
                libc::close(fd);
            }
            return Err(error.into());
        }
        struct Stream(*mut libc::DIR);
        impl Drop for Stream {
            fn drop(&mut self) {
                unsafe {
                    libc::closedir(self.0);
                }
            }
        }
        let stream = Stream(raw);
        let mut names = Vec::new();
        loop {
            unsafe {
                *errno_ptr() = 0;
            }
            let entry = unsafe { libc::readdir(stream.0) };
            if entry.is_null() {
                let errno = unsafe { *errno_ptr() };
                if errno != 0 {
                    return Err(io::Error::from_raw_os_error(errno).into());
                }
                break;
            }
            let bytes = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) }.to_bytes();
            if bytes != b"." && bytes != b".." {
                names.push(OsString::from_vec(bytes.to_vec()));
            }
        }
        names.sort();
        Ok(names)
    }

    pub fn remove(&self, relative: &Path, directory: bool) -> Result<bool> {
        let (dir, name) = match self.parent(relative, false) {
            Ok(p) => p,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        let flags = if directory { libc::AT_REMOVEDIR } else { 0 };
        if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), flags) } != 0 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::NotFound {
                return Ok(false);
            }
            return Err(error.into());
        }
        Ok(true)
    }

    pub fn copy_from(
        &self,
        relative: &Path,
        source: &Tree,
        source_relative: &Path,
        mtime: i64,
    ) -> Result<()> {
        let mut input = source.file(source_relative)?;
        let (dir, name) = self.parent(relative, true)?;
        let tmp = CString::new(format!(".linker-tmp-{}", uuid::Uuid::new_v4())).unwrap();
        let result = (|| -> Result<()> {
            let mut output = open_at(&dir, &tmp, libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL)?;
            io::copy(&mut input, &mut output)?;
            output.set_permissions(input.metadata()?.permissions())?;
            output.flush()?;
            filetime::set_file_handle_times(
                &output,
                None,
                Some(filetime::FileTime::from_unix_time(mtime, 0)),
            )?;
            output.sync_all()?;
            if unsafe {
                libc::renameat(
                    dir.as_raw_fd(),
                    tmp.as_ptr(),
                    dir.as_raw_fd(),
                    name.as_ptr(),
                )
            } != 0
            {
                return Err(io::Error::last_os_error().into());
            }
            Ok(())
        })();
        if result.is_err() {
            unsafe {
                libc::unlinkat(dir.as_raw_fd(), tmp.as_ptr(), 0);
            }
        }
        result
    }

    pub fn is_current(&self) -> Result<bool> {
        use std::os::unix::fs::MetadataExt;
        let now = fs::symlink_metadata(&self.path)?;
        let pinned = self.dir.metadata()?;
        Ok(now.is_dir() && now.dev() == pinned.dev() && now.ino() == pinned.ino())
    }
}

#[cfg(target_os = "macos")]
unsafe fn errno_ptr() -> *mut libc::c_int {
    unsafe { libc::__error() }
}
#[cfg(not(target_os = "macos"))]
unsafe fn errno_ptr() -> *mut libc::c_int {
    unsafe { libc::__errno_location() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn creation_cannot_follow_a_parent_swapped_to_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().canonicalize().unwrap();
        let root = base.join("root");
        let outside = base.join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        let resolved = root.join("new/nested");
        fs::rename(&root, base.join("moved")).unwrap();
        symlink(&outside, &root).unwrap();
        assert!(Tree::create_directory_path(&resolved).is_err());
        assert!(!outside.join("new").exists());
        assert!(!base.join("moved/new").exists());
        Tree::create_directory_path(&base.join("safe/new/nested")).unwrap();
        assert!(base.join("safe/new/nested").is_dir());
    }

    #[test]
    fn removal_cannot_follow_a_parent_swapped_to_a_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("root");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(root.join("cache")).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(root.join("cache/secret"), "old").unwrap();
        fs::write(outside.join("secret"), "keep").unwrap();
        let tree = Tree::open(&root).unwrap();
        assert_eq!(
            tree.kind(Path::new("cache/secret")).unwrap(),
            Some(Kind::File)
        );
        fs::rename(root.join("cache"), root.join("moved")).unwrap();
        symlink(&outside, root.join("cache")).unwrap();
        assert!(tree.remove(Path::new("cache/secret"), false).is_err());
        assert_eq!(fs::read_to_string(outside.join("secret")).unwrap(), "keep");
        assert!(tree.remove(Path::new("cache"), false).unwrap());
        assert!(outside.join("secret").exists());
    }
}
