//! Exercise the actual patched Vulkan handoff, not a Rust model of it.
#![cfg(target_os = "linux")]
use libloading::Library;
use std::{
    fs,
    process::Command,
    sync::{mpsc, Mutex},
    time::Duration,
};
static WAKE: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);
unsafe extern "C" fn wake() {
    if let Some(tx) = &*WAKE.lock().unwrap() {
        let _ = tx.try_send(());
    }
}

#[test]
fn producer_cannot_reuse_delivered_image_until_completed_and_abort_joins() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("psx-vulkan-handoff-{}", std::process::id()));
    let relative = "pcsx2/GS/Renderers/Vulkan/VKLibretro.cpp";
    fs::create_dir_all(dir.join("pcsx2/GS/Renderers/Vulkan")).unwrap();
    fs::copy(
        root.join("vendor/armsx2").join(relative),
        dir.join(relative),
    )
    .unwrap();
    let patch = Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/Vulkan/VKLibretro.cpp",
        ])
        .arg(root.join("patches/armsx2/0010-vulkan-frame-completion.patch"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        patch.status.success(),
        "{}",
        String::from_utf8_lossy(&patch.stderr)
    );
    let actual = fs::read_to_string(dir.join(relative)).unwrap();
    let body = actual
        .split_once("\tstatic std::mutex s_frame_mutex;")
        .unwrap()
        .1;
    let source = format!(
        r#"
#include <mutex>
#include <condition_variable>
#include <cstdint>
using u64=uint64_t; using VkImage=uintptr_t;
constexpr VkImage VK_NULL_HANDLE=0;
namespace VKLibretro {{
struct InitInfo {{}}; InitInfo Init; void* s_hw_render=nullptr;
struct Frame {{u64 serial=0; VkImage image=0;}};
static std::mutex s_frame_mutex;
{body}
static VKLibretro::Frame current, stale;
extern "C" void publish() {{ VKLibretro::Frame f; f.image=42; VKLibretro::PublishFrame(f); }}
extern "C" void publish_image(uintptr_t image) {{ VKLibretro::Frame f; f.image=image; VKLibretro::PublishFrame(f); }}
extern "C" bool consume() {{ return VKLibretro::ConsumeFrame(&current); }}
extern "C" uintptr_t current_image() {{ return current.image; }}
extern "C" bool complete() {{ return VKLibretro::CompleteFrame(current); }}
extern "C" void save_stale() {{ stale=current; }}
extern "C" bool complete_stale() {{ return VKLibretro::CompleteFrame(stale); }}
extern "C" void pacing(bool enabled) {{ VKLibretro::SetPacing(enabled); }}
extern "C" void callback(void(*f)()) {{ VKLibretro::SetFrameReadyCallback(f); }}
extern "C" void shutdown() {{ VKLibretro::Shutdown(); }}
"#
    );
    fs::write(dir.join("handoff.cpp"), source).unwrap();
    let compiled = Command::new("c++")
        .args([
            "-std=c++17",
            "-shared",
            "-fPIC",
            "-pthread",
            "handoff.cpp",
            "-o",
            "handoff.so",
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    unsafe {
        let library = Library::new(dir.join("handoff.so")).unwrap();
        let publish = *library.get::<unsafe extern "C" fn()>(b"publish\0").unwrap();
        let consume = *library
            .get::<unsafe extern "C" fn() -> bool>(b"consume\0")
            .unwrap();
        let complete = *library
            .get::<unsafe extern "C" fn() -> bool>(b"complete\0")
            .unwrap();
        let stale = *library
            .get::<unsafe extern "C" fn()>(b"save_stale\0")
            .unwrap();
        let complete_stale = *library
            .get::<unsafe extern "C" fn() -> bool>(b"complete_stale\0")
            .unwrap();
        let pacing = *library
            .get::<unsafe extern "C" fn(bool)>(b"pacing\0")
            .unwrap();
        let callback = *library
            .get::<unsafe extern "C" fn(Option<unsafe extern "C" fn()>)>(b"callback\0")
            .unwrap();
        let shutdown = *library
            .get::<unsafe extern "C" fn()>(b"shutdown\0")
            .unwrap();
        let (tx, rx) = mpsc::sync_channel(1);
        *WAKE.lock().unwrap() = Some(tx);
        callback(Some(wake));
        pacing(true);
        let (done, completed) = mpsc::channel();
        let producer = std::thread::spawn(move || {
            publish();
            done.send(()).unwrap();
        });
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(consume());
        assert!(!consume());
        stale();
        assert!(
            completed.recv_timeout(Duration::from_millis(30)).is_err(),
            "delivery prematurely released the producer"
        );
        assert!(complete());
        assert!(!complete());
        completed.recv_timeout(Duration::from_secs(1)).unwrap();
        producer.join().unwrap();
        let producer = std::thread::spawn(move || {
            publish();
        });
        rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(consume());
        assert!(
            !complete_stale(),
            "old acknowledgement released a reused image"
        );
        // Failed frontend: no completion. Owner's stop must still join.
        pacing(false);
        producer.join().unwrap();
        callback(None);
        shutdown();
        assert!(!consume());
        assert!(!complete());
        // Actual pause disables pacing; it leaves a published startup image
        // pending. A restore must drain it before GS defrost invalidates that
        // target, then wait for a newly produced picture after unpausing.
        let publish_image = *library
            .get::<unsafe extern "C" fn(usize)>(b"publish_image\0")
            .unwrap();
        let current_image = *library
            .get::<unsafe extern "C" fn() -> usize>(b"current_image\0")
            .unwrap();
        publish_image(100);
        pacing(false);
        assert!(
            consume(),
            "pause unexpectedly discarded the startup handoff"
        );
        assert_eq!(current_image(), 100);
        assert!(complete());
        assert!(!consume(), "drained startup image remained deliverable");
        publish_image(200);
        assert!(consume());
        assert_eq!(current_image(), 200);
        assert!(complete());
        shutdown();
        *WAKE.lock().unwrap() = None;
    }
    fs::remove_dir_all(dir).unwrap();
}
