//! One frozen normal-player package, without compilation or resource generation.
//! This is deliberately separate from the full build's unchanged8GiB budget.
#[path = "frozen-package/checks.rs"]
mod frozen_checks;
#[path = "../package_checks.rs"]
mod package_checks;
#[allow(dead_code)]
#[path = "../storage.rs"]
mod storage;
use frozen_checks::*;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    fs::{self, File},
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const SOURCE: &str = "51fff5bd36a662fbc875671c654df8dd4ba12e36";
const BASE_SOURCE: &str = "441637f65dfb0572a2e98e17599e4e6de7c5e3e1";
const BASE: &str = "build/psp-relink-441637f-acceptance-apk/psx-development.apk";
const BASE_SHA: &str = "90393b9a2cc97565f89d8393570924bbe6bfc128dedc5db7806aa13debbdbd6d";
const RUST: &str = "build/psp-relink-product-rust-51fff5b/libpsx.so";
const RUST_SHA: &str = "56a3bbc6a4d995f9a1e2fee83389fbcd7a30a19e9608e76ea19a81322c845e79";
const SYMBOLS: &str = "build/psp-relink-product-rust-51fff5b/libpsx-symbols.so";
const SYMBOLS_SHA: &str = "c08b69a99b4b86220a40292c21199ba41eb3c3b69338de1c28a5e216bcbace1d";
const SIGNER: &str = "391bb44bb79ba157f06f550ec73dcf3074040e004d0fd5d72c232211d850fc25";
const STAGE: &str = "build/frozen-normal-51fff5b-v1";
const SELF: &[u8] = include_bytes!("frozen-package.rs");
const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_STAGE: u64 = 1024 * 1024 * 1024;
const MAX_CAPTURE: u64 = 8 * 1024 * 1024;
const MAX_LOGS: u64 = 64 * 1024 * 1024;
#[repr(C)]
struct Limit {
    current: u64,
    maximum: u64,
}
unsafe extern "C" {
    fn setrlimit(resource: i32, limit: *const Limit) -> i32;
    fn kill(pid: i32, signal: i32) -> i32;
}

fn hash(path: &Path) -> Result<String> {
    digest(File::open(path)?)
}
fn digest(mut input: impl Read) -> Result<String> {
    let mut sha = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
fn regular(path: &Path, sha: &str, max: u64) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_file() || m.is_symlink() || m.len() > max || hash(path)? != sha {
        return Err(format!("Frozen input differs: {}", path.display()).into());
    }
    Ok(())
}
fn durable(path: &Path, value: &Value) -> Result<()> {
    let temp = path.with_extension("pending");
    let mut f = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    f.write_all(&serde_json::to_vec_pretty(value)?)?;
    f.sync_all()?;
    fs::rename(&temp, path)?;
    File::open(path.parent().ok_or("Receipt parent")?)?.sync_all()?;
    Ok(())
}
fn size(path: &Path) -> Result<u64> {
    let m = fs::symlink_metadata(path)?;
    if m.is_symlink() {
        return Err("Redirected package output".into());
    }
    if m.is_file() {
        return Ok(m.len());
    }
    if !m.is_dir() {
        return Err("Special package output".into());
    }
    let mut bytes = 0u64;
    for e in fs::read_dir(path)? {
        bytes = bytes
            .checked_add(size(&e?.path())?)
            .ok_or("Output overflow")?;
    }
    Ok(bytes)
}
fn small(cmd: &mut Command) -> Result<Vec<u8>> {
    // Read-only fixed Git/ZIP metadata, with bounded in-memory output.
    let out = cmd.output()?;
    if !out.status.success()
        || out.stdout.len() > MAX_CAPTURE as usize
        || out.stderr.len() > MAX_CAPTURE as usize
    {
        return Err("Package metadata command failed or exceeded bound".into());
    }
    Ok(out.stdout)
}
fn git(args: &[&str]) -> Result<Vec<u8>> {
    small(Command::new("git").args(args))
}
fn inventory(apk: &Path) -> Result<BTreeMap<String, String>> {
    let list = String::from_utf8(small(Command::new("unzip").arg("-Z1").arg(apk))?)?;
    let mut result = BTreeMap::new();
    for name in names(&list)? {
        let mut child = Command::new("unzip")
            .arg("-p")
            .arg(apk)
            .arg(&name)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let output = child.stdout.take().ok_or("ZIP pipe")?;
        let value = bounded_digest(output, MAX_FILE);
        if value.is_err() {
            let _ = child.kill();
        }
        if !child.wait()?.success() {
            return Err("ZIP member integrity failed".into());
        }
        let value = value?;
        result.insert(name, value);
    }
    Ok(result)
}
fn bounded_digest(mut input: impl Read, limit: u64) -> Result<String> {
    let mut bytes = 0u64;
    let mut sha = Sha256::new();
    let mut buf = [0; 65536];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > limit {
            return Err("ZIP member exceeds128MiB".into());
        }
        sha.update(&buf[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}
struct Runner {
    stage: PathBuf,
    peak: u64,
    logs: u64,
    sequence: u32,
    command_limit: Duration,
    file_limit: u64,
    stage_limit: u64,
}
impl Runner {
    fn save_logs(&mut self, out: &[u8], err: &[u8]) -> Result<()> {
        let mut truncated = false;
        for (suffix, bytes) in [("out", out), ("err", err)] {
            let n = bytes.len().min((MAX_LOGS - self.logs) as usize);
            fs::write(
                self.stage.join(format!("{:02}.{suffix}", self.sequence)),
                &bytes[..n],
            )?;
            self.logs += n as u64;
            truncated |= n != bytes.len();
        }
        if truncated {
            return Err("Package log budget exhausted; bounded prefixes preserved".into());
        }
        Ok(())
    }
    fn observe(&mut self) -> Result<()> {
        let n = size(&self.stage)?;
        self.peak = self.peak.max(n);
        if n >= self.stage_limit {
            return Err("Frozen package exceeded stage ceiling".into());
        }
        Ok(())
    }
    fn command(&mut self, cmd: &mut Command) -> Result<Vec<u8>> {
        self.execute(cmd, None)
    }
    fn extract(&mut self, cmd: &mut Command, path: &Path) -> Result<()> {
        self.execute(cmd, Some(path.to_owned()))?;
        Ok(())
    }
    fn execute(&mut self, cmd: &mut Command, destination: Option<PathBuf>) -> Result<Vec<u8>> {
        storage::between_steps()?;
        self.observe()?;
        self.sequence += 1;
        if self.sequence > 32 {
            return Err("Package operation count exceeded".into());
        }
        durable(
            &self.stage.join("phase.json"),
            &json!({"phase":"before-command","sequence":self.sequence,"tool":cmd.get_program().to_string_lossy(),"observedPeakBytes":self.peak}),
        )?;
        cmd.stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0);
        // Linux64 RLIMIT_FSIZE bounds each child-created APK/temp/log file.
        // This affects this child only; no shared process/volume policy changes.
        let file_limit = self.file_limit;
        unsafe {
            cmd.pre_exec(move || {
                let limit = Limit {
                    current: file_limit,
                    maximum: file_limit,
                };
                if setrlimit(1, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let mut child = cmd.spawn()?;
        let group = child.id() as i32;
        let oversized = Arc::new(AtomicBool::new(false));
        let collect =
            |mut reader: Box<dyn Read + Send>, flag: Arc<AtomicBool>, path: Option<PathBuf>| {
                thread::spawn(move || -> std::io::Result<Vec<u8>> {
                    let limit = if path.is_some() {
                        MAX_FILE
                    } else {
                        MAX_CAPTURE
                    };
                    let mut file = path.map(File::create).transpose()?;
                    let mut bytes = Vec::new();
                    let mut total = 0u64;
                    let mut buf = [0; 65536];
                    loop {
                        let n = reader.read(&mut buf)?;
                        if n == 0 {
                            break;
                        }
                        total += n as u64;
                        if total > limit {
                            flag.store(true, Ordering::SeqCst);
                            break;
                        }
                        if let Some(f) = &mut file {
                            f.write_all(&buf[..n])?;
                        } else {
                            bytes.extend_from_slice(&buf[..n]);
                        }
                    }
                    if let Some(f) = file {
                        f.sync_all()?;
                    }
                    Ok(bytes)
                })
            };
        let stdout = collect(
            Box::new(child.stdout.take().ok_or("stdout")?),
            oversized.clone(),
            destination,
        );
        let stderr = collect(
            Box::new(child.stderr.take().ok_or("stderr")?),
            oversized.clone(),
            None,
        );
        let started = Instant::now();
        let status = loop {
            let bounds = self.observe();
            if bounds.is_err()
                || oversized.load(Ordering::SeqCst)
                || started.elapsed() > self.command_limit
            {
                // Child has not been reaped: its process-group ID cannot be
                // reused. Retire only this owned command and descendants.
                unsafe {
                    kill(-group, 9);
                }
                let _ = child.wait();
                let out = stdout.join().ok().and_then(|r| r.ok()).unwrap_or_default();
                let err = stderr.join().ok().and_then(|r| r.ok()).unwrap_or_default();
                let _ = self.save_logs(&out, &err);
                return Err(
                    "Owned package command stopped at output/time boundary; stage preserved".into(),
                );
            }
            // Keep the leader unreaped until readers close. A descendant
            // retaining a pipe cannot extend the deadline or permit PID reuse.
            if stdout.is_finished() && stderr.is_finished() {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
            }
            thread::sleep(Duration::from_millis(25));
        };
        let out = stdout.join().map_err(|_| "stdout collector")??;
        let err = stderr.join().map_err(|_| "stderr collector")??;
        if oversized.load(Ordering::SeqCst) {
            return Err("Package command capture exceeded bound".into());
        }
        self.save_logs(&out, &err)?;
        self.observe()?;
        storage::between_steps()?;
        if !status.success() {
            return Err(format!(
                "Package command failed: {}",
                cmd.get_program().to_string_lossy()
            )
            .into());
        }
        Ok(out)
    }
}
fn tools() -> Result<(PathBuf, PathBuf)> {
    let sdk =
        PathBuf::from(env::var_os("ANDROID_SDK_ROOT").ok_or("Installed Android SDK required")?);
    let ndk = PathBuf::from(env::var_os("ANDROID_NDK_HOME").ok_or("Installed NDK required")?);
    Ok((
        sdk.join("build-tools").join("37.0.0"),
        ndk.join("toolchains/llvm/prebuilt/linux-x86_64/bin/llvm-readelf"),
    ))
}
fn main() {
    let mode = env::args().nth(1);
    let result = match mode.as_deref() {
        Some("check") => run(false),
        Some("package") => run(true),
        _ => Err("Usage: frozen-package check | package (one frozen normal51 profile)".into()),
    };
    if let Err(e) = result {
        eprintln!("Frozen package refused/failed: {e}");
        std::process::exit(1);
    }
}
fn run(package: bool) -> Result<()> {
    if !cfg!(all(target_os = "linux", target_pointer_width = "64")) {
        return Err("This frozen packaging operation qualifies Linux64 hosts only".into());
    }
    env::set_current_dir(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or("Project root")?,
    )?;
    let root = env::current_dir()?.canonicalize()?;
    if env::var_os("PSX_BUILD_WORKSPACE").is_some() {
        return Err("Frozen package currently qualifies internal project storage only".into());
    }
    let _lock = storage::Lock::acquire(&root)?;
    let _growth = storage::Growth::begin(&root, 2 * storage::GIB)?;
    if !git(&["status", "--porcelain"])?.is_empty() {
        return Err("Commit the exact operator/source before packaging".into());
    }
    let head = String::from_utf8(git(&["rev-parse", "HEAD"])?)?
        .trim()
        .to_owned();
    if head.len() != 40 || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Exact source revision required".into());
    }
    for (path, embedded) in [
        ("xtask/src/bin/frozen-package.rs", SELF),
        (
            "xtask/src/bin/frozen-package/checks.rs",
            include_bytes!("frozen-package/checks.rs").as_slice(),
        ),
        (
            "xtask/src/package_checks.rs",
            include_bytes!("../package_checks.rs").as_slice(),
        ),
        (
            "xtask/src/storage.rs",
            include_bytes!("../storage.rs").as_slice(),
        ),
    ] {
        if git(&["show", &format!("{head}:{path}")])? != embedded {
            return Err("Executed operator source differs".into());
        }
    }
    for path in [
        "Cargo.toml",
        "Cargo.lock",
        "upstream.lock.json",
        ".cargo",
        "crates",
        "android",
        "assets",
        "patches",
        "vendor",
    ] {
        if !git(&["diff", "--name-only", SOURCE, &head, "--", path])?.is_empty() {
            return Err(format!("Normal payload source changed: {path}").into());
        }
    }
    payload_generators(
        &git(&["show", &format!("{SOURCE}:xtask/src/main.rs")])?,
        &git(&["show", &format!("{head}:xtask/src/main.rs")])?,
    )?;
    for path in ["xtask/src/ps2.rs", "xtask/src/psp.rs"] {
        if !git(&["diff", "--name-only", SOURCE, &head, "--", path])?.is_empty() {
            return Err("Native resource generator changed".into());
        }
    }
    // Complete51 source and the separately delivered operator revision are
    // both required. New bundle includes the unchanged application sources.
    regular(
        Path::new("build/psx-source-51fff5bd36a6.tar.gz"),
        "0c2d8ee6c864fcc47681e33e810b4e3c6a00e294126bf77601d0066164b391d4",
        512 * 1024 * 1024,
    )?;
    let archive = root.join(format!("build/psx-source-{}.tar.gz", &head[..12]));
    let receipt: Value =
        serde_json::from_slice(&fs::read(archive.with_extension("receipt.json"))?)?;
    if receipt["targetRevision"] != head
        || receipt["streamedReadback"] != true
        || receipt["phase"] != "verified-complete"
    {
        return Err("Complete matching source delivery required".into());
    }
    regular(
        &archive,
        receipt["archiveSha256"].as_str().ok_or("Source digest")?,
        512 * 1024 * 1024,
    )?;
    regular(Path::new(BASE), BASE_SHA, MAX_FILE)?;
    regular(Path::new(RUST), RUST_SHA, 8 * 1024 * 1024)?;
    regular(Path::new(SYMBOLS), SYMBOLS_SHA, 8 * 1024 * 1024)?;
    coherence(
        &git(&["diff", "--name-only", BASE_SOURCE, SOURCE])?,
        &git(&[
            "show",
            &format!("{BASE_SOURCE}:crates/player/src/resume.rs"),
        ])?,
        &git(&["show", &format!("{SOURCE}:crates/player/src/resume.rs")])?,
    )?;
    let base = inventory(Path::new(BASE))?;
    baseline(&base)?;
    let (build, readelf) = tools()?;
    let key = root.join(".local/signing/development.p12");
    let key_meta = fs::symlink_metadata(&key)?;
    if !key_meta.is_file() || key_meta.is_symlink() || key_meta.mode() & 0o077 != 0 {
        return Err(
            "Existing private signing identity required; never generate another key".into(),
        );
    }
    if !package {
        signer(
            &String::from_utf8(small(
                Command::new(build.join("apksigner"))
                    .args(["verify", "--verbose", "--print-certs"])
                    .arg(BASE),
            )?)?,
            SIGNER,
        )?;
        let headers = String::from_utf8(small(
            Command::new(&readelf).args(["-h", "-lW", "-d", RUST]),
        )?)?;
        let symbols = String::from_utf8(small(Command::new(&readelf).args([
            "--dyn-syms",
            "--wide",
            RUST,
        ]))?)?;
        package_checks::elf(&headers, &symbols, "libpsx.so", None)?;
        println!("Frozen inputs/source/flavor/signing/normal ELF verified; no package stage created. Payload entries: {}",base.len());
        return Ok(());
    }
    let stage = root.join(STAGE);
    fs::create_dir(&stage)?;
    fs::set_permissions(&stage, fs::Permissions::from_mode(0o700))?;
    File::open(root.join("build"))?.sync_all()?;
    let mut runner = Runner {
        stage: stage.clone(),
        peak: 0,
        logs: 0,
        sequence: 0,
        command_limit: Duration::from_secs(120),
        file_limit: MAX_FILE,
        stage_limit: MAX_STAGE,
    };
    let result = (|| -> Result<()> {
        durable(
            &stage.join("inputs.json"),
            &json!({"applicationSourceRevision":SOURCE,"baseApplicationSourceRevision":BASE_SOURCE,"packagerRevision":head,"operatorSha256":hash(&env::current_exe()?)?,"correspondingSourceArchive":archive,"sourceArchiveSha256":receipt["archiveSha256"],"baseApkSha256":BASE_SHA,"rustSha256":RUST_SHA,"rustSymbolsSha256":SYMBOLS_SHA,"originalSignerSha256":SIGNER,"growthBytes":2*storage::GIB,"reserveBytes":storage::RESERVE,"stageLimitBytes":MAX_STAGE,"basePayload":base}),
        )?;
        let cert = runner.command(
            Command::new(build.join("apksigner"))
                .args(["verify", "--verbose", "--print-certs"])
                .arg(BASE),
        )?;
        signer(&String::from_utf8(cert)?, SIGNER)?;
        let unsigned = stage.join("unsigned.apk");
        fs::copy(BASE, &unsigned)?;
        runner.command(
            Command::new("zip")
                .arg("-q")
                .arg("-d")
                .arg(&unsigned)
                .args([
                    CAPABILITY,
                    "META-INF/PSX-DEVE.SF",
                    "META-INF/PSX-DEVE.RSA",
                    "META-INF/MANIFEST.MF",
                ]),
        )?;
        let lib = stage.join("lib/arm64-v8a");
        fs::create_dir_all(&lib)?;
        fs::copy(RUST, lib.join("libpsx.so"))?;
        runner.command(
            Command::new("zip")
                .args(["-q", "-0"])
                .arg(&unsigned)
                .arg(RUST_ENTRY)
                .current_dir(&stage),
        )?;
        let aligned = stage.join("aligned.apk");
        runner.command(
            Command::new(build.join("zipalign"))
                .args(["-P", "16", "4"])
                .arg(&unsigned)
                .arg(&aligned),
        )?;
        let apk = stage.join("psx-development.apk");
        runner.command(
            Command::new(build.join("apksigner"))
                .arg("sign")
                .arg("--ks")
                .arg(&key)
                .args([
                    "--ks-key-alias",
                    "psx-development",
                    "--ks-pass",
                    "pass:android",
                    "--out",
                ])
                .arg(&apk)
                .arg(&aligned),
        )?;
        let cert = runner.command(
            Command::new(build.join("apksigner"))
                .args(["verify", "--verbose", "--print-certs"])
                .arg(&apk),
        )?;
        signer(&String::from_utf8(cert)?, SIGNER)?;
        runner.command(
            Command::new(build.join("zipalign"))
                .args(["-c", "-P", "16", "4"])
                .arg(&apk),
        )?;
        runner.command(Command::new("unzip").arg("-tqq").arg(&apk))?;
        let actual = inventory(&apk)?;
        permitted(&base, &actual, RUST_SHA)?;
        let elf = stage.join("verify-lib.so");
        for name in [
            "libpsx.so",
            "libswanstation.so",
            "libarmsx2.so",
            "libppsspp.so",
        ] {
            runner.extract(
                Command::new("unzip")
                    .arg("-p")
                    .arg(&apk)
                    .arg(format!("lib/arm64-v8a/{name}")),
                &elf,
            )?;
            if hash(&elf)? != actual[&format!("lib/arm64-v8a/{name}")] {
                return Err("APK ELF extraction differs".into());
            }
            runner.observe()?;
            let headers = String::from_utf8(
                runner.command(Command::new(&readelf).args(["-h", "-lW", "-d"]).arg(&elf))?,
            )?;
            let symbols = String::from_utf8(
                runner.command(
                    Command::new(&readelf)
                        .args(["--dyn-syms", "--wide"])
                        .arg(&elf),
                )?,
            )?;
            package_checks::elf(&headers, &symbols, name, None)?;
        }
        regular(Path::new(BASE), BASE_SHA, MAX_FILE)?;
        regular(Path::new(RUST), RUST_SHA, 8 * 1024 * 1024)?;
        regular(Path::new(SYMBOLS), SYMBOLS_SHA, 8 * 1024 * 1024)?;
        if !git(&["status", "--porcelain"])?.is_empty()
            || git(&["rev-parse", "HEAD"])? != format!("{head}\n").as_bytes()
        {
            return Err("Source changed during frozen packaging".into());
        }
        let output = json!({"phase":"verified-complete","apkSha256":hash(&apk)?,"apkBytes":fs::metadata(&apk)?.len(),"applicationSourceRevision":SOURCE,"packagerRevision":head,"operatorSha256":hash(&env::current_exe()?)?,"correspondingSourceArchive":archive,"sourceArchiveSha256":receipt["archiveSha256"],"baseApkSha256":BASE_SHA,"rustSha256":RUST_SHA,"rustSymbolsSha256":SYMBOLS_SHA,"signerSha256":SIGNER,"observedPeakBytes":runner.peak,"stageLimitBytes":MAX_STAGE,"growthBytes":2*storage::GIB,"reserveBytes":storage::RESERVE,"fullBuildGrowthBytesUnchanged":8*storage::GIB,"payload":actual,"nativeBuild":false,"cargoBuild":false,"deviceQualified":false});
        durable(&stage.join("binding.json"), &output)?;
        // Keep every failure. On success record exact generated copies before
        // retiring only this new stage's verified packaging intermediates.
        let mut retired = Vec::new();
        for path in [&unsigned, &aligned, &elf, &lib.join("libpsx.so")] {
            let before = fs::symlink_metadata(path)?;
            let sha = hash(path)?;
            durable(
                &stage.join("retention.json"),
                &json!({"beforeRemoval":path,"sha256":sha,"device":before.dev(),"inode":before.ino(),"completedBinding":output,"previous":retired}),
            )?;
            let now = fs::symlink_metadata(path)?;
            if !now.is_file()
                || now.is_symlink()
                || now.dev() != before.dev()
                || now.ino() != before.ino()
                || hash(path)? != sha
            {
                return Err("Generated intermediate changed; preserved".into());
            }
            fs::remove_file(path)?;
            retired.push(json!({"path":path,"sha256":sha}));
        }
        durable(
            &stage.join("retention.json"),
            &json!({"verifiedRetiredIntermediates":retired,"retainedApkSha256":output["apkSha256"]}),
        )?;
        runner.observe()?;
        File::open(&stage)?.sync_all()?;
        storage::between_steps()?;
        println!(
            "Verified frozen normal APK: {}\nSHA-256: {}\nObserved stage peak: {} bytes",
            apk.display(),
            output["apkSha256"],
            runner.peak
        );
        Ok(())
    })();
    if let Err(e) = &result {
        let _ = durable(
            &stage.join("failure.json"),
            &json!({"error":e.to_string(),"observedPeakBytes":runner.peak,"stagePreserved":true}),
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let p = env::temp_dir().join(format!(
                "psx-frozen-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::SeqCst)
            ));
            fs::create_dir(&p).unwrap();
            Self(p)
        }
        fn runner(&self) -> Runner {
            Runner {
                stage: self.0.clone(),
                peak: 0,
                logs: 0,
                sequence: 0,
                command_limit: Duration::from_millis(150),
                file_limit: 4096,
                stage_limit: 65536,
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn command_timeout_stops_owned_descendants_and_keeps_failure_evidence() {
        let f = Fixture::new();
        let mut r = f.runner();
        let script = format!(
            "printf started; sleep 2; printf late > {}/late",
            f.0.display()
        );
        assert!(r.command(Command::new("sh").args(["-c", &script])).is_err());
        assert_eq!(fs::read(f.0.join("01.out")).unwrap(), b"started");
        assert!(f.0.join("phase.json").is_file());
        assert!(!f.0.join("late").exists());
    }
    #[test]
    fn file_limit_failure_preserves_bounded_partial_output() {
        let f = Fixture::new();
        let mut r = f.runner();
        r.command_limit = Duration::from_secs(3);
        let script = format!("head -c 8192 /dev/zero > {}/partial", f.0.display());
        assert!(r.command(Command::new("sh").args(["-c", &script])).is_err());
        assert_eq!(fs::metadata(f.0.join("partial")).unwrap().len(), 4096);
        assert!(f.0.join("phase.json").is_file());
    }
    #[test]
    fn stage_limit_refuses_before_command_and_log_limit_keeps_prefixes() {
        let f = Fixture::new();
        let mut r = f.runner();
        r.stage_limit = 8;
        fs::write(f.0.join("owned"), b"12345678").unwrap();
        assert!(r
            .command(Command::new("sh").args(["-c", "printf should-not-run"]))
            .is_err());
        assert_eq!(r.sequence, 0);
        r.logs = MAX_LOGS - 2;
        assert!(r.save_logs(b"abcd", b"ef").is_err());
        assert_eq!(fs::read(f.0.join("00.out")).unwrap(), b"ab");
        assert_eq!(r.logs, MAX_LOGS);
    }
    #[test]
    fn streamed_member_digest_rejects_overflow_without_truncation_acceptance() {
        assert!(bounded_digest(&b"same"[..], 4).is_ok());
        assert!(bounded_digest(&b"excess"[..], 4).is_err());
    }
}
