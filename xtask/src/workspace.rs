//! Optional admitted build output. No device or admission policy lives here.
use super::*;
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::MetadataExt,
};

pub fn path(relative: &str) -> String {
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
        return Ok(());
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
        let file = OpenOptions::new().read(true).write(true).open(lease)?;
        file.try_lock()
            .map_err(|e| format!("Shared workspace capacity busy: {e}"))?;
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
        Ok(Self {
            root,
            _lease: Some(file),
        })
    }
}
