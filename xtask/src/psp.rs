//! Pinned PPSSPP build. This never rebuilds either qualified PlayStation core.
use super::*;
mod composition {
    include!("../../patches/ppsspp/compose.rs");
}

pub const REVISION: &str = "fc9ca4d1f78a5d60ae03f3311c6b8ab49273f6a8";

pub fn verify() -> Result<String> {
    let source = Path::new("vendor/ppsspp");
    if hash(&source.join("LICENSE.TXT"))?
        != "ff265754cbb2e0d3b674f90a79ef5bbaa1f7f1075aec9f5e241524520e5049fa"
    {
        return Err("PPSSPP's original license differs from its pin".into());
    }
    if source.join(".psx-source-manifest").is_file() {
        ps2::verify_manifest(source, REVISION)?;
        let deps = fs::read_to_string(source.join(".psx-dependencies"))?;
        for line in deps.lines() {
            let (revision, path) = line
                .split_once('\t')
                .ok_or("Invalid PSP dependency source manifest")?;
            if Path::new(path)
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("Invalid PSP dependency source path".into());
            }
            ps2::verify_manifest(&source.join(path), revision)?;
        }
        return Ok(deps);
    }
    if output(&mut command(
        "git",
        ["-C", "vendor/ppsspp", "rev-parse", "HEAD"],
    ))?
    .trim()
        != REVISION
    {
        return Err("PPSSPP source differs from the pinned revision".into());
    }
    if !output(&mut command(
        "git",
        ["-C", "vendor/ppsspp", "status", "--porcelain"],
    ))?
    .is_empty()
    {
        return Err("PPSSPP has unknown source changes; preserve them before building".into());
    }
    let dependencies = output(&mut command(
        "git",
        ["-C", "vendor/ppsspp", "submodule", "status", "--recursive"],
    ))?;
    if dependencies.lines().any(|l| !l.starts_with(' ')) {
        return Err("A PPSSPP dependency is absent or differs from its pinned commit".into());
    }
    if !source.join("LICENSE.TXT").is_file() {
        return Err("PPSSPP's original license is missing".into());
    }
    Ok(dependencies)
}

pub fn build() -> Result<()> {
    let dependencies = verify()?;
    let (_, ndk, _) = tools()?;
    let stage = PathBuf::from(artifact("build/ppsspp-android"));
    fs::create_dir_all(&stage)?;
    fs::write(stage.join("dependency-revisions.txt"), dependencies)?;
    compose(&stage)?;
    let overlay = env::current_dir()?.join("patches/ppsspp/host.cmake");
    let adapter = stage.canonicalize()?.join("libretro-psx.cpp");
    execute(&mut command(
        "cmake",
        [
            OsString::from("-S"),
            "vendor/ppsspp".into(),
            "-B".into(),
            stage.as_os_str().into(),
            "-G".into(),
            "Ninja".into(),
            format!(
                "-DCMAKE_TOOLCHAIN_FILE={}",
                ndk.join("build/cmake/android.toolchain.cmake").display()
            )
            .into(),
            "-DANDROID_ABI=arm64-v8a".into(),
            "-DANDROID_PLATFORM=android-26".into(),
            "-DANDROID_STL=c++_static".into(),
            "-DCMAKE_BUILD_TYPE=RelWithDebInfo".into(),
            "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384".into(),
            "-DCMAKE_POLICY_VERSION_MINIMUM=3.5".into(),
            "-DLIBRETRO=ON".into(),
            "-DHEADLESS=OFF".into(),
            "-DUNITTEST=OFF".into(),
            "-DATLAS_TOOL=OFF".into(),
            "-DUSING_GLES2=ON".into(),
            "-DUSE_DISCORD=OFF".into(),
            "-DUSE_MINIUPNPC=OFF".into(),
            "-DUSE_SYSTEM_LIBPNG=OFF".into(),
            "-DUSE_SYSTEM_FFMPEG=OFF".into(),
            "-DUSE_CCACHE=OFF".into(),
            format!("-DCMAKE_PROJECT_INCLUDE={}", overlay.display()).into(),
            format!("-DPSX_PSP_ADAPTER={}", adapter.display()).into(),
        ],
    ))?;
    storage::check(&stage, 8 * storage::GIB)?;
    execute(&mut command(
        "cmake",
        [
            OsString::from("--build"),
            stage.as_os_str().into(),
            "--target".into(),
            "ppsspp_libretro".into(),
            "-j".into(),
            "2".into(),
        ],
    ))?;
    let native = stage.join("ppsspp_libretro_android.so");
    if !native.is_file() {
        return Err("PPSSPP build did not produce its Android shared core".into());
    }
    let symbols = stage.join("libppsspp-symbols.so");
    let packaged = stage.join("libppsspp-packaged.so");
    fs::copy(&native, &symbols)?;
    fs::copy(&native, &packaged)?;
    let bin = ndk.join("toolchains/llvm/prebuilt/linux-x86_64/bin");
    execute(&mut command(
        bin.join("llvm-strip"),
        [OsStr::new("--strip-unneeded"), packaged.as_os_str()],
    ))?;
    let exports = output(&mut command(
        bin.join("llvm-nm"),
        [
            OsStr::new("-D"),
            OsStr::new("--defined-only"),
            packaged.as_os_str(),
        ],
    ))?;
    for symbol in [
        "retro_init",
        "retro_load_game",
        "retro_run",
        "retro_serialize",
        "retro_unserialize",
        "retro_unload_game",
        "retro_deinit",
        "psx_psp_pause",
        "psx_psp_ready",
        "psx_psp_flush",
        "psx_psp_log",
        "psx_psp_set_log",
        "psx_psp_auto_firmware",
    ] {
        if !exports
            .lines()
            .any(|l| l.split_whitespace().last() == Some(symbol))
        {
            return Err(format!("PPSSPP lacks required export {symbol}").into());
        }
    }
    fs::write(
        stage.join("native-binding.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "upstream":REVISION, "nativeSha256":hash(&packaged)?,
            "symbolsSha256":hash(&symbols)?, "licenseSha256":hash(Path::new("vendor/ppsspp/LICENSE.TXT"))?,
            "dependencyRevisionsSha256":hash(&stage.join("dependency-revisions.txt"))?,
            "hostAdapter":true, "deviceQualified":false,
            "adapterSha256":hash(&adapter)?,
            "bridgeSha256":hash(Path::new("patches/ppsspp/host-api.inc"))?,
            "cmakeOverlaySha256":hash(&overlay)?,
        }))?,
    )?;
    Ok(())
}

fn compose(stage: &Path) -> Result<()> {
    let source = composition::adapter(
        &fs::read_to_string("vendor/ppsspp/libretro/libretro.cpp")?,
        &fs::read_to_string("patches/ppsspp/host-api.inc")?,
    )?;
    let path = stage.join("libretro-psx.cpp");
    if fs::read(&path).ok().as_deref() != Some(source.as_bytes()) {
        fs::write(path, source)?;
    }
    Ok(())
}

pub fn verified_native() -> Result<PathBuf> {
    let dependencies = verify()?;
    let stage = PathBuf::from(artifact("build/ppsspp-android"));
    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(stage.join("native-binding.json"))?)?;
    let expected = composition::adapter(
        &fs::read_to_string("vendor/ppsspp/libretro/libretro.cpp")?,
        &fs::read_to_string("patches/ppsspp/host-api.inc")?,
    )?;
    let native = stage.join("libppsspp-packaged.so");
    if binding["upstream"] != REVISION
        || binding["hostAdapter"] != true
        || binding["adapterSha256"] != format!("{:x}", Sha256::digest(expected.as_bytes()))
        || binding["bridgeSha256"] != hash(Path::new("patches/ppsspp/host-api.inc"))?
        || binding["cmakeOverlaySha256"] != hash(Path::new("patches/ppsspp/host.cmake"))?
        || binding["nativeSha256"] != hash(&native)?
        || binding["symbolsSha256"] != hash(&stage.join("libppsspp-symbols.so"))?
        || binding["dependencyRevisionsSha256"]
            != format!("{:x}", Sha256::digest(dependencies.as_bytes()))
    {
        return Err("Retained PSP artifact differs from the current source/owner bridge".into());
    }
    Ok(native)
}

pub fn resources() -> Result<()> {
    verify()?;
    let root = Path::new("vendor/ppsspp/assets");
    let destination = PathBuf::from(artifact("build/apk/assets/psp/resources"));
    let mut files = Vec::new();
    walk(root, &mut files)?;
    files.sort();
    let mut index = String::new();
    for file in files {
        let name = file.strip_prefix(root)?;
        // The donor's browser debugger/upload UI is not a player capability.
        if name.starts_with("debugger") || name.starts_with("upload") {
            continue;
        }
        if !file.is_file() {
            continue;
        }
        let target = destination.join(name);
        fs::create_dir_all(target.parent().ok_or("PSP resource parent")?)?;
        fs::copy(&file, &target)?;
        index.push_str(&format!("{}\t{}\n", name.display(), hash(&file)?));
    }
    fs::write(artifact("build/apk/assets/psp/resources.tsv"), index)?;
    fs::copy(
        artifact("build/ppsspp-android/native-binding.json"),
        artifact("build/apk/assets/psp/native-binding.json"),
    )?;
    Ok(())
}

pub fn verify_package(apk: &Path) -> Result<()> {
    fn member(apk: &Path, name: &str) -> Result<Vec<u8>> {
        let result = command(
            "unzip",
            [OsStr::new("-p"), apk.as_os_str(), OsStr::new(name)],
        )
        .output()?;
        if !result.status.success() {
            return Err(format!("Missing PSP APK member {name}").into());
        }
        Ok(result.stdout)
    }
    let native = verified_native()?;
    let binding = fs::read(artifact("build/ppsspp-android/native-binding.json"))?;
    if member(apk, "assets/psp/native-binding.json")? != binding
        || format!(
            "{:x}",
            Sha256::digest(member(apk, "lib/arm64-v8a/libppsspp.so")?)
        ) != hash(&native)?
        || member(apk, "assets/psp/resources.tsv")?
            != fs::read(artifact("build/apk/assets/psp/resources.tsv"))?
    {
        return Err("Packaged PSP source/native/resource binding differs".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn function(source: &str, name: &str) -> String {
        let start = source.find(name).unwrap();
        let brace = start + source[start..].find('{').unwrap();
        let mut depth = 0;
        for (i, b) in source.as_bytes().iter().enumerate().skip(brace) {
            if *b == b'{' {
                depth += 1;
            } else if *b == b'}' {
                depth -= 1;
                if depth == 0 {
                    return source[start..=i].into();
                }
            }
        }
        panic!("Retained function ended without a closing brace")
    }
    #[test]
    fn actual_serialization_keeps_the_owner_hold_and_preserves_normal_resume() {
        let donor = fs::read_to_string("../vendor/ppsspp/libretro/libretro.cpp")
            .or_else(|_| fs::read_to_string("vendor/ppsspp/libretro/libretro.cpp"))
            .unwrap();
        let bridge = include_str!("../../patches/ppsspp/host-api.inc");
        let patched = composition::adapter(&donor, bridge).unwrap();
        let temp = env::temp_dir().join(format!("psx-psp-owner-test-{}", std::process::id()));
        fs::create_dir_all(&temp).unwrap();
        for (label, source, held_resumes) in [
            ("upstream", donor.as_str(), 1),
            ("owner", patched.as_str(), 0),
        ] {
            let body = [
                "size_t retro_serialize_size(",
                "bool retro_serialize(",
                "bool retro_unserialize(",
            ]
            .map(|name| function(source, name))
            .join("\n");
            let fixture = format!(
                r#"
#include <cassert>
#include <cstdlib>
#include <cstring>
#include <string>
using u8=unsigned char;
static bool gpu=true,useEmuThread=true,psx_host_paused=true;
static int pauses=0,resumes=0;
static void EmuThreadPause() {{++pauses;}}
static void EmuThreadStart() {{++resumes;}}
namespace SaveState {{ struct SaveStart {{}}; }}
struct CChunkFileReader {{
 enum Error {{ERROR_NONE,ERROR_BAD_ALLOC}};
 static inline Error result=ERROR_NONE;
 static size_t MeasurePtr(SaveState::SaveStart&) {{return 16;}}
 static Error MeasureAndSavePtr(SaveState::SaveStart&,u8**,size_t*) {{return result;}}
 static Error LoadPtr(u8*,size_t,SaveState::SaveStart&,std::string*) {{return result;}}
}};
static void *unserialize_data=nullptr;
static size_t unserialize_size=0;
#define ERROR_LOG(...) ((void)0)
{body}
int main() {{
 u8 state[16]{{}};
 retro_serialize_size(); assert(resumes==0 && pauses==1);
 assert(retro_serialize(state,sizeof(state))); assert(resumes=={held_resumes});
 resumes=0; assert(retro_unserialize(state,sizeof(state))); assert(resumes=={held_resumes});
 CChunkFileReader::result=CChunkFileReader::ERROR_BAD_ALLOC;
 resumes=0; assert(!retro_serialize(state,sizeof(state))); assert(resumes=={held_resumes});
 psx_host_paused=false; CChunkFileReader::result=CChunkFileReader::ERROR_NONE;
 resumes=0; assert(retro_serialize(state,sizeof(state))); assert(resumes==1);
 resumes=0; assert(retro_unserialize(state,sizeof(state))); assert(resumes==1);
}}
"#
            );
            let source = temp.join(format!("{label}.cpp"));
            let binary = temp.join(label);
            fs::write(&source, fixture).unwrap();
            let result = Command::new("c++")
                .arg("-std=c++17")
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            assert!(Command::new(binary).status().unwrap().success());
        }
        fs::remove_dir_all(temp).unwrap();
    }
}
