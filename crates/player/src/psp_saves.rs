//! PSP Memory Stick identity and durability. The paused engine owns mutation;
//! the save worker only synchronizes retained descriptors and immutable hashes.
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::Path,
};

const MAX_FILES: usize = 256;
const MAX_BYTES: u64 = 512 * 1024 * 1024;
const MAX_DEPTH: usize = 16;

pub struct Pending {
    identity: Vec<u8>,
    handles: Vec<File>,
}
impl Pending {
    pub fn synchronize(self) -> io::Result<Vec<u8>> {
        // Files precede their containing directories. No emulation/native call
        // or path reopen is permitted on the background checkpoint writer.
        for file in &self.handles {
            file.sync_all()?;
        }
        Ok(self.identity)
    }
}

fn open(path: &Path, directory: bool) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | if directory { libc::O_DIRECTORY } else { 0 });
    options.open(path)
}

fn same_file(a: &fs::Metadata, b: &fs::Metadata) -> bool {
    #[cfg(unix)]
    {
        a.dev() == b.dev()
            && a.ino() == b.ino()
            && a.len() == b.len()
            && a.mtime() == b.mtime()
            && a.mtime_nsec() == b.mtime_nsec()
            && a.ctime() == b.ctime()
            && a.ctime_nsec() == b.ctime_nsec()
    }
    #[cfg(not(unix))]
    {
        a.len() == b.len() && a.modified().ok() == b.modified().ok()
    }
}

/// Capture while the game is paused and its asynchronous save operations have
/// completed. Exclude only the donor's documented reconstructible cache tree.
pub fn capture(root: &Path) -> io::Result<Pending> {
    fn walk(
        root: &Path,
        relative: &Path,
        depth: usize,
        total: &mut u64,
        count: &mut usize,
        hash: &mut Sha256,
        handles: &mut Vec<File>,
    ) -> io::Result<()> {
        if depth > MAX_DEPTH {
            return Err(io::Error::other("PSP save folders are too deeply nested."));
        }
        let path = root.join(relative);
        let before = fs::symlink_metadata(&path)?;
        if before.file_type().is_symlink() || !before.is_dir() {
            return Err(io::Error::other(
                "PSP save storage contains a redirected folder.",
            ));
        }
        let directory = open(&path, true)?;
        if !same_file(&before, &directory.metadata()?) {
            return Err(io::Error::other("PSP save storage changed during capture."));
        }
        let mut entries = fs::read_dir(&path)?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let relative = relative.join(entry.file_name());
            if relative == Path::new("PSP/SYSTEM/CACHE") {
                continue;
            }
            *count += 1;
            if *count > MAX_FILES {
                return Err(io::Error::other("PSP save storage has too many files."));
            }
            let name = relative
                .to_str()
                .ok_or_else(|| io::Error::other("A PSP save name is invalid."))?;
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                return Err(io::Error::other(
                    "PSP saves cannot contain redirected files.",
                ));
            }
            if metadata.is_dir() {
                hash.update(b"directory");
                walk(root, &relative, depth + 1, total, count, hash, handles)?;
            } else if metadata.is_file() {
                *total = total
                    .checked_add(metadata.len())
                    .filter(|n| *n <= MAX_BYTES)
                    .ok_or_else(|| {
                        io::Error::other("PSP saved data exceeds the checkpoint inspection limit.")
                    })?;
                hash.update(b"file");
                hash.update(metadata.len().to_le_bytes());
                let mut file = open(&path, false)?;
                if !same_file(&metadata, &file.metadata()?) {
                    return Err(io::Error::other("A PSP save changed during capture."));
                }
                let mut digest = Sha256::new();
                let mut buffer = [0; 64 * 1024];
                let mut read = 0u64;
                loop {
                    let n = file.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    read += n as u64;
                    if read > metadata.len() {
                        return Err(io::Error::other("A PSP save grew during capture."));
                    }
                    digest.update(&buffer[..n]);
                }
                if read != metadata.len()
                    || !same_file(&metadata, &file.metadata()?)
                    || !same_file(&metadata, &fs::symlink_metadata(&path)?)
                {
                    return Err(io::Error::other("A PSP save changed during capture."));
                }
                hash.update(digest.finalize());
                handles.push(file);
            } else {
                return Err(io::Error::other(
                    "PSP save storage contains a special file.",
                ));
            }
        }
        if !same_file(&before, &fs::symlink_metadata(&path)?) {
            return Err(io::Error::other("PSP save storage changed during capture."));
        }
        handles.push(directory);
        Ok(())
    }
    let mut hash = Sha256::new();
    hash.update(b"psx-psp-saves-v1");
    let mut handles = Vec::new();
    walk(
        root,
        Path::new(""),
        0,
        &mut 0,
        &mut 0,
        &mut hash,
        &mut handles,
    )?;
    Ok(Pending {
        identity: hash.finalize().to_vec(),
        handles,
    })
}

pub fn identity(root: &Path) -> io::Result<Vec<u8>> {
    capture(root)?.synchronize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            let p = std::env::temp_dir().join(format!(
                "psx-psp-save-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn newer_save_cannot_match_an_older_checkpoint_and_other_game_stays_independent() {
        let a = Scratch::new();
        let b = Scratch::new();
        fs::create_dir_all(a.0.join("PSP/SAVEDATA/GAMEA")).unwrap();
        fs::write(a.0.join("PSP/SAVEDATA/GAMEA/DATA.BIN"), b"older").unwrap();
        let old = identity(&a.0).unwrap();
        let other = identity(&b.0).unwrap();
        fs::write(a.0.join("PSP/SAVEDATA/GAMEA/DATA.BIN"), b"newer").unwrap();
        assert_ne!(old, identity(&a.0).unwrap());
        assert_eq!(other, identity(&b.0).unwrap());
        assert_eq!(
            fs::read(a.0.join("PSP/SAVEDATA/GAMEA/DATA.BIN")).unwrap(),
            b"newer"
        );
    }
    #[test]
    fn additions_deletions_names_and_empty_directories_are_bound() {
        let s = Scratch::new();
        let empty = identity(&s.0).unwrap();
        fs::create_dir(s.0.join("SAVEDATA")).unwrap();
        let directory = identity(&s.0).unwrap();
        assert_ne!(empty, directory);
        fs::write(s.0.join("SAVEDATA/a"), b"data").unwrap();
        let named = identity(&s.0).unwrap();
        fs::rename(s.0.join("SAVEDATA/a"), s.0.join("SAVEDATA/b")).unwrap();
        assert_ne!(named, identity(&s.0).unwrap());
        fs::remove_file(s.0.join("SAVEDATA/b")).unwrap();
        assert_eq!(directory, identity(&s.0).unwrap());
    }
    #[cfg(unix)]
    #[test]
    fn redirected_save_is_refused_without_touching_its_target() {
        let s = Scratch::new();
        let other = Scratch::new();
        fs::write(other.0.join("save"), b"keep").unwrap();
        std::os::unix::fs::symlink(other.0.join("save"), s.0.join("link")).unwrap();
        assert!(capture(&s.0).is_err());
        assert_eq!(fs::read(other.0.join("save")).unwrap(), b"keep");
    }
}
