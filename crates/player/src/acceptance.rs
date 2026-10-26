//! Build-only full-player acceptance storage. No renderer/observer/settings mode.
//! The ordinary player always receives its original root unchanged.
use std::{
    io,
    path::{Path, PathBuf},
};
pub const DIRECTORY: &str = "player-acceptance-v1";
pub const MARKER: &[u8] = b"PSX_PLAYER_ACCEPTANCE_V1\n";

pub fn storage_root(normal: &Path) -> io::Result<PathBuf> {
    #[cfg(not(feature = "player-acceptance"))]
    {
        Ok(normal.to_owned())
    }
    #[cfg(feature = "player-acceptance")]
    {
        if cfg!(feature = "graphics-observer") {
            return Err(io::Error::other(
                "Player acceptance cannot share an observer execution mode.",
            ));
        }
        // The admitted Android NativeActivity supplies its app-private files
        // directory. Its sibling is inside the same UID's app data, while
        // creating/removing it leaves even the normal files-directory mtime alone.
        if normal.file_name() != Some(std::ffi::OsStr::new("files")) {
            return Err(io::Error::other(
                "Acceptance needs the qualified app-private files root.",
            ));
        }
        let parent = normal
            .parent()
            .ok_or_else(|| io::Error::other("Missing app-private parent."))?
            .canonicalize()?;
        let scratch = parent.join(DIRECTORY);
        let metadata = std::fs::symlink_metadata(&scratch)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(io::Error::other(
                "Acceptance scratch must be an actual prepared directory.",
            ));
        }
        let marker = scratch.join("prepared-v1");
        let metadata = std::fs::symlink_metadata(&marker)?;
        if !metadata.is_file()
            || metadata.file_type().is_symlink()
            || metadata.len() != MARKER.len() as u64
            || std::fs::read(marker)? != MARKER
        {
            return Err(io::Error::other(
                "Acceptance scratch preparation is missing or changed.",
            ));
        }
        Ok(scratch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "player-acceptance"))]
    #[test]
    fn ordinary_root_never_probes_or_creates_a_test_folder() {
        let root = Path::new("/uncreated/ordinary/files");
        assert_eq!(storage_root(root).unwrap(), root);
    }
    #[cfg(all(
        feature = "player-acceptance",
        not(feature = "graphics-observer"),
        unix
    ))]
    #[test]
    fn only_prepared_private_sibling_is_selected_and_links_are_refused() {
        use std::fs;
        let parent =
            std::env::temp_dir().join(format!("psx-acceptance-root-{}", std::process::id()));
        fs::create_dir(&parent).unwrap();
        fs::create_dir(parent.join("files")).unwrap();
        fs::write(parent.join("files/owner-save"), b"normal progress").unwrap();
        let normal = parent.join("files");
        let scratch = parent.join(DIRECTORY);
        assert!(storage_root(&normal).is_err());
        fs::create_dir(&scratch).unwrap();
        assert!(storage_root(&normal).is_err());
        fs::write(scratch.join("prepared-v1"), b"wrong").unwrap();
        assert!(storage_root(&normal).is_err());
        fs::write(scratch.join("prepared-v1"), MARKER).unwrap();
        assert_eq!(storage_root(&normal).unwrap(), scratch);
        assert_eq!(
            fs::read(normal.join("owner-save")).unwrap(),
            b"normal progress"
        );
        fs::remove_file(scratch.join("prepared-v1")).unwrap();
        fs::write(parent.join("marker"), MARKER).unwrap();
        std::os::unix::fs::symlink(parent.join("marker"), scratch.join("prepared-v1")).unwrap();
        assert!(storage_root(&normal).is_err());
        fs::remove_file(scratch.join("prepared-v1")).unwrap();
        fs::remove_dir(&scratch).unwrap();
        std::os::unix::fs::symlink(parent.join("files"), &scratch).unwrap();
        assert!(storage_root(&normal).is_err());
        assert_eq!(
            fs::read(normal.join("owner-save")).unwrap(),
            b"normal progress"
        );
        fs::remove_dir_all(parent).unwrap();
    }
}
