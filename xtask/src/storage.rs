//! Project-owned capacity checks and verified source-staging retention.
use crate::{hash, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Component, Path, PathBuf},
    process::{Command, Output},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const GIB: u64 = 1024 * 1024 * 1024;
pub const RESERVE: u64 = 12 * GIB;
static GROWING: Mutex<Option<PathBuf>> = Mutex::new(None);
pub struct Growth;
impl Growth {
    pub fn begin(root: &Path, planned: u64) -> Result<Self> {
        check(root, planned)?;
        *GROWING.lock().map_err(|_| "Storage guard poisoned")? = Some(root.canonicalize()?);
        Ok(Self)
    }
}
impl Drop for Growth {
    fn drop(&mut self) {
        if let Ok(mut root) = GROWING.lock() {
            *root = None;
        }
    }
}
pub fn between_steps() -> Result<()> {
    if let Some(root) = GROWING
        .lock()
        .map_err(|_| "Storage guard poisoned")?
        .as_ref()
    {
        space(root)?.require(0)?;
    }
    Ok(())
}

fn owned_dir(root: &Path, relative: &str) -> Result<PathBuf> {
    if root.canonicalize()? != root {
        return Err("Use the canonical project root".into());
    }
    let mut path = root.to_owned();
    for part in Path::new(relative).components() {
        if !matches!(part, Component::Normal(_)) {
            return Err("Invalid owned directory".into());
        }
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(m) if m.is_dir() && !m.is_symlink() => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::create_dir(&path)?,
            _ => {
                return Err(
                    "Owned directory is a symlink or non-directory; preserve for review".into(),
                )
            }
        }
    }
    Ok(path)
}
pub fn source_parent(root: &Path) -> Result<PathBuf> {
    owned_dir(root, "build/source-bundles")
}

pub struct Lock(File);
impl Lock {
    pub fn acquire(root: &Path) -> Result<Self> {
        let path = owned_dir(root, "build")?.join(".psx-storage.lock");
        if fs::symlink_metadata(&path).is_ok_and(|m| !m.is_file() || m.is_symlink()) {
            return Err("The project storage lock is not an ordinary file.".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock()
            .map_err(|e| format!("Another psx build/retention operation owns storage: {e}"))?;
        Ok(Self(file))
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

#[derive(Debug, PartialEq)]
pub struct Space {
    pub available: u64,
    pub metadata: Option<(u64, u64)>,
    pub unallocated: Option<u64>,
    pub global_reserve: Option<(u64, u64)>,
}
impl Space {
    fn value(&self) -> Value {
        json!({"availableBytes":self.available,"metadataAllocatedUsed":self.metadata,"deviceUnallocatedBytes":self.unallocated,"globalReservedUsed":self.global_reserve})
    }
    pub fn require(&self, planned: u64) -> Result<()> {
        let needed = RESERVE
            .checked_add(planned)
            .ok_or("Storage budget overflow")?;
        if self.available < needed {
            return Err(format!("Storage pressure: {:.2} GiB available; {:.2} GiB needed including the provisional 12 GiB shared-host reserve. Run cargo xtask storage; keep unfinished work.", self.available as f64/GIB as f64, needed as f64/GIB as f64).into());
        }
        if let Some((total, used)) = self.metadata {
            let (reserved, emergency_used) = self
                .global_reserve
                .ok_or("Missing Btrfs emergency-reserve measurement; no growth authorized")?;
            let unallocated = self
                .unallocated
                .ok_or("Missing Btrfs allocation measurement; no growth authorized")?;
            // Unused metadata is already allocated workspace. Unallocated device
            // bytes are needed for expansion, not every ordinary write. Exclude
            // the emergency reserve, and retain twice the ordinary margin when
            // no complete new chunk is available. Never spend emergency space.
            let margin = if unallocated < GIB { GIB } else { GIB / 2 };
            if used > total
                || reserved > total.saturating_sub(used)
                || emergency_used != 0
                || total - used - reserved < margin
            {
                return Err("Btrfs metadata pressure: retain 512 MiB outside the unused emergency reserve, or 1 GiB when device-unallocated space is low. No automatic balance/repair.".into());
            }
        }
        Ok(())
    }
}
fn successful(mut command: Command) -> Result<Output> {
    let out = command.output()?;
    if !out.status.success() {
        return Err(format!(
            "{}: {}",
            command.get_program().to_string_lossy(),
            String::from_utf8_lossy(&out.stderr)
        )
        .into());
    }
    Ok(out)
}
fn cmd(program: &str, args: &[&str]) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    command
}
fn number_after(text: &str, label: &str) -> Result<u64> {
    Ok(text
        .split_once(label)
        .ok_or("Missing storage metric")?
        .1
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()
        .ok_or("Invalid storage metric")?
        .parse()?)
}
fn parse_btrfs(text: &str, available: u64) -> Result<Space> {
    let mut total = 0u64;
    let mut used = 0u64;
    for row in text.lines().filter(|s| s.starts_with("Metadata,")) {
        total = total
            .checked_add(number_after(row, "Size:")?)
            .ok_or("Metadata size overflow")?;
        used = used
            .checked_add(number_after(row, "Used:")?)
            .ok_or("Metadata usage overflow")?;
    }
    if total == 0 || used > total {
        return Err("Btrfs metadata measurements are unavailable; no growth authorized.".into());
    }
    Ok(Space {
        available,
        metadata: Some((total, used)),
        unallocated: Some(number_after(text, "Device unallocated:")?),
        global_reserve: Some((
            number_after(text, "Global reserve:")?,
            number_after(text, "(used:")?,
        )),
    })
}
pub fn space(root: &Path) -> Result<Space> {
    let mut df = cmd("df", &["-B1", "--output=avail,fstype"]);
    df.arg(root).env("LC_ALL", "C");
    let output = String::from_utf8(successful(df)?.stdout)?;
    let mut fields = output
        .lines()
        .nth(1)
        .ok_or("No filesystem measurement")?
        .split_whitespace();
    let available = fields.next().ok_or("No free-space measurement")?.parse()?;
    if fields.next() != Some("btrfs") {
        return Ok(Space {
            available,
            metadata: None,
            unallocated: None,
            global_reserve: None,
        });
    }
    let mut btrfs = cmd("btrfs", &["filesystem", "usage", "-b"]);
    btrfs.arg(root).env("LC_ALL", "C");
    parse_btrfs(&String::from_utf8(successful(btrfs)?.stdout)?, available)
}
pub fn check(root: &Path, planned: u64) -> Result<()> {
    let current = space(root)?;
    eprintln!(
        "Storage: {:.2} GiB available; planned {:.2} GiB plus 12 GiB reserve",
        current.available as f64 / GIB as f64,
        planned as f64 / GIB as f64
    );
    current.require(planned)
}

#[derive(Clone, Debug, PartialEq)]
struct Stamp {
    bytes: u64,
    blocks: u64,
    inode: u64,
    device: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    mode: u32,
    link: Option<PathBuf>,
    kind: u8,
}
fn tree(path: &Path) -> Result<BTreeMap<String, Stamp>> {
    let root = fs::symlink_metadata(path)?;
    if !root.is_dir() || root.is_symlink() {
        return Err("Inventory root must be an ordinary directory".into());
    }
    fn visit(base: &Path, path: &Path, out: &mut BTreeMap<String, Stamp>) -> Result<()> {
        if out.len() >= 1_000_000 {
            return Err("Inventory limit reached; preserve this tree for review.".into());
        }
        let m = fs::symlink_metadata(path)?;
        let name = path
            .strip_prefix(base)?
            .to_str()
            .ok_or("Non-UTF8 staging path; preserve for review")?
            .to_owned();
        if name.contains(['\n', '\r']) {
            return Err("Multiline staging path; preserve for review".into());
        }
        let kind = if m.is_symlink() {
            2
        } else if m.is_dir() {
            1
        } else if m.is_file() {
            0
        } else {
            return Err("Special staging entry; preserve for review".into());
        };
        out.insert(
            name,
            Stamp {
                bytes: m.len(),
                blocks: m.blocks(),
                inode: m.ino(),
                device: m.dev(),
                modified: (m.mtime(), m.mtime_nsec()),
                changed: (m.ctime(), m.ctime_nsec()),
                mode: m.mode(),
                link: if kind == 2 {
                    Some(fs::read_link(path)?)
                } else {
                    None
                },
                kind,
            },
        );
        if kind == 1 {
            for child in fs::read_dir(path)? {
                visit(base, &child?.path(), out)?;
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    for child in fs::read_dir(path)? {
        visit(path, &child?.path(), &mut out)?;
    }
    Ok(out)
}
fn ordinary(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || m.is_symlink() {
        return Err(format!("{} is not an ordinary file", path.display()).into());
    }
    Ok(())
}
fn revision(text: &str) -> Result<&str> {
    if text.len() != 40 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("Invalid source revision; preserve for review".into());
    }
    Ok(text)
}
pub fn verify_archive(root: &Path, rev: &str) -> Result<PathBuf> {
    revision(rev)?;
    let name = format!("psx-source-{}", &rev[..12]);
    let archive = root.join("build").join(format!("{name}.tar.gz"));
    let checksum = archive.with_extension("gz.sha256");
    ordinary(&archive)?;
    ordinary(&checksum)?;
    let contents = fs::read_to_string(checksum)?;
    let expected: Vec<_> = contents.split_whitespace().collect();
    if expected.len() != 2
        || expected[0].len() != 64
        || expected[1] != format!("{name}.tar.gz")
        || hash(&archive)? != expected[0]
    {
        return Err("Source archive checksum mismatch; preserve all copies".into());
    }
    let mut gzip = cmd("gzip", &["-t"]);
    gzip.arg(&archive);
    successful(gzip)?;
    let mut tar = cmd("tar", &["-xOf"]);
    tar.arg(&archive).arg(format!("{name}/SOURCE-REVISION.txt"));
    let identity = String::from_utf8(successful(tar)?.stdout)?;
    if identity.lines().next() != Some(format!("psx {rev}").as_str()) {
        return Err("Archived revision differs; preserve all copies".into());
    }
    Ok(archive)
}
struct Proof {
    path: PathBuf,
    revision: String,
    archive: PathBuf,
    archive_sha: String,
    entries: BTreeMap<String, Stamp>,
}
fn proof(root: &Path, path: &Path) -> Result<Proof> {
    let parent = root.join("build/source-bundles");
    if parent.canonicalize()? != parent
        || path.parent() != Some(parent.as_path())
        || fs::symlink_metadata(path)?.is_symlink()
    {
        return Err(
            "Staging path must be a direct ordinary child of this project's source-bundles".into(),
        );
    }
    let marker = path.join(".psx-source-bundle");
    ordinary(&marker)?;
    let rev = fs::read_to_string(marker)?;
    revision(&rev)?;
    let name = format!("psx-source-{}", &rev[..12]);
    if path.file_name() != Some(name.as_ref()) {
        return Err("Source marker and directory disagree".into());
    }
    let mut git = cmd("git", &["cat-file", "-e"]);
    git.arg(format!("{rev}^{{commit}}")).current_dir(root);
    successful(git)?;
    let archive = verify_archive(root, &rev)?;
    let archive_sha = hash(&archive)?;
    let entries = tree(path)?;
    let mut listing = cmd("tar", &["--quoting-style=literal", "-tzf"]);
    listing.arg(&archive);
    let names = String::from_utf8(successful(listing)?.stdout)?;
    let mut archived = BTreeSet::new();
    for name_in_archive in names.lines() {
        let member = Path::new(name_in_archive);
        if member.is_absolute()
            || member
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err("Unsafe archive path; preserve copies".into());
        }
        let relative = member
            .strip_prefix(&name)?
            .to_str()
            .ok_or("Invalid archive name")?;
        if !relative.is_empty() {
            archived.insert(relative.trim_end_matches('/').to_owned());
        }
    }
    let staged: BTreeSet<_> = entries
        .keys()
        .filter(|n| n.as_str() != ".psx-source-bundle")
        .cloned()
        .collect();
    if archived != staged {
        return Err("Extra or missing staging entries; preserve unfinished work".into());
    }
    let mut compare = cmd("tar", &["--quoting-style=literal", "--compare", "-zf"]);
    compare
        .arg(&archive)
        .arg("-C")
        .arg(&parent)
        .env("LC_ALL", "C");
    let compared = compare.output()?;
    let differences = String::from_utf8(compared.stdout)?;
    // Release tarballs deliberately normalize ownership and timestamps. File
    // data, type, mode, links and the complete entry set must still agree.
    if !matches!(compared.status.code(), Some(0 | 1))
        || !compared.stderr.is_empty()
        || differences.lines().any(|l| {
            ![": Mod time differs", ": Uid differs", ": Gid differs"]
                .iter()
                .any(|s| l.ends_with(s))
        })
    {
        return Err("Archive and staging contents/types differ; preserve all copies".into());
    }
    if tree(path)? != entries || hash(&archive)? != archive_sha {
        return Err("Source changed during inspection; preserve it".into());
    }
    Ok(Proof {
        path: path.to_owned(),
        revision: rev,
        archive_sha,
        archive,
        entries,
    })
}
fn receipt(root: &Path, value: &Value) -> Result<PathBuf> {
    let folder = owned_dir(root, ".local/storage-retention")?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let path = folder.join(format!("{stamp}-{}.json", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(&serde_json::to_vec_pretty(value)?)?;
    file.sync_all()?;
    File::open(&folder)?.sync_all()?;
    Ok(path)
}
pub fn retire_stage(root: &Path, path: &Path, dry: bool) -> Result<()> {
    let p = proof(root, path)?;
    File::open(&p.archive)?.sync_all()?;
    File::open(p.archive.with_extension("gz.sha256"))?.sync_all()?;
    File::open(p.archive.parent().ok_or("Missing archive parent")?)?.sync_all()?;
    let before = space(root)?;
    let value = json!({"phase":"verified-before-removal","dryRun":dry,"stage":p.path.strip_prefix(root)?,"revision":p.revision,"retainedArchive":p.archive.strip_prefix(root)?,"archiveSha256":p.archive_sha,"entries":p.entries.len(),"estimatedAllocatedBytes":p.entries.values().map(|v|v.blocks*512).sum::<u64>(),"space":before.value()});
    let recorded = receipt(root, &value)?;
    if !dry {
        ordinary(&p.archive)?;
        if path
            .parent()
            .ok_or("Missing staging parent")?
            .canonicalize()?
            != root.join("build/source-bundles")
            || tree(path)? != p.entries
            || hash(&p.archive)? != p.archive_sha
        {
            return Err("Source changed after verification; preserved".into());
        }
        fs::remove_dir_all(path)?;
        let after = space(root)?;
        receipt(
            root,
            &json!({"phase":"removed-verified-staging","proofReceipt":recorded.strip_prefix(root)?,"stage":path.strip_prefix(root)?,"space":after.value()}),
        )?;
    }
    println!(
        "{} {} (archive and receipt retained)",
        if dry { "Verified" } else { "Removed" },
        path.strip_prefix(root)?.display()
    );
    Ok(())
}

/// Compact an inspected device-backup expansion, retaining its complete TAR.
/// Never infer age/disposability of a backup or remove its canonical archive.
fn backup_resource_link(base: &Path, name: &str) -> bool {
    let parts: Vec<_> = name.split('/').collect();
    parts.len() == 5
        && parts[0] == "files"
        && parts[1] == "ps2"
        && parts[3] == "pcsx2"
        && parts[2].len() == 64
        && parts[2].bytes().all(|c| c.is_ascii_hexdigit())
        && matches!(parts[4], "bios" | "resources")
        && fs::read_link(base.join(name)).is_ok_and(|target| {
            target
                == Path::new("/data/user/0/com.machinerhythm.psx/files/system/ps2").join(parts[4])
        })
}
fn retire_backup(root: &Path, path: &Path, dry: bool) -> Result<()> {
    let parent = root.join(".local");
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("Invalid backup name")?;
    if root.canonicalize()? != root
        || parent.canonicalize()? != parent
        || path.parent() != Some(parent.as_path())
        || !name.starts_with("player-2-")
        || !name
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("Not a project-owned PLAYER-2 backup expansion".into());
    }
    let entries = tree(path)?;
    // These backups contain app files only. The donor's two exact resource
    // links are compared as links and never followed. Preserve other links,
    // mixed workspaces, private notes and extra diagnostics for human review.
    if entries.is_empty()
        || entries.iter().any(|(n, s)| {
            (s.kind == 2 && !backup_resource_link(path, n))
                || !(n == "files" || n.starts_with("files/"))
        })
    {
        return Err("Mixed or linked backup expansion; preserve for review".into());
    }
    let archive = parent.join(format!("{name}.tar"));
    ordinary(&archive)?;
    let archive_sha = hash(&archive)?;
    let mut listing = cmd("tar", &["--quoting-style=literal", "-tf"]);
    listing.arg(&archive);
    let mut archived = BTreeSet::new();
    for entry in String::from_utf8(successful(listing)?.stdout)?.lines() {
        let entry = entry.trim_end_matches('/');
        if Path::new(entry).is_absolute()
            || Path::new(entry)
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            || !archived.insert(entry.to_owned())
        {
            return Err("Unsafe or repeated backup member; preserve copies".into());
        }
    }
    if archived != entries.keys().cloned().collect() {
        return Err("Extra or missing backup entries; preserve both copies".into());
    }
    let mut compare = cmd("tar", &["--quoting-style=literal", "--compare", "-f"]);
    compare.arg(&archive).arg("-C").arg(path).env("LC_ALL", "C");
    let compared = compare.output()?;
    if !matches!(compared.status.code(), Some(0 | 1))
        || !compared.stderr.is_empty()
        || String::from_utf8(compared.stdout)?.lines().any(|l| {
            l != "files: Mode differs"
                && ![": Mod time differs", ": Uid differs", ": Gid differs"]
                    .iter()
                    .any(|suffix| l.ends_with(suffix))
        })
    {
        return Err("Backup bytes, modes or types differ; preserve both copies".into());
    }
    File::open(&archive)?.sync_all()?;
    File::open(&parent)?.sync_all()?;
    let proof = receipt(
        root,
        &json!({
            "phase":"verified-backup-expansion", "dryRun":dry,
            "expansion":path.strip_prefix(root)?, "retainedArchive":archive.strip_prefix(root)?,
            "archiveSha256":archive_sha, "entries":entries.len(),
        "expandedAppDirectoryMode":fs::symlink_metadata(path.join("files"))?.mode(),
        "normalization":"host ownership/timestamps and enclosing app-directory mode only; archive metadata retained",
            "estimatedAllocatedBytes":entries.values().map(|s|s.blocks*512).sum::<u64>(),
            "space":space(root)?.value()
        }),
    )?;
    ordinary(&archive)?;
    if path.parent().ok_or("No backup parent")?.canonicalize()? != parent
        || tree(path)? != entries
        || hash(&archive)? != archive_sha
    {
        return Err("Backup changed during verification; preserve both copies".into());
    }
    if !dry {
        fs::remove_dir_all(path)?;
        File::open(&parent)?.sync_all()?;
        receipt(
            root,
            &json!({"phase":"compacted-backup-expansion", "proofReceipt":proof.strip_prefix(root)?, "space":space(root)?.value()}),
        )?;
    }
    println!(
        "{} {} (complete TAR and receipt retained)",
        if dry { "Verified" } else { "Compacted" },
        path.strip_prefix(root)?.display()
    );
    Ok(())
}

pub fn run(root: &Path, args: &[String]) -> Result<()> {
    if args.len()
        > if args.first().map(String::as_str) == Some("check") {
            2
        } else {
            1
        }
    {
        return Err("Unexpected storage arguments; no action taken".into());
    }
    match args.first().map(String::as_str) {
        Some("check") => check(
            root,
            args.get(1)
                .ok_or("Use storage check <planned-GiB>")?
                .parse::<u64>()?
                .checked_mul(GIB)
                .ok_or("Storage budget overflow")?,
        ),
        Some("audit-backups" | "compact-backups") => {
            let _lock = Lock::acquire(root)?;
            let parent = root.join(".local");
            if parent.canonicalize()? != parent {
                return Err("Backup parent is redirected; preserve for review".into());
            }
            let mut paths = fs::read_dir(&parent)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            for path in paths {
                let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                    continue;
                };
                if name.starts_with("player-2-")
                    && path.is_dir()
                    && parent.join(format!("{name}.tar")).exists()
                {
                    if let Err(e) = retire_backup(root, &path, args[0] == "audit-backups") {
                        println!("Kept {}: {e}", path.strip_prefix(root)?.display());
                    }
                }
            }
            Ok(())
        }
        Some("audit" | "prune") => {
            let _lock = Lock::acquire(root)?;
            let parent = root.join("build/source-bundles");
            if !parent.exists() {
                return Ok(());
            }
            if parent.canonicalize()? != parent {
                return Err("Source staging parent is a symlink; preserve for review".into());
            }
            let mut paths = fs::read_dir(parent)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            paths.sort();
            for path in paths {
                if let Err(e) = retire_stage(root, &path, args[0] == "audit") {
                    println!("Kept {}: {e}", path.strip_prefix(root)?.display());
                }
            }
            Ok(())
        }
        None => {
            println!("{}", serde_json::to_string_pretty(&space(root)?.value())?);
            for name in ["build", "target", ".local", "vendor", ".git"] {
                let path = root.join(name);
                if path.is_dir() {
                    let entries = tree(&path)?;
                    let mut seen = BTreeSet::new();
                    let bytes: u64 = entries
                        .values()
                        .filter(|v| seen.insert((v.device, v.inode)))
                        .map(|v| v.blocks * 512)
                        .sum();
                    println!("{name}: {:.3} GiB allocated estimate, {} entries; Btrfs extents may be shared/compressed",bytes as f64/GIB as f64,entries.len());
                }
            }
            println!("storage audit verifies completed source staging; storage prune removes only verified copies. All archives, builds, private evidence and unfinished trees are retained.");
            Ok(())
        }
        _ => Err(
            "Use storage [check <planned-GiB> | audit | prune | audit-backups | compact-backups]"
                .into(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    struct Fixture {
        root: PathBuf,
        stage: PathBuf,
        archive: PathBuf,
        rev: String,
    }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "psx-retention-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            let root = root.canonicalize().unwrap();
            let mut git = cmd("git", &["init", "-q"]);
            git.arg(&root);
            successful(git).unwrap();
            let mut git = cmd(
                "git",
                &[
                    "-c",
                    "user.name=Storage test",
                    "-c",
                    "user.email=test@invalid",
                    "commit",
                    "--allow-empty",
                    "-qm",
                    "Fixture",
                ],
            );
            git.current_dir(&root);
            successful(git).unwrap();
            let mut git = cmd("git", &["rev-parse", "HEAD"]);
            git.current_dir(&root);
            let rev = String::from_utf8(successful(git).unwrap().stdout)
                .unwrap()
                .trim()
                .to_owned();
            let name = format!("psx-source-{}", &rev[..12]);
            let stage = root.join("build/source-bundles").join(&name);
            fs::create_dir_all(&stage).unwrap();
            fs::write(stage.join(".psx-source-bundle"), &rev).unwrap();
            fs::write(stage.join("SOURCE-REVISION.txt"), format!("psx {rev}\n")).unwrap();
            fs::write(stage.join("source.rs"), "retained source\n").unwrap();
            fs::create_dir(stage.join("nested")).unwrap();
            fs::write(stage.join("nested/notice"), "exact notice\n").unwrap();
            symlink("notice", stage.join("nested/link")).unwrap();
            let archive = root.join("build").join(format!("{name}.tar.gz"));
            let mut tar = cmd(
                "tar",
                &[
                    "--sort=name",
                    "--mtime=@0",
                    "--owner=12345",
                    "--group=12345",
                    "--exclude=.psx-source-bundle",
                    "-czf",
                ],
            );
            tar.arg(&archive)
                .arg("-C")
                .arg(stage.parent().unwrap())
                .arg(&name);
            successful(tar).unwrap();
            fs::write(
                archive.with_extension("gz.sha256"),
                format!("{}  {name}.tar.gz\n", hash(&archive).unwrap()),
            )
            .unwrap();
            Self {
                root,
                stage,
                archive,
                rev,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }
    #[test]
    fn exact_archive_retires_only_its_generated_copy_and_keeps_receipts() {
        let f = Fixture::new();
        let private = owned_dir(&f.root, ".local/private").unwrap();
        fs::write(private.join("save"), "unique progress").unwrap();
        let archive_hash = hash(&f.archive).unwrap();
        retire_stage(&f.root, &f.stage, true).unwrap();
        assert!(f.stage.exists());
        retire_stage(&f.root, &f.stage, false).unwrap();
        assert!(!f.stage.exists());
        assert_eq!(hash(&f.archive).unwrap(), archive_hash);
        assert_eq!(
            fs::read_to_string(private.join("save")).unwrap(),
            "unique progress"
        );
        assert_eq!(
            fs::read_dir(f.root.join(".local/storage-retention"))
                .unwrap()
                .count(),
            3
        );
    }
    #[test]
    fn changed_extra_incomplete_and_redirected_staging_are_preserved() {
        let f = Fixture::new();
        fs::write(f.stage.join("unfinished.rs"), "unfinished work").unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        fs::remove_file(f.stage.join("unfinished.rs")).unwrap();
        fs::write(f.stage.join("source.rs"), "different bytes\n").unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        fs::write(f.stage.join("source.rs"), "retained source\n").unwrap();
        let old = fs::metadata(f.stage.join("source.rs"))
            .unwrap()
            .permissions();
        fs::set_permissions(f.stage.join("source.rs"), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        fs::set_permissions(f.stage.join("source.rs"), old).unwrap();
        fs::remove_file(f.stage.join("nested/link")).unwrap();
        symlink("source.rs", f.stage.join("nested/link")).unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        fs::remove_file(f.stage.join("nested/link")).unwrap();
        symlink("notice", f.stage.join("nested/link")).unwrap();
        fs::write(f.stage.join(".psx-source-bundle"), "bad marker").unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        fs::write(f.stage.join(".psx-source-bundle"), &f.rev).unwrap();
        let sidecar = f.archive.with_extension("gz.sha256");
        let checksum = fs::read(&sidecar).unwrap();
        fs::remove_file(&sidecar).unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        fs::write(&sidecar, checksum).unwrap();
        let moved = f.root.join("preserved");
        fs::rename(&f.stage, &moved).unwrap();
        symlink(&moved, &f.stage).unwrap();
        assert!(proof(&f.root, &f.stage).is_err());
        assert!(moved.join("source.rs").exists());
        assert!(proof(&f.root, &f.root.join(".local")).is_err());
    }
    #[test]
    fn source_archive_corruption_never_authorizes_deletion() {
        let f = Fixture::new();
        OpenOptions::new()
            .append(true)
            .open(&f.archive)
            .unwrap()
            .write_all(b"damaged")
            .unwrap();
        assert!(retire_stage(&f.root, &f.stage, false).is_err());
        assert!(f.stage.exists());
    }
    #[test]
    fn project_lock_serializes_builds_and_refuses_redirected_paths() {
        let f = Fixture::new();
        let lock = Lock::acquire(&f.root).unwrap();
        assert!(Lock::acquire(&f.root).is_err());
        drop(lock);
        assert!(Lock::acquire(&f.root).is_ok());
        fs::remove_file(f.root.join("build/.psx-storage.lock")).unwrap();
        symlink(
            f.root.join("do-not-touch"),
            f.root.join("build/.psx-storage.lock"),
        )
        .unwrap();
        assert!(Lock::acquire(&f.root).is_err());
        assert!(!f.root.join("do-not-touch").exists());
    }
    #[test]
    fn unexpected_prune_options_never_delete_a_stage() {
        let f = Fixture::new();
        assert!(run(&f.root, &["prune".into(), "--dry-run".into()]).is_err());
        assert!(f.stage.join("source.rs").exists());
    }
    fn backup_fixture() -> (Fixture, PathBuf, PathBuf) {
        let f = Fixture::new();
        let path = f.root.join(".local/player-2-before-fixture");
        fs::create_dir_all(path.join("files/saves")).unwrap();
        fs::write(path.join("files/saves/card.mcd"), b"durable fixture").unwrap();
        let archive = path.with_extension("tar");
        let status = Command::new("tar")
            .arg("-cf")
            .arg(&archive)
            .arg("-C")
            .arg(&path)
            .arg("files")
            .status()
            .unwrap();
        assert!(status.success());
        (f, path, archive)
    }
    #[test]
    fn backup_compaction_keeps_the_complete_archive_and_durable_proof() {
        let (f, path, archive) = backup_fixture();
        let before = hash(&archive).unwrap();
        fs::set_permissions(path.join("files"), fs::Permissions::from_mode(0o700)).unwrap();
        retire_backup(&f.root, &path, true).unwrap();
        assert!(path.join("files/saves/card.mcd").exists());
        retire_backup(&f.root, &path, false).unwrap();
        assert!(!path.exists());
        assert_eq!(hash(&archive).unwrap(), before);
        assert!(
            fs::read_dir(f.root.join(".local/storage-retention"))
                .unwrap()
                .count()
                >= 3
        );
    }
    #[test]
    fn changed_incomplete_or_extra_backup_data_never_authorizes_removal() {
        let (f, path, archive) = backup_fixture();
        let card = path.join("files/saves/card.mcd");
        fs::write(&card, b"changed fixture").unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        fs::write(&card, b"durable fixture").unwrap();
        fs::write(path.join("files/unfinished"), b"keep").unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        fs::remove_file(path.join("files/unfinished")).unwrap();
        fs::set_permissions(&card, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        fs::set_permissions(&card, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&archive, b"interrupted archive").unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        assert!(card.exists());
    }
    #[test]
    fn backup_compaction_refuses_links_mixed_workspaces_and_extra_options() {
        let (f, path, archive) = backup_fixture();
        fs::write(path.join("notes.txt"), b"unique evidence").unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        fs::remove_file(path.join("notes.txt")).unwrap();
        symlink("saves/card.mcd", path.join("files/link")).unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        fs::remove_file(path.join("files/link")).unwrap();
        let moved = archive.with_extension("preserved");
        fs::rename(&archive, &moved).unwrap();
        symlink(&moved, &archive).unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        assert!(run(&f.root, &["compact-backups".into(), "extra".into()]).is_err());
        assert!(path.exists());
        assert!(moved.exists());
    }
    #[test]
    fn backup_resource_links_are_compared_without_following_them() {
        let (f, path, archive) = backup_fixture();
        let donor = path.join("files/ps2").join("a".repeat(64)).join("pcsx2");
        fs::create_dir_all(&donor).unwrap();
        symlink(
            "/data/user/0/com.machinerhythm.psx/files/system/ps2/bios",
            donor.join("bios"),
        )
        .unwrap();
        assert!(Command::new("tar")
            .arg("-cf")
            .arg(&archive)
            .arg("-C")
            .arg(&path)
            .arg("files")
            .status()
            .unwrap()
            .success());
        retire_backup(&f.root, &path, true).unwrap();
        fs::remove_file(donor.join("bios")).unwrap();
        symlink("/outside/unknown", donor.join("bios")).unwrap();
        assert!(retire_backup(&f.root, &path, false).is_err());
        assert!(path.exists());
    }
    #[test]
    fn budgets_and_btrfs_metadata_are_checked_independently_of_df() {
        let mut s = Space {
            available: RESERVE + 3 * GIB,
            metadata: None,
            unallocated: None,
            global_reserve: None,
        };
        assert!(s.require(3 * GIB).is_ok());
        assert!(s.require(4 * GIB).is_err());
        assert!(s.require(u64::MAX).is_err());
        s.metadata = Some((5 * GIB, 5 * GIB - 128 * 1024 * 1024));
        s.global_reserve = Some((GIB / 2, 0));
        s.unallocated = Some(2 * GIB);
        assert!(s.require(0).is_err());
        s.metadata = Some((5 * GIB, 3 * GIB));
        s.unallocated = Some(1024 * 1024);
        assert!(s.require(0).is_ok()); // Already allocated usable metadata remains.
        s.metadata = Some((5 * GIB, 4 * GIB));
        assert!(s.require(0).is_err()); // Emergency reserve is never counted free.
        s.unallocated = Some(2 * GIB);
        assert!(s.require(0).is_ok());
        s.global_reserve = Some((GIB / 2, 1));
        assert!(s.require(0).is_err());
        s.global_reserve = None;
        assert!(s.require(0).is_err());
        let parsed=parse_btrfs("Device unallocated: 2148532224\nMetadata,DUP: Size:5368709120, Used:3611951104 (67.28%)\nGlobal reserve: 536870912 (used: 0)\n",s.available).unwrap();
        assert_eq!(parsed.metadata, Some((5368709120, 3611951104)));
        assert!(parsed.require(0).is_ok());
        assert_eq!(parsed.global_reserve, Some((536870912, 0)));
        assert!(parse_btrfs(
            "Device unallocated: 2148532224\nMetadata,DUP: Size:5368709120, Used:3611951104\n",
            s.available
        )
        .is_err());
        assert!(parse_btrfs("Device unallocated: 0", s.available).is_err());
    }
}
