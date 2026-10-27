//! One-translation-unit successor of the frozen cf6f23e Vulkan candidate.
//! Reproduce its link first: cached dependency inputs are not trusted merely
//! because their paths still exist. Never mutate a predecessor or cold-build.
use super::*;
const NATIVE: &str = "8641f6ff59fe3a2228807fe8a8b6abda13733cfb7ca08e717405484eff8adf9d";
const SYMBOLS: &str = "5f171a83042b5f73f0c9545e255ae4d5bbaa0dad5883bdcdbec9551b6edc1fea";
const FILE: &str = "pcsx2/GS/Renderers/Vulkan/VKLibretro.cpp";
const PATCH: &str = "patches/armsx2/0011-vulkan-producer-overlap.patch";
const PRIOR: [&str; 3] = [
    "0001-host-pause-and-card-flush.patch",
    "0004-stream-buffer-failure.patch",
    "0010-vulkan-frame-completion.patch",
];
fn text_sha(s: &str) -> String {
    format!("{:x}", Sha256::digest(s.as_bytes()))
}
fn safe_relative(file: &str) -> Result<&Path> {
    let p = Path::new(file);
    if p.is_absolute()
        || !p
            .components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Unsafe retained source name".into());
    }
    Ok(p)
}
pub fn build(
    root: &Path,
    workspace: &Path,
    cache: &Path,
    stage: &Path,
    previous: &Path,
    expected: &Value,
    revision: &str,
) -> Result<()> {
    let previous = previous.canonicalize()?;
    if previous.parent() != Some(workspace.join("build").as_path()) || previous == stage {
        return Err("Predecessor must be a distinct direct admitted build stage".into());
    }
    let before: Value = serde_json::from_slice(&fs::read(previous.join("native-binding.json"))?)?;
    if before["sourceRevision"] != "cf6f23e2d30867e01a158b6e09294113ae7e73d1"
        || before["upstream"] != expected["upstream"]
        || before["commonSource"] != *expected
        || before["nativeSha256"] != NATIVE
        || hash(&previous.join("libarmsx2-packaged.so"))? != NATIVE
        || before["symbolsSha256"] != SYMBOLS
        || hash(&previous.join("libarmsx2-symbols.so"))? != SYMBOLS
        || before["productionPatches"] != json!(PRIOR)
        || before["patchSha256"]
            != hash(&root.join("patches/armsx2/0010-vulkan-frame-completion.patch"))?
        || before["retainedInactiveGlObservationCode"] != false
        || before["independentStrip"] != true
        || before["baselineCacheNativeSha256"] != hash(&cache.join("bin/armsx2_libretro.so"))?
    {
        return Err(
            "Frozen predecessor/cache binding differs; retain it without rebuilding".into(),
        );
    }
    let sources = before["composedSourceHashes"]
        .as_object()
        .ok_or("Composed inventory")?;
    for (file, recorded) in sources {
        if *recorded != hash(&previous.join("sources").join(safe_relative(file)?))? {
            return Err(format!("Frozen composed source differs: {file}").into());
        }
    }
    let compiled = before["compiled"]
        .as_array()
        .ok_or("Frozen object inventory")?;
    if compiled.len() != 9 {
        return Err("Unexpected predecessor object count".into());
    }
    let mut selected = None;
    let mut recipes = Vec::new();
    for entry in fs::read_dir(&previous)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(index) = name
            .strip_prefix("compile-")
            .and_then(|v| v.strip_suffix(".command"))
        {
            let index = index.parse::<usize>()?;
            recipes.push((index, fs::read_to_string(entry.path())?));
        }
    }
    if recipes.len() != compiled.len() {
        return Err("Unexpected predecessor recipe count".into());
    }
    let mut used = std::collections::BTreeSet::new();
    for entry in compiled {
        let matches: Vec<_> = recipes
            .iter()
            .filter(|(_, command)| entry["recipeSha256"] == text_sha(command))
            .collect();
        if matches.len() != 1 {
            return Err("Frozen recipe missing or ambiguous".into());
        }
        let (i, command) = matches[0];
        if entry["recipeSha256"] != text_sha(&command)
            || entry["objectSha256"] != hash(&previous.join(format!("objects/{i}.o")))?
            || sources.get(entry["file"].as_str().ok_or("Object source")?)
                != Some(&entry["sourceSha256"])
        {
            return Err("Frozen compile/object/source binding differs".into());
        }
        if !used.insert(*i) {
            return Err("Frozen recipe reused by two source records".into());
        }
        if entry["file"] == FILE {
            selected = Some((*i, command.clone()));
        }
    }
    for name in &PRIOR[..2] {
        if expected["nativePatchHashes"][*name] != hash(&root.join("patches/armsx2").join(name))? {
            return Err("Common product patch changed; single-unit reuse is invalid".into());
        }
    }
    let (index, compile) = selected.ok_or("Missing exact handoff translation unit")?;
    let link = fs::read_to_string(previous.join("link.command"))?;
    if before["linkRecipeSha256"] != text_sha(&link) {
        return Err("Frozen linker recipe changed".into());
    }
    fs::create_dir(stage)?;
    fs::create_dir(stage.join("sources"))?;
    fs::create_dir(stage.join("objects"))?;
    fs::write(
        stage.join("started.json"),
        serde_json::to_vec_pretty(&json!({
            "sourceRevision":revision,"predecessor":before,"operation":"one-unit producer overlap",
            "overlapPatchSha256":hash(&root.join(PATCH))?,"noColdBuild":true
        }))?,
    )?;
    // Re-link unchanged inputs into a new path, then compare all symbol bytes.
    // This proves dependencies omitted from the predecessor's nine-object
    // inventory, rather than silently accepting a changed shared cache.
    let old_symbols = quote(&previous.join("libarmsx2-symbols.so"))?;
    if link.matches(&old_symbols).count() != 1 {
        return Err("Frozen link output ambiguous".into());
    }
    let reproduction = stage.join("predecessor-link-reproduction.so");
    let repro_link = link.replacen(&old_symbols, &quote(&reproduction)?, 1);
    fs::write(stage.join("predecessor-link.command"), &repro_link)?;
    run(&repro_link, cache, &stage.join("predecessor-link.log"))?;
    let repro_sha = hash(&reproduction)?;
    if repro_sha != SYMBOLS {
        return Err("Unchanged link did not reproduce frozen symbols; preserve evidence".into());
    }
    let receipt = stage.join("predecessor-link-proof.json");
    fs::write(
        &receipt,
        serde_json::to_vec_pretty(&json!({"sha256":repro_sha,
        "expected":SYMBOLS,"recipeSha256":text_sha(&repro_link),"verifiedDuplicate":true}))?,
    )?;
    fs::File::open(&receipt)?.sync_all()?;
    // Retire only this newly generated and independently proven duplicate.
    if hash(&reproduction)? != SYMBOLS {
        return Err("Reproduction changed before retirement".into());
    }
    fs::remove_file(&reproduction)?;
    // All unchanged headers/translation units keep their verified identities.
    // The only copied/modified source is the .cpp named by this patch.
    let new_source = stage.join("sources").join(FILE);
    fs::create_dir_all(new_source.parent().unwrap())?;
    fs::copy(previous.join("sources").join(FILE), &new_source)?;
    output(
        command("git", ["apply", "--numstat"])
            .arg(root.join(PATCH))
            .current_dir(stage.join("sources")),
    )?
    .lines()
    .try_for_each(|line| -> Result<()> {
        if line.split('\t').last() != Some(FILE) {
            return Err("Overlap patch changes another source".into());
        }
        Ok(())
    })?;
    output(
        command("git", ["apply"])
            .arg(root.join(PATCH))
            .current_dir(stage.join("sources")),
    )?;
    for (file, _) in sources {
        if file == FILE {
            continue;
        }
        let alias = stage.join("sources").join(file);
        fs::create_dir_all(alias.parent().unwrap())?;
        std::os::unix::fs::symlink(previous.join("sources").join(file), alias)?;
    }
    let old_object = previous.join(format!("objects/{index}.o"));
    let new_object = stage.join(format!("objects/{index}.o"));
    let old_source = previous.join("sources").join(FILE);
    let old_object_str = old_object.to_str().ok_or("Object encoding")?;
    let old_source_str = old_source.to_str().ok_or("Source encoding")?;
    if compile.matches(old_object_str).count() != 1
        || compile.matches(old_source_str).count() != 1
        || link.matches(old_object_str).count() != 1
    {
        return Err("Compile/link replacement ambiguous".into());
    }
    let recipe = compile
        .replacen(old_object_str, new_object.to_str().unwrap(), 1)
        .replacen(old_source_str, new_source.to_str().unwrap(), 1);
    fs::write(stage.join(format!("compile-{index}.command")), &recipe)?;
    run(&recipe, cache, &stage.join(format!("compile-{index}.log")))?;
    let symbols = stage.join("libarmsx2-symbols.so");
    let packaged = stage.join("libarmsx2-packaged.so");
    let link = link
        .replacen(old_object_str, new_object.to_str().unwrap(), 1)
        .replacen(&old_symbols, &quote(&symbols)?, 1);
    fs::write(stage.join("link.command"), &link)?;
    run(&link, cache, &stage.join("link.log"))?;
    let tools = required("ANDROID_NDK_HOME")?.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    let independent = stage.join("libarmsx2-independent-strip.so");
    for path in [&packaged, &independent] {
        output(
            command(tools.join("llvm-strip"), ["--strip-unneeded", "-o"])
                .arg(path)
                .arg(&symbols),
        )?;
    }
    let native_sha = hash(&packaged)?;
    if hash(&independent)? != native_sha
        || hash(&previous.join("libarmsx2-symbols.so"))? != SYMBOLS
        || hash(&previous.join("libarmsx2-packaged.so"))? != NATIVE
        || before["baselineCacheNativeSha256"] != hash(&cache.join("bin/armsx2_libretro.so"))?
    {
        return Err("Strip or frozen preservation check failed".into());
    }
    let exports = output(command(tools.join("llvm-nm"), ["-D", "--defined-only"]).arg(&packaged))?;
    let prior_exports = output(
        command(tools.join("llvm-nm"), ["-D", "--defined-only"])
            .arg(previous.join("libarmsx2-packaged.so")),
    )?;
    let names = |s: &str| {
        s.lines()
            .filter_map(|l| l.split_whitespace().last())
            .map(str::to_owned)
            .collect::<std::collections::BTreeSet<_>>()
    };
    if names(&exports) != names(&prior_exports) {
        return Err("Native ABI/export set differs".into());
    }
    let mut composed = sources.clone();
    composed.insert(FILE.into(), json!(hash(&new_source)?));
    let mut binding = before.clone();
    binding["sourceRevision"] = json!(revision);
    binding["productCandidate"] = json!("Vulkan with completion-owned one-frame producer overlap");
    binding["overlapPatchSha256"] = json!(hash(&root.join(PATCH))?);
    let mut patches = PRIOR.to_vec();
    patches.push("0011-vulkan-producer-overlap.patch");
    binding["productionPatches"] = json!(patches);
    binding["composedSourceHashes"] = json!(composed);
    binding["compiled"] = json!([{"file":FILE,"sourceSha256":hash(&new_source)?,
        "recipeSha256":text_sha(&recipe),"objectSha256":hash(&new_object)?}]);
    binding["reusedObjects"] = json!(compiled
        .iter()
        .filter(|e| e["file"] != FILE)
        .collect::<Vec<_>>());
    binding["predecessorSymbolsSha256"] = json!(repro_sha);
    binding["predecessorStage"] = json!(previous);
    binding["nativeSha256"] = json!(native_sha);
    binding["symbolsSha256"] = json!(hash(&symbols)?);
    binding["linkRecipeSha256"] = json!(text_sha(&link));
    binding["symbolNotes"] = json!(output(
        command(tools.join("llvm-readelf"), ["-n"]).arg(&symbols)
    )?);
    let path = stage.join("native-binding.json");
    fs::write(&path, serde_json::to_vec_pretty(&binding)?)?;
    fs::File::open(path)?.sync_all()?;
    println!(
        "One-unit Vulkan successor verified: {} {native_sha}",
        stage.display()
    );
    Ok(())
}
