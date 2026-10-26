//! Package only the independent Rust wrapper and reviewed coherent-map candidate.
//! No CMake/native builder, dependency expansion, Java or ordinary APK rebuild.
use super::*;
use serde_json::json;
use std::io::Write;
fn output(command: &mut Command) -> Result<Vec<u8>> {
    workspace::check_present()?;
    storage::between_steps()?;
    let result = command.output()?;
    if !result.status.success() {
        return Err(format!(
            "Wrapper metadata command failed: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(result.stdout)
}
const PLAYER: &str = "8e61eb2ccdf364cc4aa80e33dfc0796021c9dac1";
const APK: &str = "c90b0a52fca2d7f63780797bb82f68f02a2cc5af2c8409d6dc47562a10b9f8e9";
const NATIVE: &str = "b6bff788bc7676d67417beb89c5880590cd47038f6d3d3b2d235220addf5d9cf";
const SYMBOLS: &str = "3a6ea954b03b6ea07718735b6fef669c8b747d15a7985828159a19b7e203c874";
const BASELINE_NATIVE: &str = "e9e648d8658c767c182d34ebccbfa52aa0672a3c3e7182ee77653ac201acf467";
const NATIVE_ID: &str = "57b560aba913a18b3eb3882f03fec1d3c12dfb20";
const SOURCE: &str = "fc8435ef043d1a01ba6205da2bbe73c17c568e9af320369929c07da22ae4338f";
const CERT: &str = "391bb44bb79ba157f06f550ec73dcf3074040e004d0fd5d72c232211d850fc25";
fn checked(path: &Path, expected: &str) -> Result<()> {
    if !path.is_file() || hash(path)? != expected {
        return Err(format!("Frozen input differs: {}", path.display()).into());
    }
    Ok(())
}
fn member_hash(apk: &Path, names: &[&str]) -> Result<String> {
    let mut c = command("unzip", [OsStr::new("-p"), apk.as_os_str()])
        .args(names)
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = c.stdout.take().ok_or("no ZIP stream")?;
    let mut h = Sha256::new();
    let mut n = 0usize;
    let mut buf = [0; 65536];
    loop {
        let len = input.read(&mut buf)?;
        if len == 0 {
            break;
        }
        n = n.checked_add(len).ok_or("ZIP size overflow")?;
        if n > 128 * 1024 * 1024 {
            let _ = c.kill();
            let _ = c.wait();
            return Err("ZIP member bound exceeded".into());
        }
        h.update(&buf[..len]);
    }
    if !c.wait()?.success() {
        return Err("ZIP read failed".into());
    }
    Ok(format!("{:x}", h.finalize()))
}
fn entries(apk: &Path) -> Result<Vec<String>> {
    Ok(String::from_utf8(output(&mut command(
        "unzip",
        [OsStr::new("-Z1"), apk.as_os_str()],
    ))?)?
    .lines()
    .map(str::to_owned)
    .collect())
}
fn assets(names: Vec<String>) -> Vec<String> {
    names
        .into_iter()
        .filter(|n| n.starts_with("assets/ps2/") || n.starts_with("assets/notices/"))
        .collect()
}
pub fn build() -> Result<()> {
    build_impl(false, false)
}
pub fn build_replay() -> Result<()> {
    build_impl(true, false)
}
pub fn build_runner() -> Result<()> {
    build_impl(true, true)
}
fn build_impl(replay: bool, runner: bool) -> Result<()> {
    let root = required("PSX_BUILD_WORKSPACE")?;
    if !Path::new("target").canonicalize()?.starts_with(&root) {
        return Err("Minimal wrapper requires the verified relocated cache".into());
    }
    if !output(&mut command("git", ["status", "--porcelain"]))?.is_empty() {
        return Err("Commit exact wrapper source before package binding".into());
    }
    let revision = String::from_utf8(output(&mut command("git", ["rev-parse", "HEAD"]))?)?
        .trim()
        .to_owned();
    let frozen = required("PSX_FROZEN_PLAYER_DIR")?;
    let player = frozen.join("psx.apk");
    let candidate = required(if runner {
        "PSX_GS_RUNNER_STAGE"
    } else if replay {
        "PSX_GS_MODIFIED_STAGE"
    } else {
        "PSX_COHERENT_NATIVE_DIR"
    })?;
    if (!replay && candidate.canonicalize()? != root.join(".local/candidates/coherent-native"))
        || (replay && (candidate.canonicalize()?.parent() != Some(root.join("build").as_path())))
    {
        return Err("Require this independently verified native candidate".into());
    }
    let native = candidate.join(if runner {
        "libgsrunner.so"
    } else {
        "libarmsx2-packaged.so"
    });
    let symbols = candidate.join(if runner {
        "libgsrunner-symbols.so"
    } else {
        "libarmsx2-symbols.so"
    });
    let candidate_binding: serde_json::Value =
        serde_json::from_slice(&fs::read(candidate.join(if replay {
            "native-binding.json"
        } else {
            "binding.json"
        }))?)?;
    let composition: serde_json::Value =
        serde_json::from_slice(&fs::read("patches/armsx2/composed-source.json")?)?;
    let common_binding = if runner {
        &candidate_binding["commonNative"]
    } else {
        &candidate_binding
    };
    if !replay
        && (candidate_binding["repair"] != "persistent-map-coherent"
            || candidate_binding["nativeSha256"] != NATIVE
            || candidate_binding["nativeSymbolsSha256"] != SYMBOLS
            || candidate_binding["nativeBuildId"] != NATIVE_ID
            || candidate_binding["baselineNativeSha256"] != BASELINE_NATIVE
            || candidate_binding["exactOneLineDelta"] != true
            || candidate_binding["mode"] != "producer-local-output-sync"
            || candidate_binding["upstream"] != ps2::REVISION
            || candidate_binding["composedSourceSha256"] != composition["composedSourceSha256"]
            || candidate_binding["nativeSourceHashes"] != composition["nativeSourceHashes"]
            || candidate_binding["nativePatchHashes"] != composition["nativePatchHashes"]
            || composition["nativeSha256"] != NATIVE
            || composition["upstream"] != ps2::REVISION
            || composition["projectRevision"] != candidate_binding["projectRevision"])
    {
        return Err("New native composition/symbol provenance differs".into());
    }
    if replay && !runner {
        if candidate_binding["experiment"] != "GS-REPLAY-1"
            || candidate_binding["role"] != "modified-capture-and-replay"
            || candidate_binding["baselineInventory"] != composition
            || candidate_binding["extraPatchSha256"]
                != hash(Path::new("patches/armsx2/0008-development-gs-replay.patch"))?
            || candidate_binding["glueHeaderSha256"]
                != hash(Path::new("patches/armsx2/gs-replay/GSReplay.h"))?
            || candidate_binding["independentStrip"] != true
        {
            return Err("GS candidate source/patch binding differs".into());
        }
        for entry in candidate_binding["compiled"]
            .as_array()
            .ok_or("GS compiled source inventory")?
        {
            let file = entry["file"].as_str().ok_or("native file")?;
            if ![
                "pcsx2/GS/Renderers/Common/GSRenderer.cpp",
                "pcsx2/GS/GSDump.cpp",
                "pcsx2/GSDumpReplayer.cpp",
                "pcsx2-libretro/Main.cpp",
            ]
            .contains(&file)
                || entry["sourceSha256"] != hash(&candidate.join("sources").join(file))?
            {
                return Err("GS compiled native source drift".into());
            }
        }
    }
    if runner {
        if candidate_binding["role"] != "pinned-runner-with-declared-matched-common-repairs"
            || candidate_binding["libretroMainLinked"] != false
            || common_binding["baselineInventory"] != composition
            || common_binding["extraPatchSha256"]
                != hash(Path::new("patches/armsx2/0008-development-gs-replay.patch"))?
            || common_binding["glueHeaderSha256"]
                != hash(Path::new("patches/armsx2/gs-replay/GSReplay.h"))?
            || candidate_binding["runnerPatchSha256"]
                != hash(Path::new("patches/armsx2/0009-development-gs-runner.patch"))?
            || candidate_binding["runnerBridgeSha256"]
                != hash(Path::new("patches/armsx2/gs-replay/RunnerBridge.inc"))?
        {
            return Err("Pinned-runner common/source binding differs".into());
        }
        for entry in candidate_binding["compiledRunner"]
            .as_array()
            .ok_or("runner source inventory")?
        {
            let file = entry["source"].as_str().ok_or("runner source name")?;
            if ![
                "pcsx2-gsrunner/Main.cpp",
                "pcsx2-gsrunner/GSLadder.cpp",
                "pcsx2-gsrunner/GSReplayPayload.cpp",
                "pcsx2-gsrunner/RenderDocCapture.cpp",
            ]
            .contains(&file)
            {
                return Err("runner source set".into());
            }
            let path = if file.ends_with("Main.cpp") {
                candidate.join("sources").join(file)
            } else {
                Path::new("vendor/armsx2").join(file)
            };
            if entry["sourceSha256"] != hash(&path)? {
                return Err("runner source drift".into());
            }
        }
    }
    let native_sha = if replay {
        candidate_binding["nativeSha256"]
            .as_str()
            .ok_or("GS native hash")?
    } else {
        NATIVE
    };
    let symbols_sha = if replay {
        candidate_binding["symbolsSha256"]
            .as_str()
            .ok_or("GS symbols hash")?
    } else {
        SYMBOLS
    };
    let native_id = if replay {
        candidate_binding[if runner { "notes" } else { "symbolNotes" }]
            .as_str()
            .and_then(|s| s.lines().find_map(|l| l.trim().strip_prefix("Build ID: ")))
            .ok_or("GS native BuildID")?
    } else {
        NATIVE_ID
    };
    checked(&player, APK)?;
    checked(&native, native_sha)?;
    checked(&symbols, symbols_sha)?;
    checked(&required("PSX_FROZEN_SOURCE_ARCHIVE")?, SOURCE)?;
    if member_hash(&player, &["lib/arm64-v8a/libarmsx2.so"])? != BASELINE_NATIVE {
        return Err("Frozen player engine member differs".into());
    }
    let manifest: serde_json::Value = serde_json::from_slice(&output(&mut command(
        "unzip",
        [
            OsStr::new("-p"),
            player.as_os_str(),
            OsStr::new("assets/development-observer.json"),
        ],
    ))?)?;
    if manifest["projectRevision"] != PLAYER
        || manifest["mode"] != "producer-local-output-sync"
        || manifest["commonRendererObserver"] != 1
    {
        return Err("Wrong frozen control mode".into());
    }
    for (key, path) in [
        (
            "normalAdapterSha256",
            "0001-host-pause-and-card-flush.patch",
        ),
        ("streamRepairSha256", "0004-stream-buffer-failure.patch"),
        (
            "observerPatchSha256",
            "0002-development-graphics-observer.patch",
        ),
        (
            "matchedPatchSha256",
            "0003-development-matched-reader.patch",
        ),
        (
            "streamObserverSha256",
            "0005-development-stream-observer.patch",
        ),
        (
            "creatorOutputPatchSha256",
            "0006-development-creator-output.patch",
        ),
        (
            "commonRendererPatchSha256",
            "0007-development-common-results.patch",
        ),
    ] {
        let current = hash(&Path::new("patches/armsx2").join(path))?;
        let baseline = if key == "streamRepairSha256" {
            "18bb6fe213db13ecba1e8b6106e201401f3aac7d50aa977b2a13488dc8699ea7"
        } else {
            &current
        };
        let hashes = if replay {
            &common_binding["baselineInventory"]["nativePatchHashes"]
        } else {
            &candidate_binding["nativePatchHashes"]
        };
        if manifest[key] != baseline || hashes[path] != current {
            return Err("Baseline or repaired native patch drift".into());
        }
    }
    let stage = root.join(format!(
        "build/{}-{}",
        if runner {
            "gs-runner"
        } else if replay {
            "gs-replay"
        } else {
            "minimal-host"
        },
        &revision[..12]
    ));
    if stage.exists() {
        return Err(format!("Existing wrapper stage retained: {}", stage.display()).into());
    }
    fs::create_dir(&stage)?;
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700))?;
    let (_, ndk, build) = tools()?;
    let sdk = required("ANDROID_SDK_ROOT")?;
    let jar = sdk
        .join("platforms")
        .join(env::var("ANDROID_PLATFORM").unwrap_or_else(|_| "android-37.0".into()))
        .join("android.jar");
    let bin = ndk.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    let native_strip = stage.join("libarmsx2-independent-strip.so");
    execute(&mut command(
        bin.join("llvm-strip"),
        [
            OsStr::new("--strip-unneeded"),
            OsStr::new("-o"),
            native_strip.as_os_str(),
            symbols.as_os_str(),
        ],
    ))?;
    if hash(&native_strip)? != native_sha {
        return Err("Native symbol-to-package independent strip differs".into());
    }
    let mut cargo = command(
        "cargo",
        [
            "build",
            "-p",
            "psx-android",
            "--target",
            "aarch64-linux-android",
            "--release",
            "--locked",
            "--offline",
            "--no-default-features",
            "--features",
            if replay {
                "gs-replay"
            } else {
                "producer-local-output-sync"
            },
        ],
    );
    cargo
        .env("PSX_MINIMAL_RENDERER_HOST", "1")
        .env("PSX_MINIMAL_SOURCE_REVISION", &revision)
        .env(
            "CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER",
            bin.join("aarch64-linux-android28-clang"),
        )
        .env(
            "CC_aarch64_linux_android",
            bin.join("aarch64-linux-android28-clang"),
        )
        .env(
            "CXX_aarch64_linux_android",
            bin.join("aarch64-linux-android28-clang++"),
        )
        .env("AR_aarch64_linux_android", bin.join("llvm-ar"));
    if replay {
        cargo.env("PSX_GS_REPLAY_NATIVE_SHA", native_sha);
    }
    execute(&mut cargo)?;
    let rust_symbols = stage.join("libpsx-symbols.so");
    fs::copy(
        "target/aarch64-linux-android/release/libpsx.so",
        &rust_symbols,
    )?;
    let package = stage.join("package");
    fs::create_dir_all(package.join("lib/arm64-v8a"))?;
    let rust = package.join("lib/arm64-v8a/libpsx.so");
    fs::copy(&rust_symbols, &rust)?;
    execute(&mut command(
        bin.join("llvm-strip"),
        [OsStr::new("--strip-unneeded"), rust.as_os_str()],
    ))?;
    let independent_strip = stage.join("libpsx-independent-strip.so");
    execute(&mut command(
        bin.join("llvm-strip"),
        [
            OsStr::new("--strip-unneeded"),
            OsStr::new("-o"),
            independent_strip.as_os_str(),
            rust_symbols.as_os_str(),
        ],
    ))?;
    if hash(&independent_strip)? != hash(&rust)? {
        return Err("Independent Rust symbol-to-package strip differs".into());
    }
    fs::copy(&native, package.join("lib/arm64-v8a/libarmsx2.so"))?;
    let asset_dir = stage.join("assets");
    fs::create_dir(&asset_dir)?;
    let asset_zip = asset_dir.join("frozen-engine-assets.zip");
    // Copy compressed members directly; avoid expanding thousands of notices.
    execute(&mut command(
        "zip",
        [
            OsStr::new("-q"),
            OsStr::new("-U"),
            player.as_os_str(),
            OsStr::new("assets/ps2/*"),
            OsStr::new("assets/notices/*"),
            OsStr::new("--out"),
            asset_zip.as_os_str(),
        ],
    ))?;
    let expected = assets(entries(&player)?);
    let actual = entries(&asset_zip)?;
    if expected.is_empty()
        || expected != actual
        || member_hash(&player, &["assets/ps2/*", "assets/notices/*"])?
            != member_hash(&asset_zip, &["assets/ps2/*", "assets/notices/*"])?
    {
        return Err("Frozen resources/notices copy differs".into());
    }
    execute(&mut command(
        "unzip",
        [OsStr::new("-tq"), asset_zip.as_os_str()],
    ))?;
    let mut packaged_native = candidate_binding.clone();
    packaged_native
        .as_object_mut()
        .ok_or("Native manifest object required")?
        .remove("proof");
    let binding = json!({"developmentOnly":true,"minimalHost":1,"experiment":if replay {"GS-REPLAY-1"} else {"coherent-mapping"},"projectRevision":revision,"nativeSourceTarget":revision,"frozenPlayerRevision":PLAYER,"frozenPlayerApkSha256":APK,"baselineNativeSha256":BASELINE_NATIVE,"nativeSha256":native_sha,"nativeSymbolsSha256":symbols_sha,"baselineCompleteSourceSha256":SOURCE,"nativeBuildId":native_id,"nativeRepair":"persistent-map-coherent","mode":"producer-local-output-sync","frozenEngineAssetsSha256":hash(&asset_zip)?,"baselineObserverManifest":manifest,"nativeManifest":packaged_native,"noNativeRebuild":false,"packagingRebuildsNative":false,"ordinaryApplicationLogic":false});
    fs::write(
        asset_dir.join("minimal-host.json"),
        serde_json::to_vec_pretty(&binding)?,
    )?;
    let unsigned = stage.join("unsigned.apk");
    execute(&mut command(
        build.join("aapt2"),
        [
            OsStr::new("link"),
            OsStr::new("--debug-mode"),
            OsStr::new("-I"),
            jar.as_os_str(),
            OsStr::new("--manifest"),
            OsStr::new("android/MinimalHostManifest.xml"),
            OsStr::new("-A"),
            asset_dir.as_os_str(),
            OsStr::new("-o"),
            unsigned.as_os_str(),
        ],
    ))?;
    execute(
        command(
            "zip",
            [
                OsStr::new("-q"),
                OsStr::new("-0"),
                unsigned.as_os_str(),
                OsStr::new("lib/arm64-v8a/libpsx.so"),
                OsStr::new("lib/arm64-v8a/libarmsx2.so"),
            ],
        )
        .current_dir(&package),
    )?;
    let aligned = stage.join("aligned.apk");
    execute(&mut command(
        build.join("zipalign"),
        [
            OsStr::new("-f"),
            OsStr::new("-P"),
            OsStr::new("16"),
            OsStr::new("4"),
            unsigned.as_os_str(),
            aligned.as_os_str(),
        ],
    ))?;
    let apk = stage.join("psx-minimal-host.apk");
    let key = Path::new(".local/signing/development.p12");
    if !key.is_file() {
        return Err("Existing signing identity required".into());
    }
    execute(&mut command(
        build.join("apksigner"),
        [
            OsStr::new("sign"),
            OsStr::new("--ks"),
            key.as_os_str(),
            OsStr::new("--ks-key-alias"),
            OsStr::new("psx-development"),
            OsStr::new("--ks-pass"),
            OsStr::new("pass:android"),
            OsStr::new("--out"),
            apk.as_os_str(),
            aligned.as_os_str(),
        ],
    ))?;
    let signer = String::from_utf8(output(&mut command(
        build.join("apksigner"),
        [
            OsStr::new("verify"),
            OsStr::new("--verbose"),
            OsStr::new("--print-certs"),
            apk.as_os_str(),
        ],
    ))?)?;
    if !signer.contains(CERT) {
        return Err("Wrapper signing identity differs".into());
    }
    execute(&mut command(
        build.join("zipalign"),
        [
            OsStr::new("-c"),
            OsStr::new("-P"),
            OsStr::new("16"),
            OsStr::new("4"),
            apk.as_os_str(),
        ],
    ))?;
    if member_hash(&apk, &["assets/frozen-engine-assets.zip"])? != hash(&asset_zip)?
        || member_hash(&apk, &["lib/arm64-v8a/libarmsx2.so"])? != native_sha
        || member_hash(&apk, &["lib/arm64-v8a/libpsx.so"])? != hash(&rust)?
        || entries(&apk)?
            .iter()
            .any(|v| v.ends_with(".dex") || v.contains("libswanstation"))
    {
        return Err("Wrapper payload mismatch".into());
    }
    for p in [&rust, &package.join("lib/arm64-v8a/libarmsx2.so")] {
        let headers = String::from_utf8(output(&mut command(
            bin.join("llvm-readelf"),
            [OsStr::new("-hlW"), p.as_os_str()],
        ))?)?;
        if !headers.contains("AArch64") {
            return Err("Wrong wrapper ABI".into());
        }
        for l in headers
            .lines()
            .filter(|l| l.trim_start().starts_with("LOAD"))
        {
            if u64::from_str_radix(
                l.split_whitespace()
                    .last()
                    .ok_or("ELF alignment")?
                    .trim_start_matches("0x"),
                16,
            )? < 16384
            {
                return Err("Wrapper ELF alignment below16KiB".into());
            }
        }
    }
    let exports = String::from_utf8(output(&mut command(
        bin.join("llvm-readelf"),
        [
            OsStr::new("--dyn-syms"),
            OsStr::new("--wide"),
            rust.as_os_str(),
        ],
    ))?)?;
    if !exports
        .lines()
        .any(|l| l.contains("ANativeActivity_onCreate") && !l.contains(" UND "))
        || exports.contains("Java_com_machinerhythm_psx_PsxActivity_")
    {
        return Err("Wrapper entrypoint/independence exports differ".into());
    }
    let native_exports = String::from_utf8(output(&mut command(
        bin.join("llvm-readelf"),
        [
            OsStr::new("--dyn-syms"),
            OsStr::new("--wide"),
            native.as_os_str(),
        ],
    ))?)?;
    let required_exports: &[&str] = if runner {
        &[
            "retro_psx_gsrunner_entry",
            "retro_psx_runner_prepare",
            "retro_psx_runner_service",
            "retro_psx_runner_stop",
            "retro_psx_runner_retired",
            "retro_psx_gs_replay_configure",
            "retro_psx_gs_replay_clear",
            "retro_psx_observer_set_callback",
            "retro_psx_observer_set_output_callback",
            "retro_psx_observer_set_result_callback",
            "retro_psx_observer_set_stream_callback",
            "retro_psx_runner_set_failure",
        ]
    } else {
        &[
            "retro_run",
            "retro_load_game",
            "retro_psx_ready",
            "retro_psx_set_paused",
            "retro_psx_set_speed",
            "retro_psx_set_frame_callback",
            "retro_psx_observer_set_callback",
            "retro_psx_observer_set_stream_callback",
            "retro_psx_observer_set_output_callback",
            "retro_psx_observer_set_result_callback",
        ]
    };
    for &symbol in required_exports {
        if !native_exports
            .lines()
            .any(|l| l.split_whitespace().last() == Some(symbol) && !l.contains(" UND "))
        {
            return Err(format!("Missing frozen export {symbol}").into());
        }
    }
    let native_notes = String::from_utf8(output(&mut command(
        bin.join("llvm-readelf"),
        [OsStr::new("-n"), native.as_os_str()],
    ))?)?;
    if !native_notes.contains(native_id) {
        return Err("Candidate native build ID differs".into());
    }
    let rust_notes = String::from_utf8(output(&mut command(
        bin.join("llvm-readelf"),
        [OsStr::new("-n"), rust_symbols.as_os_str()],
    ))?)?;
    let rust_id = rust_notes
        .lines()
        .find_map(|l| l.trim().strip_prefix("Build ID: "))
        .unwrap_or("absent");
    let manifest_dump = String::from_utf8(output(&mut command(
        build.join("aapt2"),
        [OsStr::new("dump"), OsStr::new("badging"), apk.as_os_str()],
    ))?)?;
    if !manifest_dump.contains("package: name='com.machinerhythm.psx.rendererreference'")
        || !manifest_dump.contains("launchable-activity: name='android.app.NativeActivity'")
        || manifest_dump.contains("uses-permission:")
    {
        return Err("Diagnostic package identity/permissions differ".into());
    }
    checked(&player, APK)?;
    checked(&native, native_sha)?;
    checked(&symbols, symbols_sha)?;
    let receipt = json!({"manifest":binding,"apkSha256":hash(&apk)?,"apkBytes":fs::metadata(&apk)?.len(),"rustPackagedSha256":hash(&rust)?,"rustSymbolsSha256":hash(&rust_symbols)?,"rustBuildId":rust_id,"independentStripSha256":hash(&independent_strip)?,"signerSha256":CERT,"expectedAssets":expected,"stage":stage,"sourceArchivePending":true});
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(stage.join("binding.json"))?;
    f.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
    f.sync_all()?;
    for p in [&apk, &rust, &rust_symbols, &asset_zip] {
        fs::File::open(p)?.sync_all()?;
    }
    fs::File::open(&stage)?.sync_all()?;
    println!("{}", serde_json::to_string_pretty(&receipt)?);
    Ok(())
}
