//! Compile the pinned GLStreamBuffer and real mapped-write callers with controlled GL returns.
//! This is source-level synchronization evidence, not GPU or historical-cause proof.
#![cfg(target_os = "linux")]
use libloading::Library;
use std::{fs, path::PathBuf, process::Command};
type StreamObserve = extern "C" fn(u32, u32, u32, u32, u64, u32, u32, u32, u32);
static CALLBACK_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static CALLBACK_CREATES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
static CALLBACK_WAIT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[allow(clippy::too_many_arguments)]
extern "C" fn observed(
    stage: u32,
    _implementation: u32,
    _target: u32,
    _resource: u32,
    generation: u64,
    _offset: u32,
    _length: u32,
    _capacity: u32,
    result: u32,
) {
    use std::sync::atomic::Ordering::Relaxed;
    CALLBACK_GENERATION.store(generation, Relaxed);
    if stage == 62 {
        CALLBACK_CREATES.fetch_add(1, Relaxed);
    }
    if stage == 63 {
        CALLBACK_WAIT.store(result, Relaxed);
    }
}

fn without_includes(source: &str) -> String {
    source
        .lines()
        .filter(|l| !l.trim_start().starts_with("#include") && !l.starts_with("#pragma once"))
        .collect::<Vec<_>>()
        .join("\n")
}
fn function<'a>(source: &'a str, from: &str, next: &str) -> &'a str {
    source
        .split_once(from)
        .expect("retained caller start")
        .1
        .split_once(next)
        .expect("retained caller end")
        .0
}
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
struct Report {
    wrote: u32,
    waits: u32,
    creates: u32,
    deletes: u32,
    duplicate_delete: u32,
    position: u32,
    available: u32,
    second_mapping: u32,
    notified: u32,
    reason: u32,
    result: u32,
}
struct Fixture {
    _library: Library,
    run: unsafe extern "C" fn(u32, u32, u32) -> Report,
    factory: unsafe extern "C" fn(u32) -> u32,
    dir: PathBuf,
}
#[test]
fn complete_patch_stack_composes_with_the_retained_adapter_and_observer() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let donor = repo.join("vendor/armsx2");
    let dir = std::env::temp_dir().join(format!("psx-stream-patch-stack-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let patches = [
        "0001-host-pause-and-card-flush.patch",
        "0002-development-graphics-observer.patch",
        "0003-development-matched-reader.patch",
        "0004-stream-buffer-failure.patch",
        "0005-development-stream-observer.patch",
        "0006-development-creator-output.patch",
    ];
    let mut expected = None;
    for (number, order) in [[0, 3, 1, 2, 4, 5], [0, 1, 2, 3, 4, 5]]
        .into_iter()
        .enumerate()
    {
        let index = dir.join(format!("{number}.index"));
        let git = |args: &[&str]| {
            let mut c = Command::new("git");
            c.args(args)
                .current_dir(&donor)
                .env("GIT_INDEX_FILE", &index)
                .env("GIT_OPTIONAL_LOCKS", "0");
            c
        };
        let read = git(&["read-tree", "83fe4e293f71ab5587ef81b06143725bf8c6a30b"])
            .output()
            .unwrap();
        assert!(
            read.status.success(),
            "{}",
            String::from_utf8_lossy(&read.stderr)
        );
        for position in order {
            let patch = patches[position];
            let applied = git(&["apply", "--cached"])
                .arg(repo.join("patches/armsx2").join(patch))
                .output()
                .unwrap();
            assert!(
                applied.status.success(),
                "order{number} {patch}: {}",
                String::from_utf8_lossy(&applied.stderr)
            );
        }
        let combined = git(&[
            "diff",
            "--cached",
            "--binary",
            "83fe4e293f71ab5587ef81b06143725bf8c6a30b",
        ])
        .output()
        .unwrap();
        assert!(combined.status.success());
        if let Some(expected) = &expected {
            assert_eq!(
                &combined.stdout, expected,
                "warm and fresh patch order differ"
            );
        } else {
            expected = Some(combined.stdout);
        }
    }
}
impl Fixture {
    fn build() -> Self {
        let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = std::env::temp_dir().join(format!("psx-stream-sync-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let relative = "pcsx2/GS/Renderers/OpenGL";
        let root = dir.join(relative);
        fs::create_dir_all(&root).unwrap();
        for name in ["GLStreamBuffer.cpp", "GLStreamBuffer.h", "GSDeviceOGL.cpp"] {
            fs::copy(
                repo.join("vendor/armsx2").join(relative).join(name),
                root.join(name),
            )
            .unwrap();
        }
        for patch in [
            Some("0004-stream-buffer-failure.patch"),
            cfg!(feature = "graphics-observer").then_some("0005-development-stream-observer.patch"),
        ]
        .into_iter()
        .flatten()
        {
            let applied = Command::new("git")
                .arg("apply")
                .args([
                    "--include=pcsx2/GS/Renderers/OpenGL/GLStreamBuffer.cpp",
                    "--include=pcsx2/GS/Renderers/OpenGL/GLStreamBuffer.h",
                    "--include=pcsx2/GS/Renderers/OpenGL/GSDeviceOGL.cpp",
                ])
                .arg(repo.join("patches/armsx2").join(patch))
                .current_dir(&dir)
                .output()
                .unwrap();
            assert!(
                applied.status.success(),
                "{}",
                String::from_utf8_lossy(&applied.stderr)
            );
        }
        let helper = fs::read_to_string(root.join("GLStreamBuffer.cpp")).unwrap();
        let header = fs::read_to_string(root.join("GLStreamBuffer.h")).unwrap();
        assert_eq!(
            header.contains("SetStreamObserver"),
            cfg!(feature = "graphics-observer"),
            "ordinary builds must exclude additional observations"
        );
        let callers = fs::read_to_string(root.join("GSDeviceOGL.cpp")).unwrap();
        let upload = function(
            &callers,
            "__fi static void WriteToStreamBuffer",
            "void GSDeviceOGL::VSSetUniformBuffer",
        );
        let vertex = function(
            &callers,
            "void GSDeviceOGL::IASetVertexBuffer",
            "void GSDeviceOGL::SetIndexBuffer",
        );
        let index = function(
            &callers,
            "void GSDeviceOGL::SetIndexBuffer",
            "void GSDeviceOGL::IASetIndexBuffer",
        );
        let observer_hook = if cfg!(feature = "graphics-observer") {
            "s_stream_generation.store(0x100000000ULL); GLStreamBuffer::SetStreamObserver([](u32 stage,u32 implementation,u32 target,u32 resource,u64 generation,u32 offset,u32 length,u32 capacity,u32 result){ if(!generation) std::abort(); if(stage==60) selected=implementation; if(stage==62) observed_creates++; if(stage==63) observed_wait=result; if(rust_observer)rust_observer(stage,implementation,target,resource,generation,offset,length,capacity,result); });"
        } else {
            ""
        };
        let source = format!(
            r#"
// SPDX-FileCopyrightText: 2002-2026 PCSX2 Dev Team
// SPDX-License-Identifier: GPL-3.0+
// Retained helper/header/callers below are included verbatim except includes.
// Only platform/GL declarations and controlled dispatch are generated fixtures.
#include <algorithm>
#include <array>
#include <cstring>
#include <cstdlib>
#include <memory>
#include <tuple>
#include <vector>
#include <cstdint>
#include <atomic>
#define __fi inline
#define protected public
#define pxAssert(x) ((void)0)
#define pxAssertRel(x,msg) ((void)0)
#define pxFailRel(msg) std::abort()
using u8=uint8_t; using u16=uint16_t; using u32=uint32_t; using u64=uint64_t;
using StreamObserve=void(*)(u32,u32,u32,u32,u64,u32,u32,u32,u32);
static StreamObserve rust_observer=nullptr;
extern "C" void set_test_observer(StreamObserve callback) {{rust_observer=callback;}}
using GLenum=unsigned; using GLuint=unsigned; using GLsync=void*;
constexpr GLenum GL_NO_ERROR=0, GL_STREAM_DRAW=0x88e0, GL_ARRAY_BUFFER=0x8892, GL_UNIFORM_BUFFER=0x8a11;
constexpr GLenum GL_RENDERER=0x1f01, GL_VENDOR=0x1f00, GL_SYNC_GPU_COMMANDS_COMPLETE=0x9117;
constexpr GLenum GL_SYNC_FLUSH_COMMANDS_BIT=1, GL_ALREADY_SIGNALED=0x911a, GL_TIMEOUT_EXPIRED=0x911b, GL_CONDITION_SATISFIED=0x911c, GL_WAIT_FAILED=0x911d;
constexpr GLenum GL_MAP_WRITE_BIT=2, GL_MAP_PERSISTENT_BIT=64, GL_MAP_COHERENT_BIT=128, GL_MAP_FLUSH_EXPLICIT_BIT=16;
constexpr u64 GL_TIMEOUT_IGNORED=~u64(0);
static bool GLAD_GL_VERSION_4_4=false, GLAD_GL_ARB_buffer_storage=false, GLAD_GL_EXT_buffer_storage=true;
static u8 mapped[256]; static u32 waits,creates,deletes,duplicate_delete,wait_result,null_at;
static unsigned deleted[64];static u32 notified,fail_reason,fail_result,selected,observed_creates,observed_wait;
static u32 errors_read,error_pending;static bool reject_storage=false;
static const char* renderer="Generated fixture";static const char* vendor="Generic";
static GLenum glGetError() {{ ++errors_read;const auto result=error_pending;error_pending=0;return result; }}
static const u8* glGetString(GLenum which) {{return reinterpret_cast<const u8*>(which==GL_RENDERER?renderer:vendor);}}
static void glBindBuffer(GLenum,GLuint) {{}}
static void glGenBuffers(int,GLuint* id) {{*id=1;}}
static void glDeleteBuffers(int,const GLuint*) {{}}
static void glBufferData(GLenum,u32,const void*,GLenum) {{}}
static void glBufferSubData(GLenum,u32,u32,const void*) {{}}
static void glBufferStorage(GLenum,u32,const void*,u32) {{if(reject_storage)error_pending=0x505;}}
static void glBufferStorageEXT(GLenum,u32,const void*,u32) {{if(reject_storage)error_pending=0x505;}}
static void* glMapBufferRange(GLenum,u32,u32,u32) {{return mapped;}}
static void glUnmapBuffer(GLenum) {{}}
static void glFlushMappedBufferRange(GLenum,u32,u32) {{}}
static void glBindBufferRange(GLenum,u32,GLuint,u32,u32) {{}}
static GLsync glFenceSync(GLenum,u32) {{++creates;return creates==null_at?nullptr:reinterpret_cast<void*>(uintptr_t(creates));}}
static GLenum glClientWaitSync(GLsync,u32,u64) {{++waits;return wait_result;}}
static void glDeleteSync(GLsync s) {{ if(!s) return; ++deletes; const auto id=uintptr_t(s); if(id<64&&deleted[id]++) ++duplicate_delete; }}
static void* _aligned_malloc(size_t size,size_t alignment) {{void* p=nullptr;return posix_memalign(&p,alignment,size)?nullptr:p;}}
static void _aligned_free(void* p) {{free(p);}}
namespace Common {{template<class T> T AlignUp(T value,T alignment) {{return ((value+alignment-1)/alignment)*alignment;}}}}
{header}
{helper}
struct GSVertexPT1 {{u8 bytes[32];}};
class GSDeviceOGL {{public:
 struct Span {{u32 start=0;size_t count=0;}} m_vertex,m_index;
 std::unique_ptr<GLStreamBuffer> m_vertex_stream_buffer;
 void IASetVertexBuffer(const void* vertices,size_t count,size_t align_multiplier);
 void SetIndexBuffer(std::unique_ptr<GLStreamBuffer>& buffer,const void* index,size_t count);
}};
__fi static void WriteToStreamBuffer{upload}
void GSDeviceOGL::IASetVertexBuffer{vertex}
void GSDeviceOGL::SetIndexBuffer{index}
struct Report {{u32 wrote,waits,creates,deletes,duplicate_delete,position,available,second_mapping,notified,reason,result;}};
extern "C" Report exercise(u32 status,u32 null_creation,u32 caller) {{
 waits=creates=deletes=duplicate_delete=0; std::fill(std::begin(deleted),std::end(deleted),0);
 notified=fail_reason=fail_result=selected=observed_creates=observed_wait=0;
 GLStreamBuffer::SetFailureCallback([](u32 reason,u32 result){{++notified;fail_reason=reason;fail_result=result;}});
 {observer_hook}
 wait_result=status;null_at=null_creation;std::memset(mapped,0,sizeof(mapped));
 errors_read=error_pending=0;
 Report r{{}};
 {{
 auto buffer=GLStreamBuffer::Create(GL_ARRAY_BUFFER,256);
 auto first=buffer->Map(1,256); if(!first.pointer) std::abort();
 std::memset(first.pointer,0x11,256);buffer->Unmap(256);
 u8 data[32];std::memset(data,0x55,sizeof(data));
 if(caller==0) WriteToStreamBuffer(buffer.get(),1,1,data,16);
 else if(caller==1) {{ GSDeviceOGL device;device.m_vertex_stream_buffer=std::move(buffer);device.IASetVertexBuffer(data,1,1);buffer=std::move(device.m_vertex_stream_buffer); }}
 else {{GSDeviceOGL device;device.SetIndexBuffer(buffer,data,8);}}
 r.wrote=mapped[0]==0x55;
 auto* actual=dynamic_cast<SyncingStreamBuffer*>(buffer.get());
 r.position=actual->m_position;r.available=actual->m_available_block_index;
 if((status!=GL_ALREADY_SIGNALED&&status!=GL_CONDITION_SATISFIED) || null_creation) r.second_mapping=buffer->Map(1,16).pointer!=nullptr;
 }}
 if({observing}) {{if(selected!=1||observed_creates!=creates) std::abort(); if(waits&&observed_wait!=status) std::abort();}}
 if(errors_read!=2) std::abort(); // no extra/destructive observer error reads
 r.waits=waits;r.creates=creates;r.deletes=deletes;r.duplicate_delete=duplicate_delete;r.notified=notified;r.reason=fail_reason;r.result=fail_result;return r;
}}
extern "C" u32 factory(u32 scenario) {{
 GLStreamBuffer::SetFailureCallback(nullptr);selected=0;errors_read=error_pending=0;
 GLAD_GL_EXT_buffer_storage=scenario!=1&&scenario!=2;
 renderer=scenario==3?"Adreno 650":"Generated fixture";
 vendor=scenario==2?"NVIDIA":"Generic";reject_storage=scenario==4;
 auto buffer=GLStreamBuffer::Create(GL_ARRAY_BUFFER,256);
 if(!buffer) std::abort();
 const u32 actual=dynamic_cast<BufferStorageStreamBuffer*>(buffer.get())?1:(dynamic_cast<BufferSubDataStreamBuffer*>(buffer.get())?2:3);
 if({observing}&&selected!=actual) std::abort();
 if(errors_read!=(scenario==4?4:2)) std::abort();
 return actual;
}}
"#,
            observing = if cfg!(feature = "graphics-observer") {
                "true"
            } else {
                "false"
            },
            header = without_includes(&header),
            helper = without_includes(&helper)
        );
        fs::write(dir.join("retained-stream.cpp"), source).unwrap();
        let output = Command::new("c++")
            .args([
                "-std=c++20",
                "-shared",
                "-fPIC",
                "-O1",
                "-Wall",
                "-Wextra",
                "-Wno-unused-function",
                "retained-stream.cpp",
                "-o",
                "retained-stream.so",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let library = unsafe { Library::new(dir.join("retained-stream.so")) }.unwrap();
        let run = unsafe {
            *library
                .get::<unsafe extern "C" fn(u32, u32, u32) -> Report>(b"exercise\0")
                .unwrap()
        };
        if cfg!(feature = "graphics-observer") {
            let set = unsafe {
                library
                    .get::<unsafe extern "C" fn(StreamObserve)>(b"set_test_observer\0")
                    .unwrap()
            };
            unsafe { set(observed) };
        }
        let factory = unsafe {
            *library
                .get::<unsafe extern "C" fn(u32) -> u32>(b"factory\0")
                .unwrap()
        };
        Self {
            _library: library,
            run,
            factory,
            dir,
        }
    }
}
#[test]
fn actual_wait_failures_never_authorize_retained_callers_to_write() {
    let fixture = Fixture::build();
    eprintln!("retained executable fixture {}", fixture.dir.display());
    for caller in 0..3 {
        for status in [0x911a, 0x911c] {
            let r = unsafe { (fixture.run)(status, 0, caller) };
            assert_eq!(r.wrote, 1, "successful retained caller{caller}: {r:?}");
            assert_eq!(r.duplicate_delete, 0, "{r:?}");
            assert_eq!(r.notified, 0, "{r:?}");
            assert_eq!(
                r.deletes, r.creates,
                "remaining owned fences leaked at cleanup: {r:?}"
            );
        }
        for status in [0x911b, 0x911d, 0xdeadbeef] {
            let r = unsafe { (fixture.run)(status, 0, caller) };
            assert_eq!(
                r.wrote, 0,
                "unsuccessful wait authorized a real caller{caller} write: {r:?}"
            );
            assert_eq!(
                r.second_mapping, 0,
                "failed runtime regranted a writable region: {r:?}"
            );
            assert_eq!(
                r.position, 256,
                "failed dependency rewound allocation: {r:?}"
            );
            assert_eq!(r.duplicate_delete, 0, "{r:?}");
            assert_eq!((r.notified, r.reason, r.result), (1, 1, status), "{r:?}");
            assert_eq!(r.deletes, r.creates, "{r:?}");
        }
        for null_at in [1, 5, 16] {
            let r = unsafe { (fixture.run)(0x911a, null_at, caller) };
            assert_eq!(
                r.wrote, 0,
                "null creation authorized real caller{caller} write: {r:?}"
            );
            assert_eq!(r.second_mapping, 0, "{r:?}");
            assert_eq!(r.duplicate_delete, 0, "{r:?}");
            assert_eq!((r.notified, r.reason, r.result), (1, 2, 0), "{r:?}");
            assert_eq!(r.waits, 0, "null creation still submitted a wait: {r:?}");
            assert_eq!(r.deletes + 1, r.creates, "{r:?}");
        }
    }
    for (scenario, expected) in [(0, 1), (1, 3), (2, 2), (3, 3), (4, 3)] {
        assert_eq!(
            unsafe { (fixture.factory)(scenario) },
            expected,
            "actual factory branch {scenario}"
        );
    }
    if cfg!(feature = "graphics-observer") {
        use std::sync::atomic::Ordering::Relaxed;
        assert!(
            CALLBACK_GENERATION.load(Relaxed) > u32::MAX as u64,
            "native generation truncated crossing the callback ABI"
        );
        assert!(CALLBACK_CREATES.load(Relaxed) > 0);
        assert_eq!(CALLBACK_WAIT.load(Relaxed), 0xdeadbeef);
    }
}
