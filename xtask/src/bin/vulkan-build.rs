//! Incremental Vulkan frontend candidate: retain the qualified native cache,
//! replace changed handoff users and retire inactive GL observation code.
//! No engine/dependency rebuild or mutation of a frozen output.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
#[path = "vulkan-build/overlap.rs"]
mod overlap;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[allow(dead_code)]
#[path = "../storage.rs"]
mod storage;
#[allow(dead_code)]
#[path = "../workspace.rs"]
mod workspace;
fn required(key: &str) -> Result<PathBuf> {
    Ok(env::var_os(key)
        .ok_or_else(|| format!("Missing {key}"))?
        .into())
}
fn command(p: impl AsRef<OsStr>, args: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Command {
    let mut c = Command::new(p);
    c.args(args);
    c
}
fn output(c: &mut Command) -> Result<String> {
    let o = c.output()?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).into_owned().into());
    }
    Ok(String::from_utf8(o.stdout)?)
}
fn hash(path: &Path) -> Result<String> {
    let mut f = fs::File::open(path)?;
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
fn quote(p: &Path) -> Result<String> {
    Ok(format!(
        "'{}'",
        p.to_str().ok_or("Path encoding")?.replace('\'', "'\\''")
    ))
}
fn run(recipe: &str, cwd: &Path, log: &Path) -> Result<()> {
    workspace::check_present()?;
    storage::between_steps()?;
    let f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(log)?;
    let status = Command::new("sh")
        .args(["-c", recipe])
        .current_dir(cwd)
        .stdout(Stdio::from(f.try_clone()?))
        .stderr(Stdio::from(f))
        .status()?;
    if !status.success() {
        return Err(format!("Native command {status}; retained {}", log.display()).into());
    }
    Ok(())
}
fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()?;
    env::set_current_dir(&root)?;
    let admission = workspace::Admission::acquire(&root)?;
    let _project = storage::Lock::acquire(&root)?;
    let _growth = storage::Growth::begin(&admission.root, 2 * storage::GIB)?;
    let revision = output(&mut command("git", ["rev-parse", "HEAD"]))?
        .trim()
        .to_owned();
    if !output(&mut command("git", ["status", "--porcelain"]))?.is_empty() {
        return Err("Commit candidate before native binding".into());
    }
    let cache = root.join("build/armsx2-android").canonicalize()?;
    let source = root.join(".local/upstream/armsx2");
    let expected: Value =
        serde_json::from_slice(&fs::read(root.join("patches/armsx2/composed-source.json"))?)?;
    if output(&mut command("git", ["rev-parse", "HEAD"]).current_dir(&source))?.trim()
        != expected["upstream"].as_str().ok_or("Pin")?
    {
        return Err("Native source pin differs".into());
    }
    for (path, sha) in expected["nativeSourceHashes"]
        .as_object()
        .ok_or("Source inventory")?
    {
        if Some(hash(&source.join(path))?.as_str()) != sha.as_str() {
            return Err(format!("Retained source changed: {path}").into());
        }
    }
    let stage = required("PSX_VULKAN_NATIVE_STAGE")?;
    if stage.parent() != Some(admission.root.join("build").as_path()) || stage.exists() {
        return Err("Require absent direct project native stage".into());
    }
    if let Some(previous) = env::var_os("PSX_VULKAN_PREDECESSOR_STAGE") {
        return overlap::build(
            &root,
            &admission.root,
            &cache,
            &stage,
            &PathBuf::from(previous),
            &expected,
            &revision,
        );
    }
    let recipes =
        output(&mut command("ninja", ["-t", "commands", "pcsx2-libretro"]).current_dir(&cache))?;
    let baseline_hash = hash(&cache.join("bin/armsx2_libretro.so"))?;
    let patch = root.join("patches/armsx2/0010-vulkan-frame-completion.patch");
    let patch_sha = hash(&patch)?;
    fs::create_dir(&stage)?;
    fs::create_dir(stage.join("objects"))?;
    fs::create_dir(stage.join("sources"))?;
    fs::write(
        stage.join("started.json"),
        serde_json::to_vec_pretty(
            &json!({"sourceRevision":revision,"operation":"playable Vulkan frontend","baselineCacheNativeSha256":baseline_hash,"commonSource":expected,"vulkanPatchSha256":patch_sha,"noColdBuild":true}),
        )?,
    )?;
    fs::write(stage.join("retained-recipes.txt"), &recipes)?;
    let mut files: Vec<String> = expected["nativeSourceHashes"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    files.extend(
        [
            "pcsx2/GS/Renderers/Vulkan/VKLibretro.cpp",
            "pcsx2/GS/Renderers/Vulkan/VKLibretro.h",
            "pcsx2/GS/Renderers/Vulkan/GSDeviceVK.cpp",
        ]
        .map(str::to_owned),
    );
    // Recompose production source from the pin: do not leak development-only
    // callbacks into an ordinary player just because their objects are warm.
    for file in &files {
        let p = stage.join("sources").join(file);
        fs::create_dir_all(p.parent().unwrap())?;
        fs::copy(root.join("vendor/armsx2").join(file), p)?;
    }
    let patches = [
        "0001-host-pause-and-card-flush.patch",
        "0004-stream-buffer-failure.patch",
        "0010-vulkan-frame-completion.patch",
        "0011-vulkan-producer-overlap.patch",
    ];
    for file in patches {
        output(
            command("git", ["apply"])
                .arg(root.join("patches/armsx2").join(file))
                .current_dir(stage.join("sources")),
        )?;
    }
    // pragma once is file-identity based. An unchanged copied header can be
    // reached once through an original relative include and once through the
    // overlay, causing duplicate class definitions. Keep those headers on the
    // same inode, and make quoted sibling includes resolvable without copying
    // another source/dependency tree. Only this new generated stage is edited.
    for file in &files {
        let staged = stage.join("sources").join(file);
        let original = source.join(file);
        if file.ends_with(".h") && hash(&staged)? == hash(&original)? {
            fs::remove_file(&staged)?;
            std::os::unix::fs::symlink(&original, &staged)?;
        }
        for entry in fs::read_dir(original.parent().ok_or("Source parent")?)? {
            let entry = entry?;
            if !matches!(
                entry.path().extension().and_then(OsStr::to_str),
                Some("h" | "inl")
            ) {
                continue;
            }
            let alias = staged.parent().unwrap().join(entry.file_name());
            if !alias.exists() {
                std::os::unix::fs::symlink(entry.path(), alias)?;
            }
        }
    }
    let composed: Value = Value::Object(
        files
            .iter()
            .map(|p| Ok((p.clone(), json!(hash(&stage.join("sources").join(p))?))))
            .collect::<Result<serde_json::Map<String, Value>>>()?,
    );
    let mut substitutions = Vec::new();
    let mut compiled = Vec::new();
    for (i, file) in files.iter().filter(|p| p.ends_with(".cpp")).enumerate() {
        let original = source.join(file);
        let target = stage.join("sources").join(file);
        if !file.ends_with("/GSDeviceVK.cpp") && hash(&original)? == hash(&target)? {
            continue;
        }
        let needle = format!("-c {}", original.display());
        let choices: Vec<_> = recipes.lines().filter(|l| l.contains(&needle)).collect();
        if choices.len() != 1 {
            return Err(format!("Compiler recipe ambiguous: {file}").into());
        }
        let before = choices[0];
        let object = before
            .split_once(" -o ")
            .ok_or("Compiler output")?
            .1
            .split_whitespace()
            .next()
            .ok_or("Object")?;
        if !object.ends_with(".o") || object.contains('\'') {
            return Err("Unknown object syntax".into());
        }
        let new = stage.join("objects").join(format!("{i}.o"));
        let recipe = before
            .replace(object, new.to_str().ok_or("Object encoding")?)
            .replace(original.to_str().unwrap(), target.to_str().unwrap());
        // Prepend the only new header root; common donor headers retain their
        // frozen composition and ABI in every unchanged cached translation unit.
        let (compiler, args) = recipe.split_once(' ').ok_or("Compiler syntax")?;
        let recipe = format!(
            "{compiler} -I{} -I{} {args}",
            quote(&stage.join("sources"))?,
            quote(&stage.join("sources/pcsx2"))?
        );
        fs::write(stage.join(format!("compile-{i}.command")), &recipe)?;
        run(&recipe, &cache, &stage.join(format!("compile-{i}.log")))?;
        compiled.push(json!({"file":file,"sourceSha256":hash(&target)?,"originalRecipeSha256":format!("{:x}",Sha256::digest(before.as_bytes())),"recipeSha256":format!("{:x}",Sha256::digest(recipe.as_bytes())),"objectSha256":hash(&new)?}));
        substitutions.push((object.to_owned(), new));
    }
    let choices: Vec<_> = recipes
        .lines()
        .filter(|l| {
            l.contains(" -shared ") && l.contains(" -o ") && l.contains("armsx2_libretro.so")
        })
        .collect();
    if choices.len() != 1 {
        return Err("Shared link ambiguous".into());
    }
    let mut link = choices[0].to_owned();
    for (old, new) in substitutions {
        if link.matches(&old).count() != 1 {
            return Err("Link object binding".into());
        }
        link = link.replacen(&old, new.to_str().unwrap(), 1);
    }
    let output_file = link
        .split_once(" -o ")
        .ok_or("Link output")?
        .1
        .split_whitespace()
        .next()
        .ok_or("Link filename")?
        .to_owned();
    let symbols = stage.join("libarmsx2-symbols.so");
    let packaged = stage.join("libarmsx2-packaged.so");
    link = link.replacen(
        &format!(" -o {output_file}"),
        &format!(" -o {}", quote(&symbols)?),
        1,
    );
    fs::write(stage.join("link.command"), &link)?;
    run(&link, &cache, &stage.join("link.log"))?;
    let tools = required("ANDROID_NDK_HOME")?.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    output(
        command(tools.join("llvm-strip"), ["--strip-unneeded", "-o"])
            .arg(&packaged)
            .arg(&symbols),
    )?;
    let independent = stage.join("libarmsx2-independent-strip.so");
    output(
        command(tools.join("llvm-strip"), ["--strip-unneeded", "-o"])
            .arg(&independent)
            .arg(&symbols),
    )?;
    let native_sha = hash(&packaged)?;
    if native_sha != hash(&independent)?
        || baseline_hash != hash(&cache.join("bin/armsx2_libretro.so"))?
    {
        return Err("Independent strip or frozen cache preservation failed".into());
    }
    let exports = output(command(tools.join("llvm-nm"), ["-D", "--defined-only"]).arg(&packaged))?;
    for symbol in [
        "retro_run",
        "retro_load_game",
        "retro_psx_capture_state",
        "retro_psx_set_frame_callback",
        "retro_psx_flush_cards",
    ] {
        if !exports
            .lines()
            .any(|l| l.split_whitespace().last() == Some(symbol))
        {
            return Err(format!("Missing {symbol}").into());
        }
    }
    if exports.contains("retro_psx_observer_") || exports.contains("retro_psx_gs_") {
        return Err("Development-only native exports entered product candidate".into());
    }
    let binding = json!({"productCandidate":"Vulkan with synchronous Rust staging","sourceRevision":revision,"upstream":expected["upstream"],"baselineCacheNativeSha256":baseline_hash,"commonSource":expected,"patchSha256":patch_sha,"overlapPatchSha256":hash(&root.join("patches/armsx2/0011-vulkan-producer-overlap.patch"))?,"productionPatches":patches,"composedSourceHashes":composed,"compiled":compiled,"headerSha256":hash(&stage.join("sources/pcsx2/GS/Renderers/Vulkan/VKLibretro.h"))?,"nativeSha256":native_sha,"symbolsSha256":hash(&symbols)?,"symbolNotes":output(command(tools.join("llvm-readelf"),["-n"]).arg(&symbols))?,"linkRecipeSha256":format!("{:x}",Sha256::digest(link.as_bytes())),"independentStrip":true,"retainedInactiveGlObservationCode":false,"noGeneratedProbeOrReplay":true});
    let binding_path = stage.join("native-binding.json");
    fs::write(&binding_path, serde_json::to_vec_pretty(&binding)?)?;
    fs::File::open(binding_path)?.sync_all()?;
    println!("Vulkan native candidate {} {native_sha}", stage.display());
    Ok(())
}
