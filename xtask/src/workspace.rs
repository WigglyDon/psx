//! Optional admitted build output. No device or admission policy lives here.
use super::*;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::MetadataExt,
};
#[path = "build_paths.rs"]
mod build_paths;

pub fn check_cache_paths(source: &Path) -> Result<()> {
    build_paths::check(source)?;
    Ok(())
}

/// Reuse the installed host custodian's actual open-file description. Opening
/// the path again would try to acquire our own custodian's lock and deadlock.
/// Validate inode and lock ownership before accepting the inherited descriptor;
/// an environment token alone never admits a workspace.
#[cfg(target_os = "linux")]
fn inherited_lease(fd: i32, expected: &fs::Metadata) -> Result<File> {
    use std::os::fd::FromRawFd;
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    if fd < 3 {
        return Err("Invalid inherited host lease descriptor".into());
    }
    // Linux F_DUPFD_CLOEXEC: an owned duplicate, never a borrowed invalid fd,
    // and never a descriptor leaked into unrelated child processes.
    let owned = unsafe { fcntl(fd, 1030, 3) };
    if owned < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { File::from_raw_fd(owned) };
    let actual = file.metadata()?;
    if actual.dev() != expected.dev() || actual.ino() != expected.ino() {
        return Err("Inherited host lease is not the admitted capacity inode".into());
    }
    file.try_lock()
        .map_err(|e| format!("Inherited host capacity lease unavailable: {e}"))?;
    Ok(file)
}

pub fn path(relative: &str) -> String {
    if let Some(stage) = env::var_os("PSX_APK_STAGE") {
        let stage = PathBuf::from(stage);
        if let Some(suffix) = relative.strip_prefix("build/apk/") {
            return stage.join(suffix).to_string_lossy().into_owned();
        }
        if relative == "build/psx-development.apk" {
            return stage
                .join("psx-development.apk")
                .to_string_lossy()
                .into_owned();
        }
    }
    match env::var_os("PSX_BUILD_WORKSPACE") {
        Some(root) => PathBuf::from(root)
            .join(relative)
            .to_string_lossy()
            .into_owned(),
        None => relative.to_owned(),
    }
}

pub struct Admission {
    pub root: PathBuf,
    _lease: Option<File>,
}
/// Requalify the same volume before each writing subprocess, never by free
/// bytes alone. The lease remains owned by Admission for the whole operation.
pub fn check_present() -> Result<()> {
    if env::var_os("PSX_BUILD_WORKSPACE").is_none() {
        return check_cache_paths(&env::current_dir()?);
    }
    let root = required("PSX_BUILD_WORKSPACE")?;
    let uuid = env::var("PSX_BUILD_VOLUME_UUID")?;
    let mount = required("PSX_BUILD_VOLUME_MOUNT")?;
    let listing: serde_json::Value = serde_json::from_str(&output(
        command("findmnt", ["--json", "--target"])
            .arg(&root)
            .args(["--output", "UUID,FSTYPE,TARGET"]),
    )?)?;
    let volumes = listing["filesystems"]
        .as_array()
        .filter(|v| v.len() == 1)
        .ok_or("Build volume missing")?;
    if volumes[0]["uuid"] != uuid
        || volumes[0]["fstype"] != "ext4"
        || volumes[0]["target"].as_str() != mount.to_str()
        || root.canonicalize()? != root
    {
        return Err("Build volume changed/absent; no fallback".into());
    }
    Ok(())
}
impl Admission {
    pub fn acquire(source: &Path) -> Result<Self> {
        let keys = [
            "PSX_BUILD_WORKSPACE",
            "PSX_BUILD_VOLUME_UUID",
            "PSX_BUILD_VOLUME_MOUNT",
            "PSX_BUILD_CAPACITY_LEASE",
        ];
        if keys.iter().all(|key| env::var_os(key).is_none()) {
            return Ok(Self {
                root: source.to_owned(),
                _lease: None,
            });
        }
        let root = required(keys[0])?;
        let uuid = env::var(keys[1])?;
        let mount = required(keys[2])?;
        let lease = required(keys[3])?;
        if root.to_str().is_none()
            || mount.to_str().is_none()
            || root.canonicalize()? != root
            || mount.canonicalize()? != mount
            || root == mount
            || !root.starts_with(&mount)
            || lease.starts_with(&mount)
        {
            return Err("Unqualified build workspace; no fallback".into());
        }
        let listing: serde_json::Value = serde_json::from_str(&output(
            command("findmnt", ["--json", "--target"])
                .arg(&root)
                .args(["--output", "UUID,FSTYPE,TARGET"]),
        )?)?;
        let volumes = listing["filesystems"]
            .as_array()
            .filter(|v| v.len() == 1)
            .ok_or("Missing unique build volume")?;
        let volume = &volumes[0];
        if uuid.is_empty()
            || volume["uuid"] != uuid
            || volume["fstype"] != "ext4"
            || volume["target"].as_str() != mount.to_str()
        {
            return Err("Build volume changed/absent; no fallback".into());
        }
        let owner = fs::metadata(source)?.uid();
        let m = fs::symlink_metadata(&root)?;
        if !m.is_dir() || m.is_symlink() || m.uid() != owner || m.mode() & 0o777 != 0o700 {
            return Err("Build workspace is not private".into());
        }
        let m = fs::symlink_metadata(&lease)?;
        if !m.is_file()
            || m.is_symlink()
            || lease.canonicalize()? != lease
            || m.uid() != owner
            || m.mode() & 0o777 != 0o600
        {
            return Err("Unqualified existing capacity lease".into());
        }
        let file = if let Some(fd) = env::var_os("CODEX_HOST_LEASE_FD") {
            if env::var("CODEX_HOST_LEASE_TOKEN")
                .ok()
                .filter(|t| !t.is_empty())
                .is_none()
            {
                return Err("Missing current host custodian token".into());
            }
            #[cfg(target_os = "linux")]
            {
                inherited_lease(
                    fd.to_str()
                        .ok_or("Host lease descriptor encoding")?
                        .parse()?,
                    &m,
                )?
            }
            #[cfg(not(target_os = "linux"))]
            {
                let _ = fd;
                return Err("Inherited host leases need the qualified Linux host".into());
            }
        } else {
            let file = OpenOptions::new().read(true).write(true).open(lease)?;
            file.try_lock()
                .map_err(|e| format!("Shared workspace capacity busy: {e}"))?;
            file
        };
        // Existing build caches must already have been separately admitted and
        // relocated. This command never moves data or silently creates a tree.
        for cache in ["target", "build/armsx2-android"] {
            if !source.join(cache).canonicalize()?.starts_with(&root) {
                return Err(format!("{cache} is outside the admitted output workspace").into());
            }
        }
        let temporary = env::temp_dir().canonicalize()?;
        if !temporary.starts_with(&root) {
            return Err("TMPDIR is outside the admitted build workspace".into());
        }
        if let Some(stage) = env::var_os("PSX_APK_STAGE") {
            let stage = PathBuf::from(stage);
            if stage.parent() != Some(root.join("build").as_path())
                || stage.file_name().is_none()
                || stage.is_symlink()
            {
                return Err("APK stage must be a direct admitted build child".into());
            }
        }
        Ok(Self {
            root,
            _lease: Some(file),
        })
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    #[test]
    fn inherited_capacity_is_the_same_locked_description_and_rejects_wrong_inodes() {
        let path = env::temp_dir().join(format!("psx-inherited-capacity-{}", std::process::id()));
        let owner = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        owner.try_lock().unwrap();
        let metadata = owner.metadata().unwrap();
        let duplicate = inherited_lease(owner.as_raw_fd(), &metadata).unwrap();
        assert!(inherited_lease(0, &metadata).is_err());
        assert!(inherited_lease(i32::MAX, &metadata).is_err());
        let wrong = File::open("/dev/null").unwrap();
        assert!(inherited_lease(wrong.as_raw_fd(), &metadata).is_err());
        let competitor = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        assert!(competitor.try_lock().is_err());
        drop(owner);
        assert!(
            competitor.try_lock().is_err(),
            "duplicate must retain the original lease"
        );
        drop(duplicate);
        competitor.try_lock().unwrap();
        drop(competitor);
        fs::remove_file(path).unwrap();
    }
}
