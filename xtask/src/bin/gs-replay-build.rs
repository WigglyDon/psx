//! GS-REPLAY-1: compile only changed translation units from the exact retained
//! warm recipes; preserve cache objects/frozen operands and bind every input.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::OsStr,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[allow(dead_code)]
#[path = "../storage.rs"]
mod storage;
#[allow(dead_code)]
#[path = "../workspace.rs"]
mod workspace;
fn required(k: &str) -> Result<PathBuf> {
    Ok(env::var_os(k).ok_or_else(|| format!("missing{k}"))?.into())
}
fn command(p: impl AsRef<OsStr>, a: impl IntoIterator<Item = impl AsRef<OsStr>>) -> Command {
    let mut c = Command::new(p);
    c.args(a);
    c
}
fn output(c: &mut Command) -> Result<String> {
    let r = c.output()?;
    if !r.status.success() {
        return Err(String::from_utf8_lossy(&r.stderr).into_owned().into());
    }
    Ok(String::from_utf8(r.stdout)?)
}
fn hash(p: &Path) -> Result<String> {
    let mut f = fs::File::open(p)?;
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
fn save(p: &Path, v: &Value) -> Result<()> {
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(p)?;
    f.write_all(&serde_json::to_vec_pretty(v)?)?;
    f.sync_all()?;
    Ok(())
}
fn quoted(p: &Path) -> Result<String> {
    let p = p.to_str().ok_or("nonUTF8 path")?;
    Ok(format!("'{}'", p.replace('\'', "'\\''")))
}
fn run_recipe(recipe: &str, cwd: &Path, log: &Path) -> Result<()> {
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
        return Err(format!(
            "native recipe failed: {}; retained {}",
            status,
            log.display()
        )
        .into());
    }
    Ok(())
}
fn runner(
    root: &Path,
    workspace: &Path,
    cache: &Path,
    source: &Path,
    expected: &Value,
) -> Result<()> {
    let stage = required("PSX_GS_RUNNER_STAGE")?;
    if stage.parent() != Some(workspace.join("build").as_path()) || stage.exists() {
        return Err("require absent direct runner stage".into());
    }
    let modified = required("PSX_GS_MODIFIED_STAGE")?;
    if modified.parent() != Some(workspace.join("build").as_path()) {
        return Err("modified stage outside workspace".into());
    }
    let binding: Value = serde_json::from_slice(&fs::read(modified.join("native-binding.json"))?)?;
    if binding["baselineInventory"] != *expected
        || binding["glueHeaderSha256"] != hash(&root.join("patches/armsx2/gs-replay/GSReplay.h"))?
        || binding["extraPatchSha256"]
            != hash(&root.join("patches/armsx2/0008-development-gs-replay.patch"))?
    {
        return Err("changed modified/native common source".into());
    }
    for i in 0..4 {
        if binding["compiled"][i]["objectSha256"]
            != hash(&modified.join("objects").join(format!("{i}.o")))?
        {
            return Err("modified object changed".into());
        }
    }
    fs::create_dir(&stage)?;
    fs::create_dir(stage.join("objects"))?;
    fs::create_dir_all(stage.join("sources/pcsx2-gsrunner"))?;
    let original_main = source.join("pcsx2-gsrunner/Main.cpp");
    fs::copy(
        &original_main,
        stage.join("sources/pcsx2-gsrunner/Main.cpp"),
    )?;
    output(
        command("git", ["apply"])
            .arg(root.join("patches/armsx2/0009-development-gs-runner.patch"))
            .current_dir(stage.join("sources")),
    )?;
    let recipes =
        output(command("ninja", ["-t", "commands", "pcsx2-gsrunner"]).current_dir(cache))?;
    fs::write(stage.join("retained-runner-recipes.txt"), &recipes)?;
    let common = output(command("ninja", ["-t", "commands", "pcsx2-libretro"]).current_dir(cache))?;
    let header = root.join("patches/armsx2/gs-replay/GSReplay.h");
    let glue = root.join("patches/armsx2/gs-replay");
    let mut objects = Vec::new();
    let mut compiled = Vec::new();
    for (i, name) in [
        "Main.cpp",
        "GSLadder.cpp",
        "GSReplayPayload.cpp",
        "RenderDocCapture.cpp",
    ]
    .into_iter()
    .enumerate()
    {
        let original = source.join("pcsx2-gsrunner").join(name);
        let needle = format!("-c {}", original.display());
        let choices: Vec<_> = recipes.lines().filter(|l| l.contains(&needle)).collect();
        if choices.len() != 1 {
            return Err(format!("runner compile recipe {name}").into());
        }
        let object = choices[0]
            .split_once(" -o ")
            .ok_or("runner output")?
            .1
            .split_whitespace()
            .next()
            .ok_or("runner object")?;
        let new_object = stage.join("objects").join(format!("{i}.o"));
        let body = if i == 0 {
            stage.join("sources/pcsx2-gsrunner/Main.cpp")
        } else {
            original.clone()
        };
        let recipe = format!(
            "{} -fPIC -DPSX_GS_REPLAY1=1 -include {} -I{} -I{}",
            choices[0]
                .replace(object, new_object.to_str().unwrap())
                .replace(original.to_str().unwrap(), body.to_str().unwrap()),
            quoted(&header)?,
            quoted(&glue)?,
            quoted(&source.join("pcsx2-gsrunner"))?
        );
        fs::write(stage.join(format!("compile-{i}.command")), &recipe)?;
        run_recipe(&recipe, cache, &stage.join(format!("compile-{i}.log")))?;
        compiled.push(json!({"source":format!("pcsx2-gsrunner/{name}"),"sourceSha256":hash(&body)?,"recipeSha256":format!("{:x}",Sha256::digest(recipe.as_bytes())),"objectSha256":hash(&new_object)?}));
        objects.push(new_object);
    }
    let mut link = common
        .lines()
        .find(|l| l.contains(" -shared ") && l.contains(" -o ") && l.contains("armsx2_libretro.so"))
        .ok_or("runner shared-link base")?
        .to_owned();
    for (i, path) in [
        "pcsx2/GS/Renderers/Common/GSRenderer.cpp",
        "pcsx2/GS/GSDump.cpp",
        "pcsx2/GSDumpReplayer.cpp",
        "pcsx2-libretro/Main.cpp",
    ]
    .into_iter()
    .enumerate()
    {
        let needle = format!("-c {}", source.join(path).display());
        let recipe = common
            .lines()
            .find(|l| l.contains(&needle))
            .ok_or("common compile recipe")?;
        let old = recipe
            .split_once(" -o ")
            .ok_or("common output")?
            .1
            .split_whitespace()
            .next()
            .ok_or("common object")?;
        if link.matches(old).count() != 1 {
            return Err("common link binding".into());
        }
        let new = if i == 3 {
            objects
                .iter()
                .map(|p| quoted(p))
                .collect::<Result<Vec<_>>>()?
                .join(" ")
        } else {
            quoted(&modified.join("objects").join(format!("{i}.o")))?
        };
        link = link.replacen(old, &new, 1);
    }
    let raw = stage.join("libgsrunner-symbols.so");
    let packaged = stage.join("libgsrunner.so");
    let old = link
        .split_once(" -o ")
        .ok_or("runner link output")?
        .1
        .split_whitespace()
        .next()
        .ok_or("runner output")?
        .to_owned();
    link = link.replacen(&format!(" -o {old}"), &format!(" -o {}", quoted(&raw)?), 1);
    fs::write(stage.join("link.command"), &link)?;
    run_recipe(&link, cache, &stage.join("link.log"))?;
    let bin = required("ANDROID_NDK_HOME")?.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    output(
        &mut command(bin.join("llvm-strip"), ["--strip-unneeded", "-o"])
            .arg(&packaged)
            .arg(&raw),
    )?;
    let exports =
        output(&mut command(bin.join("llvm-nm"), ["-D", "--defined-only"]).arg(&packaged))?;
    for name in [
        "retro_psx_gsrunner_entry",
        "retro_psx_runner_prepare",
        "retro_psx_runner_service",
        "retro_psx_runner_stop",
        "retro_psx_runner_retired",
    ] {
        if !exports
            .lines()
            .any(|l| l.split_whitespace().last() == Some(name))
        {
            return Err(format!("runner export {name}").into());
        }
    }
    save(
        &stage.join("native-binding.json"),
        &json!({"experiment":"GS-REPLAY-1","role":"pinned-runner-with-declared-matched-common-repairs","upstream":"83fe4e293f71ab5587ef81b06143725bf8c6a30b","nativeSha256":hash(&packaged)?,"symbolsSha256":hash(&raw)?,"notes":output(&mut command(bin.join("llvm-readelf"),["-n"]).arg(&raw))?,"compiledRunner":compiled,"commonNative":binding,"runnerPatchSha256":hash(&root.join("patches/armsx2/0009-development-gs-runner.patch"))?,"runnerBridgeSha256":hash(&glue.join("RunnerBridge.inc"))?,"intentionalCommonRenderer":"Same repaired/instrumented GS implementation and creator output dependency. Pinned runner Main/host replaces libretro Main; this is NOT untouched upstream or an independent driver control.","libretroMainLinked":false}),
    )?;
    println!("runner verified {}", stage.display());
    Ok(())
}
fn main() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .canonicalize()?;
    env::set_current_dir(&root)?;
    let admitted = workspace::Admission::acquire(&root)?;
    let _project = storage::Lock::acquire(&root)?;
    let _growth = storage::Growth::begin(&admitted.root, 8 * storage::GIB)?;
    let cache = root.join("build/armsx2-android").canonicalize()?;
    let native_source = root.join(".local/upstream/armsx2");
    let expected: Value =
        serde_json::from_slice(&fs::read(root.join("patches/armsx2/composed-source.json"))?)?;
    if output(
        &mut command("git", ["-C"])
            .arg(&native_source)
            .args(["rev-parse", "HEAD"]),
    )?
    .trim()
        != "83fe4e293f71ab5587ef81b06143725bf8c6a30b"
    {
        return Err("source pin".into());
    }
    for (name, sha) in expected["nativeSourceHashes"]
        .as_object()
        .ok_or("source inventory")?
    {
        if Some(hash(&native_source.join(name))?.as_str()) != sha.as_str() {
            return Err(format!("source changed {name}").into());
        }
    }
    if env::args().nth(1).as_deref() == Some("runner") {
        return runner(&root, &admitted.root, &cache, &native_source, &expected);
    }
    let stage = required("PSX_GS_BUILD_STAGE")?;
    if stage.parent() != Some(admitted.root.join("build").as_path()) || stage.exists() {
        return Err("require absent direct admitted build stage".into());
    }
    fs::create_dir(&stage)?;
    fs::create_dir(stage.join("objects"))?;
    fs::create_dir(stage.join("sources"))?;
    save(
        &stage.join("started.json"),
        &json!({"operation":"GS-REPLAY-1 modified capture/replay","sourceRevision":output(&mut command("git",["rev-parse","HEAD"]))?.trim(),"priorNative":expected["nativeSha256"],"noFrozenOverwrite":true,"reserve":storage::RESERVE}),
    )?;
    let paths = [
        "pcsx2/GS/Renderers/Common/GSRenderer.cpp",
        "pcsx2/GS/GSDump.cpp",
        "pcsx2/GSDumpReplayer.cpp",
        "pcsx2-libretro/Main.cpp",
    ];
    for path in paths {
        let p = stage.join("sources").join(path);
        fs::create_dir_all(p.parent().unwrap())?;
        fs::copy(native_source.join(path), p)?;
    }
    output(
        command("git", ["apply"])
            .arg(root.join("patches/armsx2/0008-development-gs-replay.patch"))
            .current_dir(stage.join("sources")),
    )?;
    let header = root.join("patches/armsx2/gs-replay/GSReplay.h");
    let header_hash = hash(&header)?;
    let patch_hash = hash(&root.join("patches/armsx2/0008-development-gs-replay.patch"))?;
    let recipes =
        output(command("ninja", ["-t", "commands", "pcsx2-libretro"]).current_dir(&cache))?;
    fs::write(stage.join("retained-recipes.txt"), &recipes)?;
    let mut replacements = Vec::new();
    let mut bindings = Vec::new();
    let previous = env::var_os("PSX_GS_PREVIOUS_STAGE").map(PathBuf::from);
    let previous_binding: Option<Value> = if let Some(p) = &previous {
        if p.canonicalize()?.parent() != Some(admitted.root.join("build").as_path()) {
            return Err("previous native outside workspace".into());
        }
        let b: Value = serde_json::from_slice(&fs::read(p.join("native-binding.json"))?)?;
        if b["baselineInventory"] != expected || b["glueHeaderSha256"] != header_hash {
            return Err("previous common headers/defines differ".into());
        }
        Some(b)
    } else {
        None
    };
    for (i, path) in paths.into_iter().enumerate() {
        let original = native_source.join(path);
        let needle = format!("-c {}", original.display());
        let choices: Vec<_> = recipes.lines().filter(|l| l.contains(&needle)).collect();
        if choices.len() != 1 {
            return Err(format!("ambiguous compiler {path}").into());
        }
        let original_recipe = choices[0];
        let (_, tail) = original_recipe.split_once(" -o ").ok_or("compile output")?;
        let object = tail.split_whitespace().next().ok_or("object")?;
        if !object.ends_with(".o") || object.contains('\'') {
            return Err("unknown object syntax".into());
        }
        let new_object = stage.join("objects").join(format!("{i}.o"));
        let source = stage.join("sources").join(path);
        let source_hash = hash(&source)?;
        let old_recipe_hash = format!("{:x}", Sha256::digest(original_recipe.as_bytes()));
        if let (Some(previous), Some(binding)) = (&previous, &previous_binding) {
            let entry = &binding["compiled"][i];
            if entry["file"] == path
                && entry["sourceSha256"] == source_hash
                && entry["oldRecipeSha256"] == old_recipe_hash
            {
                // Reuse only an independently verified object with identical
                // source, compiler recipe, macro set and forced glue header.
                let prior_object = previous.join("objects").join(format!("{i}.o"));
                if entry["objectSha256"] != hash(&prior_object)? {
                    return Err("previous object changed".into());
                }
                fs::hard_link(&prior_object, &new_object)?;
                if entry["objectSha256"] != hash(&new_object)? {
                    return Err("reused object readback differs".into());
                }
                let mut entry = entry.clone();
                entry["reusedFromNative"] = binding["nativeSha256"].clone();
                bindings.push(entry);
                replacements.push((object.to_owned(), new_object));
                continue;
            }
        }
        let recipe = format!(
            "{} -DPSX_GS_REPLAY1=1 -include {}",
            original_recipe
                .replace(object, new_object.to_str().ok_or("object path")?)
                .replace(original.to_str().unwrap(), source.to_str().unwrap()),
            quoted(&header)?
        );
        fs::write(stage.join(format!("compile-{i}.command")), &recipe)?;
        run_recipe(&recipe, &cache, &stage.join(format!("compile-{i}.log")))?;
        bindings.push(json!({"file":path,"sourceSha256":hash(&source)?,"oldRecipeSha256":format!("{:x}",Sha256::digest(original_recipe.as_bytes())),"recipeSha256":format!("{:x}",Sha256::digest(recipe.as_bytes())),"objectSha256":hash(&new_object)?}));
        replacements.push((object.to_owned(), new_object));
    }
    let links: Vec<_> = recipes
        .lines()
        .filter(|l| {
            l.contains(" -shared ") && l.contains(" -o ") && l.contains("armsx2_libretro.so")
        })
        .collect();
    if links.len() != 1 {
        return Err("ambiguous shared link".into());
    }
    let mut link = links[0].to_owned();
    for (old, new) in &replacements {
        if link.matches(old).count() != 1 {
            return Err("link object binding".into());
        }
        link = link.replacen(old, new.to_str().unwrap(), 1);
    }
    let raw = stage.join("libarmsx2-symbols.so");
    let packaged = stage.join("libarmsx2-packaged.so");
    let output_path = link
        .split_once(" -o ")
        .ok_or("link output")?
        .1
        .split_whitespace()
        .next()
        .ok_or("link output path")?
        .to_owned();
    link = link.replacen(
        &format!(" -o {output_path}"),
        &format!(" -o {}", quoted(&raw)?),
        1,
    );
    fs::write(stage.join("link.command"), &link)?;
    run_recipe(&link, &cache, &stage.join("link.log"))?;
    let tools = required("ANDROID_NDK_HOME")?.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    output(
        &mut command(tools.join("llvm-strip"), ["--strip-unneeded", "-o"])
            .arg(&packaged)
            .arg(&raw),
    )?;
    let independent = stage.join("libarmsx2-independent-strip.so");
    output(
        &mut command(tools.join("llvm-strip"), ["--strip-unneeded", "-o"])
            .arg(&independent)
            .arg(&raw),
    )?;
    let native_sha = hash(&packaged)?;
    if native_sha != hash(&independent)? {
        return Err("independent strip mismatch".into());
    }
    let notes = output(&mut command(tools.join("llvm-readelf"), ["-n"]).arg(&raw))?;
    let exports =
        output(&mut command(tools.join("llvm-nm"), ["-D", "--defined-only"]).arg(&packaged))?;
    for required in [
        "retro_psx_gs_replay_configure",
        "retro_psx_gs_replay_clear",
        "retro_psx_observer_set_callback",
        "retro_load_game",
    ] {
        if !exports
            .lines()
            .any(|l| l.split_whitespace().last() == Some(required))
        {
            return Err(format!("missing export {required}").into());
        }
    }
    save(
        &stage.join("native-binding.json"),
        &json!({"experiment":"GS-REPLAY-1","role":"modified-capture-and-replay","upstream":"83fe4e293f71ab5587ef81b06143725bf8c6a30b","sourceRevision":output(&mut command("git",["rev-parse","HEAD"]))?.trim(),"nativeSha256":native_sha,"symbolsSha256":hash(&raw)?,"symbolNotes":notes,"extraPatchSha256":patch_hash,"glueHeaderSha256":header_hash,"baselineInventory":expected,"compiled":bindings,"linkSha256":format!("{:x}",Sha256::digest(link.as_bytes())),"independentStrip":true,"frozenOutputsUnchanged":true}),
    )?;
    println!("native verified {}", stage.display());
    if env::args().nth(1).as_deref() == Some("pair") {
        runner(&root, &admitted.root, &cache, &native_source, &expected)?;
    }
    Ok(())
}
