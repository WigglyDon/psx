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
/// CMake uses the output object for dependency targets as well as -o. Only
/// those explicit operands may change; unrelated flags/includes stay exact.
fn rewrite_compile(
    recipe: &str,
    old: &str,
    new: &str,
    source: &str,
    target: &str,
) -> Result<String> {
    let mut outputs = 0;
    for (index, _) in recipe.match_indices(old) {
        let prefix = &recipe[..index];
        let suffix = &recipe[index + old.len()..];
        let boundary = |s: &str| s.is_empty() || s.starts_with(char::is_whitespace);
        if prefix.ends_with(" -MF ") && suffix.starts_with(".d") && boundary(&suffix[2..]) {
            continue;
        }
        if (prefix.ends_with(" -MT ") || prefix.ends_with(" -o ")) && boundary(suffix) {
            outputs += usize::from(prefix.ends_with(" -o "));
            continue;
        }
        return Err("Object occurs outside checked compiler output operands".into());
    }
    let positions: Vec<_> = recipe.match_indices(source).collect();
    if outputs != 1
        || positions.len() != 1
        || !recipe[..positions[0].0].ends_with(" -c ")
        || !recipe[positions[0].0 + source.len()..]
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    {
        return Err("Compiler output/source is missing or ambiguous".into());
    }
    Ok(recipe.replace(old, new).replacen(source, target, 1))
}

fn sibling_includes(recipe: &str, directory: &Path) -> Result<String> {
    let (compiler, arguments) = recipe.split_once(' ').ok_or("Compiler syntax")?;
    // A relocated .cpp and its aliased interface retain quoted includes into
    // the predecessor's verified sibling headers. Keep their physical identity.
    Ok(format!(
        "{compiler} -iquote {} {arguments}",
        quote(directory)?
    ))
}

fn sibling_header_hashes(
    root: &Path,
    previous: &Path,
    sources: &serde_json::Map<String, Value>,
) -> Result<Value> {
    let parent = Path::new(FILE).parent().unwrap();
    let mut hashes = serde_json::Map::new();
    for entry in fs::read_dir(previous.join("sources").join(parent))? {
        let entry = entry?;
        if !matches!(
            entry.path().extension().and_then(OsStr::to_str),
            Some("h" | "inl")
        ) {
            continue;
        }
        let relative = parent.join(entry.file_name());
        let actual = hash(&entry.path())?;
        let expected = match sources.get(relative.to_str().ok_or("Header encoding")?) {
            Some(sha) => sha.as_str().ok_or("Header digest")?.to_owned(),
            None => hash(&root.join("vendor/armsx2").join(&relative))?,
        };
        if actual != expected {
            return Err("Predecessor sibling header differs from composed or pinned source".into());
        }
        hashes.insert(relative.to_string_lossy().into_owned(), json!(actual));
    }
    if hashes.is_empty() {
        return Err("Missing predecessor sibling headers".into());
    }
    Ok(json!(hashes))
}

fn compose_cpp(root: &Path, previous: &Path, destination: &Path) -> Result<PathBuf> {
    let target = destination.join(FILE);
    fs::create_dir_all(target.parent().unwrap())?;
    fs::copy(previous.join("sources").join(FILE), &target)?;
    let stat = output(
        command("git", ["apply", "--numstat"])
            .arg(root.join(PATCH))
            .current_dir(destination),
    )?;
    if stat.lines().count() != 1
        || stat.lines().next().and_then(|l| l.split('\t').last()) != Some(FILE)
    {
        return Err("Overlap patch changes another source".into());
    }
    output(
        command("git", ["apply"])
            .arg(root.join(PATCH))
            .current_dir(destination),
    )?;
    Ok(target)
}

fn verify_source_tree(
    stage: &Path,
    previous: &Path,
    sources: &serde_json::Map<String, Value>,
) -> Result<()> {
    let base = stage.join("sources");
    let kind = fs::symlink_metadata(&base)?;
    if !kind.is_dir() || kind.is_symlink() {
        return Err("Redirected resume source root".into());
    }
    let mut expected = std::collections::BTreeSet::new();
    for file in sources.keys() {
        let mut p = safe_relative(file)?;
        while !p.as_os_str().is_empty() {
            expected.insert(p.to_path_buf());
            p = p.parent().unwrap();
        }
    }
    let mut pending = vec![base.clone()];
    let mut seen = std::collections::BTreeSet::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            let relative = path.strip_prefix(&base)?;
            let metadata = fs::symlink_metadata(&path)?;
            if !expected.contains(relative) {
                return Err("Unknown resume source/header; preserve unfinished stage".into());
            }
            seen.insert(relative.to_path_buf());
            if metadata.is_dir() && !metadata.is_symlink() {
                if sources.contains_key(relative.to_str().ok_or("Source encoding")?) {
                    return Err("Changed resume source to a directory".into());
                }
                pending.push(path);
            } else if relative == Path::new(FILE) {
                if !metadata.is_file() || metadata.is_symlink() {
                    return Err("Changed primary source type".into());
                }
            } else if !sources.contains_key(relative.to_str().ok_or("Source encoding")?)
                || !metadata.is_symlink()
                || fs::read_link(&path)? != previous.join("sources").join(relative)
            {
                return Err("Changed resume header/source alias".into());
            }
        }
    }
    if seen != expected {
        return Err("Incomplete resume source inventory".into());
    }
    Ok(())
}
pub fn build(
    root: &Path,
    workspace: &Path,
    cache: &Path,
    stage: &Path,
    previous: &Path,
    expected: &Value,
    revision: &str,
    resume: bool,
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
    let sibling_headers = sibling_header_hashes(root, &previous, sources)?;
    let link = fs::read_to_string(previous.join("link.command"))?;
    if before["linkRecipeSha256"] != text_sha(&link) {
        return Err("Frozen linker recipe changed".into());
    }
    let native_revision;
    let logs = if resume {
        for name in [
            "started.json",
            "predecessor-link-proof.json",
            "predecessor-link.command",
        ] {
            let metadata = fs::symlink_metadata(stage.join(name))?;
            if !metadata.is_file() || metadata.is_symlink() {
                return Err("Redirected resume metadata".into());
            }
        }
        let objects = fs::symlink_metadata(stage.join("objects"))?;
        if !objects.is_dir() || objects.is_symlink() {
            return Err("Redirected resume object root".into());
        }
        let started: Value = serde_json::from_slice(&fs::read(stage.join("started.json"))?)?;
        if started["predecessor"] != before
            || started["overlapPatchSha256"] != hash(&root.join(PATCH))?
            || started["operation"] != "one-unit producer overlap"
            || started["noColdBuild"] != true
            || fs::read_dir(stage.join("objects"))?.next().is_some()
            || [
                "native-binding.json",
                "libarmsx2-symbols.so",
                "libarmsx2-packaged.so",
                "libarmsx2-independent-strip.so",
            ]
            .iter()
            .any(|p| fs::symlink_metadata(stage.join(p)).is_ok())
        {
            return Err("Stage is not the retained precompile failure; preserve it".into());
        }
        native_revision = started["sourceRevision"]
            .as_str()
            .ok_or("Native source revision")?
            .to_owned();
        if native_revision.len() != 40 || !native_revision.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid retained native revision".into());
        }
        let proof: Value =
            serde_json::from_slice(&fs::read(stage.join("predecessor-link-proof.json"))?)?;
        let old_recipe = fs::read_to_string(stage.join("predecessor-link.command"))?;
        if proof["sha256"] != SYMBOLS
            || proof["expected"] != SYMBOLS
            || proof["verifiedDuplicate"] != true
            || proof["recipeSha256"] != text_sha(&old_recipe)
        {
            return Err("Retained predecessor proof differs".into());
        }
        verify_source_tree(stage, &previous, sources)?;
        let logs = (1..=8)
            .map(|n| stage.join(format!("resume-{n}")))
            .find(|p| {
                fs::symlink_metadata(p).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            })
            .ok_or("No unused bounded resume attempt; preserve all existing attempts")?;
        fs::create_dir(&logs)?; // Never overwrite an earlier failed resume.
        logs
    } else {
        native_revision = revision.to_owned();
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
        stage.to_path_buf()
    };
    // Re-link unchanged inputs into a new path, then compare all symbol bytes.
    // This proves dependencies omitted from the predecessor's nine-object
    // inventory, rather than silently accepting a changed shared cache.
    let old_symbols = quote(&previous.join("libarmsx2-symbols.so"))?;
    if link.matches(&old_symbols).count() != 1 {
        return Err("Frozen link output ambiguous".into());
    }
    let reproduction = logs.join("predecessor-link-reproduction.so");
    let repro_link = link.replacen(&old_symbols, &quote(&reproduction)?, 1);
    fs::write(logs.join("predecessor-link.command"), &repro_link)?;
    run(&repro_link, cache, &logs.join("predecessor-link.log"))?;
    let repro_sha = hash(&reproduction)?;
    if repro_sha != SYMBOLS {
        return Err("Unchanged link did not reproduce frozen symbols; preserve evidence".into());
    }
    let receipt = logs.join("predecessor-link-proof.json");
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
    if resume {
        let verify = logs.join("verify-source");
        fs::create_dir(&verify)?;
        let recomposed = compose_cpp(root, &previous, &verify)?;
        if hash(&recomposed)? != hash(&new_source)? {
            return Err("Retained patched source differs; keep both for review".into());
        }
    } else {
        compose_cpp(root, &previous, &stage.join("sources"))?;
        for (file, _) in sources {
            if file == FILE {
                continue;
            }
            let alias = stage.join("sources").join(file);
            fs::create_dir_all(alias.parent().unwrap())?;
            std::os::unix::fs::symlink(previous.join("sources").join(file), alias)?;
        }
    }
    let old_object = previous.join(format!("objects/{index}.o"));
    let new_object = stage.join(format!("objects/{index}.o"));
    let old_source = previous.join("sources").join(FILE);
    let old_object_str = old_object.to_str().ok_or("Object encoding")?;
    if link.matches(old_object_str).count() != 1 {
        return Err("Link replacement ambiguous".into());
    }
    let recipe = rewrite_compile(
        &compile,
        old_object_str,
        &quote(&new_object)?,
        old_source.to_str().ok_or("Source encoding")?,
        &quote(&new_source)?,
    )?;
    let recipe = sibling_includes(&recipe, old_source.parent().ok_or("Sibling header root")?)?;
    fs::write(logs.join(format!("compile-{index}.command")), &recipe)?;
    run(&recipe, cache, &logs.join(format!("compile-{index}.log")))?;
    let symbols = stage.join("libarmsx2-symbols.so");
    let packaged = stage.join("libarmsx2-packaged.so");
    let link = link
        .replacen(old_object_str, &quote(&new_object)?, 1)
        .replacen(&old_symbols, &quote(&symbols)?, 1);
    fs::write(logs.join("link.command"), &link)?;
    run(&link, cache, &logs.join("link.log"))?;
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
    if native_sha == NATIVE
        || hash(&independent)? != native_sha
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
    binding["sourceRevision"] = json!(native_revision);
    binding["builderSourceRevision"] = json!(revision);
    binding["resumedPrecompileStage"] = json!(resume);
    binding["quotedSiblingHeaderHashes"] = sibling_headers;
    binding["attempt"] = json!(logs.strip_prefix(stage)?.to_string_lossy());
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cmake_dependency_outputs_follow_the_object_without_changing_other_flags() {
        let old = "/before/12.o";
        let source = "/before/source.cpp";
        let recipe = "clang++ -I/before/header -MT /before/12.o -MF /before/12.o.d -o /before/12.o -c /before/source.cpp";
        let result = rewrite_compile(
            recipe,
            old,
            "'/after path/12.o'",
            source,
            "'/after path/source.cpp'",
        )
        .unwrap();
        assert_eq!(result,"clang++ -I/before/header -MT '/after path/12.o' -MF '/after path/12.o'.d -o '/after path/12.o' -c '/after path/source.cpp'");
        for bad in [
            recipe.replace("-MT", "-unknown"),
            format!("{recipe} -o /before/12.o"),
            recipe.replace("/before/source.cpp", "/before/source.cpp.other"),
            recipe.replace("-o /before/12.o", "-o other.o"),
        ] {
            assert!(rewrite_compile(&bad, old, "new.o", source, "new.cpp").is_err());
        }
    }

    #[test]
    fn retained_source_inventory_rejects_extra_headers_and_redirected_aliases() {
        let temp = env::temp_dir().join(format!(
            "vulkan-resume-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous = temp.join("previous");
        let stage = temp.join("stage");
        let header = "pcsx2/GS/Renderers/Vulkan/VKLibretro.h";
        fs::create_dir_all(
            previous
                .join("sources")
                .join(Path::new(header).parent().unwrap()),
        )
        .unwrap();
        fs::create_dir_all(
            stage
                .join("sources")
                .join(Path::new(FILE).parent().unwrap()),
        )
        .unwrap();
        fs::write(previous.join("sources").join(header), b"header").unwrap();
        fs::write(stage.join("sources").join(FILE), b"composed cpp").unwrap();
        let alias = stage.join("sources").join(header);
        std::os::unix::fs::symlink(previous.join("sources").join(header), &alias).unwrap();
        let sources = serde_json::Map::from_iter([
            (FILE.into(), json!("cpp")),
            (header.into(), json!("header")),
        ]);
        verify_source_tree(&stage, &previous, &sources).unwrap();
        let injected = alias.parent().unwrap().join("Injected.h");
        fs::write(&injected, b"unknown").unwrap();
        assert!(verify_source_tree(&stage, &previous, &sources).is_err());
        assert_eq!(fs::read(&injected).unwrap(), b"unknown");
        fs::remove_file(injected).unwrap(); // Only this test's synthetic file.
        fs::remove_file(&alias).unwrap();
        fs::write(&alias, b"header").unwrap();
        assert!(verify_source_tree(&stage, &previous, &sources).is_err());
        fs::remove_file(&alias).unwrap();
        fs::create_dir(&alias).unwrap();
        assert!(verify_source_tree(&stage, &previous, &sources).is_err());
        fs::remove_dir_all(temp).unwrap();
    }

    #[test]
    fn relocated_interface_can_include_its_original_sibling_header() {
        let temp = env::temp_dir().join(format!(
            "vulkan-quoted-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let previous = temp.join("original headers");
        let overlay = temp.join("overlay");
        fs::create_dir_all(&previous).unwrap();
        fs::create_dir_all(&overlay).unwrap();
        fs::write(
            previous.join("Sibling.h"),
            "constexpr int sibling_value=42;\n",
        )
        .unwrap();
        fs::write(previous.join("Interface.h"), "#include \"Sibling.h\"\n").unwrap();
        std::os::unix::fs::symlink(previous.join("Interface.h"), overlay.join("Interface.h"))
            .unwrap();
        let source = overlay.join("unit.cpp");
        fs::write(
            &source,
            "#include \"Interface.h\"\nstatic_assert(sibling_value==42);\n",
        )
        .unwrap();
        let recipe = sibling_includes(
            &format!("c++ -fsyntax-only {}", quote(&source).unwrap()),
            &previous,
        )
        .unwrap();
        let result = Command::new("sh").args(["-c", &recipe]).output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        fs::remove_dir_all(temp).unwrap();
    }
}
