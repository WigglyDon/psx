//! Exercise the patched donor's actual handoff with GL/context calls stubbed.
//! This checks notification/ownership, not GPU ordering or emulation speed.
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
using u64 = std::uint64_t;
using GLsync = void*;
void glDeleteSync(GLsync) {{}}
namespace GLLibretro {{
bool Active = false;
void ReleaseFrontendContext() {{}}
struct Frame {{ unsigned texture = 0; GLsync fence = nullptr; unsigned width = 0, height = 0; }};
{handoff}
extern "C" void set_callback(void (*cb)()) {{ GLLibretro::SetFrameReadyCallback(cb); }}
extern "C" void set_pacing(bool value) {{ GLLibretro::SetPacing(value); }}
extern "C" void publish() {{ GLLibretro::Frame frame; frame.texture = 1; GLLibretro::PublishFrame(frame); }}
extern "C" bool consume() {{ GLLibretro::Frame frame; return GLLibretro::ConsumeFrame(&frame); }}
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
    }
    fs::remove_dir_all(temp).unwrap();
}
