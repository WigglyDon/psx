use sha2::{Digest, Sha256};
mod ps2;
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CORE_REV: &str = "e436e9b85374def58d211df7f78880f0f4a917b4";
const CORE_LICENSE: &str = "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903";
const FIRMWARE_REV: &str = "08f1b3e1b3289b27fda955f6105fc9e6afd9ffe6";
const FIRMWARE_SHA: &str = "c13794bc14de130c363786f45710d6e472c092070c0524ddede50f41f948bc45";
const FIRMWARE_SOURCE_SHA: &str =
    "407256c55987dcb098fe8dfb25cf2c0d41836be57d14c39ea0a1d266550e089d";
const FIRMWARE_BUILDER: &str = "ghcr.io/grumpycoders/pcsx-redux-build@sha256:d9ae6cbb23a5d0d98d8c3702cc6f512698ee1670bf8d59b236b2baabe99a66e4";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("Build failed: {e}");
            ExitCode::FAILURE
        }
    }
}
fn run() -> Result<()> {
    env::set_current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())?;
    match env::args().nth(1).as_deref() {
        Some("apk") => apk(),
        Some("verify") => verify_apk(),
        Some("notices") => notices(),
        Some("source") => source_bundle(),
        Some("firmware") => firmware(),
        Some("ps2") => ps2::build(),
        _ => {
            eprintln!("Usage: cargo xtask apk | verify | notices | source | firmware\nSet ANDROID_SDK_ROOT, ANDROID_NDK_HOME, ANDROID_BUILD_TOOLS (default 37.0.0), ANDROID_PLATFORM (default android-37.0). Compile SDK may exceed target API 36. Requires Java with javac module, cmake, ninja, zip, unzip, Rust Android target. Source bundles also use tar, gzip and cargo vendor. Firmware reproduction uses Podman and the pinned Linux amd64 compiler image.");
            Ok(())
        }
    }
}
fn command(
    program: impl AsRef<OsStr>,
    args: impl IntoIterator<Item = impl AsRef<OsStr>>,
) -> Command {
    let mut command = Command::new(program);
    command.args(args);
    command
}
fn execute(cmd: &mut Command) -> Result<()> {
    eprintln!("Running {}", cmd.get_program().to_string_lossy());
    if !cmd.status()?.success() {
        return Err(format!("{} failed", cmd.get_program().to_string_lossy()).into());
    }
    Ok(())
}
fn output(cmd: &mut Command) -> Result<String> {
    let result = cmd.output()?;
    if !result.status.success() {
        return Err(format!(
            "{}: {}",
            cmd.get_program().to_string_lossy(),
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(String::from_utf8(result.stdout)?)
}
fn required(name: &str) -> Result<PathBuf> {
    Ok(env::var_os(name).map(PathBuf::from).ok_or(format!(
        "Set {name} to your installed Android tool directory."
    ))?)
}
fn tools() -> Result<(PathBuf, PathBuf, PathBuf)> {
    let sdk = required("ANDROID_SDK_ROOT")?;
    let ndk = required("ANDROID_NDK_HOME")?;
    let build = sdk
        .join("build-tools")
        .join(env::var("ANDROID_BUILD_TOOLS").unwrap_or("37.0.0".into()));
    Ok((sdk, ndk, build))
}
fn hash(path: &Path) -> Result<String> {
    let mut input = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn archive_into(repository: &Path, revision: &str, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    let mut archive = command("git", ["archive", "--format=tar", revision])
        .current_dir(repository)
        .stdout(Stdio::piped())
        .spawn()?;
    let extracted = command(
        "tar",
        [
            OsStr::new("-xf"),
            OsStr::new("-"),
            OsStr::new("-C"),
            destination.as_os_str(),
        ],
    )
    .stdin(Stdio::from(
        archive.stdout.take().ok_or("Missing archive output")?,
    ))
    .status()?;
    let archived = archive.wait()?;
    if !archived.success() || !extracted.success() {
        return Err("Source archive extraction failed.".into());
    }
    Ok(())
}

fn source_bundle() -> Result<()> {
    if !Path::new(".git").exists() {
        return Err("Create source bundles from the reviewed Git checkout.".into());
    }
    verify_core()?;
    if !output(&mut command("git", ["status", "--porcelain"]))?
        .trim()
        .is_empty()
    {
        return Err("Commit public source changes before creating a release source bundle.".into());
    }
    let revision = output(&mut command("git", ["rev-parse", "HEAD"]))?
        .trim()
        .to_owned();
    let short = &revision[..12];
    let name = format!("psx-source-{short}");
    let staging = Path::new("build/source-bundles").join(&name);
    // Only tracked public project/core files enter this tree. Private ignored
    // games, logs, cards and keys never participate in recursive host copying.
    let marker = staging.join(".psx-source-bundle");
    if staging.exists() {
        if !fs::read_to_string(&marker).is_ok_and(|value| value == revision) {
            return Err("Source staging directory lacks this task's revision marker.".into());
        }
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;
    fs::write(&marker, &revision)?;
    archive_into(Path::new("."), &revision, &staging)?;
    archive_into(
        Path::new("vendor/swanstation"),
        CORE_REV,
        &staging.join("vendor/swanstation"),
    )?;
    ps2::archive(&staging)?;
    fs::write(
        staging.join("SOURCE-CHECK.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "projectRevision": revision,
            "coreRevision": CORE_REV,
            "coreCommitTimestamp": core_epoch()?,
            "coreSourceSha256": core_source_hash(&staging.join("vendor/swanstation"))?,
        }))?,
    )?;
    let crates = staging.join("vendor/rust");
    execute(&mut command(
        "cargo",
        [
            OsStr::new("vendor"),
            OsStr::new("--locked"),
            OsStr::new("--versioned-dirs"),
            crates.as_os_str(),
        ],
    ))?;
    let config = staging.join(".cargo/config.toml");
    let mut content = fs::read_to_string(&config)?;
    content.push_str("\n[source.crates-io]\nreplace-with = \"release-vendor\"\n\n[source.release-vendor]\ndirectory = \"vendor/rust\"\n");
    fs::write(config, content)?;
    fs::write(staging.join("SOURCE-REVISION.txt"), format!("psx {revision}\nSwanStation {CORE_REV}\n\nRust crates are included under vendor/rust with their original notices.\nSee docs/build.md. SDK/NDK, the Rust toolchain and development signing keys are not included.\n"))?;
    let destination = Path::new("build").join(format!("{name}.tar.gz"));
    let mut tar = command(
        "tar",
        [
            OsStr::new("--sort=name"),
            OsStr::new("--mtime=@0"),
            OsStr::new("--owner=0"),
            OsStr::new("--group=0"),
            OsStr::new("--numeric-owner"),
            OsStr::new("--exclude=.psx-source-bundle"),
            OsStr::new("-cf"),
            OsStr::new("-"),
            OsStr::new("-C"),
            OsStr::new("build/source-bundles"),
            OsStr::new(&name),
        ],
    )
    .stdout(Stdio::piped())
    .spawn()?;
    let packed = command("gzip", ["-n"])
        .stdin(Stdio::from(tar.stdout.take().ok_or("Missing tar output")?))
        .stdout(fs::File::create(&destination)?)
        .status()?;
    let archived = tar.wait()?;
    if !packed.success() || !archived.success() {
        return Err("Source bundle compression failed.".into());
    }
    let digest = hash(&destination)?;
    fs::write(
        destination.with_extension("gz.sha256"),
        format!("{digest}  {name}.tar.gz\n"),
    )?;
    println!(
        "Source bundle: {}\nSHA-256: {digest}",
        destination.display()
    );
    Ok(())
}

fn core_source_hash(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    let mut digest = Sha256::new();
    for file in files {
        let relative = file.strip_prefix(root)?;
        if relative == Path::new(".git") || relative == Path::new("src/libretro/version.h") {
            continue;
        }
        digest.update(relative.as_os_str().as_encoded_bytes());
        digest.update([0]);
        digest.update(hash(&file)?.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn verify_core() -> Result<()> {
    verify_firmware()?;
    if hash(Path::new("vendor/swanstation/LICENSE"))? != CORE_LICENSE {
        return Err("SwanStation license differs from the reviewed upstream copy.".into());
    }
    if !Path::new(".git").exists() && !Path::new("vendor/swanstation/.git").exists() {
        // Release source archives have no Git metadata. Their original core
        // inventory is recorded after exporting the verified pinned revision.
        let source: serde_json::Value = serde_json::from_slice(&fs::read("SOURCE-CHECK.json")?)?;
        if source["coreRevision"].as_str() != Some(CORE_REV)
            || source["coreSourceSha256"].as_str()
                != Some(core_source_hash(Path::new("vendor/swanstation"))?.as_str())
        {
            return Err("Source-bundle core differs from its recorded pinned inventory.".into());
        }
        return Ok(());
    }
    let rev = output(&mut command(
        "git",
        ["-C", "vendor/swanstation", "rev-parse", "HEAD"],
    ))?;
    if rev.trim() != CORE_REV {
        return Err("SwanStation revision differs from upstream.lock.json.".into());
    }
    let changes = output(&mut command(
        "git",
        ["-C", "vendor/swanstation", "diff", "--name-only", "HEAD"],
    ))?;
    if !changes.trim().is_empty() {
        return Err(
            "The borrowed core has local modifications. Record and review them before building."
                .into(),
        );
    }
    Ok(())
}

fn verify_firmware() -> Result<()> {
    if hash(Path::new("vendor/openbios/openbios-fast.bin"))? != FIRMWARE_SHA {
        return Err("The free firmware differs from its reviewed build.".into());
    }
    let source = core_source_hash(Path::new("vendor/openbios/source"))?;
    if source != FIRMWARE_SOURCE_SHA {
        return Err(format!("OpenBIOS source differs from the pinned inventory: {source}").into());
    }
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn firmware() -> Result<()> {
    verify_firmware()?;
    let work = Path::new("build/openbios-work");
    let marker = work.join(".psx-firmware-work");
    if work.exists() {
        if !fs::read_to_string(&marker).is_ok_and(|value| value == FIRMWARE_REV) {
            return Err("Firmware staging directory lacks its revision marker.".into());
        }
        fs::remove_dir_all(work)?;
    }
    copy_tree(Path::new("vendor/openbios/source"), work)?;
    fs::write(marker, FIRMWARE_REV)?;
    let volume = format!("{}:/project:Z", fs::canonicalize(work)?.display());
    let mut compiler = Command::new("podman");
    compiler
        .arg("--root")
        .arg(env::current_dir()?.join(".local/firmware-build-storage"))
        .arg("--runroot")
        .arg(env::temp_dir().join("psx-firmware-run"))
        .args([
            "run",
            "--rm",
            "--platform",
            "linux/amd64",
            "--network",
            "none",
            "--read-only",
            "--cap-drop",
            "all",
            "--security-opt",
            "no-new-privileges",
            "--userns",
            "keep-id",
            "--tmpfs",
            "/tmp:rw,size=128m",
            "-v",
        ])
        .arg(volume)
        .args([
            "-w",
            "/project/openbios",
            "--entrypoint",
            "/usr/bin/make",
            FIRMWARE_BUILDER,
            "BOOT_MODE=fast",
            "OPENBIOS_SHELL=shell",
            "BUILD=Release",
            "-j4",
        ]);
    execute(&mut compiler)?;
    if hash(&work.join("openbios/openbios.bin"))? != FIRMWARE_SHA {
        return Err("The rebuilt free firmware differs from the bundled image.".into());
    }
    println!("OpenBIOS {FIRMWARE_REV}: clean fast-boot rebuild matches {FIRMWARE_SHA}");
    Ok(())
}

fn core_epoch() -> Result<String> {
    if Path::new("vendor/swanstation/.git").exists() {
        return Ok(output(&mut command(
            "git",
            [
                "-C",
                "vendor/swanstation",
                "show",
                "-s",
                "--format=%ct",
                "HEAD",
            ],
        ))?
        .trim()
        .to_owned());
    }
    let source: serde_json::Value = serde_json::from_slice(&fs::read("SOURCE-CHECK.json")?)?;
    Ok(source["coreCommitTimestamp"]
        .as_str()
        .filter(|value| value.parse::<u64>().is_ok())
        .ok_or("Missing source-bundle core timestamp")?
        .to_owned())
}

fn apk() -> Result<()> {
    verify_core()?;
    ps2::build()?;
    let (sdk, ndk, build) = tools()?;
    let platform = env::var("ANDROID_PLATFORM").unwrap_or("android-37.0".into());
    let jar = sdk.join("platforms").join(platform).join("android.jar");
    if !jar.is_file() {
        return Err("The requested compile SDK is not installed.".into());
    }
    fs::create_dir_all("build/apk/classes")?;
    fs::create_dir_all("build/apk/dex")?;
    let toolchain = ndk.join("build/cmake/android.toolchain.cmake");
    let epoch = core_epoch()?;
    execute(
        command(
            "cmake",
            [
                "-S",
                "vendor/swanstation",
                "-B",
                "build/core-android",
                "-G",
                "Ninja",
                &format!("-DCMAKE_TOOLCHAIN_FILE={}", toolchain.display()),
                "-DANDROID_ABI=arm64-v8a",
                "-DANDROID_PLATFORM=android-26",
                "-DANDROID_STL=c++_static",
                "-DCMAKE_BUILD_TYPE=Release",
                "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384",
                "-DCMAKE_POLICY_VERSION_MINIMUM=3.5",
                &format!("-DGIT_HASH={}", &CORE_REV[..7]),
                &format!("-DGIT_VERSION={CORE_REV}"),
            ],
        )
        .env("SOURCE_DATE_EPOCH", epoch.trim()),
    )?;
    execute(&mut command(
        "cmake",
        [
            "--build",
            "build/core-android",
            "--target",
            "swanstation_libretro",
            "-j",
            "4",
        ],
    ))?;
    let bin = ndk.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    let mut rust = command(
        "cargo",
        [
            "build",
            "-p",
            "psx-android",
            "--target",
            "aarch64-linux-android",
            "--release",
            "--locked",
        ],
    );
    let scoped_storage = env::var_os("PSX_SCOPED_STORAGE").is_some();
    if scoped_storage {
        rust.arg("--no-default-features");
    }
    let mut manifest = fs::read_to_string("android/AndroidManifest.xml")?;
    if !scoped_storage {
        manifest = manifest.replace("  <application", "  <uses-permission android:name=\"android.permission.MANAGE_EXTERNAL_STORAGE\" />\n  <application");
    }
    fs::write("build/apk/AndroidManifest.xml", manifest)?;
    rust.env(
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
    execute(&mut rust)?;
    let java = Path::new("build/apk/PsxActivity.java");
    // NativeActivity exposes no activity-result or Java listener callbacks to
    // Rust. This generated class only forwards those exact signatures. Every
    // picker, menu, focus decision and file operation is implemented in Rust.
    fs::write(
        java,
        r#"package com.machinerhythm.psx;
public final class PsxActivity extends android.app.NativeActivity implements
 android.media.AudioManager.OnAudioFocusChangeListener {
 static { System.loadLibrary("psx"); }
 private native void documentResult(int request, int result, android.content.Intent data);
 private native void audioFocusChanged(int focus);
 private native void gameIntent(android.content.Intent intent);
 @Override protected void onNewIntent(android.content.Intent intent) {
  super.onNewIntent(intent); gameIntent(intent);
 }
 @Override protected void onActivityResult(int request, int result, android.content.Intent data) {
  super.onActivityResult(request, result, data); documentResult(request, result, data);
 }
 @Override public void onAudioFocusChange(int focus) { audioFocusChanged(focus); }
}
"#,
    )?;
    let service_java = Path::new("build/apk/PlaybackService.java");
    let shortcut_java = Path::new("build/apk/ShortcutReceiver.java");
    // BroadcastReceiver lifecycle cannot be registered through the NDK.
    // Only the callback is forwarded; pin and launch policy lives in Rust.
    fs::write(
        shortcut_java,
        r#"package com.machinerhythm.psx;
public final class ShortcutReceiver extends android.content.BroadcastReceiver {
 static { System.loadLibrary("psx"); }
 private native void confirmed(android.content.Context context, android.content.Intent intent);
 @Override public void onReceive(android.content.Context context, android.content.Intent intent) { confirmed(context, intent); }
}
"#,
    )?;
    // Service callbacks, like NativeActivity callbacks, cannot be registered
    // directly through the NDK. Lifecycle/notification policy remains Rust.
    fs::write(
        service_java,
        r#"package com.machinerhythm.psx;
public final class PlaybackService extends android.app.Service {
 static { System.loadLibrary("psx"); }
 private native int started(android.content.Intent intent);
 private native void stopped();
 @Override public int onStartCommand(android.content.Intent intent, int flags, int id) { return started(intent); }
 @Override public android.os.IBinder onBind(android.content.Intent intent) { return null; }
 @Override public void onDestroy() { stopped(); super.onDestroy(); }
}
"#,
    )?;
    execute(&mut command(
        "java",
        [
            OsString::from("com.sun.tools.javac.Main"),
            "-source".into(),
            "8".into(),
            "-target".into(),
            "8".into(),
            "-classpath".into(),
            jar.as_os_str().into(),
            "-d".into(),
            "build/apk/classes".into(),
            java.as_os_str().into(),
            service_java.as_os_str().into(),
            shortcut_java.as_os_str().into(),
        ],
    ))?;
    execute(&mut command(
        build.join("d8"),
        [
            OsString::from("--min-api"),
            "26".into(),
            "--lib".into(),
            jar.as_os_str().into(),
            "--output".into(),
            "build/apk/dex".into(),
            "build/apk/classes/com/machinerhythm/psx/PsxActivity.class".into(),
            "build/apk/classes/com/machinerhythm/psx/PlaybackService.class".into(),
            "build/apk/classes/com/machinerhythm/psx/ShortcutReceiver.class".into(),
        ],
    ))?;
    notices()?;
    ps2::resources()?;
    execute(&mut command(
        build.join("aapt2"),
        [
            "compile",
            "--dir",
            "android/res",
            "-o",
            "build/apk/resources.zip",
        ],
    ))?;
    execute(&mut command(
        build.join("aapt2"),
        [
            OsString::from("link"),
            "--debug-mode".into(),
            "-I".into(),
            jar.as_os_str().into(),
            "--manifest".into(),
            "build/apk/AndroidManifest.xml".into(),
            "-A".into(),
            "build/apk/assets".into(),
            "build/apk/resources.zip".into(),
            "-o".into(),
            "build/apk/unsigned.apk".into(),
        ],
    ))?;
    fs::create_dir_all("build/apk/package/lib/arm64-v8a")?;
    fs::copy(
        "target/aarch64-linux-android/release/libpsx.so",
        "build/apk/package/lib/arm64-v8a/libpsx.so",
    )?;
    fs::copy(
        "build/core-android/swanstation_libretro_android.so",
        "build/apk/package/lib/arm64-v8a/libswanstation.so",
    )?;
    fs::copy(
        "build/armsx2-android/bin/armsx2_libretro.so",
        "build/apk/package/lib/arm64-v8a/libarmsx2.so",
    )?;
    fs::copy("build/apk/dex/classes.dex", "build/apk/package/classes.dex")?;
    execute(&mut command(
        bin.join("llvm-strip"),
        [
            "--strip-unneeded",
            "build/apk/package/lib/arm64-v8a/libpsx.so",
            "build/apk/package/lib/arm64-v8a/libswanstation.so",
            "build/apk/package/lib/arm64-v8a/libarmsx2.so",
        ],
    ))?;
    let destination = env::current_dir()?.join("build/apk/unsigned.apk");
    execute(
        command(
            "zip",
            [
                OsString::from("-q"),
                "-0".into(),
                destination.into_os_string(),
                "classes.dex".into(),
                "lib/arm64-v8a/libpsx.so".into(),
                "lib/arm64-v8a/libswanstation.so".into(),
                "lib/arm64-v8a/libarmsx2.so".into(),
            ],
        )
        .current_dir("build/apk/package"),
    )?;
    execute(&mut command(
        build.join("zipalign"),
        [
            "-f",
            "-P",
            "16",
            "4",
            "build/apk/unsigned.apk",
            "build/apk/aligned.apk",
        ],
    ))?;
    fs::create_dir_all(".local/signing")?;
    fs::set_permissions(".local", fs::Permissions::from_mode(0o700))?;
    fs::set_permissions(".local/signing", fs::Permissions::from_mode(0o700))?;
    let key = ".local/signing/development.p12";
    if !Path::new(key).exists() {
        execute(&mut command(
            "keytool",
            [
                "-genkeypair",
                "-keystore",
                key,
                "-storetype",
                "PKCS12",
                "-storepass",
                "android",
                "-keypass",
                "android",
                "-alias",
                "psx-development",
                "-keyalg",
                "RSA",
                "-keysize",
                "2048",
                "-validity",
                "3650",
                "-dname",
                "CN=psx development,O=Machine Rhythm",
            ],
        ))?;
    }
    fs::set_permissions(key, fs::Permissions::from_mode(0o600))?;
    execute(&mut command(
        build.join("apksigner"),
        [
            "sign",
            "--ks",
            key,
            "--ks-key-alias",
            "psx-development",
            "--ks-pass",
            "pass:android",
            "--out",
            "build/psx-development.apk",
            "build/apk/aligned.apk",
        ],
    ))?;
    verify_apk()?;
    println!(
        "Development APK: build/psx-development.apk\nSHA-256: {}",
        hash(Path::new("build/psx-development.apk"))?
    );
    Ok(())
}
fn verify_apk() -> Result<()> {
    let (_, ndk, build) = tools()?;
    execute(&mut command(
        build.join("apksigner"),
        ["verify", "--verbose", "build/psx-development.apk"],
    ))?;
    execute(&mut command(
        build.join("zipalign"),
        ["-c", "-P", "16", "4", "build/psx-development.apk"],
    ))?;
    let readelf = ndk.join("toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf");
    for name in ["libpsx.so", "libswanstation.so", "libarmsx2.so"] {
        let path = format!("build/apk/package/lib/arm64-v8a/{name}");
        let headers = output(&mut command(&readelf, ["-h", "-lW", "-d", &path]))?;
        if !headers.contains("AArch64") {
            return Err(format!("Wrong ABI in {name}").into());
        }
        if name == "libpsx.so" && !headers.contains("Shared library: [libEGL.so]") {
            return Err("The Rust host must declare its Android EGL dependency.".into());
        }
        for line in headers
            .lines()
            .filter(|l| l.trim_start().starts_with("LOAD"))
        {
            let alignment = line.split_whitespace().last().unwrap_or("");
            let value = u64::from_str_radix(alignment.trim_start_matches("0x"), 16)?;
            if value < 16384 {
                return Err(format!("{name} contains a LOAD segment aligned below 16KB").into());
            }
        }
        if headers.contains("libc++_shared.so") {
            return Err("The APK is missing a required C++ shared runtime.".into());
        }
        println!("{name}: AArch64; all ELF LOAD alignments >=16KB");
    }
    let entries = output(&mut command("unzip", ["-Z1", "build/psx-development.apk"]))?;
    for entry in [
        "classes.dex",
        "resources.arsc",
        "res/drawable/ic_psx.xml",
        "res/mipmap-anydpi-v26/ic_launcher.xml",
        "lib/arm64-v8a/libpsx.so",
        "lib/arm64-v8a/libswanstation.so",
        "lib/arm64-v8a/libarmsx2.so",
        "assets/notices/armsx2/COPYING.GPLv3",
        "assets/notices/host/LICENSE",
        "assets/notices/swanstation/LICENSE",
        "assets/notices/swanstation/dep/openbios/LICENSE.txt",
        "assets/notices/openbios/LICENSE",
        "assets/notices/openbios/UPSTREAM.md",
    ] {
        if !entries.lines().any(|e| e == entry) {
            return Err(format!("Missing APK entry: {entry}").into());
        }
    }
    Ok(())
}
fn walk(path: &Path, result: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            if entry.file_name() != ".git" {
                walk(&path, result)?;
            }
        } else if entry.file_type()?.is_file() {
            result.push(path);
        }
    }
    Ok(())
}
fn is_notice(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_uppercase();
    name.contains("-LICENSE")
        || name.contains("-COPYRIGHT")
        || [
            "LICENSE",
            "LICENCE",
            "COPYING",
            "NOTICE",
            "AUTHORS",
            "COPYRIGHT",
        ]
        .iter()
        .any(|p| name.starts_with(p))
}
fn embedded_notice(path: &Path) -> bool {
    if !matches!(
        path.extension().and_then(OsStr::to_str),
        Some("h" | "hpp" | "c" | "cpp" | "rs")
    ) {
        return false;
    }
    let Ok(input) = fs::File::open(path) else {
        return false;
    };
    let mut prefix = String::new();
    if input.take(16384).read_to_string(&mut prefix).is_err() {
        return false;
    }
    prefix.contains("Permission is hereby granted")
        || prefix.contains("Redistribution and use")
        || prefix.contains("SPDX-License-Identifier")
        || prefix.contains("public domain")
}
fn copy_notices(source: &Path, destination: &Path) -> Result<usize> {
    let mut paths = Vec::new();
    walk(source, &mut paths)?;
    let mut count = 0;
    for path in paths
        .into_iter()
        .filter(|p| is_notice(p) || embedded_notice(p))
    {
        let target = destination.join(path.strip_prefix(source)?);
        fs::create_dir_all(target.parent().unwrap())?;
        fs::copy(path, target)?;
        count += 1;
    }
    Ok(count)
}
fn copy_dependency_notices(source: &Path, destination: &Path) -> Result<()> {
    let names = if source.join(".git").exists() {
        output(command("git", ["ls-tree", "-rz", "--name-only", "HEAD"]).current_dir(source))?
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    } else {
        fs::read_to_string(source.join(".psx-source-manifest"))?
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once('\t').map(|(_, name)| name.to_owned()))
            .collect()
    };
    for name in names {
        let file = source.join(&name);
        if is_notice(&file) || embedded_notice(&file) {
            let target = destination.join(&name);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(file, target)?;
        }
    }
    Ok(())
}
fn notices() -> Result<()> {
    verify_core()?;
    ps2::verify()?;
    let destination = Path::new("build/apk/assets/notices");
    fs::create_dir_all(destination.join("host"))?;
    fs::copy("LICENSE", destination.join("host/LICENSE"))?;
    fs::copy("THIRD_PARTY.md", destination.join("host/THIRD_PARTY.md"))?;
    fs::create_dir_all(destination.join("openbios"))?;
    fs::copy(
        "vendor/openbios/source/LICENSE",
        destination.join("openbios/LICENSE"),
    )?;
    fs::copy(
        "vendor/openbios/UPSTREAM.md",
        destination.join("openbios/UPSTREAM.md"),
    )?;
    fs::create_dir_all(destination.join("fonts"))?;
    fs::copy(
        "assets/fonts/LICENSE",
        destination.join("fonts/NotoSans-LICENSE"),
    )?;
    fs::copy(
        "assets/fonts/SOURCE.md",
        destination.join("fonts/SOURCE.md"),
    )?;
    fs::copy(
        "upstream.lock.json",
        destination.join("host/upstream.lock.json"),
    )?;
    let count = copy_notices(
        Path::new("vendor/swanstation"),
        &destination.join("swanstation"),
    )?;
    copy_notices(Path::new("vendor/armsx2"), &destination.join("armsx2"))?;
    let dependency_root = ps2::dependency_root();
    for line in fs::read_to_string("vendor/armsx2-dependencies/revisions.tsv")?.lines() {
        let name = line.split('\t').next().ok_or("Invalid dependency lock")?;
        copy_dependency_notices(
            &dependency_root.join(name),
            &destination.join("armsx2-dependencies").join(name),
        )?;
    }
    fs::create_dir_all(destination.join("armsx2-dependencies"))?;
    fs::copy(
        "vendor/armsx2-dependencies/revisions.tsv",
        destination.join("armsx2-dependencies/revisions.tsv"),
    )?;
    let metadata: serde_json::Value = serde_json::from_str(&output(&mut command(
        "cargo",
        ["metadata", "--locked", "--offline", "--format-version=1"],
    ))?)?;
    let mut index=String::from("psx source: https://github.com/WigglyDon/psx\nOriginal Rust host: MIT\nBundled SwanStation: GPLv3 and retained dependency terms.\nExact core revision: e436e9b85374def58d211df7f78880f0f4a917b4\n\nRust dependency source and declared licenses (includes build and other-platform dependencies):\n");
    for package in metadata["packages"]
        .as_array()
        .ok_or("Invalid Cargo metadata")?
    {
        if package["source"].is_null() {
            continue;
        }
        let name = package["name"].as_str().ok_or("Missing crate name")?;
        let version = package["version"].as_str().ok_or("Missing crate version")?;
        let source = Path::new(
            package["manifest_path"]
                .as_str()
                .ok_or("Missing crate path")?,
        )
        .parent()
        .unwrap();
        copy_notices(
            source,
            &destination.join("rust").join(format!("{name}-{version}")),
        )?;
        index.push_str(&format!(
            "{name} {version}: {}\nhttps://crates.io/crates/{name}/{version}\n",
            package["license"]
                .as_str()
                .unwrap_or("See retained notices")
        ));
    }
    let sysroot = output(&mut command("rustc", ["--print", "sysroot"]))?;
    let licenses = Path::new(sysroot.trim()).join("share/doc/rust/licenses");
    if licenses.is_dir() {
        fs::create_dir_all(destination.join("rust-toolchain"))?;
        for entry in fs::read_dir(licenses)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                fs::copy(
                    entry.path(),
                    destination.join("rust-toolchain").join(entry.file_name()),
                )?;
            }
        }
    }
    let copyright = Path::new(sysroot.trim()).join("share/doc/rust/COPYRIGHT.html");
    if copyright.is_file() {
        fs::copy(copyright, destination.join("rust-toolchain/COPYRIGHT.html"))?;
    }
    let ndk = required("ANDROID_NDK_HOME")?;
    fs::create_dir_all(destination.join("ndk"))?;
    for name in ["NOTICE", "NOTICE.toolchain"] {
        fs::copy(ndk.join(name), destination.join("ndk").join(name))?;
    }
    fs::write(destination.join("INDEX.txt"), index)?;
    println!("Retained {count} upstream notices and Cargo dependency license files in APK assets.");
    Ok(())
}
