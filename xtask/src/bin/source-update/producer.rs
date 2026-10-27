//! Fixed-predecessor source delivery; the Vulkan route admits only pinned ash.
use super::*;
const FROM: &str = "123e831e4afb6c7cec2d8e05a030e3cd82a72bba";
const FROM_SHA: &str = "b14f6e2a0ad5910058bc015b109896c4b631fe33bce71ec8381eddafb114b56f";
const SELF_SOURCE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/bin/source-update/producer.rs"
));

pub fn run(target: &str) -> Result<()> {
    selected(target, false)
}
pub fn vulkan(target: &str) -> Result<()> {
    selected(target, true)
}
fn has_patch(target: &str, path: &str) -> Result<bool> {
    // Output lives outside Git. Absence is distinct from a failed Git lookup.
    let found = git(&["ls-tree", "--name-only", target, "--", path])?;
    if found.is_empty() {
        Ok(false)
    } else if found == format!("{path}\n").as_bytes() {
        Ok(true)
    } else {
        Err("Ambiguous native patch lookup".into())
    }
}
fn selected(target: &str, vulkan: bool) -> Result<()> {
    let (from, from_sha, from_bytes) = if vulkan {
        (
            "d11d03e6d02c6c4293b972af9aadb8f639bd1184",
            "2790fbef78e3bdb82e3a34ef0ae2887ae9f3e2c0642e4fd36b131ff4439b4cb4",
            204743679,
        )
    } else {
        (FROM, FROM_SHA, 204585917)
    };

    if target.len() != 40
        || !target.bytes().all(|c| c.is_ascii_hexdigit())
        || git(&["rev-parse", target])? != format!("{target}\n").as_bytes()
    {
        return Err("Require exact immutable target commit".into());
    }
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("Commit source/tool before archive publication".into());
    }
    for (path, compiled) in [
        ("xtask/src/bin/source-update.rs", TOOL_SOURCE),
        ("xtask/src/bin/source-update/producer.rs", SELF_SOURCE),
    ] {
        if git(&["show", &format!("HEAD:{path}")])? != compiled {
            return Err("Executed source utility differs from its committed source".into());
        }
    }
    let root = std::env::current_dir()?;
    if vulkan
        && git(&["show", "HEAD:xtask/src/bin/source-update/vulkan_ash.rs"])?
            != super::vulkan_ash::SELF_SOURCE
    {
        return Err("Executed ash source utility differs from committed source".into());
    }
    let _growth = storage::Growth::begin(&root, storage::GIB / 2)?;
    let base = Path::new("build").join(format!("{}.tar.gz", name(from)));
    let meta = fs::symlink_metadata(&base)?;
    if !meta.is_file() || meta.is_symlink() || meta.len() != from_bytes || hash(&base)? != from_sha
    {
        return Err("Retained complete predecessor differs".into());
    }
    let (inventory, payload) = scan(&base, &name(from), &Payloads::new(), None)?;
    validate(&inventory, &payload, from)?;
    let changes = String::from_utf8(git(&[
        "diff",
        "--name-status",
        "--no-renames",
        from,
        target,
    ])?)?;
    if vulkan {
        super::vulkan_ash::lock_change(
            &git(&["show", &format!("{from}:Cargo.lock")])?,
            &git(&["show", &format!("{target}:Cargo.lock")])?,
        )?;
    }
    let mut replacements = Payloads::new();
    let mut additions = BTreeMap::new();
    if changes.lines().count() > 64 {
        return Err("Project delta exceeds this bounded increment".into());
    }
    let mut changed_bytes = 0usize;
    for line in changes.lines() {
        let (status, path) = line.split_once('\t').ok_or("Invalid Git delta")?;
        safe(path)?;
        // Legacy updates keep dependencies unchanged. The Vulkan route checks
        // its sole lockfile exception independently before accepting this delta.
        if !matches!(status, "A" | "M")
            || path.starts_with("vendor/")
            || (path == "Cargo.lock" && !vulkan)
            || path == "Cargo.toml"
            || path == "upstream.lock.json"
            || path.starts_with(".cargo/")
        {
            return Err(format!("Outside retained-input update scope: {line}").into());
        }
        let entry = String::from_utf8(git(&["ls-tree", target, "--", path])?)?;
        let mode = entry.split_whitespace().next().ok_or("Missing Git mode")?;
        let mode = u32::from_str_radix(mode, 8)?;
        if ![0o100644, 0o100755].contains(&mode) {
            return Err("Only regular project file changes admitted".into());
        }
        let body = git(&["show", &format!("{target}:{path}")])?;
        changed_bytes = changed_bytes
            .checked_add(body.len())
            .ok_or("Project delta overflow")?;
        if body.len() > 8 * 1024 * 1024 || changed_bytes > 16 * 1024 * 1024 {
            return Err("Project replacement exceeds bound".into());
        }
        if status == "M" {
            let old = inventory.get(path).ok_or("Modified member missing")?;
            if old["mode"] != mode || !old["hardlink"].is_null() {
                return Err("Modified member mode/type differs".into());
            }
            replacements.insert(path.to_owned(), body);
        } else {
            if inventory.contains_key(path) {
                return Err("Added member already present".into());
            }
            let mut parent = Path::new(path).parent();
            while let Some(p) = parent {
                let p = p.to_str().ok_or("Path encoding")?;
                if p.is_empty() || inventory.contains_key(p) {
                    break;
                }
                additions
                    .entry(p.to_owned())
                    .or_insert((0o040755, Vec::new()));
                parent = Path::new(p).parent();
            }
            additions.insert(path.to_owned(), (mode, body));
        }
    }
    if vulkan {
        let crate_file = std::env::var_os("PSX_SOURCE_ASH_CRATE")
            .map(PathBuf::from)
            .ok_or("Pinned ash crate input required")?;
        for (path, value) in super::vulkan_ash::additions(&crate_file)? {
            if inventory.contains_key(&path) || additions.insert(path, value).is_some() {
                return Err("Pinned Rust dependency source collision".into());
            }
        }
    }
    let mut check: Value = serde_json::from_slice(&payload["SOURCE-CHECK.json"])?;
    check["projectRevision"] = json!(target);
    let mut patches = serde_json::Map::new();
    for p in [
        "0001-host-pause-and-card-flush.patch",
        "0002-development-graphics-observer.patch",
        "0003-development-matched-reader.patch",
        "0004-stream-buffer-failure.patch",
        "0005-development-stream-observer.patch",
        "0006-development-creator-output.patch",
        "0007-development-common-results.patch",
    ] {
        patches.insert(
            p.into(),
            json!(digest(&git(&[
                "show",
                &format!("{target}:patches/armsx2/{p}")
            ])?)),
        );
    }
    if vulkan {
        let path = "patches/armsx2/0010-vulkan-frame-completion.patch";
        patches.insert(
            "0010-vulkan-frame-completion.patch".into(),
            json!(digest(&git(&["show", &format!("{target}:{path}")])?)),
        );
        check["productNativePatches"] = json!([
            "0001-host-pause-and-card-flush.patch",
            "0004-stream-buffer-failure.patch",
            "0010-vulkan-frame-completion.patch"
        ]);
        let overlap = "patches/armsx2/0011-vulkan-producer-overlap.patch";
        if has_patch(target, overlap)? {
            patches.insert(
                "0011-vulkan-producer-overlap.patch".into(),
                json!(digest(&git(&["show", &format!("{target}:{overlap}")])?)),
            );
            check["productNativePatches"]
                .as_array_mut()
                .ok_or("Product patches")?
                .push(json!("0011-vulkan-producer-overlap.patch"));
        }
        check["additionalRegistrySource"] = json!({"name":"ash","version":super::vulkan_ash::VERSION,"checksum":super::vulkan_ash::SHA});
    }
    check["nativePatches"] = Value::Object(patches);
    replacements.insert(
        "SOURCE-CHECK.json".into(),
        serde_json::to_vec_pretty(&check)?,
    );
    replacements.insert(
        "SOURCE-REVISION.txt".into(),
        String::from_utf8(payload["SOURCE-REVISION.txt"].clone())?
            .replacen(from, target, 1)
            .into_bytes(),
    );
    let final_path = Path::new("build").join(format!("{}.tar.gz", name(target)));
    let partial = final_path.with_extension("gz.incomplete");
    if final_path.exists() || partial.exists() {
        return Err("Existing complete/partial archive retained".into());
    }
    let f = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&partial)?;
    durable(
        &partial.with_extension("receipt.json"),
        &json!({"phase":"before-stream","predecessor":from,"predecessorSha256":from_sha,"targetRevision":target,"gitDelta":changes,"replacements":replacements.keys().collect::<Vec<_>>(),"additions":additions.keys().collect::<Vec<_>>()}),
    )?;
    let (written, _) = scan_added(
        &base,
        &name(from),
        &replacements,
        &additions,
        Some((Path::new(&name(target)), &f)),
    )?;
    f.sync_all()?;
    output_root()?;
    storage::between_steps()?;
    gzip_check(&partial)?;
    let (actual, metadata) = scan(&partial, &name(target), &Payloads::new(), None)?;
    if actual != written {
        return Err("Independent full streaming readback differs".into());
    }
    validate(&actual, &metadata, target)?;
    // Unmodified members are independently compared with the verified input;
    // target tracked files are independently compared with Git by validate().
    for (path, old) in &inventory {
        if replacements.contains_key(path) {
            continue;
        }
        let mut expected = old.clone();
        if let Some(link) = old["hardlink"].as_str() {
            expected["hardlink"] = json!(link.replacen(&name(from), &name(target), 1));
        }
        if actual.get(path) != Some(&expected) {
            return Err(format!("Unmodified source member changed: {path}").into());
        }
    }
    if actual.len() != inventory.len() + additions.len() || hash(&base)? != from_sha {
        return Err("Source set/predecessor changed".into());
    }
    let sha = hash(&partial)?;
    let utility = String::from_utf8(git(&["rev-parse", "HEAD"])?)?;
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("Source changed during streaming".into());
    }
    output_root()?;
    storage::between_steps()?;
    fs::hard_link(&partial, &final_path)?;
    File::open("build")?.sync_all()?;
    let side = final_path.with_extension("gz.sha256");
    let mut s = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(side)?;
    writeln!(s, "{sha}  {}.tar.gz", name(target))?;
    s.sync_all()?;
    durable(
        &final_path.with_extension("receipt.json"),
        &json!({"phase":"verified-complete","targetRevision":target,"utilityRevision":utility.trim(),"utilitySourceSha256":digest(TOOL_SOURCE),"utilityProducerSourceSha256":digest(SELF_SOURCE),"utilityBinarySha256":hash(&std::env::current_exe()?)?,"predecessorRevision":from,"predecessorSha256":from_sha,"archiveSha256":sha,"archiveBytes":f.metadata()?.len(),"members":actual.len(),"streamedReadback":true,"nativePatches":check["nativePatches"]}),
    )?;
    fs::remove_file(partial)?;
    File::open("build")?.sync_all()?;
    println!(
        "Complete source {}: {sha}; target{target}, utility{}",
        final_path.display(),
        utility.trim()
    );
    Ok(())
}

/// Repair only the demonstrated metadata omission in the frozen player bundle.
/// Its tracked source and application revision remain exact; the repair utility
/// revision is separate. Never replace the original archive or expand its tree.
pub fn repair_overlap_metadata() -> Result<()> {
    const TARGET: &str = "47a703c5185c7f4b3c8b0df31778b697ccf1afc3";
    const INPUT_SHA: &str = "cb22d08122895d3a117b6b892a050b3eb7dc9804ee7444a33d85013501fb04a5";
    const PATCH: &str = "0011-vulkan-producer-overlap.patch";
    let _growth = storage::Growth::begin(&std::env::current_dir()?, storage::GIB / 2)?;
    if !git(&["status", "--porcelain"])?.is_empty()
        || git(&["show", "HEAD:xtask/src/bin/source-update.rs"])? != TOOL_SOURCE
        || git(&["show", "HEAD:xtask/src/bin/source-update/producer.rs"])? != SELF_SOURCE
    {
        return Err("Commit the exact executed metadata utility before publication".into());
    }
    let root = name(TARGET);
    let input = Path::new("build").join(format!("{root}.tar.gz"));
    let meta = fs::symlink_metadata(&input)?;
    if !meta.is_file() || meta.is_symlink() || meta.len() != 205228692 || hash(&input)? != INPUT_SHA
    {
        return Err("Frozen source input changed".into());
    }
    let (inventory, payload) = scan(&input, &root, &Payloads::new(), None)?;
    validate(&inventory, &payload, TARGET)?;
    let mut check: Value = serde_json::from_slice(&payload["SOURCE-CHECK.json"])?;
    let previous = json!([
        "0001-host-pause-and-card-flush.patch",
        "0004-stream-buffer-failure.patch",
        "0010-vulkan-frame-completion.patch"
    ]);
    if !check["nativePatches"][PATCH].is_null() || check["productNativePatches"] != previous {
        return Err("Not the exact demonstrated patch-metadata omission".into());
    }
    let body = git(&["show", &format!("{TARGET}:patches/armsx2/{PATCH}")])?;
    let patch_sha = digest(&body);
    if patch_sha != "414bf589247a7ae3ca0d3260ed43442a0d61117fc6c4779d20a5fa74a8cc91a9"
        || inventory[&format!("patches/armsx2/{PATCH}")]["sha256"] != patch_sha
    {
        return Err("Archived overlap source differs from actual native input".into());
    }
    check["nativePatches"][PATCH] = json!(patch_sha);
    check["productNativePatches"]
        .as_array_mut()
        .ok_or("Product patches")?
        .push(json!(PATCH));
    let replacements = Payloads::from([(
        "SOURCE-CHECK.json".into(),
        serde_json::to_vec_pretty(&check)?,
    )]);
    let final_path = Path::new("build").join(format!("{root}-metadata-v2.tar.gz"));
    let partial = final_path.with_extension("gz.incomplete");
    if final_path.exists() || partial.exists() {
        return Err("Preserve existing metadata repair output".into());
    }
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&partial)?;
    durable(
        &partial.with_extension("receipt.json"),
        &json!({"phase":"before-stream","targetRevision":TARGET,"predecessorSha256":INPUT_SHA,"onlyChangedMember":"SOURCE-CHECK.json"}),
    )?;
    let (written, _) = scan(
        &input,
        &root,
        &replacements,
        Some((Path::new(&root), &file)),
    )?;
    file.sync_all()?;
    output_root()?;
    storage::between_steps()?;
    gzip_check(&partial)?;
    let (actual, metadata) = scan(&partial, &root, &Payloads::new(), None)?;
    if actual != written || actual.len() != inventory.len() {
        return Err("Metadata repair readback differs".into());
    }
    validate(&actual, &metadata, TARGET)?;
    for (path, old) in &inventory {
        if path != "SOURCE-CHECK.json" && actual.get(path) != Some(old) {
            return Err(format!("Unchanged source member differs: {path}").into());
        }
    }
    let readback: Value = serde_json::from_slice(&metadata["SOURCE-CHECK.json"])?;
    if readback != check || hash(&input)? != INPUT_SHA {
        return Err("Patch metadata or frozen source changed".into());
    }
    let sha = hash(&partial)?;
    let utility = String::from_utf8(git(&["rev-parse", "HEAD"])?)?;
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("Utility changed during metadata repair".into());
    }
    output_root()?;
    storage::between_steps()?;
    fs::hard_link(&partial, &final_path)?;
    File::open("build")?.sync_all()?;
    let mut side = OpenOptions::new()
        .write(true)
        .create_new(true)
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
        &json!({"phase":"verified-complete","targetRevision":TARGET,"utilityRevision":utility.trim(),"utilityBinarySha256":hash(&std::env::current_exe()?)?,"utilityProducerSourceSha256":digest(SELF_SOURCE),"predecessorSha256":INPUT_SHA,"archiveSha256":sha,"archiveBytes":file.metadata()?.len(),"members":actual.len(),"streamedReadback":true,"onlyChangedMember":"SOURCE-CHECK.json","nativePatches":readback["nativePatches"],"productNativePatches":readback["productNativePatches"]}),
    )?;
    fs::remove_file(partial)?;
    File::open("build")?.sync_all()?;
    println!(
        "Complete metadata-corrected source {}: {sha}; target{TARGET}, utility{}",
        final_path.display(),
        utility.trim()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_patch_lookup_is_anchored_outside_repository() {
        const PATH: &str = "patches/armsx2/0011-vulkan-producer-overlap.patch";
        if std::env::var_os("PSX_SOURCE_PATCH_LOOKUP_CHILD").is_some() {
            assert!(has_patch("47a703c5185c7f4b3c8b0df31778b697ccf1afc3", PATH).unwrap());
            assert!(!has_patch("cf6f23e2d30867e01a158b6e09294113ae7e73d1", PATH).unwrap());
            assert!(has_patch("invalid-revision", PATH).is_err());
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "psx-patch-lookup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        assert!(!Command::new("git")
            .args([
                "cat-file",
                "-e",
                &format!("47a703c5185c7f4b3c8b0df31778b697ccf1afc3:{PATH}")
            ])
            .current_dir(&dir)
            .output()
            .unwrap()
            .status
            .success());
        assert!(Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "producer::tests::native_patch_lookup_is_anchored_outside_repository"
            ])
            .env("PSX_SOURCE_PATCH_LOOKUP_CHILD", "1")
            .current_dir(&dir)
            .status()
            .unwrap()
            .success());
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn streaming_project_additions_preserve_bodies_and_reject_collisions() {
        let dir = std::env::temp_dir().join(format!("psx-source-additions-{}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let base = dir.join("base.tar.gz");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&base)
            .unwrap();
        let mut writer = Writer::open(&file).unwrap();
        for (name, mode, body) in [
            ("old", 0o040755, b"".as_slice()),
            ("old/a", 0o100644, b"preserved".as_slice()),
            ("old/b", 0o100644, b"old body".as_slice()),
        ] {
            let e = Entry(unsafe { archive_entry_new() });
            let p = CString::new(name).unwrap();
            unsafe {
                archive_entry_set_pathname(e.0, p.as_ptr());
                archive_entry_set_mode(e.0, mode);
                archive_entry_set_size(e.0, body.len() as i64);
            }
            writer.header(&e).unwrap();
            writer.write(body).unwrap();
        }
        writer.finish().unwrap();
        file.sync_all().unwrap();
        let changes = BTreeMap::from([("b".into(), b"new body".to_vec())]);
        let additions = BTreeMap::from([
            ("new".into(), (0o040755, vec![])),
            (
                "new/test.rs".into(),
                (0o100644, b"immutable Git body".to_vec()),
            ),
        ]);
        let target = dir.join("target.tar.gz");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&target)
            .unwrap();
        let (written, _) = scan_added(
            &base,
            "old",
            &changes,
            &additions,
            Some((Path::new("next"), &file)),
        )
        .unwrap();
        file.sync_all().unwrap();
        let (readback, _) = scan(&target, "next", &Payloads::new(), None).unwrap();
        assert_eq!(written, readback);
        assert_eq!(readback["a"]["sha256"], digest(b"preserved"));
        assert_eq!(readback["b"]["sha256"], digest(b"new body"));
        assert_eq!(
            readback["new/test.rs"]["sha256"],
            digest(b"immutable Git body")
        );
        for bad in [
            BTreeMap::from([("a".into(), (0o100644, vec![]))]),
            BTreeMap::from([("missing/test.rs".into(), (0o100644, vec![]))]),
            BTreeMap::from([("unsafe".into(), (0o106644, vec![]))]),
        ] {
            assert!(scan_added(&base, "old", &Payloads::new(), &bad, None).is_err());
        }
    }
}
