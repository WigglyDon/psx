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

/// A normal stage directory can be verified. Redirected/non-directory stages
/// refuse; creation has a separate absence requirement.
pub fn stage_path(stage: &Path, output: &Path) -> io::Result<()> {
    if stage.components().any(|part| {
        matches!(
            part,
            std::path::Component::ParentDir | std::path::Component::CurDir
        )
    }) || stage.file_name().is_none()
        || stage.parent().is_none()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid APK stage path",
        ));
    }
    if stage.parent().unwrap().canonicalize()? != output.join("build").canonicalize()? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "APK stage must be a direct qualified build child",
        ));
    }
    match fs::symlink_metadata(stage) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(metadata) if metadata.is_dir() && !metadata.is_symlink() => Ok(()),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "APK stage is redirected or not a directory; preserve it",
        )),
    }
}

/// Verification may read an existing stage; creation must preserve it.
pub fn new_stage(stage: &Path, output: &Path) -> io::Result<()> {
    stage_path(stage, output)?;
    match fs::symlink_metadata(stage) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
        Ok(_) => Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "APK stage already exists; preserve it and choose a new name",
        )),
    }
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

    #[test]
    fn new_local_stage_is_absent_and_direct_without_creating_it() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("build")).unwrap();
        let stage = fixture.0.join("build/next-apk");
        new_stage(&stage, &fixture.0).unwrap();
        assert!(!stage.exists());
        fs::create_dir(&stage).unwrap();
        fs::write(stage.join("failure-evidence"), b"keep").unwrap();
        stage_path(&stage, &fixture.0).unwrap();
        assert!(new_stage(&stage, &fixture.0).is_err());
        assert_eq!(fs::read(stage.join("failure-evidence")).unwrap(), b"keep");
    }

    #[test]
    fn stage_refuses_redirects_outside_and_ambiguous_paths() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("build")).unwrap();
        fs::create_dir(fixture.0.join("other")).unwrap();
        assert!(new_stage(&fixture.0.join("other/apk"), &fixture.0).is_err());
        assert!(new_stage(&fixture.0.join("build/../other/apk"), &fixture.0).is_err());
        symlink("absent", fixture.0.join("build/link-apk")).unwrap();
        assert!(new_stage(&fixture.0.join("build/link-apk"), &fixture.0).is_err());
        assert!(fs::symlink_metadata(fixture.0.join("build/link-apk"))
            .unwrap()
            .is_symlink());
        symlink("../other", fixture.0.join("build/redirect")).unwrap();
        assert!(new_stage(&fixture.0.join("build/redirect/apk"), &fixture.0).is_err());
    }
}
