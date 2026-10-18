use sha2::{Digest, Sha256};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const CORE_REV: &str = "e436e9b85374def58d211df7f78880f0f4a917b4";
const CORE_LICENSE: &str = "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903";

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
        _ => {
            eprintln!("Usage: cargo xtask apk | verify | notices\nSet ANDROID_SDK_ROOT, ANDROID_NDK_HOME, ANDROID_BUILD_TOOLS (default 37.0.0), ANDROID_PLATFORM (default android-37.0). Compile SDK may exceed target API 36. Requires Java with javac module, cmake, ninja, zip, unzip, Rust Android target.");
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
fn verify_core() -> Result<()> {
    let rev = output(&mut command(
        "git",
        ["-C", "vendor/swanstation", "rev-parse", "HEAD"],
    ))?;
    if rev.trim() != CORE_REV {
        return Err("SwanStation revision differs from upstream.lock.json.".into());
    }
    if hash(Path::new("vendor/swanstation/LICENSE"))? != CORE_LICENSE {
        return Err("SwanStation license differs from the reviewed upstream copy.".into());
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
fn apk() -> Result<()> {
    verify_core()?;
    let (sdk, ndk, build) = tools()?;
    let platform = env::var("ANDROID_PLATFORM").unwrap_or("android-37.0".into());
    let jar = sdk.join("platforms").join(platform).join("android.jar");
    if !jar.is_file() {
        return Err("The requested compile SDK is not installed.".into());
    }
    fs::create_dir_all("build/apk/classes")?;
    fs::create_dir_all("build/apk/dex")?;
    let toolchain = ndk.join("build/cmake/android.toolchain.cmake");
    let epoch = output(&mut command(
        "git",
        [
            "-C",
            "vendor/swanstation",
            "show",
            "-s",
            "--format=%ct",
            "HEAD",
        ],
    ))?;
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
 android.content.DialogInterface.OnClickListener,
 android.content.DialogInterface.OnCancelListener,
 android.media.AudioManager.OnAudioFocusChangeListener {
 private native void documentResult(int request, int result, android.content.Intent data);
 private native void dialogResult(int index);
 private native void dialogCancel();
 private native void audioFocusChanged(int focus);
 @Override protected void onActivityResult(int request, int result, android.content.Intent data) {
  super.onActivityResult(request, result, data); documentResult(request, result, data);
 }
 @Override public void onClick(android.content.DialogInterface dialog, int index) { dialogResult(index); }
 @Override public void onCancel(android.content.DialogInterface dialog) { dialogCancel(); }
 @Override public void onAudioFocusChange(int focus) { audioFocusChanged(focus); }
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
        ],
    ))?;
    notices()?;
    execute(&mut command(
        build.join("aapt2"),
        [
            OsString::from("link"),
            "--debug-mode".into(),
            "-I".into(),
            jar.as_os_str().into(),
            "--manifest".into(),
            "android/AndroidManifest.xml".into(),
            "-A".into(),
            "build/apk/assets".into(),
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
    fs::copy("build/apk/dex/classes.dex", "build/apk/package/classes.dex")?;
    execute(&mut command(
        bin.join("llvm-strip"),
        [
            "--strip-unneeded",
            "build/apk/package/lib/arm64-v8a/libpsx.so",
            "build/apk/package/lib/arm64-v8a/libswanstation.so",
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
    for name in ["libpsx.so", "libswanstation.so"] {
        let path = format!("build/apk/package/lib/arm64-v8a/{name}");
        let headers = output(&mut command(&readelf, ["-h", "-lW", "-d", &path]))?;
        if !headers.contains("AArch64") {
            return Err(format!("Wrong ABI in {name}").into());
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
        "lib/arm64-v8a/libpsx.so",
        "lib/arm64-v8a/libswanstation.so",
        "assets/notices/host/LICENSE",
        "assets/notices/swanstation/LICENSE",
        "assets/notices/swanstation/dep/openbios/LICENSE.txt",
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
    [
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
fn notices() -> Result<()> {
    let destination = Path::new("build/apk/assets/notices");
    fs::create_dir_all(destination.join("host"))?;
    fs::copy("LICENSE", destination.join("host/LICENSE"))?;
    fs::copy("THIRD_PARTY.md", destination.join("host/THIRD_PARTY.md"))?;
    fs::copy(
        "upstream.lock.json",
        destination.join("host/upstream.lock.json"),
    )?;
    let count = copy_notices(
        Path::new("vendor/swanstation"),
        &destination.join("swanstation"),
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
