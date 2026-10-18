use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

pub const CARD_SIZE: usize = 128 * 1024;

/// Same-directory replacement: the old complete file survives interrupted
/// writes. Sync the data before rename, then sync its directory.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing save directory"))?;
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()
}

pub struct MemoryCard {
    path: PathBuf,
    last: Vec<u8>,
}

impl MemoryCard {
    pub fn open(path: PathBuf) -> io::Result<Self> {
        let last = match fs::read(&path) {
            Ok(data) if data.len() == CARD_SIZE => data,
            Ok(_) => {
                return Err(io::Error::other(
                    "The saved memory card is damaged; it has been preserved.",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        Ok(Self { path, last })
    }

    pub fn restore(&self, core_ram: &mut [u8]) -> io::Result<()> {
        if core_ram.len() != CARD_SIZE {
            return Err(io::Error::other(
                "The core did not expose a PS1 memory card.",
            ));
        }
        if !self.last.is_empty() {
            core_ram.copy_from_slice(&self.last);
        }
        Ok(())
    }

    pub fn flush(&mut self, core_ram: &[u8]) -> io::Result<bool> {
        if core_ram.len() != CARD_SIZE {
            return Err(io::Error::other("The core memory card changed size."));
        }
        if self.last == core_ram {
            return Ok(false);
        }
        atomic_write(&self.path, core_ram)?;
        self.last.clear();
        self.last.extend_from_slice(core_ram);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn card_survives_reopen_and_bad_size_is_preserved() {
        let root = std::env::temp_dir().join(format!("psx-save-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("game.srm");
        let mut card = MemoryCard::open(path.clone()).unwrap();
        let mut ram = vec![0; CARD_SIZE];
        ram[123] = 87;
        assert!(card.flush(&ram).unwrap());
        assert!(!card.flush(&ram).unwrap());
        let card = MemoryCard::open(path.clone()).unwrap();
        ram.fill(0);
        card.restore(&mut ram).unwrap();
        assert_eq!(ram[123], 87);
        atomic_write(&path, b"bad").unwrap();
        assert!(MemoryCard::open(path.clone()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"bad");
        fs::remove_dir_all(root).unwrap();
    }
}
