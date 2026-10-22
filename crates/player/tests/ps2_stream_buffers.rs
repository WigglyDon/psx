//! Retained donor factory selection with mechanical GL/factory types. This
//! checks the exact workaround boundary, not GPU execution or performance.
#![cfg(target_os = "linux")]

use libloading::Library;
use std::{fs, process::Command};

#[test]
fn old_adreno_upload_fallback_does_not_disable_other_persistent_buffers() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("psx-stream-selection-{}", std::process::id()));
    let relative = "pcsx2/GS/Renderers/OpenGL/GLStreamBuffer.cpp";
    fs::create_dir_all(temp.join("pcsx2/GS/Renderers/OpenGL")).unwrap();
    fs::copy(
        repo.join("vendor/armsx2").join(relative),
        temp.join(relative),
    )
    .unwrap();
    let patch = Command::new("git")
        .args(["apply", "--allow-empty"])
        .arg(format!("--include={relative}"))
        .arg(repo.join("patches/armsx2/0001-host-pause-and-card-flush.patch"))
        .current_dir(&temp)
        .output()
        .unwrap();
    assert!(
        patch.status.success(),
        "{}",
        String::from_utf8_lossy(&patch.stderr)
    );
    let source = fs::read_to_string(temp.join(relative)).unwrap();
    let start = source
        .find("std::unique_ptr<GLStreamBuffer> GLStreamBuffer::Create(")
        .unwrap();
    let factory = &source[start..];
    // Only declarations/factory results are mechanical. The selection body is
    // taken verbatim from the pinned GPL donor plus the packaged tracked patch.
    let fixture = format!(
        r#"
#include <cstdint>
#include <cstring>
#include <memory>
using u32 = std::uint32_t;
using GLenum = unsigned;
static constexpr GLenum GL_RENDERER=1, GL_VENDOR=2, GL_VERSION=3;
static const char *renderer, *version;
static constexpr bool GLAD_GL_VERSION_4_4=true, GLAD_GL_ARB_buffer_storage=false,
    GLAD_GL_EXT_buffer_storage=false;
const unsigned char* glGetString(GLenum name) {{
    return reinterpret_cast<const unsigned char*>(name==GL_RENDERER ? renderer :
        name==GL_VERSION ? version : "Qualcomm");
}}
struct GLStreamBuffer {{
    unsigned strategy;
    static std::unique_ptr<GLStreamBuffer> Create(GLenum, u32);
}};
struct BufferStorageStreamBuffer {{
    static auto Create(GLenum, u32) {{ return std::make_unique<GLStreamBuffer>(GLStreamBuffer{{1}}); }}
}};
struct BufferDataStreamBuffer {{
    static auto Create(GLenum, u32) {{ return std::make_unique<GLStreamBuffer>(GLStreamBuffer{{2}}); }}
}};
struct BufferSubDataStreamBuffer {{
    static auto Create(GLenum, u32) {{ return std::make_unique<GLStreamBuffer>(GLStreamBuffer{{3}}); }}
}};
{factory}
extern "C" unsigned choose(const char* r, const char* v) {{
    renderer=r; version=v;
    return GLStreamBuffer::Create(0, 128)->strategy;
}}
"#
    );
    fs::write(temp.join("fixture.cpp"), fixture).unwrap();
    let build = Command::new("c++")
        .args([
            "-std=c++20",
            "-shared",
            "-fPIC",
            "fixture.cpp",
            "-o",
            "fixture.so",
        ])
        .current_dir(&temp)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    unsafe {
        let library = Library::new(temp.join("fixture.so")).unwrap();
        let choose = library
            .get::<unsafe extern "C" fn(*const std::ffi::c_char, *const std::ffi::c_char) -> u32>(
                b"choose\0",
            )
            .unwrap();
        for (renderer, version, expected) in [
            ("Adreno (TM) 640", "OpenGL ES 3.2 V@0490.0", 2),
            ("Adreno (TM) 640", "OpenGL ES 3.2 V@0502.0", 1),
            ("Adreno (TM) 640", "OpenGL ES 3.2 Mesa 25.2", 1),
            ("Adreno (TM) 650", "OpenGL ES 3.2 V@0490.0", 2),
            ("Adreno (TM) 730", "OpenGL ES 3.2 V@0490.0", 1),
            ("Adreno (TM) 840", "OpenGL ES 3.2 V@0490.0", 1),
            ("Mali-G710", "OpenGL ES 3.2 V@0490.0", 1),
        ] {
            let r = std::ffi::CString::new(renderer).unwrap();
            let v = std::ffi::CString::new(version).unwrap();
            assert_eq!(
                choose(r.as_ptr(), v.as_ptr()),
                expected,
                "{renderer} / {version}"
            );
        }
    }
    fs::remove_dir_all(temp).unwrap();
}
