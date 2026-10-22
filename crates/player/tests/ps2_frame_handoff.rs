//! Exercise the patched donor's actual handoff with GL/context calls stubbed.
//! This checks notification and cross-context fence ownership, not GPU execution
//! or emulation speed.
#![cfg(target_os = "linux")]

use libloading::Library;
use std::{fs, process::Command, sync::mpsc, sync::Mutex, time::Duration};

static WAKE: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);
unsafe extern "C" fn wake() {
    if let Some(tx) = &*WAKE.lock().unwrap() {
        let _ = tx.try_send(());
    }
}

#[test]
fn published_frames_wake_without_waiting_for_a_frontend_tick_and_retire_cleanly() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("psx-frame-handoff-{}", std::process::id()));
    fs::create_dir_all(temp.join("pcsx2/GS/Renderers/OpenGL")).unwrap();
    let relative = "pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp";
    fs::copy(
        repo.join("vendor/armsx2").join(relative),
        temp.join(relative),
    )
    .unwrap();
    let patch = Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp",
        ])
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
    let (_, handoff) = source
        .split_once("// GS thread -> retro_run frame handoff")
        .unwrap();
    // Mechanical fixture types/ABI only. The mutex/CV/notification logic below
    // is extracted verbatim from pinned GPL donor source plus our tracked patch.
    let fixture = format!(
        r#"
#include <mutex>
#include <condition_variable>
#include <cstdint>
#include <atomic>
#include <vector>
using u64 = std::uint64_t;
using GLuint = unsigned;
using GLsync = void*;
constexpr auto GL_TIMEOUT_IGNORED = ~std::uint64_t(0);
static std::atomic<unsigned> read_waits = 0, read_deletes = 0, bad_order = 0;
void glWaitSync(GLsync fence, unsigned, std::uint64_t) {{
    if (fence == reinterpret_cast<void*>(1)) read_waits++;
}}
void glDeleteSync(GLsync fence) {{
    if (fence == reinterpret_cast<void*>(1)) {{
        if (read_waits.load() <= read_deletes.load()) bad_order++;
        read_deletes++;
    }}
}}
namespace GLLibretro {{
bool Active = false;
void ReleaseFrontendContext() {{}}
struct Frame {{ unsigned texture = 0; GLsync fence = nullptr; unsigned width = 0, height = 0; }};
{handoff}
extern "C" void set_callback(void (*cb)()) {{ GLLibretro::SetFrameReadyCallback(cb); }}
extern "C" void set_pacing(bool value) {{ GLLibretro::SetPacing(value); }}
extern "C" void publish() {{ GLLibretro::Frame frame; frame.texture = 1; GLLibretro::PublishFrame(frame); }}
static GLLibretro::Frame consumed;
extern "C" bool consume() {{ return GLLibretro::ConsumeFrame(&consumed); }}
extern "C" void complete() {{ GLLibretro::CompleteFrame(consumed, reinterpret_cast<void*>(1)); }}
extern "C" void reuse(unsigned texture) {{ GLLibretro::WaitForFrontend(texture); }}
extern "C" unsigned waits() {{ return read_waits.load(); }}
extern "C" unsigned deletes() {{ return read_deletes.load(); }}
extern "C" unsigned order_errors() {{ return bad_order.load(); }}
extern "C" void shutdown_handoff() {{ GLLibretro::Shutdown(); }}
"#
    );
    fs::write(temp.join("fixture.cpp"), fixture).unwrap();
    let build = Command::new("c++")
        .args([
            "-std=c++20",
            "-shared",
            "-fPIC",
            "-pthread",
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
        let callback = *library
            .get::<unsafe extern "C" fn(Option<unsafe extern "C" fn()>)>(b"set_callback\0")
            .unwrap();
        let pacing = *library
            .get::<unsafe extern "C" fn(bool)>(b"set_pacing\0")
            .unwrap();
        let publish = *library.get::<unsafe extern "C" fn()>(b"publish\0").unwrap();
        let consume = *library
            .get::<unsafe extern "C" fn() -> bool>(b"consume\0")
            .unwrap();
        let complete = *library
            .get::<unsafe extern "C" fn()>(b"complete\0")
            .unwrap();
        let reuse = *library
            .get::<unsafe extern "C" fn(u32)>(b"reuse\0")
            .unwrap();
        let waits = *library
            .get::<unsafe extern "C" fn() -> u32>(b"waits\0")
            .unwrap();
        let deletes = *library
            .get::<unsafe extern "C" fn() -> u32>(b"deletes\0")
            .unwrap();
        let shutdown = *library
            .get::<unsafe extern "C" fn()>(b"shutdown_handoff\0")
            .unwrap();
        let order_errors = *library
            .get::<unsafe extern "C" fn() -> u32>(b"order_errors\0")
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        *WAKE.lock().unwrap() = Some(tx);
        pacing(true);
        callback(Some(wake));
        let (done_tx, done_rx) = mpsc::sync_channel(1);
        let producer = std::thread::spawn(move || {
            publish();
            done_tx.send(()).unwrap();
        });
        let notified = rx.recv_timeout(Duration::from_secs(2)).is_ok();
        let blocked = done_rx.try_recv().is_err();
        let consumed = consume();
        complete();
        callback(None);
        pacing(false); // Release the producer even if a check will fail.
        let returned = done_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        producer.join().unwrap();
        assert!(notified && blocked && consumed && returned);
        assert!(!consume());

        // A frame already waiting when a frontend attaches must also wake it.
        pacing(true);
        let producer = std::thread::spawn(move || publish());
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let attached = loop {
            callback(Some(wake));
            if rx.try_recv().is_ok() {
                break true;
            }
            if std::time::Instant::now() >= deadline {
                break false;
            }
            std::thread::yield_now();
        };
        callback(None); // Retire the callback before releasing platform state.
        *WAKE.lock().unwrap() = None;
        pacing(false); // Interruption/shutdown cannot leave the producer parked.
        producer.join().unwrap();
        assert!(attached);

        // A producer may use another backbuffer, but cannot reuse the one
        // currently being read just because ConsumeFrame returned. Disabling
        // pacing for pause/teardown must not bypass that ownership boundary.
        shutdown();
        publish();
        assert!(consume());
        assert!(!consume());
        reuse(2);
        assert_eq!(waits(), 0);
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (reused_tx, reused_rx) = mpsc::sync_channel(1);
        let producer = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            reuse(1);
            reused_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let premature = reused_rx.recv_timeout(Duration::from_millis(50)).is_ok();
        pacing(false);
        complete();
        let retired = reused_rx.recv_timeout(Duration::from_secs(2)).is_ok();
        producer.join().unwrap();
        assert!(!premature && retired);
        // The server wait must precede deletion, exactly once on reuse.
        assert_eq!(waits(), 1);
        assert_eq!(deletes(), 1);
        reuse(1);
        assert_eq!(waits(), 1);

        // An unconsumed frame lapped while pacing is disabled must disappear
        // before its texture is overwritten, rather than be acquired later.
        publish();
        reuse(1);
        assert!(!consume());
        // Normal resource destruction uses the same retirement operation.
        publish();
        assert!(consume());
        complete();
        reuse(1);
        assert_eq!(waits(), 2);
        assert_eq!(deletes(), 2);
        assert_eq!(order_errors(), 0);
        shutdown();
    }
    fs::remove_dir_all(temp).unwrap();
}
