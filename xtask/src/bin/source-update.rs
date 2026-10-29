//! Bounded Linux source delivery/retention from verified complete predecessors.
//! Uses the installed BSD-licensed libarchive ABI, not a TAR parser or extractor.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::{c_char, c_int, c_void, CStr, CString},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, OpenOptionsExt},
        },
    },
    path::{Path, PathBuf},
    process::Command,
    ptr,
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[path = "source-update/producer.rs"]
mod producer;
#[allow(dead_code)]
#[path = "../storage.rs"]
mod storage;
#[path = "source-update/vulkan_ash.rs"]
mod vulkan_ash;
const BASE: &str = "09e27cdb8ad000836ba47220179d7b563790940d";
const TARGET: &str = "123e831e4afb6c7cec2d8e05a030e3cd82a72bba";
const BASE_SHA: &str = "6ba74c41f828a4d9f5dcda8919fc9a504ef17a5145e3cd3ae43088a0d512b521";
const TOOL_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bin/source-update.rs"
));
const CHANGED: [&str; 4] = [
    "crates/player/src/graphics_observer.rs",
    "crates/player/tests/observer_stop.rs",
    "crates/player/tests/ps2_frame_handoff.rs",
    "docs/graphics-observer.md",
];
type Inventory = BTreeMap<String, Value>;
type Payloads = BTreeMap<String, Vec<u8>>;

// Opaque stable libarchive 3 ABI; no borrowed implementation is translated here.
#[link(name = "libarchive.so.13", kind = "dylib", modifiers = "+verbatim")]
unsafe extern "C" {
    fn archive_read_new() -> *mut c_void;
    fn archive_read_support_filter_gzip(a: *mut c_void) -> c_int;
    fn archive_read_support_format_tar(a: *mut c_void) -> c_int;
    fn archive_read_open_filename(a: *mut c_void, p: *const c_char, block: usize) -> c_int;
    fn archive_read_next_header(a: *mut c_void, e: *mut *mut c_void) -> c_int;
    fn archive_read_data(a: *mut c_void, b: *mut c_void, n: usize) -> isize;
    fn archive_read_close(a: *mut c_void) -> c_int;
    fn archive_read_free(a: *mut c_void) -> c_int;
    fn archive_error_string(a: *mut c_void) -> *const c_char;
    fn archive_entry_pathname(e: *mut c_void) -> *const c_char;
    fn archive_entry_mode(e: *mut c_void) -> u32;
    fn archive_entry_size(e: *mut c_void) -> i64;
    fn archive_entry_symlink(e: *mut c_void) -> *const c_char;
    fn archive_entry_hardlink(e: *mut c_void) -> *const c_char;
    fn archive_entry_clone(e: *mut c_void) -> *mut c_void;
    fn archive_entry_new() -> *mut c_void;
    fn archive_entry_free(e: *mut c_void);
    fn archive_entry_set_pathname(e: *mut c_void, p: *const c_char);
    fn archive_entry_set_size(e: *mut c_void, n: i64);
    fn archive_entry_set_mode(e: *mut c_void, mode: u32);
    fn archive_entry_set_hardlink(e: *mut c_void, p: *const c_char);
    fn archive_write_new() -> *mut c_void;
    fn archive_write_set_format_pax_restricted(a: *mut c_void) -> c_int;
    fn archive_write_add_filter_gzip(a: *mut c_void) -> c_int;
    fn archive_write_open_fd(a: *mut c_void, fd: c_int) -> c_int;
    fn archive_write_header(a: *mut c_void, e: *mut c_void) -> c_int;
    fn archive_write_data(a: *mut c_void, b: *const c_void, n: usize) -> isize;
    fn archive_write_close(a: *mut c_void) -> c_int;
    fn archive_write_free(a: *mut c_void) -> c_int;
}
fn ctext(p: *const c_char) -> Result<String> {
    if p.is_null() {
        return Err("Missing archive string".into());
    }
    Ok(unsafe { CStr::from_ptr(p) }.to_str()?.to_owned())
}
fn optional(p: *const c_char) -> Result<Option<String>> {
    if p.is_null() {
        Ok(None)
    } else {
        Ok(Some(ctext(p)?))
    }
}
fn ok(a: *mut c_void, n: c_int) -> Result<()> {
    if n != 0 {
        return Err(format!(
            "libarchive status {n}: {}",
            ctext(unsafe { archive_error_string(a) }).unwrap_or_default()
        )
        .into());
    }
    Ok(())
}
struct Reader(*mut c_void);
impl Reader {
    fn open(path: &Path) -> Result<Self> {
        let r = Self(unsafe { archive_read_new() });
        if r.0.is_null() {
            return Err("Archive allocation failed".into());
        }
        let p = CString::new(path.as_os_str().as_bytes())?;
        unsafe {
            ok(r.0, archive_read_support_filter_gzip(r.0))?;
            ok(r.0, archive_read_support_format_tar(r.0))?;
            ok(r.0, archive_read_open_filename(r.0, p.as_ptr(), 65536))?;
        }
        Ok(r)
    }
    fn next(&mut self) -> Result<Option<Entry>> {
        let mut p = ptr::null_mut();
        let n = unsafe { archive_read_next_header(self.0, &mut p) };
        if n == 1 {
            return Ok(None);
        }
        ok(self.0, n)?;
        let e = Entry(unsafe { archive_entry_clone(p) });
        if e.0.is_null() {
            return Err("Entry clone failed".into());
        }
        Ok(Some(e))
    }
    fn read(&mut self, b: &mut [u8]) -> Result<usize> {
        let n = unsafe { archive_read_data(self.0, b.as_mut_ptr().cast(), b.len()) };
        if n < 0 {
            return Err(format!(
                "Archive data: {}",
                ctext(unsafe { archive_error_string(self.0) }).unwrap_or_default()
            )
            .into());
        }
        Ok(n as usize)
    }
    fn finish(&mut self) -> Result<()> {
        ok(self.0, unsafe { archive_read_close(self.0) })
    }
}
impl Drop for Reader {
    fn drop(&mut self) {
        unsafe {
            archive_read_free(self.0);
        }
    }
}
struct Entry(*mut c_void);
impl Drop for Entry {
    fn drop(&mut self) {
        unsafe {
            archive_entry_free(self.0);
        }
    }
}
struct Writer(*mut c_void);
impl Writer {
    fn open(file: &File) -> Result<Self> {
        let w = Self(unsafe { archive_write_new() });
        if w.0.is_null() {
            return Err("Archive allocation failed".into());
        }
        unsafe {
            ok(w.0, archive_write_set_format_pax_restricted(w.0))?;
            ok(w.0, archive_write_add_filter_gzip(w.0))?;
            ok(w.0, archive_write_open_fd(w.0, file.as_raw_fd()))?;
        }
        Ok(w)
    }
    fn header(&mut self, e: &Entry) -> Result<()> {
        ok(self.0, unsafe { archive_write_header(self.0, e.0) })
    }
    fn write(&mut self, b: &[u8]) -> Result<()> {
        if unsafe { archive_write_data(self.0, b.as_ptr().cast(), b.len()) } != b.len() as isize {
            return Err("Short/failed archive write; incomplete output retained".into());
        }
        Ok(())
    }
    fn finish(&mut self) -> Result<()> {
        ok(self.0, unsafe { archive_write_close(self.0) })
    }
}
impl Drop for Writer {
    fn drop(&mut self) {
        unsafe {
            archive_write_free(self.0);
        }
    }
}

fn safe(name: &str) -> Result<String> {
    let n = name.strip_suffix('/').unwrap_or(name);
    if n.is_empty()
        || n.starts_with('/')
        || n.contains(['\\', '\0', '\n', '\r'])
        || n.split('/').any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(format!("Unsafe archive path: {name:?}").into());
    }
    Ok(n.into())
}
fn safe_link(name: &str, link: &str, hard: bool, root: &str) -> Result<()> {
    if hard {
        let p = safe(link)?;
        if p != root && !p.starts_with(&format!("{root}/")) {
            return Err("Hardlink escapes archive root".into());
        }
        return Ok(());
    }
    if link.is_empty() || link.starts_with('/') || link.contains(['\\', '\0', '\n', '\r']) {
        return Err("Unsafe symlink".into());
    }
    let mut parts: Vec<_> = name.split('/').collect();
    parts.pop();
    for p in link.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                if parts.len() <= 1 {
                    return Err("Symlink escapes archive root".into());
                }
                parts.pop();
            }
            _ => parts.push(p),
        }
    }
    Ok(())
}
fn digest(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn hash(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut h = Sha256::new();
    let mut b = [0; 65536];
    loop {
        let n = f.read(&mut b)?;
        if n == 0 {
            break;
        }
        h.update(&b[..n]);
    }
    Ok(format!("{:x}", h.finalize()))
}
fn capture(n: &str) -> bool {
    matches!(
        n,
        "SOURCE-CHECK.json" | "SOURCE-REVISION.txt" | ".cargo/config.toml"
    ) || n.ends_with("/.psx-source-manifest")
        || n.ends_with("/.cargo-checksum.json")
}
// One pass hashes every body. No extraction, last-wins handling, or raw TAR copying.
fn scan(
    path: &Path,
    root: &str,
    replacements: &Payloads,
    output: Option<(&Path, &File)>,
) -> Result<(Inventory, Payloads)> {
    scan_added(path, root, replacements, &BTreeMap::new(), output)
}
// Additions are immutable Git-bound project files/directories, never source
// dependency trees. The original fixed four-file operation supplies none.
fn scan_added(
    path: &Path,
    root: &str,
    replacements: &Payloads,
    additions: &BTreeMap<String, (u32, Vec<u8>)>,
    output: Option<(&Path, &File)>,
) -> Result<(Inventory, Payloads)> {
    let mut r = Reader::open(path)?;
    let mut w = output.map(|(_, f)| Writer::open(f)).transpose()?;
    let new_root = output
        .map(|(p, _)| p.to_str().ok_or("Invalid root"))
        .transpose()?
        .unwrap_or(root);
    let mut inventory = Inventory::new();
    let mut payload = Payloads::new();
    let mut present = BTreeSet::new();
    let mut captured = 0usize;
    while let Some(e) = r.next()? {
        let full = safe(&ctext(unsafe { archive_entry_pathname(e.0) })?)?;
        let name = if full == root {
            ""
        } else {
            full.strip_prefix(&format!("{root}/"))
                .ok_or("Unexpected archive root")?
        }
        .to_owned();
        if !present.insert(name.clone()) {
            return Err(format!("Duplicate canonical member {name}").into());
        }
        if present.len() > 100000 {
            return Err("Archive entry bound exceeded".into());
        }
        let mode = unsafe { archive_entry_mode(e.0) };
        let kind = mode & 0o170000;
        if ![0o100000, 0o040000, 0o120000].contains(&kind) || mode & 0o7000 != 0 {
            return Err("Unsupported archive entry type/mode".into());
        }
        let size = u64::try_from(unsafe { archive_entry_size(e.0) })?;
        let symlink = optional(unsafe { archive_entry_symlink(e.0) })?;
        let hardlink = optional(unsafe { archive_entry_hardlink(e.0) })?;
        if let Some(link) = &symlink {
            safe_link(&full, link, false, root)?;
        }
        if let Some(link) = &hardlink {
            safe_link(&full, link, true, root)?;
        }
        if replacements.contains_key(&name) && (kind != 0o100000 || hardlink.is_some()) {
            return Err("Replacement is not an ordinary file".into());
        }
        let replacement = replacements.get(&name);
        let keep = capture(&name);
        let mut data = Vec::new();
        let mut total = 0u64;
        let mut h = Sha256::new();
        let out_link = hardlink.as_ref().map(|s| s.replacen(root, new_root, 1));
        if let Some(w) = &mut w {
            let p = CString::new(if name.is_empty() {
                new_root.to_owned()
            } else {
                format!("{new_root}/{name}")
            })?;
            unsafe {
                archive_entry_set_pathname(e.0, p.as_ptr());
            }
            if let Some(b) = replacement {
                unsafe {
                    archive_entry_set_size(e.0, b.len() as i64);
                }
            }
            if let Some(l) = &out_link {
                let l = CString::new(l.as_str())?;
                unsafe {
                    archive_entry_set_hardlink(e.0, l.as_ptr());
                }
            }
            w.header(&e)?;
            if let Some(b) = replacement {
                w.write(b)?;
            }
        }
        let mut buf = [0u8; 65536];
        loop {
            let n = r.read(&mut buf)?;
            if n == 0 {
                break;
            }
            total += n as u64;
            h.update(&buf[..n]);
            if keep {
                captured += n;
                if data.len() + n > 8 * 1024 * 1024 || captured > 64 * 1024 * 1024 {
                    return Err("Metadata capture bound exceeded".into());
                }
                data.extend_from_slice(&buf[..n]);
            }
            if replacement.is_none() {
                if let Some(w) = &mut w {
                    w.write(&buf[..n])?;
                }
            }
        }
        if total != size {
            return Err(format!("Truncated/unsupported body {name}: {total}/{size}").into());
        }
        let actual_hash = format!("{:x}", h.finalize());
        inventory.insert(name.clone(), json!({"mode":mode,"size":replacement.map_or(size, |b|b.len() as u64),"sha256":replacement.map_or(actual_hash, |b|digest(b)),"symlink":symlink,"hardlink":out_link}));
        if keep {
            payload.insert(name, replacement.cloned().unwrap_or(data));
        }
    }
    r.finish()?;
    for (name, (mode, body)) in additions {
        if safe(name)? != *name
            || !present.insert(name.clone())
            || inventory.contains_key(name)
            || ![0o100644, 0o100755, 0o040755].contains(mode)
            || (*mode == 0o040755 && !body.is_empty())
        {
            return Err("Invalid/duplicate project addition".into());
        }
        let parent = Path::new(name)
            .parent()
            .ok_or("Addition parent")?
            .to_str()
            .ok_or("Addition parent encoding")?;
        if inventory
            .get(parent)
            .is_none_or(|v| v["mode"].as_u64().unwrap_or(0) & 0o170000 != 0o040000)
        {
            return Err("Missing ordinary addition parent".into());
        }
        if let Some(w) = &mut w {
            let e = Entry(unsafe { archive_entry_new() });
            if e.0.is_null() {
                return Err("Entry allocation failed".into());
            }
            let full = CString::new(format!("{new_root}/{name}"))?;
            unsafe {
                archive_entry_set_pathname(e.0, full.as_ptr());
                archive_entry_set_mode(e.0, *mode);
                archive_entry_set_size(e.0, body.len() as i64);
            }
            w.header(&e)?;
            w.write(body)?;
        }
        inventory.insert(name.clone(),json!({"mode":mode,"size":body.len(),"sha256":digest(body),"symlink":null,"hardlink":null}));
        if capture(name) {
            payload.insert(name.clone(), body.clone());
        }
    }
    if let Some(w) = &mut w {
        w.finish()?;
    }
    if replacements.keys().any(|p| !present.contains(p)) {
        return Err("Missing required replacement member".into());
    }
    for (p, v) in &inventory {
        if let Some(link) = v["hardlink"].as_str() {
            let rel = link
                .strip_prefix(&format!("{new_root}/"))
                .ok_or("Invalid hardlink")?;
            if inventory.get(rel).is_none_or(|v| {
                v["mode"].as_u64().unwrap_or(0) & 0o170000 != 0o100000 || !v["hardlink"].is_null()
            }) {
                return Err(format!("Invalid hardlink target {p}").into());
            }
        }
    }
    Ok((inventory, payload))
}

fn output(c: &mut Command) -> Result<Vec<u8>> {
    let o = c.output()?;
    if !o.status.success() {
        return Err(format!("Command failed: {}", String::from_utf8_lossy(&o.stderr)).into());
    }
    Ok(o.stdout)
}
fn git(args: &[&str]) -> Result<Vec<u8>> {
    output(
        Command::new("git")
            .args(args)
            .current_dir(source_root())
            .env("GIT_OPTIONAL_LOCKS", "0"),
    )
}
fn source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("project root")
        .to_path_buf()
}
fn external_setting(key: &str) -> Result<PathBuf> {
    std::env::var_os(key)
        .map(PathBuf::from)
        .ok_or_else(|| format!("Missing admitted workspace setting {key}; no fallback").into())
}
fn output_root() -> Result<PathBuf> {
    let keys = [
        "PSX_SOURCE_WORKSPACE",
        "PSX_SOURCE_VOLUME_UUID",
        "PSX_SOURCE_VOLUME_MOUNT",
        "PSX_SOURCE_CAPACITY_LEASE",
    ];
    if keys.iter().all(|k| std::env::var_os(k).is_none()) {
        return Ok(source_root().canonicalize()?);
    }
    let root = external_setting(keys[0])?;
    let uuid = std::env::var(keys[1])?;
    let mount = external_setting(keys[2])?;
    let lease = external_setting(keys[3])?;
    if root.canonicalize()? != root
        || mount.canonicalize()? != mount
        || root == mount
        || !root.starts_with(&mount)
        || lease.starts_with(&mount)
    {
        return Err("Unqualified or redirected external workspace; no fallback".into());
    }
    let listing: Value = serde_json::from_slice(&output(
        Command::new("findmnt")
            .args(["--json", "--target"])
            .arg(&root)
            .args(["--output", "UUID,FSTYPE,TARGET"]),
    )?)?;
    let fs = listing["filesystems"]
        .as_array()
        .filter(|v| v.len() == 1)
        .and_then(|v| v.first())
        .ok_or("Missing unique mounted workspace")?;
    if uuid.is_empty()
        || fs["uuid"] != uuid
        || fs["fstype"] != "ext4"
        || fs["target"].as_str() != mount.to_str()
    {
        return Err("Workspace UUID/type/mount changed or absent; no fallback".into());
    }
    let m = fs::symlink_metadata(&root)?;
    if !m.is_dir()
        || m.is_symlink()
        || m.mode() & 0o777 != 0o700
        || m.uid() != fs::metadata(source_root())?.uid()
    {
        return Err("Workspace ownership/private mode mismatch".into());
    }
    Ok(root)
}
fn capacity_lease() -> Result<Option<File>> {
    if std::env::var_os("PSX_SOURCE_WORKSPACE").is_none() {
        output_root()?;
        return Ok(None);
    }
    let path = external_setting("PSX_SOURCE_CAPACITY_LEASE")?;
    let m = fs::symlink_metadata(&path)?;
    if !m.is_file()
        || m.is_symlink()
        || path.canonicalize()? != path
        || m.mode() & 0o777 != 0o600
        || m.uid() != fs::metadata(source_root())?.uid()
    {
        return Err("Unqualified existing capacity lease".into());
    }
    let file = if let Some(fd) = std::env::var_os("CODEX_HOST_LEASE_FD") {
        use std::os::fd::FromRawFd;
        unsafe extern "C" {
            fn fcntl(fd: i32, command: i32, ...) -> i32;
        }
        let fd: i32 = fd.to_str().ok_or("Host lease encoding")?.parse()?;
        if fd < 3
            || std::env::var("CODEX_HOST_LEASE_TOKEN")
                .ok()
                .filter(|t| !t.is_empty())
                .is_none()
        {
            return Err("Invalid inherited host capacity lease".into());
        }
        let owned = unsafe { fcntl(fd, 1030, 3) }; // Linux F_DUPFD_CLOEXEC
        if owned < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let file = unsafe { File::from_raw_fd(owned) };
        let actual = file.metadata()?;
        if actual.dev() != m.dev() || actual.ino() != m.ino() {
            return Err("Inherited source capacity lease inode differs".into());
        }
        file
    } else {
        OpenOptions::new().read(true).write(true).open(path)?
    };
    file.try_lock()
        .map_err(|e| format!("Shared workspace capacity busy: {e}"))?;
    Ok(Some(file))
}
fn gzip_check(path: &Path) -> Result<()> {
    output(Command::new("gzip").arg("-t").arg(path))?;
    Ok(())
}
fn name(rev: &str) -> String {
    format!("psx-source-{}", &rev[..12])
}
fn base_path() -> PathBuf {
    source_root()
        .join("build")
        .join(format!("{}.tar.gz", name(BASE)))
}
fn baseline() -> Result<(Inventory, Payloads)> {
    let p = base_path();
    let m = fs::symlink_metadata(&p)?;
    if !m.is_file() || m.is_symlink() || m.len() != 205443766 || hash(&p)? != BASE_SHA {
        return Err("Predecessor bytes/binding mismatch".into());
    }
    gzip_check(&p)?;
    let expected = CHANGED.join("\n") + "\n";
    if git(&["diff", "--name-only", BASE, TARGET])? != expected.as_bytes() {
        return Err("Target is not the exact reviewed four-file successor".into());
    }
    let result = scan(&p, &name(BASE), &Payloads::new(), None)?;
    validate(&result.0, &result.1, BASE)?;
    Ok(result)
}
fn validate(inv: &Inventory, payload: &Payloads, revision: &str) -> Result<()> {
    let check: Value = serde_json::from_slice(
        payload
            .get("SOURCE-CHECK.json")
            .ok_or("Missing release metadata")?,
    )?;
    if check["projectRevision"] != revision
        || check["coreRevision"] != "e436e9b85374def58d211df7f78880f0f4a917b4"
    {
        return Err("Release identity mismatch".into());
    }
    if !payload["SOURCE-REVISION.txt"].starts_with(format!("psx {revision}\n").as_bytes()) {
        return Err("Source revision text mismatch".into());
    }
    let cfg = String::from_utf8(git(&["show", &format!("{revision}:.cargo/config.toml")])?)?;
    let expected = cfg
        + "\n[source.crates-io]\nreplace-with = \"release-vendor\"\n\n[source.release-vendor]\ndirectory = \"vendor/rust\"\n";
    if payload[".cargo/config.toml"] != expected.as_bytes() {
        return Err("Vendor configuration differs".into());
    }
    let mut core = Sha256::new();
    for (p, v) in inv.range("vendor/swanstation/".to_owned()..) {
        let Some(rel) = p.strip_prefix("vendor/swanstation/") else {
            break;
        };
        if v["mode"].as_u64().ok_or("mode")? & 0o170000 == 0o100000
            && rel != "src/libretro/version.h"
        {
            core.update(rel.as_bytes());
            core.update([0]);
            core.update(v["sha256"].as_str().ok_or("hash")?.as_bytes());
        }
    }
    if check["coreSourceSha256"] != format!("{:x}", core.finalize()) {
        return Err("SwanStation inventory hash mismatch".into());
    }
    let locks = String::from_utf8(git(&[
        "show",
        &format!("{revision}:vendor/armsx2-dependencies/revisions.tsv"),
    ])?)?;
    let mut pinned = BTreeMap::from([(
        "vendor/armsx2".to_owned(),
        "83fe4e293f71ab5587ef81b06143725bf8c6a30b".to_owned(),
    )]);
    for line in locks.lines() {
        let f: Vec<_> = line.split('\t').collect();
        if f.len() != 3 {
            return Err("Bad dependency lock".into());
        }
        pinned.insert(
            format!("vendor/armsx2-dependencies/source/{}", f[0]),
            f[2].into(),
        );
    }
    for (prefix, rev) in &pinned {
        let key = format!("{prefix}/.psx-source-manifest");
        let manifest =
            String::from_utf8(payload.get(&key).ok_or("Missing native manifest")?.clone())?;
        let mut lines = manifest.lines();
        if lines.next() != Some(rev.as_str()) {
            return Err("Native manifest pin mismatch".into());
        }
        let mut listed = BTreeSet::new();
        for line in lines {
            let (h, p) = line.split_once('\t').ok_or("Bad native manifest")?;
            safe(p)?;
            let full = format!("{prefix}/{p}");
            if !listed.insert(full.clone()) || inv.get(&full).is_none_or(|v| v["sha256"] != h) {
                return Err(format!("Native source differs: {full}").into());
            }
        }
        let actual: BTreeSet<_> = inv
            .iter()
            .filter(|(p, v)| {
                p.starts_with(&format!("{prefix}/"))
                    && *p != &key
                    && !pinned.keys().any(|child| {
                        child != prefix
                            && child.starts_with(&format!("{prefix}/"))
                            && p.starts_with(&format!("{child}/"))
                    })
                    && v["mode"].as_u64().unwrap_or(0) & 0o170000 == 0o100000
            })
            .map(|(p, _)| p.clone())
            .collect();
        if actual != listed {
            return Err(format!("Native manifest entry-set mismatch: {prefix}").into());
        }
    }
    for (key, b) in payload
        .iter()
        .filter(|(p, _)| p.ends_with("/.cargo-checksum.json"))
    {
        let prefix = key
            .strip_suffix("/.cargo-checksum.json")
            .ok_or("checksum")?;
        let j: Value = serde_json::from_slice(b)?;
        let files = j["files"].as_object().ok_or("Cargo checksum files")?;
        let mut listed = BTreeSet::new();
        for (p, h) in files {
            safe(p)?;
            let full = format!("{prefix}/{p}");
            listed.insert(full.clone());
            if inv.get(&full).is_none_or(|v| v["sha256"] != *h) {
                return Err(format!("Cargo source differs: {full}").into());
            }
        }
        let actual: BTreeSet<_> = inv
            .iter()
            .filter(|(p, v)| {
                p.starts_with(&format!("{prefix}/"))
                    && *p != key
                    && v["mode"].as_u64().unwrap_or(0) & 0o170000 != 0o040000
            })
            .map(|(p, _)| p.clone())
            .collect();
        if actual != listed {
            return Err("Cargo checksum entry-set mismatch".into());
        }
    }
    for p in [
        "xtask/src/main.rs",
        "docs/build.md",
        "vendor/armsx2/COPYING.GPLv3",
        "vendor/swanstation/LICENSE",
        "vendor/openbios/source/LICENSE",
        "LICENSE",
        "Cargo.lock",
    ] {
        if !inv.contains_key(p) {
            return Err(format!("Missing source/notice {p}").into());
        }
    }
    // Every tracked project member is independently bound to the immutable Git
    // tree. The one generated Cargo configuration was checked separately above.
    let tree = git(&["ls-tree", "-rz", "--full-tree", revision])?;
    let mut tracked = BTreeSet::new();
    for record in tree.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let s = std::str::from_utf8(record)?;
        let (spec, p) = s.split_once('\t').ok_or("Bad Git tree")?;
        safe(p)?;
        let mode = u32::from_str_radix(spec.split_whitespace().next().ok_or("Git mode")?, 8)?;
        tracked.insert(p.to_owned());
        if mode == 0o160000 {
            if inv
                .get(p)
                .is_none_or(|v| v["mode"].as_u64().unwrap_or(0) & 0o170000 != 0o040000)
            {
                return Err("Missing exported submodule".into());
            }
            continue;
        }
        let v = inv.get(p).ok_or("Missing tracked project member")?;
        if v["mode"].as_u64().ok_or("mode")? & 0o170111 != u64::from(mode & 0o170111) {
            return Err(format!("Project type/executable mode differs: {p}").into());
        }
        if p == ".cargo/config.toml" {
            continue;
        }
        let b = git(&["show", &format!("{revision}:{p}")])?;
        if mode & 0o170000 == 0o120000 {
            if v["symlink"].as_str() != Some(std::str::from_utf8(&b)?) {
                return Err("Project symlink differs".into());
            }
        } else if v["sha256"] != digest(&b) {
            return Err(format!("Project content differs: {p}").into());
        }
    }
    let cargo_roots: Vec<_> = payload
        .keys()
        .filter_map(|p| p.strip_suffix("/.cargo-checksum.json"))
        .collect();
    for (p, v) in inv {
        if v["mode"].as_u64().unwrap_or(0) & 0o170000 == 0o040000
            || tracked.contains(p)
            || matches!(p.as_str(), "SOURCE-CHECK.json" | "SOURCE-REVISION.txt")
        {
            continue;
        }
        if p.starts_with("vendor/swanstation/")
            || pinned
                .keys()
                .any(|prefix| p.starts_with(&format!("{prefix}/")))
            || cargo_roots
                .iter()
                .any(|prefix| p.starts_with(&format!("{prefix}/")))
        {
            continue;
        }
        return Err(format!("Unexpected source member {p}").into());
    }
    Ok(())
}
fn replacements(payload: &Payloads) -> Result<Payloads> {
    let mut out = Payloads::new();
    for p in CHANGED {
        out.insert(p.into(), git(&["show", &format!("{TARGET}:{p}")])?);
    }
    let mut check: Value = serde_json::from_slice(&payload["SOURCE-CHECK.json"])?;
    check["projectRevision"] = json!(TARGET);
    out.insert(
        "SOURCE-CHECK.json".into(),
        serde_json::to_vec_pretty(&check)?,
    );
    let text = String::from_utf8(payload["SOURCE-REVISION.txt"].clone())?;
    out.insert(
        "SOURCE-REVISION.txt".into(),
        text.replacen(BASE, TARGET, 1).into_bytes(),
    );
    Ok(out)
}
fn durable(path: &Path, value: &Value) -> Result<()> {
    output_root()?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(&serde_json::to_vec_pretty(value)?)?;
    f.sync_all()?;
    File::open(path.parent().ok_or("parent")?)?.sync_all()?;
    Ok(())
}
fn update(inv: &Inventory, payload: &Payloads) -> Result<()> {
    output_root()?;
    let root = std::env::current_dir()?;
    let _growth = storage::Growth::begin(&root, storage::GIB / 2)?;
    let replacements = replacements(payload)?;
    let mut expected = inv.clone();
    for (p, b) in &replacements {
        let v = expected.get_mut(p).ok_or("Replacement absent")?;
        if v["mode"].as_u64().ok_or("mode")? & 0o170000 != 0o100000 {
            return Err("Non-file replacement".into());
        }
        v["size"] = json!(b.len());
        v["sha256"] = json!(digest(b));
    }
    // Hardlinks, if present, bind the renamed root while symlinks remain relative.
    for v in expected.values_mut() {
        if let Some(s) = v["hardlink"].as_str() {
            v["hardlink"] = json!(s.replacen(&name(BASE), &name(TARGET), 1));
        }
    }
    let final_path = Path::new("build").join(format!("{}.tar.gz", name(TARGET)));
    let partial = final_path.with_extension("gz.incomplete");
    if final_path.exists() || partial.exists() {
        return Err("Existing archive/partial retained; inspect before retry".into());
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&partial)?;
    let (written, _) = scan(
        &base_path(),
        &name(BASE),
        &replacements,
        Some((Path::new(&name(TARGET)), &file)),
    )?;
    if written != expected {
        return Err("Writer inventory differs".into());
    }
    file.sync_all()?;
    output_root()?;
    storage::between_steps()?;
    gzip_check(&partial)?;
    let (actual, metadata) = scan(&partial, &name(TARGET), &Payloads::new(), None)?;
    if actual != expected {
        return Err("Independent streamed readback differs".into());
    }
    validate(&actual, &metadata, TARGET)?;
    if hash(&base_path())? != BASE_SHA {
        return Err("Predecessor changed".into());
    }
    let sha = hash(&partial)?;
    storage::between_steps()?;
    let utility = String::from_utf8(git(&["rev-parse", "HEAD"])?)?;
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err(
            "Commit packaging utility before canonical publication; partial retained".into(),
        );
    }
    let utility_source_sha = digest(TOOL_SOURCE);
    if hash(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src/bin/source-update.rs"))?
        != utility_source_sha
    {
        return Err(
            "Executed utility differs from current committed source; partial retained".into(),
        );
    }
    let utility_binary_sha = hash(&std::env::current_exe()?)?;
    output_root()?;
    // No-overwrite durable publication, only after separate full readback.
    fs::hard_link(&partial, &final_path)?;
    File::open("build")?.sync_all()?;
    let checksum = final_path.with_extension("gz.sha256");
    let mut side = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&checksum)?;
    writeln!(side, "{sha}  {}.tar.gz", name(TARGET))?;
    side.sync_all()?;
    File::open("build")?.sync_all()?;
    durable(
        Path::new(".local/source-update-123e831-complete.json"),
        &json!({"phase":"verified-complete","targetRevision":TARGET,"utilityRevision":utility.trim(),"utilitySourceSha256":utility_source_sha,"utilityBinarySha256":utility_binary_sha,"predecessorRevision":BASE,"predecessorSha256":BASE_SHA,"archive":final_path,"archiveSha256":sha,"archiveBytes":file.metadata()?.len(),"members":actual.len(),"replacements":replacements.keys().collect::<Vec<_>>(),"streamedReadback":true}),
    )?;
    fs::remove_file(&partial)?;
    File::open("build")?.sync_all()?;
    println!(
        "Complete source archive {}: {sha}; target {TARGET}, utility {}",
        final_path.display(),
        utility.trim()
    );
    Ok(())
}

fn stage_tree(root: &Path) -> Result<Inventory> {
    fn visit(root: &Path, path: &Path, inv: &mut Inventory) -> Result<()> {
        let m = fs::symlink_metadata(path)?;
        let rel = path
            .strip_prefix(root)?
            .to_str()
            .ok_or("Non-UTF8 stage path")?;
        if !rel.is_empty() {
            safe(rel)?;
        }
        let link = if m.is_symlink() {
            Some(
                fs::read_link(path)?
                    .to_str()
                    .ok_or("Non-UTF8 symlink")?
                    .to_owned(),
            )
        } else {
            None
        };
        let sha = if m.is_file() {
            Some(hash(path)?)
        } else {
            link.as_ref().map(|l| digest(l.as_bytes()))
        };
        inv.insert(rel.into(), json!({"mode":m.mode(),"size":m.len(),"device":m.dev(),"inode":m.ino(),"mtime":[m.mtime(),m.mtime_nsec()],"ctime":[m.ctime(),m.ctime_nsec()],"symlink":link,"sha256":sha}));
        if inv.len() > 100000 {
            return Err("Stage inventory bound".into());
        }
        if m.is_dir() && !m.is_symlink() {
            for e in fs::read_dir(path)? {
                visit(root, &e?.path(), inv)?;
            }
        }
        Ok(())
    }
    if !fs::symlink_metadata(root)?.is_dir() || fs::symlink_metadata(root)?.is_symlink() {
        return Err("Nonordinary stage".into());
    }
    let mut out = Inventory::new();
    visit(root, root, &mut out)?;
    Ok(out)
}
fn stage(retire: bool, inv: &Inventory, payload: &Payloads) -> Result<()> {
    let root = source_root().canonicalize()?;
    let parent = root.join("build/source-bundles");
    let stage = parent.join(name(TARGET));
    if parent.canonicalize()? != parent {
        return Err("Redirected source parent".into());
    }
    let marker = stage.join(".psx-source-bundle");
    let mm = fs::symlink_metadata(&marker)?;
    if !mm.is_file() || mm.is_symlink() || fs::read_to_string(&marker)? != TARGET {
        return Err("Wrong stage marker".into());
    }
    let before = stage_tree(&stage)?;
    let replacements = replacements(payload)?;
    let mut proven = BTreeMap::new();
    // All unchanged entries derive from the pinned complete archive, not the stage.
    for (p, v) in &before {
        if p == ".psx-source-bundle" {
            continue;
        }
        let Some(expected) = inv.get(p) else { continue };
        if v["mode"] != expected["mode"] {
            continue;
        }
        let kind = v["mode"].as_u64().ok_or("mode")? & 0o170000;
        let matches = if kind == 0o040000 {
            true
        } else if kind == 0o120000 {
            v["symlink"] == expected["symlink"]
        } else if kind == 0o100000 && expected["hardlink"].is_null() {
            let desired = replacements
                .get(p)
                .map(|b| digest(b))
                .unwrap_or_else(|| expected["sha256"].as_str().unwrap_or("").to_owned());
            v["sha256"] == desired
        } else {
            false
        };
        if matches {
            proven.insert(p.clone(), json!({"independent":"verified-predecessor-plus-target","targetRevision":TARGET,"sha256":replacements.get(p).map(|b|digest(b)).unwrap_or_else(||expected["sha256"].as_str().unwrap_or("").to_owned())}));
        }
    }
    // The interrupted stage legitimately still has Git's pre-vendoring config.
    let p = ".cargo/config.toml";
    if let Some(v) = before.get(p) {
        let b = git(&["show", &format!("{TARGET}:{p}")])?;
        if v["mode"] == 0o100644 && v["sha256"] == digest(&b) {
            proven.insert(p.into(), json!({"independent":"immutable-target-Git-blob","revision":TARGET,"path":p,"sha256":digest(&b)}));
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let receipt =
        Path::new(".local/storage-retention").join(format!("source-123e831-partial-{stamp}.json"));
    let failure = root.join(".local/observer-123e831-source-final.log");
    durable(
        &receipt,
        &json!({"phase":"audited-before-removal","retire":retire,"targetRevision":TARGET,"marker":TARGET,"failureLog":failure,"failureLogSha256":hash(&failure)?,"independentArchiveSha256":BASE_SHA,"inventory":before,"proven":proven,"protected":before.keys().filter(|p|!proven.contains_key(*p)).collect::<Vec<_>>()}),
    )?;
    println!(
        "{} entries audited; {} independently proven; receipt {}",
        before.len(),
        proven.len(),
        receipt.display()
    );
    if !retire {
        return Ok(());
    }
    if hash(&base_path())? != BASE_SHA || stage_tree(&stage)? != before {
        return Err("Inputs changed after receipt; preserve stage".into());
    }
    let mut files = 0;
    for p in proven.keys() {
        let kind = before[p]["mode"].as_u64().ok_or("mode")? & 0o170000;
        if kind == 0o040000 {
            continue;
        }
        let root_now = fs::symlink_metadata(&stage)?;
        if !root_now.is_dir()
            || root_now.is_symlink()
            || before[""]["inode"] != root_now.ino()
            || before[""]["device"] != root_now.dev()
        {
            return Err("Stage root changed; remaining entries retained".into());
        }
        // Revalidate every ancestor without following a redirected directory.
        let mut current = stage.clone();
        for c in Path::new(p).parent().ok_or("parent")?.components() {
            current.push(c);
            let m = fs::symlink_metadata(&current)?;
            let rel = current.strip_prefix(&stage)?.to_str().ok_or("path")?;
            if !m.is_dir()
                || m.is_symlink()
                || before[rel]["inode"] != m.ino()
                || before[rel]["device"] != m.dev()
            {
                return Err("Stage parent changed; remaining entries retained".into());
            }
        }
        let single = fs::symlink_metadata(stage.join(p))?;
        if before[p]["inode"] != single.ino()
            || before[p]["device"] != single.dev()
            || before[p]["mode"] != single.mode()
            || before[p]["mtime"] != json!([single.mtime(), single.mtime_nsec()])
            || before[p]["ctime"] != json!([single.ctime(), single.ctime_nsec()])
        {
            return Err("Entry changed; remaining entries retained".into());
        }
        fs::remove_file(stage.join(p))?;
        files += 1;
    }
    let mut dirs: Vec<_> = proven
        .keys()
        .filter(|p| {
            before[*p]["mode"].as_u64().unwrap_or(0) & 0o170000 == 0o040000 && !p.is_empty()
        })
        .collect();
    dirs.sort_by_key(|p| std::cmp::Reverse(p.split('/').count()));
    for p in dirs {
        if fs::read_dir(stage.join(p))?.next().is_none() {
            fs::remove_dir(stage.join(p))?;
        }
    }
    let remaining = stage_tree(&stage)?;
    if remaining
        .keys()
        .all(|p| p.is_empty() || p == ".psx-source-bundle")
    {
        fs::remove_file(marker)?;
        fs::remove_dir(&stage)?;
    }
    File::open(&parent)?.sync_all()?;
    durable(
        &receipt.with_extension("completed.json"),
        &json!({"phase":"retired-proven-duplicates","proofReceipt":receipt,"removedFiles":files,"remaining":if stage.exists(){Some(stage_tree(&stage)?)}else{None}}),
    )?;
    println!("Retired {files} proven duplicate files; unknown/divergent material preserved");
    Ok(())
}

fn main() -> Result<()> {
    let _capacity = capacity_lease()?;
    let work = output_root()?;
    let _lock = storage::Lock::acquire(&source_root().canonicalize()?)?;
    // Existing growth checks measure the admitted output filesystem. Immutable
    // inputs and Git operations remain anchored to the retained source tree.
    std::env::set_current_dir(&work)?;
    let op = std::env::args()
        .nth(1)
        .ok_or("Usage: source-update audit | retire | update | finish | verify")?;
    if op == "repair-overlap-metadata" {
        return producer::repair_overlap_metadata();
    }
    if op == "producer-update" {
        return producer::run(
            &std::env::args()
                .nth(2)
                .ok_or("Missing exact target revision")?,
        );
    }
    if op == "vulkan-update" {
        return producer::vulkan(
            &std::env::args()
                .nth(2)
                .ok_or("Missing exact target revision")?,
        );
    }
    if op == "retained-update" {
        return producer::retained(
            &std::env::args()
                .nth(2)
                .ok_or("Missing exact frozen target revision")?,
        );
    }
    if op == "player-update" {
        return producer::player(
            &std::env::args()
                .nth(2)
                .ok_or("Missing exact player target revision")?,
        );
    }
    let (inv, payload) = baseline()?;
    match op.as_str() {
        "audit" => stage(false, &inv, &payload),
        "retire" => stage(true, &inv, &payload),
        "update" => update(&inv, &payload),
        "finish" => {
            stage(true, &inv, &payload)?;
            update(&inv, &payload)
        }
        "verify" => verify_target(&inv, &payload),
        _ => Err("Unknown operation".into()),
    }
}

fn verify_target(inv: &Inventory, payload: &Payloads) -> Result<()> {
    let replacements = replacements(payload)?;
    let p = Path::new("build").join(format!("{}.tar.gz", name(TARGET)));
    gzip_check(&p)?;
    let (actual, metadata) = scan(&p, &name(TARGET), &Payloads::new(), None)?;
    let mut expected = inv.clone();
    for (p, b) in replacements {
        expected.get_mut(&p).ok_or("missing replacement")?["sha256"] = json!(digest(&b));
        expected.get_mut(&p).unwrap()["size"] = json!(b.len());
    }
    for v in expected.values_mut() {
        if let Some(l) = v["hardlink"].as_str() {
            v["hardlink"] = json!(l.replacen(&name(BASE), &name(TARGET), 1));
        }
    }
    if actual != expected {
        return Err("Readback inventory mismatch".into());
    }
    validate(&actual, &metadata, TARGET)?;
    println!("Verified complete target archive: {}", hash(&p)?);
    Ok(())
}

#[cfg(test)]
mod focused {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let n = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let p = output_root()
                .unwrap()
                .join("build")
                .join(format!("source-update-fixture-{}-{n}", std::process::id()));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn archive(&self, duplicate: bool) -> PathBuf {
            fs::create_dir(self.0.join("old")).unwrap();
            fs::write(self.0.join("old/unchanged"), b"independent payload").unwrap();
            fs::write(self.0.join("old/changed"), b"old content").unwrap();
            let tar = self.0.join("input.tar");
            let mut c = Command::new("tar");
            c.arg("-cf")
                .arg(&tar)
                .arg("-C")
                .arg(&self.0)
                .args(["old/unchanged", "old/changed"]);
            if duplicate {
                c.arg("old/changed");
            }
            assert!(c.status().unwrap().success());
            tar
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[test]
    fn preserves_unchanged_replaces_and_renames_once() {
        let f = Fixture::new();
        let src = f.archive(false);
        let dst = f.0.join("output.gz.incomplete");
        let file = File::create(&dst).unwrap();
        let replacements =
            Payloads::from([("changed".into(), b"new exact target content".to_vec())]);
        let (expected, _) =
            scan(&src, "old", &replacements, Some((Path::new("new"), &file))).unwrap();
        file.sync_all().unwrap();
        gzip_check(&dst).unwrap();
        let (actual, _) = scan(&dst, "new", &Payloads::new(), None).unwrap();
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 2);
        assert_eq!(
            actual["unchanged"]["sha256"],
            digest(b"independent payload")
        );
        assert_eq!(
            actual["changed"]["sha256"],
            digest(b"new exact target content")
        );
        assert!(scan(&dst, "old", &Payloads::new(), None).is_err());
    }
    #[test]
    fn rejects_duplicate_member_instead_of_last_wins() {
        let f = Fixture::new();
        let p = f.archive(true);
        assert!(scan(&p, "old", &Payloads::new(), None).is_err());
    }
    #[test]
    fn rejects_unsafe_paths_and_escaping_links() {
        for p in [
            "/absolute",
            "old/../escape",
            "old//alias",
            "./old/file",
            "old\\file",
        ] {
            assert!(safe(p).is_err(), "{p}");
        }
        assert!(safe_link("old/file", "../../outside", false, "old").is_err());
        assert!(safe_link("old/file", "other/file", true, "old").is_err());
        assert!(safe_link("old/dir/link", "../file", false, "old").is_ok());
        let f = Fixture::new();
        let _ = f.archive(false);
        let bad = f.0.join("unsafe.tar");
        assert!(Command::new("tar")
            .arg("-cf")
            .arg(&bad)
            .arg("--transform=s,^old,../escape,")
            .arg("-C")
            .arg(&f.0)
            .arg("old/changed")
            .status()
            .unwrap()
            .success());
        assert!(scan(&bad, "old", &Payloads::new(), None).is_err());
        std::os::unix::fs::symlink("../outside", f.0.join("old/link")).unwrap();
        let link = f.0.join("link.tar");
        assert!(Command::new("tar")
            .arg("-cf")
            .arg(&link)
            .arg("-C")
            .arg(&f.0)
            .arg("old/link")
            .status()
            .unwrap()
            .success());
        assert!(scan(&link, "old", &Payloads::new(), None).is_err());
    }
    #[test]
    fn rejects_truncation_and_missing_replacement() {
        let f = Fixture::new();
        let p = f.archive(false);
        let b = fs::read(&p).unwrap();
        fs::write(&p, &b[..1000]).unwrap();
        assert!(scan(&p, "old", &Payloads::new(), None).is_err());
        let g = Fixture::new();
        let p = g.archive(false);
        assert!(scan(
            &p,
            "old",
            &Payloads::from([("absent".into(), vec![])]),
            None
        )
        .is_err());
    }
    #[test]
    fn propagates_delayed_writer_failure() {
        let f = Fixture::new();
        let p = f.archive(false);
        let full = OpenOptions::new().write(true).open("/dev/full").unwrap();
        assert!(scan(&p, "old", &Payloads::new(), Some((Path::new("new"), &full))).is_err());
    }
    #[test]
    fn complete_gzip_check_rejects_missing_footer() {
        let f = Fixture::new();
        let p = f.archive(false);
        let mut b = output(Command::new("gzip").arg("-c").arg(&p)).unwrap();
        b.truncate(b.len() - 8);
        let broken = f.0.join("truncated.tar.gz");
        fs::write(&broken, b).unwrap();
        assert!(gzip_check(&broken).is_err());
    }
    #[test]
    fn regenerates_revision_metadata_without_altering_other_fields() {
        let mut p = Payloads::new();
        p.insert(
            "SOURCE-CHECK.json".into(),
            format!("{{\"projectRevision\":\"{BASE}\",\"coreSourceSha256\":\"preserve\"}}")
                .into_bytes(),
        );
        p.insert(
            "SOURCE-REVISION.txt".into(),
            format!("psx {BASE}\nunchanged recipe and provenance\n").into_bytes(),
        );
        let r = replacements(&p).unwrap();
        let j: Value = serde_json::from_slice(&r["SOURCE-CHECK.json"]).unwrap();
        assert_eq!(j["projectRevision"], TARGET);
        assert_eq!(j["coreSourceSha256"], "preserve");
        assert_eq!(
            r["SOURCE-REVISION.txt"],
            format!("psx {TARGET}\nunchanged recipe and provenance\n").as_bytes()
        );
        assert_eq!(r.len(), 6);
    }
}
