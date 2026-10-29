//! Output links must remain usable before a build starts writing.
use std::{fs, io, path::Path};

pub fn check(source: &Path) -> io::Result<()> {
    for relative in ["target", "build/armsx2-android"] {
        let path = source.join(relative);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_symlink() => {
                if !fs::metadata(&path).is_ok_and(|target| target.is_dir()) {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("Build cache {relative} links to unavailable output. Requalify its location and required inputs explicitly; no fallback or automatic directory creation."),
                    ));
                }
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Build cache {relative} is not a directory; preserve it for review."),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::{os::unix::fs::symlink, path::PathBuf};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "psx-build-paths-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn clean_checkout_needs_no_cache_and_creates_nothing() {
        let fixture = Fixture::new();
        check(&fixture.0).unwrap();
        assert_eq!(fs::read_dir(&fixture.0).unwrap().count(), 0);
    }

    #[test]
    fn missing_link_target_refuses_without_creating_fallback() {
        let fixture = Fixture::new();
        let absent = fixture.0.join("retired/output");
        symlink(&absent, fixture.0.join("target")).unwrap();
        let error = check(&fixture.0).unwrap_err();
        assert!(error.to_string().contains("no fallback"));
        assert!(!fixture.0.join("retired").exists());
        assert!(fs::symlink_metadata(fixture.0.join("target"))
            .unwrap()
            .is_symlink());
    }

    #[test]
    fn native_cache_link_is_checked_with_valid_rust_output() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("target")).unwrap();
        fs::create_dir(fixture.0.join("build")).unwrap();
        symlink("absent-native", fixture.0.join("build/armsx2-android")).unwrap();
        assert!(check(&fixture.0)
            .unwrap_err()
            .to_string()
            .contains("build/armsx2-android"));
        assert!(!fixture.0.join("build/absent-native").exists());
    }

    #[test]
    fn valid_directory_links_pass_but_files_and_loops_refuse() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("qualified-output")).unwrap();
        symlink("qualified-output", fixture.0.join("target")).unwrap();
        check(&fixture.0).unwrap();
        fs::remove_file(fixture.0.join("target")).unwrap();
        fs::write(fixture.0.join("target"), b"unknown owner data").unwrap();
        assert!(check(&fixture.0).is_err());
        assert_eq!(
            fs::read(fixture.0.join("target")).unwrap(),
            b"unknown owner data"
        );
        fs::remove_file(fixture.0.join("target")).unwrap();
        symlink("target", fixture.0.join("target")).unwrap();
        assert!(check(&fixture.0).is_err());
    }
}
