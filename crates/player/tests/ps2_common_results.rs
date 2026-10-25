//! Actual development call sites with controlled existing API results. No GPU,
//! proprietary input or extra error/readback query is part of this fixture.
#![cfg(all(target_os = "linux", feature = "producer-local-output-sync"))]
use libloading::Library;
use std::{fs, path::PathBuf, process::Command};
fn body<'a>(source: &'a str, first: &str, next: &str) -> &'a str {
    let from = source.find(first).expect(first);
    let end = source[from..].find(next).expect(next);
    &source[from..from + end]
}
#[test]
fn existing_native_reads_poll_results_and_unsafe_retirement_keep_their_contract() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("psx-common-native-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let relative = "pcsx2/GS/Renderers/OpenGL";
    for file in [
        "GSTextureOGL.cpp",
        "GSDeviceOGL.cpp",
        "GLStreamBuffer.cpp",
        "GLLibretro.h",
    ] {
        let path = dir.join(relative).join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::copy(repo.join("vendor/armsx2").join(relative).join(file), path).unwrap();
    }
    for patch in [
        "0001-host-pause-and-card-flush.patch",
        "0004-stream-buffer-failure.patch",
        "0002-development-graphics-observer.patch",
        "0003-development-matched-reader.patch",
        "0005-development-stream-observer.patch",
        "0006-development-creator-output.patch",
        "0007-development-common-results.patch",
    ] {
        let o = Command::new("git")
            .args([
                "apply",
                "--include=pcsx2/GS/Renderers/OpenGL/GSTextureOGL.cpp",
                "--include=pcsx2/GS/Renderers/OpenGL/GSDeviceOGL.cpp",
                "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.h",
                "--include=pcsx2/GS/Renderers/OpenGL/GLStreamBuffer.cpp",
            ])
            .arg(repo.join("patches/armsx2").join(patch))
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{patch}:{}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    let header = fs::read_to_string(dir.join(relative).join("GLLibretro.h")).unwrap();
    let helper = format!(
        "namespace GSObservation{}",
        header.split_once("namespace GSObservation").unwrap().1
    );
    let texture = fs::read_to_string(dir.join(relative).join("GSTextureOGL.cpp")).unwrap();
    let device = fs::read_to_string(dir.join(relative).join("GSDeviceOGL.cpp")).unwrap();
    let stream = fs::read_to_string(dir.join(relative).join("GLStreamBuffer.cpp")).unwrap();
    let factory = body(
        &stream,
        "static std::unique_ptr<GLStreamBuffer> Create(GLenum target, u32 size)",
        "return std::unique_ptr<GLStreamBuffer>(new BufferSubDataStreamBuffer",
    )
    .split_once('{')
    .unwrap()
    .1;
    let flush = body(
        &texture,
        "void GSDownloadTextureOGL::Flush()",
        "#ifdef PCSX2_DEVBUILD",
    );
    let teardown = body(
        &device,
        "void GSDeviceOGL::DestroyDeviceObjects()",
        "void GSDeviceOGL::Destroy()",
    );
    let fixture = format!(
        r#"
#include <cstdint>
using u32=uint32_t; using u64=uint64_t; using GLenum=unsigned;
using GLuint=unsigned; static constexpr unsigned GL_STREAM_DRAW=0x88e0,GL_NO_ERROR=0;
static unsigned errors=0,error_value=0,waits=0,deletes=0,wait_result=0x911a,gl_cleanup=0;
static unsigned factory_error=0;
static constexpr unsigned GL_SYNC_FLUSH_COMMANDS_BIT=1,GL_ALREADY_SIGNALED=0x911a,GL_CONDITION_SATISFIED=0x911c;
static constexpr uint64_t GL_TIMEOUT_IGNORED=~uint64_t(0);
unsigned glGetError() {{ ++errors; const unsigned e=error_value;error_value=0;return e; }}
void glGenBuffers(int,unsigned* p) {{*p=88;}} void glBindBuffer(unsigned,unsigned) {{}} void glBufferData(unsigned,unsigned,const void*,unsigned) {{error_value=factory_error;}} void glDeleteBuffers(int,const unsigned*) {{}}
unsigned glClientWaitSync(void*,unsigned,uint64_t) {{ ++waits;return wait_result; }}
void glDeleteSync(void*) {{ ++deletes; }}
{helper}
namespace GSObservation {{ static Callback callback=nullptr; void SetCallback(Callback c) {{callback=c;}} u32 Result(u32 o,u32 k,std::uintptr_t p,u64 a,u64 b,u64 c,u64 d,u64 e) {{return callback?callback(o,k,p,a,b,c,d,e):0;}} }}
struct GSDownloadTextureOGL {{ bool m_needs_flush=false;void* m_sync=nullptr;void* m_cpu_buffer=nullptr;void Flush();bool Poll(); }};
struct GSDevice {{ void Destroy() {{}} }};
namespace GLLibretro {{void Deactivate() {{}} void AbortPacing() {{}}}}
struct GSDeviceOGL:GSDevice {{ bool m_objects_destroyed=false,m_gl_context=true;void DestroyDeviceObjects();void DestroyShaderChain(){{++gl_cleanup;}}void DestroyTimestampQueries(){{++gl_cleanup;}}void DestroyPipelineStatisticsQueries(){{++gl_cleanup;}}void DestroyResources(){{++gl_cleanup;}} }};
{flush}
{teardown}
extern "C" unsigned factory_check(unsigned target,unsigned size,unsigned result){{factory_error=result;{factory}return 1;}}
extern "C" void set_callback(GSObservation::Callback c){{GSObservation::SetCallback(c);}}
extern "C" unsigned consumed_error(unsigned e,bool probe){{error_value=e;return GSObservation::Error(333,probe);}}
extern "C" unsigned error_reads(){{return errors;}}
extern "C" unsigned exercise(bool active,unsigned result,bool flush){{GSDownloadTextureOGL d;d.m_needs_flush=active;d.m_sync=active?reinterpret_cast<void*>(99):nullptr;wait_result=result;return flush?(d.Flush(),0):d.Poll();}}
extern "C" unsigned wait_calls(){{return waits;}} extern "C" unsigned delete_calls(){{return deletes;}}
extern "C" unsigned destroy(){{gl_cleanup=0;GSDeviceOGL d;d.DestroyDeviceObjects();return gl_cleanup;}}
"#
    );
    fs::write(dir.join("fixture.cpp"), fixture).unwrap();
    let o = Command::new("c++")
        .args([
            "-std=c++17",
            "-shared",
            "-fPIC",
            "-O0",
            "fixture.cpp",
            "-o",
            "fixture.so",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    psx_player::graphics_observer::common::begin();
    psx_player::graphics_observer::output_sync::begin().unwrap();
    unsafe {
        let lib = Library::new(dir.join("fixture.so")).unwrap();
        type Callback = extern "C" fn(u32, u32, usize, u64, u64, u64, u64, u64) -> u32;
        lib.get::<unsafe extern "C" fn(Option<Callback>)>(b"set_callback\0")
            .unwrap()(Some(psx_player::graphics_observer::common::callback));
        let error = lib
            .get::<unsafe extern "C" fn(u32, bool) -> u32>(b"consumed_error\0")
            .unwrap();
        let reads = lib
            .get::<unsafe extern "C" fn() -> u32>(b"error_reads\0")
            .unwrap();
        let destroy = lib
            .get::<unsafe extern "C" fn() -> u32>(b"destroy\0")
            .unwrap();
        assert_eq!(error(0x500, true), 0x500);
        assert_eq!(reads(), 1);
        assert_eq!(destroy(), 4);
        let factory_check = lib
            .get::<unsafe extern "C" fn(u32, u32, u32) -> u32>(b"factory_check\0")
            .unwrap();
        assert_eq!(factory_check(0x8892, 64, 0), 1);
        assert_eq!(factory_check(0x8892, 64, 0x500), 0);
        assert_eq!(reads(), 5);
        let run = lib
            .get::<unsafe extern "C" fn(bool, u32, bool) -> u32>(b"exercise\0")
            .unwrap();
        let waits = lib
            .get::<unsafe extern "C" fn() -> u32>(b"wait_calls\0")
            .unwrap();
        let deletes = lib
            .get::<unsafe extern "C" fn() -> u32>(b"delete_calls\0")
            .unwrap();
        assert_eq!(run(false, 0x911b, false), 1);
        assert_eq!(waits(), 0);
        assert_eq!(run(true, 0x911b, false), 0);
        assert_eq!(deletes(), 0);
        assert_eq!(run(true, 0x911c, false), 1);
        assert_eq!(deletes(), 1);
        run(true, 0x911d, true);
        assert_eq!(waits(), 3);
        assert_eq!(deletes(), 2);
        let before = psx_player::graphics_observer::common::snapshot();
        assert_eq!(before.first_runtime_failure.unwrap().operation, 8);
        assert_eq!(before.nonblocking_timeouts, 1);
        psx_player::graphics_observer::retiring();
        assert_eq!(error(0x505, false), 0x505);
        assert_eq!(reads(), 6);
        assert_eq!(destroy(), 0);
        let p = psx_player::graphics_observer::common::snapshot();
        assert_eq!(p.first_fatal.unwrap().phase, 9);
        assert_eq!(p.first_runtime_failure.unwrap().operation, 8);
        assert_eq!(psx_player::graphics_observer::take_fatal(), None);
        assert_eq!(psx_player::graphics_observer::take_fatal(), None);
    }
}
