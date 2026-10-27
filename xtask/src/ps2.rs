//! Reproducible source preparation and original donor Android build operations.
use super::*;
pub const REVISION: &str = "83fe4e293f71ab5587ef81b06143725bf8c6a30b";
const PATCHES_RESOURCE: &str = "platforms/android/app/src/main/assets/resources/patches.zip";
const PATCHES_SHA256: &str = "b8ab00d2d68376cc15b792d5776a150c678a2c0ea8fd896ed20492aecc6643db";
pub fn resources() -> Result<()> {
    verify()?;
    let root = Path::new("vendor/armsx2/bin/resources");
    let destination = PathBuf::from(artifact("build/apk/assets/ps2/resources"));
    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    let mut index = String::new();
    for file in files {
        let name = file.strip_prefix(root)?;
        let target = destination.join(name);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(&file, &target)?;
        index.push_str(&format!("{}\t{}\n", name.display(), hash(&file)?));
    }
    // The donor's Android archive lives outside bin/resources. Use its exact
    // pinned copy, never the CMake floating-release download or an empty ZIP.
    let patches = Path::new("vendor/armsx2").join(PATCHES_RESOURCE);
    if hash(&patches)? != PATCHES_SHA256 {
        return Err("PS2 built-in patches differ from the reviewed donor resource".into());
    }
    execute(&mut command(
        "unzip",
        [OsStr::new("-tqq"), patches.as_os_str()],
    ))?;
    fs::copy(patches, destination.join("patches.zip"))?;
    index.push_str(&format!("patches.zip\t{PATCHES_SHA256}\n"));
    fs::write(artifact("build/apk/assets/ps2/resources.tsv"), index)?;
    Ok(())
}
pub fn verify_resources(apk: &Path) -> Result<()> {
    let patches = command(
        "unzip",
        [
            OsStr::new("-p"),
            apk.as_os_str(),
            OsStr::new("assets/ps2/resources/patches.zip"),
        ],
    )
    .output()?;
    if !patches.status.success()
        || format!("{:x}", Sha256::digest(&patches.stdout)) != PATCHES_SHA256
    {
        return Err("The APK lacks the exact pinned PS2 built-in patches archive".into());
    }
    let index = output(&mut command(
        "unzip",
        [
            OsStr::new("-p"),
            apk.as_os_str(),
            OsStr::new("assets/ps2/resources.tsv"),
        ],
    ))?;
    if !index
        .lines()
        .any(|line| line == format!("patches.zip\t{PATCHES_SHA256}"))
    {
        return Err("The APK PS2 resource index does not identify its built-in patches".into());
    }
    Ok(())
}
const PATCH: &str = "patches/armsx2/0001-host-pause-and-card-flush.patch";
const OBSERVER_PATCH: &str = "patches/armsx2/0002-development-graphics-observer.patch";
const MATCHED_PATCH: &str = "patches/armsx2/0003-development-matched-reader.patch";
const STREAM_PATCH: &str = "patches/armsx2/0004-stream-buffer-failure.patch";
const STREAM_OBSERVER_PATCH: &str = "patches/armsx2/0005-development-stream-observer.patch";
const CREATOR_OUTPUT_PATCH: &str = "patches/armsx2/0006-development-creator-output.patch";
const COMMON_RESULTS_PATCH: &str = "patches/armsx2/0007-development-common-results.patch";
const VULKAN_PATCH: &str = "patches/armsx2/0010-vulkan-frame-completion.patch";
const VULKAN_OVERLAP_PATCH: &str = "patches/armsx2/0011-vulkan-producer-overlap.patch";
/// Build-only reuse of an independently bound incremental candidate. This
/// changes neither runtime settings nor the ordinary zero-config interface.
pub fn verified_vulkan_artifact() -> Result<Option<PathBuf>> {
    let Some(stage) = env::var_os("PSX_VULKAN_NATIVE_STAGE") else {
        return Ok(None);
    };
    if observer_enabled() {
        return Err("Product Vulkan reuse cannot select a diagnostic mode".into());
    }
    let stage = PathBuf::from(stage).canonicalize()?;
    let build = PathBuf::from(workspace::path("build")).canonicalize()?;
    if stage.parent() != Some(build.as_path()) {
        return Err("Vulkan native stage outside admitted output".into());
    }
    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(stage.join("native-binding.json"))?)?;
    let native = stage.join("libarmsx2-packaged.so");
    if binding["upstream"] != REVISION
        || binding["patchSha256"] != hash(Path::new(VULKAN_PATCH))?
        || binding["overlapPatchSha256"] != hash(Path::new(VULKAN_OVERLAP_PATCH))?
        || binding["independentStrip"] != true
        || binding["retainedInactiveGlObservationCode"] != false
        || binding["nativeSha256"] != hash(&native)?
        || binding["symbolsSha256"] != hash(&stage.join("libarmsx2-symbols.so"))?
        || binding["productionPatches"]
            != serde_json::json!([
                "0001-host-pause-and-card-flush.patch",
                "0004-stream-buffer-failure.patch",
                "0010-vulkan-frame-completion.patch",
                "0011-vulkan-producer-overlap.patch"
            ])
    {
        return Err("Vulkan native candidate/binding differs".into());
    }
    for (file, recorded) in binding["composedSourceHashes"]
        .as_object()
        .ok_or("Vulkan composed source inventory")?
    {
        let relative = Path::new(file);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || *recorded != hash(&stage.join("sources").join(relative))?
        {
            return Err("Vulkan composed source changed".into());
        }
    }
    Ok(Some(native))
}
pub fn observer_mode() -> Result<&'static str> {
    match env::var("PSX_GRAPHICS_OBSERVER_MODE").as_deref() {
        Err(_) | Ok("normal") => Ok("normal"),
        Ok("no-game-image-reader") if observer_enabled() => Ok("no-game-image-reader"),
        Ok("producer-local-output-sync") if observer_enabled() => Ok("producer-local-output-sync"),
        _ => Err("Invalid development observer mode".into()),
    }
}
pub fn observer_enabled() -> bool {
    env::var("PSX_GRAPHICS_OBSERVER").as_deref() == Ok("1")
}
pub fn warm_observer_cache() -> bool {
    if !observer_enabled()
        || !Path::new("build/armsx2-android/bin/armsx2_libretro.so").is_file()
        || !Path::new("build/armsx2-deps/lib/libshaderc_combined.a").is_file()
    {
        return false;
    }
    let Some(source) =
        env::var_os("PSX_PS2_CORE_SOURCE").and_then(|path| PathBuf::from(path).canonicalize().ok())
    else {
        return false;
    };
    let Some(ndk) =
        env::var_os("ANDROID_NDK_HOME").and_then(|p| PathBuf::from(p).canonicalize().ok())
    else {
        return false;
    };
    let Ok(prefix) = Path::new("build/armsx2-deps").canonicalize() else {
        return false;
    };
    fs::read_to_string("build/armsx2-android/CMakeCache.txt").is_ok_and(|cache| {
        let settings = [
            ("CMAKE_HOME_DIRECTORY", source.display().to_string()),
            (
                "CMAKE_TOOLCHAIN_FILE",
                ndk.join("build/cmake/android.toolchain.cmake")
                    .display()
                    .to_string(),
            ),
            ("CMAKE_INSTALL_PREFIX", prefix.display().to_string()),
            ("CMAKE_BUILD_TYPE", "Release".into()),
            ("ANDROID_ABI", "arm64-v8a".into()),
            ("ANDROID_PLATFORM", "android-28".into()),
            ("ANDROID_STL", "c++_static".into()),
            ("ENABLE_LIBRETRO", "ON".into()),
            ("USE_GLES", "ON".into()),
            ("DISABLE_ADVANCE_SIMD", "TRUE".into()),
            ("ARMSX2_ANDROID_HOST_PAGE_SIZE", "0x1000".into()),
            ("CMAKE_DISABLE_PRECOMPILE_HEADERS", "ON".into()),
            ("ARMSX2_EMBED_PATCHES", "OFF".into()),
            (
                "CMAKE_SHARED_LINKER_FLAGS",
                format!(
                    "-Wl,-z,max-page-size=16384 -Wl,--version-script={}",
                    source.join("pcsx2-libretro/link.T").display()
                ),
            ),
        ];
        settings.iter().all(|(key, value)| {
            cache.lines().any(|line| {
                line.split_once('=').is_some_and(|(k, v)| {
                    k.split_once(':').is_some_and(|(k, _)| k == *key) && v == value
                })
            })
        })
    }) && output(command("git", ["rev-parse", "HEAD"]).current_dir(source))
        .is_ok_and(|head| head.trim() == REVISION)
}
fn patch_diff(source: &Path, patches: &[&str]) -> Result<Vec<u8>> {
    // A separate temporary index derives the exact combined patch without
    // changing the retained donor's real index or accepting unknown edits.
    let index = env::temp_dir().join(format!("psx-native-source-{}.index", std::process::id()));
    if index.exists() {
        return Err("Unfinished observer index is preserved; inspect before reuse".into());
    }
    let git = |args: &[&str]| {
        let mut c = command("git", args);
        c.current_dir(source).env("GIT_INDEX_FILE", &index);
        c
    };
    execute(&mut git(&["read-tree", REVISION]))?;
    for path in patches {
        execute(git(&["apply", "--cached"]).arg(Path::new(path).canonicalize()?))?;
    }
    let output = git(&["diff", "--cached", "--binary"]).output()?;
    if !output.status.success() {
        return Err("Could not derive observer source binding".into());
    }
    fs::remove_file(index)?;
    Ok(output.stdout)
}
fn active_patches() -> Vec<&'static str> {
    let mut patches = vec![PATCH, STREAM_PATCH];
    if observer_enabled() {
        patches.extend([OBSERVER_PATCH, MATCHED_PATCH, STREAM_OBSERVER_PATCH]);
        if env::var("PSX_GRAPHICS_OBSERVER_MODE").as_deref() == Ok("producer-local-output-sync") {
            patches.push(CREATOR_OUTPUT_PATCH);
            patches.push(COMMON_RESULTS_PATCH);
        }
    } else {
        patches.push(VULKAN_PATCH);
        patches.push(VULKAN_OVERLAP_PATCH);
    }
    patches
}
pub fn verify() -> Result<()> {
    let source = Path::new("vendor/armsx2");
    if hash(&source.join("COPYING.GPLv3"))? != CORE_LICENSE {
        return Err("PS2 upstream license differs from its reviewed copy".into());
    }
    if source.join(".psx-source-manifest").exists() {
        verify_manifest(source, REVISION)?;
    } else if output(command("git", ["rev-parse", "HEAD"]).current_dir(source))?.trim() != REVISION
        || !output(command("git", ["diff", "--name-only"]).current_dir(source))?
            .trim()
            .is_empty()
    {
        return Err("PS2 upstream revision or tracked source differs from its pin".into());
    }
    Ok(())
}
pub fn dependency_root() -> PathBuf {
    env::var_os("PSX_PS2_DEPENDENCY_SOURCES")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if Path::new("vendor/armsx2-dependencies/source").is_dir() {
                PathBuf::from("vendor/armsx2-dependencies/source")
            } else {
                PathBuf::from("build/armsx2-sources")
            }
        })
}
fn locked_sources(root: &Path) -> Result<()> {
    for line in fs::read_to_string("vendor/armsx2-dependencies/revisions.tsv")?.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 {
            return Err("Invalid PS2 dependency lock".into());
        }
        let path = root.join(fields[0]);
        if path.join(".psx-source-manifest").exists() {
            verify_manifest(&path, fields[2])?;
            continue;
        }
        if !path.join(".git").exists() {
            fs::create_dir_all(&path)?;
            execute(command("git", ["init"]).current_dir(&path))?;
            execute(command("git", ["remote", "add", "origin", fields[1]]).current_dir(&path))?;
            execute(
                command("git", ["fetch", "--depth", "1", "origin", fields[2]]).current_dir(&path),
            )?;
            execute(command("git", ["checkout", "--detach", "FETCH_HEAD"]).current_dir(&path))?;
        }
        if output(command("git", ["rev-parse", "HEAD"]).current_dir(&path))?.trim() != fields[2]
            || !output(command("git", ["diff", "--name-only"]).current_dir(&path))?.is_empty()
        {
            return Err(format!("PS2 dependency differs from its lock: {}", fields[0]).into());
        }
    }
    Ok(())
}
pub fn build() -> Result<()> {
    observer_mode()?;
    verify()?;
    let (_, ndk, _) = tools()?;
    let sources = dependency_root();
    locked_sources(&sources)?;
    fs::create_dir_all("build/armsx2-deps")?;
    let prefix = Path::new("build/armsx2-deps").canonicalize()?;
    let jobs = env::var("PSX_BUILD_JOBS").unwrap_or("2".into());
    let _: u16 = jobs.parse()?;
    let ndk = ndk.canonicalize()?;
    let toolchain = format!(
        "-DCMAKE_TOOLCHAIN_FILE={}",
        ndk.join("build/cmake/android.toolchain.cmake").display()
    );
    let common = vec![
        toolchain,
        "-DANDROID_ABI=arm64-v8a".into(),
        "-DANDROID_PLATFORM=android-28".into(),
        "-DANDROID_STL=c++_static".into(),
        "-DCMAKE_POSITION_INDEPENDENT_CODE=ON".into(),
        format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()),
        format!("-DCMAKE_PREFIX_PATH={}", prefix.display()),
        format!("-DCMAKE_FIND_ROOT_PATH={}", prefix.display()),
        "-DCMAKE_BUILD_TYPE=Release".into(),
        "-DBUILD_SHARED_LIBS=OFF".into(),
    ];
    let recipes: [(&str, &str, &str, &[&str]); 12] = [
        (
            "libpng",
            "libpng",
            "build",
            &[
                "-DPNG_SHARED=OFF",
                "-DPNG_STATIC=ON",
                "-DPNG_TESTS=OFF",
                "-DPNG_TOOLS=OFF",
                "-DPNG_FRAMEWORK=OFF",
            ],
        ),
        (
            "libjpeg-turbo",
            "libjpeg-turbo",
            "build",
            &[
                "-DENABLE_SHARED=OFF",
                "-DENABLE_STATIC=ON",
                "-DWITH_TURBOJPEG=OFF",
            ],
        ),
        (
            "zstd",
            "zstd/build/cmake",
            "b",
            &[
                "-DZSTD_BUILD_SHARED=OFF",
                "-DZSTD_BUILD_STATIC=ON",
                "-DZSTD_BUILD_PROGRAMS=OFF",
                "-DZSTD_BUILD_TESTS=OFF",
            ],
        ),
        (
            "lz4",
            "lz4/build/cmake",
            "b",
            &["-DLZ4_BUILD_CLI=OFF", "-DLZ4_BUILD_LEGACY_LZ4C=OFF"],
        ),
        (
            "libwebp",
            "libwebp",
            "build",
            &[
                "-DWEBP_BUILD_ANIM_UTILS=OFF",
                "-DWEBP_BUILD_CWEBP=OFF",
                "-DWEBP_BUILD_DWEBP=OFF",
                "-DWEBP_BUILD_GIF2WEBP=OFF",
                "-DWEBP_BUILD_IMG2WEBP=OFF",
                "-DWEBP_BUILD_VWEBP=OFF",
                "-DWEBP_BUILD_WEBPINFO=OFF",
                "-DWEBP_BUILD_WEBPMUX=OFF",
                "-DWEBP_BUILD_EXTRAS=OFF",
            ],
        ),
        (
            "sdl3",
            "sdl3",
            "build",
            &[
                "-DSDL_SHARED=OFF",
                "-DSDL_STATIC=ON",
                "-DSDL_TESTS=OFF",
                "-DSDL_EXAMPLES=OFF",
            ],
        ),
        (
            "freetype",
            "freetype",
            "build",
            &[
                "-DFT_DISABLE_HARFBUZZ=ON",
                "-DFT_DISABLE_BROTLI=ON",
                "-DFT_DISABLE_PNG=ON",
                "-DFT_DISABLE_ZLIB=ON",
                "-DFT_DISABLE_BZIP2=ON",
            ],
        ),
        (
            "plutovg",
            "plutovg",
            "build",
            &["-DPLUTOVG_BUILD_EXAMPLES=OFF"],
        ),
        (
            "plutosvg",
            "plutosvg",
            "build",
            &[
                "-DPLUTOSVG_ENABLE_FREETYPE=ON",
                "-DPLUTOSVG_BUILD_EXAMPLES=OFF",
            ],
        ),
        (
            "curl",
            "curl",
            "build",
            &[
                "-DCURL_ENABLE_SSL=OFF",
                "-DBUILD_CURL_EXE=OFF",
                "-DCURL_DISABLE_LDAP=ON",
                "-DCURL_DISABLE_LDAPS=ON",
                "-DCURL_USE_LIBPSL=OFF",
                "-DBUILD_TESTING=OFF",
                "-DBUILD_EXAMPLES=OFF",
            ],
        ),
        (
            "libpcap",
            "libpcap",
            "build",
            &[
                "-DDISABLE_DBUS=ON",
                "-DDISABLE_RDMA=ON",
                "-DDISABLE_DAG=ON",
                "-DDISABLE_SEPTEL=ON",
                "-DDISABLE_SNF=ON",
                "-DDISABLE_TC=ON",
                "-DDISABLE_NETMAP=ON",
                "-DDISABLE_DPDK=ON",
                "-DDISABLE_BLUETOOTH=ON",
                "-DDISABLE_LINUX_USBMON=ON",
                "-DBUILD_WITH_LIBNL=OFF",
                "-DENABLE_REMOTE=OFF",
            ],
        ),
        (
            "shaderc",
            "shaderc",
            "b",
            &[
                "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
                "-DSHADERC_SKIP_TESTS=ON",
                "-DSHADERC_SKIP_EXAMPLES=ON",
                "-DSHADERC_SKIP_COPYRIGHT_CHECK=ON",
            ],
        ),
    ];
    // Qualified warm observation builds retain the existing locked dependency
    // outputs. Source pins are checked above; missing links still fail normally.
    // Never run their configure/install phases on a constrained source volume.
    let reuse_dependencies = warm_observer_cache();
    if env::var_os("PSX_BUILD_WORKSPACE").is_some() && !reuse_dependencies {
        return Err(
            "External build requires the verified warm native dependency cache; no cold fallback"
                .into(),
        );
    }
    for (name, src, bld, extra) in recipes.into_iter().filter(|_| !reuse_dependencies) {
        let source = sources.join(src);
        let target = sources.join(name).join(bld);
        execute(
            command(
                "cmake",
                [
                    OsStr::new("-S"),
                    source.as_os_str(),
                    OsStr::new("-B"),
                    target.as_os_str(),
                    OsStr::new("-G"),
                    OsStr::new("Ninja"),
                ],
            )
            .args(&common)
            .args(extra),
        )?;
        let mut build = command(
            "cmake",
            [
                OsStr::new("--build"),
                target.as_os_str(),
                OsStr::new("--parallel"),
                OsStr::new(&jobs),
            ],
        );
        if name == "shaderc" {
            build.args(["--target", "shaderc_combined"]);
        }
        execute(&mut build)?;
        if name == "shaderc" {
            fs::copy(
                target.join("libshaderc/libshaderc_combined.a"),
                prefix.join("lib/libshaderc_combined.a"),
            )?;
            copy_tree(
                &source.join("libshaderc/include/shaderc"),
                &prefix.join("include/shaderc"),
            )?;
        } else {
            execute(&mut command(
                "cmake",
                [OsStr::new("--install"), target.as_os_str()],
            ))?;
        }
    }
    let upstream = Path::new("vendor/armsx2");
    if upstream.join(".psx-source-manifest").exists() {
        verify_manifest(upstream, REVISION)?;
    } else if output(command("git", ["rev-parse", "HEAD"]).current_dir(upstream))?.trim()
        != REVISION
    {
        return Err("PS2 core pin differs".into());
    }
    let source = env::var_os("PSX_PS2_CORE_SOURCE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("build/armsx2-source"));
    let patches = active_patches();
    if source.join(".git").exists() {
        if output(command("git", ["rev-parse", "HEAD"]).current_dir(&source))?.trim() != REVISION {
            return Err("PS2 qualification source pin differs".into());
        }
        let actual = command("git", ["diff", "HEAD", "--binary"])
            .current_dir(&source)
            .output()?
            .stdout;
        let mut recognized = None;
        let mut phases = vec![vec![], vec![PATCH], vec![PATCH, STREAM_PATCH]];
        if observer_enabled() {
            phases.extend([
                vec![PATCH, OBSERVER_PATCH],
                vec![PATCH, OBSERVER_PATCH, MATCHED_PATCH],
                vec![PATCH, STREAM_PATCH, OBSERVER_PATCH],
                vec![PATCH, STREAM_PATCH, OBSERVER_PATCH, MATCHED_PATCH],
                vec![
                    PATCH,
                    STREAM_PATCH,
                    OBSERVER_PATCH,
                    MATCHED_PATCH,
                    STREAM_OBSERVER_PATCH,
                ],
                vec![
                    PATCH,
                    STREAM_PATCH,
                    OBSERVER_PATCH,
                    MATCHED_PATCH,
                    STREAM_OBSERVER_PATCH,
                    CREATOR_OUTPUT_PATCH,
                ],
                patches.clone(),
            ]);
        } else {
            phases.extend([vec![PATCH, STREAM_PATCH, VULKAN_PATCH], patches.clone()]);
        }
        for phase in phases {
            if patch_diff(&source, &phase)? == actual {
                recognized = Some(phase);
                break;
            }
        }
        let have = recognized.ok_or("Qualification PS2 source has unknown changes; preserve it")?;
        for path in &patches {
            if !have.contains(path) {
                execute(
                    command("git", ["apply"])
                        .arg(Path::new(path).canonicalize()?)
                        .current_dir(&source),
                )?;
            }
        }
        let expected = patch_diff(&source, &patches)?;
        if command("git", ["diff", "HEAD", "--binary"])
            .current_dir(&source)
            .output()?
            .stdout
            != expected
        {
            return Err("Qualification PS2 source differs after recorded patches".into());
        }
    } else {
        let marker = source.join(".psx-source-pin");
        let mut binding = format!("{REVISION}\n");
        for patch in &patches {
            binding.push_str(&format!("{patch}:{}\n", hash(Path::new(patch))?));
        }
        if !source.exists() {
            if upstream.join(".psx-source-manifest").exists() {
                copy_tree(upstream, &source)?;
            } else {
                archive_into(upstream, REVISION, &source)?;
            }
            for patch in &patches {
                execute(
                    command("git", ["apply", "--directory"])
                        .arg(&source)
                        .arg(Path::new(patch).canonicalize()?),
                )?;
            }
            fs::write(&marker, &binding)?;
            // A marker alone would miss later edits in reusable source staging.
            let _ = fs::remove_file(source.join(".psx-source-manifest"));
            write_manifest(&source, REVISION)?;
        }
        if fs::read_to_string(marker)? != binding {
            return Err("PS2 source staging differs; use a new empty staging path".into());
        }
        verify_manifest(&source, REVISION)?;
    }
    let source = source.canonicalize()?;
    let export = format!(
        "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384 -Wl,--version-script={}",
        source.join("pcsx2-libretro/link.T").display()
    );
    if !reuse_dependencies {
        execute(
            command(
                "cmake",
                [
                    OsStr::new("-S"),
                    source.as_os_str(),
                    OsStr::new("-B"),
                    OsStr::new("build/armsx2-android"),
                    OsStr::new("-G"),
                    OsStr::new("Ninja"),
                ],
            )
            .args(&common)
            .args([
                "-DENABLE_LIBRETRO=ON",
                "-DENABLE_QT_UI=OFF",
                "-DENABLE_SDL_FRONTEND=OFF",
                "-DENABLE_GSRUNNER=OFF",
                "-DENABLE_TESTS=OFF",
                "-DENABLE_RECOMPILER_TEST_HOOKS=OFF",
                "-DENABLE_SETCAP=OFF",
                "-DUSE_BACKTRACE=OFF",
                "-DX11_API=OFF",
                "-DWAYLAND_API=OFF",
                "-DDISABLE_ADVANCE_SIMD=TRUE",
                "-DARMSX2_ANDROID_HOST_PAGE_SIZE=0x1000",
                "-DCMAKE_DISABLE_PRECOMPILE_HEADERS=ON",
                "-DUSE_GLES=ON",
                "-DARMSX2_EMBED_PATCHES=OFF",
                "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
                &export,
            ]),
        )?;
    }
    execute(&mut command(
        "cmake",
        [
            "--build",
            "build/armsx2-android",
            "--parallel",
            &jobs,
            "--target",
            "armsx2_libretro",
        ],
    ))?;
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &path)?;
        } else if entry.file_type()?.is_file() {
            fs::copy(entry.path(), path)?;
        }
    }
    Ok(())
}
pub fn archive_sources() -> Result<PathBuf> {
    verify()?;
    let root = dependency_root();
    let locks = fs::read_to_string("vendor/armsx2-dependencies/revisions.tsv")?;
    // Refuse an exported build cache before fetching anything or creating the
    // project stage. Such a cache can contain ignored native build output.
    for line in locks.lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 {
            return Err("Invalid PS2 dependency lock".into());
        }
        let path = root.join(fields[0]);
        if path.join(".psx-source-manifest").exists() && !path.join(".git").exists() {
            archive_repository(&path)?;
        }
    }
    locked_sources(&root)?;
    for line in locks.lines() {
        archive_repository(&root.join(line.split('\t').next().unwrap()))?;
    }
    Ok(root)
}

/// Exact upstream source, including nested dependency trees and original notices.
pub fn archive(staging: &Path, root: &Path) -> Result<()> {
    verify()?;
    let core = staging.join("vendor/armsx2");
    archive_into(Path::new("vendor/armsx2"), REVISION, &core)?;
    write_manifest(&core, REVISION)?;
    for line in fs::read_to_string("vendor/armsx2-dependencies/revisions.tsv")?.lines() {
        let f: Vec<_> = line.split('\t').collect();
        let target = staging.join("vendor/armsx2-dependencies/source").join(f[0]);
        archive_into(&root.join(f[0]), f[2], &target)?;
        write_manifest(&target, f[2])?;
    }
    Ok(())
}
fn write_manifest(root: &Path, revision: &str) -> Result<()> {
    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    let mut manifest = format!("{revision}\n");
    for file in files {
        manifest.push_str(&format!(
            "{}\t{}\n",
            hash(&file)?,
            file.strip_prefix(root)?.display()
        ));
    }
    fs::write(root.join(".psx-source-manifest"), manifest)?;
    Ok(())
}
fn verify_manifest(root: &Path, revision: &str) -> Result<()> {
    let manifest = fs::read_to_string(root.join(".psx-source-manifest"))?;
    let mut lines = manifest.lines();
    if lines.next() != Some(revision) {
        return Err("Corresponding PS2 source revision mismatch".into());
    }
    for line in lines {
        let (expected, name) = line.split_once('\t').ok_or("Invalid source manifest")?;
        if Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
            || hash(&root.join(name))? != expected
        {
            return Err("Corresponding PS2 source differs from its manifest".into());
        }
    }
    Ok(())
}
