//! Add the pinned PSP source graph to a verified complete player archive.
//! Immutable Git blobs are read in bounded batches; no dependency tree is copied.
use super::*;
use std::io::{BufRead, BufReader};
use std::process::Stdio;

const FROM: &str = "b71679c5bc0b74018920bf919417dbfd31511946";
const FROM_SHA: &str = "bb7460ee4bafe8e8784225019b5cdf68259b47e7ffa144b3cfbf30075bc1b64f";
const PIN: &str = "fc9ca4d1f78a5d60ae03f3311c6b8ab49273f6a8";
const PREFIX: &str = "vendor/ppsspp";
const SELF_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bin/source-update/psp.rs"
));
type Additions = BTreeMap<String, (u32, Vec<u8>)>;
type Tree = BTreeMap<String, (u32, String)>;

fn tree(prefix: &str, revision: &str) -> Result<Tree> {
    let bytes = output(
        Command::new("git")
            .arg("-C")
            .arg(source_root().join(prefix))
            .args(["ls-tree", "-rz", revision])
            .env("GIT_OPTIONAL_LOCKS", "0"),
    )?;
    let mut entries = Tree::new();
    for line in bytes.split(|b| *b == 0).filter(|l| !l.is_empty()) {
        let (spec, path) = std::str::from_utf8(line)?
            .split_once('\t')
            .ok_or("Invalid PSP Git tree")?;
        safe(path)?;
        let fields: Vec<_> = spec.split_whitespace().collect();
        if fields.len() != 3
            || entries
                .insert(
                    path.into(),
                    (u32::from_str_radix(fields[0], 8)?, fields[2].into()),
                )
                .is_some()
        {
            return Err("Invalid/duplicate PSP Git entry".into());
        }
    }
    Ok(entries)
}

pub(super) fn graph(target: &str) -> Result<BTreeMap<String, String>> {
    let root = String::from_utf8(git(&["ls-tree", target, "--", PREFIX])?)?;
    if root != format!("160000 commit {PIN}\t{PREFIX}\n") {
        return Err("PSP source delivery requires the exact pinned root".into());
    }
    let mut graph = BTreeMap::from([(PREFIX.into(), PIN.into())]);
    let mut pending = vec![(PREFIX.to_owned(), PIN.to_owned())];
    while let Some((prefix, rev)) = pending.pop() {
        for (path, (mode, child)) in tree(&prefix, &rev)? {
            if mode == 0o160000 {
                let full = format!("{prefix}/{path}");
                if graph.len() >= 64 || graph.insert(full.clone(), child.clone()).is_some() {
                    return Err("PSP dependency graph exceeds its bound or repeats".into());
                }
                pending.push((full, child));
            }
        }
    }
    Ok(graph)
}

fn parents(path: &str, inventory: &Inventory, additions: &mut Additions) {
    let mut parent = Path::new(path).parent();
    while let Some(p) = parent.and_then(Path::to_str).filter(|p| !p.is_empty()) {
        if inventory.contains_key(p) {
            break;
        }
        additions.entry(p.into()).or_insert((0o040755, Vec::new()));
        parent = Path::new(p).parent();
    }
}

fn blobs(
    prefix: &str,
    tree: &Tree,
    mut consume: impl FnMut(&str, u32, Vec<u8>) -> Result<()>,
) -> Result<()> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(source_root().join(prefix))
        .args(["cat-file", "--batch"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().ok_or("Missing Git batch input")?;
    let mut reader = BufReader::new(child.stdout.take().ok_or("Missing Git batch output")?);
    let result: Result<()> = (|| {
        for (path, (mode, oid)) in tree.iter().filter(|(_, (m, _))| *m != 0o160000) {
            if ![0o100644, 0o100755, 0o120000].contains(mode) {
                return Err("Unsupported PSP Git mode".into());
            }
            writeln!(input, "{oid}")?;
            input.flush()?;
            let mut header = String::new();
            reader.read_line(&mut header)?;
            let fields: Vec<_> = header.split_whitespace().collect();
            if fields.len() != 3 || fields[0] != oid || fields[1] != "blob" {
                return Err("PSP Git blob response differs".into());
            }
            let size: usize = fields[2].parse()?;
            if size > 128 * 1024 * 1024 {
                return Err("PSP source member exceeds bound".into());
            }
            let mut body = vec![0; size];
            reader.read_exact(&mut body)?;
            let mut newline = [0];
            reader.read_exact(&mut newline)?;
            if newline != [b'\n'] {
                return Err("Invalid Git batch delimiter".into());
            }
            consume(path, *mode, body)?;
        }
        Ok(())
    })();
    drop(input);
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    result?;
    if !status.success() {
        return Err("PSP Git batch failed".into());
    }
    Ok(())
}

fn exports(target: &str) -> Result<Additions> {
    let graph = graph(target)?;
    let deps = graph
        .iter()
        .filter(|(p, _)| p.as_str() != PREFIX)
        .map(|(p, r)| format!("{r}\t{}\n", p.strip_prefix(&format!("{PREFIX}/")).unwrap()))
        .collect::<String>();
    let mut additions = Additions::new();
    let mut total = 0usize;
    for (prefix, rev) in graph {
        parents(
            &format!("{prefix}/.psx-source-manifest"),
            &Inventory::new(),
            &mut additions,
        );
        let mut manifest = rev.clone() + "\n";
        blobs(&prefix, &tree(&prefix, &rev)?, |path, mode, body| {
            total = total
                .checked_add(body.len())
                .filter(|n| *n <= 768 * 1024 * 1024)
                .ok_or("PSP source graph exceeds memory bound")?;
            let full = format!("{prefix}/{path}");
            if mode != 0o120000 {
                manifest.push_str(&format!("{}\t{path}\n", digest(&body)));
            }
            parents(&full, &Inventory::new(), &mut additions);
            if additions
                .insert(full, (if mode == 0o120000 { 0o120777 } else { mode }, body))
                .is_some()
            {
                return Err("PSP source export collision".into());
            }
            Ok(())
        })?;
        if prefix == PREFIX {
            manifest.push_str(&format!("{}\t.psx-dependencies\n", digest(deps.as_bytes())));
            additions.insert(
                format!("{PREFIX}/.psx-dependencies"),
                (0o100644, deps.as_bytes().to_vec()),
            );
        }
        additions.insert(
            format!("{prefix}/.psx-source-manifest"),
            (0o100644, manifest.into_bytes()),
        );
    }
    Ok(additions)
}

pub(super) fn validate_export(inv: &Inventory, payload: &Payloads, target: &str) -> Result<()> {
    let exported = exports(target)?;
    let expected: BTreeSet<_> = exported.keys().filter(|p| p.starts_with(PREFIX)).collect();
    let actual: BTreeSet<_> = inv.keys().filter(|p| p.starts_with(PREFIX)).collect();
    if expected != actual {
        return Err("PSP source export contains missing or extra members".into());
    }
    let metadata: Value = serde_json::from_slice(&payload["SOURCE-CHECK.json"])?;
    if metadata["psp"]["revision"] != PIN
        || metadata["psp"]["dependencyPins"] != json!(graph(target)?)
    {
        return Err("PSP release metadata differs from its exact source graph".into());
    }
    for (path, (mode, body)) in exported {
        let Some(actual) = inv.get(&path) else {
            return Err(format!("Missing PSP source {path}").into());
        };
        if actual["mode"] != mode {
            return Err(format!("PSP source mode differs {path}").into());
        }
        if mode == 0o120777 {
            if actual["symlink"] != std::str::from_utf8(&body)? {
                return Err("PSP source link differs".into());
            }
        } else if mode != 0o040755 && actual["sha256"] != digest(&body) {
            return Err(format!("PSP source blob differs {path}").into());
        }
        if capture(&path) && payload.get(&path) != Some(&body) {
            return Err("PSP manifest readback differs".into());
        }
    }
    Ok(())
}

fn dependency_change(from: &[u8], to: &[u8]) -> Result<()> {
    let from = std::str::from_utf8(from)?;
    let to = std::str::from_utf8(to)?;
    // The log crate was already completely vendored. Only the player edge is new.
    let start = to
        .find("name = \"psx-player\"\n")
        .ok_or("Missing player package")?;
    let end = to[start..]
        .find("\n[[package]]")
        .map_or(to.len(), |n| start + n);
    let section = &to[start..end];
    if section.matches(" \"log\",\n").count() != 1 {
        return Err("Unexpected PSP lockfile edge".into());
    }
    let reduced = to[..start].to_owned() + &section.replacen(" \"log\",\n", "", 1) + &to[end..];
    if reduced != from {
        return Err("PSP source route permits only the already-vendored log edge".into());
    }
    Ok(())
}

pub(super) fn run(target: &str) -> Result<()> {
    target_revision(target)?;
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("Require a clean committed PSP target".into());
    }
    run_committed(target)
}
fn target_revision(target: &str) -> Result<()> {
    if target.len() != 40
        || !target.bytes().all(|c| c.is_ascii_hexdigit())
        || git(&["rev-parse", "--verify", &format!("{target}^{{commit}}")])?
            != format!("{target}\n").as_bytes()
    {
        return Err("Require an exact existing PSP commit".into());
    }
    Ok(())
}
fn run_committed(target: &str) -> Result<()> {
    for (path, source) in [
        ("xtask/src/bin/source-update.rs", TOOL_SOURCE),
        ("xtask/src/bin/source-update/psp.rs", SELF_SOURCE),
    ] {
        if git(&["show", &format!("HEAD:{path}")])? != source {
            return Err("Executed PSP utility differs from committed source".into());
        }
    }
    let _growth = storage::Growth::begin(&output_root()?, 2 * storage::GIB)?;
    let base = Path::new("build").join(format!("{}.tar.gz", name(FROM)));
    if fs::symlink_metadata(&base)?.is_symlink()
        || fs::metadata(&base)?.len() != 205248492
        || hash(&base)? != FROM_SHA
    {
        return Err("Retained complete PSP predecessor differs".into());
    }
    let (inventory, payload) = scan(&base, &name(FROM), &Payloads::new(), None)?;
    validate(&inventory, &payload, FROM)?;
    dependency_change(
        &git(&["show", &format!("{FROM}:Cargo.lock")])?,
        &git(&["show", &format!("{target}:Cargo.lock")])?,
    )?;
    let old: Value =
        serde_json::from_slice(&git(&["show", &format!("{FROM}:upstream.lock.json")])?)?;
    let mut new: Value =
        serde_json::from_slice(&git(&["show", &format!("{target}:upstream.lock.json")])?)?;
    if new["psp"]["revision"] != PIN {
        return Err("PSP upstream pin differs".into());
    }
    new.as_object_mut()
        .ok_or("Upstream metadata")?
        .remove("psp");
    if new != old {
        return Err("Existing native provenance changed".into());
    }
    let mut additions = exports(target)?;
    additions.retain(|path, _| !inventory.contains_key(path));
    let mut replacements = Payloads::new();
    let changes = String::from_utf8(git(&[
        "diff",
        "--name-status",
        "--no-renames",
        FROM,
        target,
    ])?)?;
    if changes.lines().count() > 128 {
        return Err("PSP project delta exceeds bound".into());
    }
    let mut bytes = 0usize;
    for line in changes.lines() {
        let (status, path) = line.split_once('\t').ok_or("Invalid PSP project delta")?;
        safe(path)?;
        if path == PREFIX && status == "A" {
            continue;
        }
        if !matches!(status, "A" | "M")
            || path.starts_with("vendor/")
            || path.starts_with("patches/armsx2/")
            || path.starts_with(".cargo/")
            || path == "Cargo.toml"
        {
            return Err(format!("PSP delta changes retained dependency input: {path}").into());
        }
        let entry = String::from_utf8(git(&["ls-tree", target, "--", path])?)?;
        let mode = u32::from_str_radix(
            entry
                .split_whitespace()
                .next()
                .ok_or("Missing project Git mode")?,
            8,
        )?;
        if ![0o100644, 0o100755].contains(&mode) {
            return Err("PSP project delta requires regular files".into());
        }
        let body = git(&["show", &format!("{target}:{path}")])?;
        bytes += body.len();
        if body.len() > 8 * 1024 * 1024 || bytes > 16 * 1024 * 1024 {
            return Err("PSP project delta exceeds memory bound".into());
        }
        if status == "M" {
            if inventory
                .get(path)
                .is_none_or(|v| v["mode"] != mode || !v["hardlink"].is_null())
            {
                return Err("PSP replacement mode/type differs".into());
            }
            replacements.insert(path.into(), body);
        } else {
            parents(path, &inventory, &mut additions);
            if additions.insert(path.into(), (mode, body)).is_some() {
                return Err("PSP project addition collision".into());
            }
        }
    }
    let mut check: Value = serde_json::from_slice(&payload["SOURCE-CHECK.json"])?;
    check["projectRevision"] = json!(target);
    check["psp"] = json!({"revision":PIN,"dependencyPins":graph(target)?,"completeGitSource":true});
    replacements.insert(
        "SOURCE-CHECK.json".into(),
        serde_json::to_vec_pretty(&check)?,
    );
    replacements.insert(
        "SOURCE-REVISION.txt".into(),
        String::from_utf8(payload["SOURCE-REVISION.txt"].clone())?
            .replacen(FROM, target, 1)
            .into_bytes(),
    );
    let final_path = Path::new("build").join(format!("{}.tar.gz", name(target)));
    let partial = final_path.with_extension("gz.incomplete");
    if final_path.exists() || partial.exists() {
        return Err("Preserve existing complete/partial PSP archive".into());
    }
    let file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&partial)?;
    durable(
        &partial.with_extension("receipt.json"),
        &json!({"phase":"before-stream","targetRevision":target,"predecessorSha256":FROM_SHA,"gitDelta":changes,"additionCount":additions.len()}),
    )?;
    let (written, _) = scan_added(
        &base,
        &name(FROM),
        &replacements,
        &additions,
        Some((Path::new(&name(target)), &file)),
    )?;
    let count = additions.len();
    drop(additions);
    file.sync_all()?;
    storage::between_steps()?;
    gzip_check(&partial)?;
    let (actual, metadata) = scan(&partial, &name(target), &Payloads::new(), None)?;
    if actual != written || actual.len() != inventory.len() + count {
        return Err("Independent PSP streaming readback differs".into());
    }
    validate(&actual, &metadata, target)?;
    for (path, old) in &inventory {
        if replacements.contains_key(path) {
            continue;
        }
        let mut expected = old.clone();
        if let Some(link) = old["hardlink"].as_str() {
            expected["hardlink"] = json!(link.replacen(&name(FROM), &name(target), 1));
        }
        if actual.get(path) != Some(&expected) {
            return Err(format!("Retained source member changed: {path}").into());
        }
    }
    if hash(&base)? != FROM_SHA || !git(&["status", "--porcelain"])?.is_empty() {
        return Err("Source changed during PSP delivery".into());
    }
    output_root()?;
    storage::between_steps()?;
    let sha = hash(&partial)?;
    fs::hard_link(&partial, &final_path)?;
    File::open("build")?.sync_all()?;
    let mut side = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(final_path.with_extension("gz.sha256"))?;
    writeln!(
        side,
        "{sha}  {}",
        final_path.file_name().unwrap().to_str().unwrap()
    )?;
    side.sync_all()?;
    durable(
        &final_path.with_extension("receipt.json"),
        &json!({"phase":"verified-complete","targetRevision":target,"utilityRevision":String::from_utf8(git(&["rev-parse","HEAD"])?)?.trim(),"utilitySha256":hash(&std::env::current_exe()?)?,"archiveSha256":sha,"archiveBytes":file.metadata()?.len(),"members":actual.len(),"streamedReadback":true,"psp":check["psp"],"predecessorSha256":FROM_SHA}),
    )?;
    fs::remove_file(&partial)?;
    File::open("build")?.sync_all()?;
    println!("Complete PSP source {}: {sha}", final_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_hex_string_must_resolve_to_an_actual_commit() {
        target_revision(FROM).unwrap();
        assert!(target_revision("0000000000000000000000000000000000000000").is_err());
        assert!(target_revision("HEAD").is_err());
    }
    #[test]
    fn source_route_admits_only_the_existing_log_edge() {
        let old="name = \"psx-player\"\ndependencies = [\n \"libc\",\n]\n\n[[package]]\nname = \"other\"\n";
        let new = old.replacen(" \"libc\",\n", " \"libc\",\n \"log\",\n", 1);
        dependency_change(old.as_bytes(), new.as_bytes()).unwrap();
        assert!(dependency_change(old.as_bytes(), old.as_bytes()).is_err());
        assert!(
            dependency_change(old.as_bytes(), new.replace("libc", "newcrate").as_bytes()).is_err()
        );
    }
}
