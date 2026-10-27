//! Exercise the actual patched Vulkan handoff, not a Rust model of it.
#![cfg(target_os = "linux")]
use libloading::Library;
use std::{
    fs,
    path::{Path, PathBuf},
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

fn compile(dir: &Path, actual: &str, name: &str) -> PathBuf {
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
    fs::write(dir.join(format!("{name}.cpp")), source).unwrap();
    let compiled = Command::new("c++")
        .args([
            "-std=c++17",
            "-shared",
            "-fPIC",
            "-pthread",
            &format!("{name}.cpp"),
            "-o",
            &format!("{name}.so"),
        ])
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    dir.join(format!("{name}.so"))
}

#[test]
fn frozen_and_overlapped_publication_preserve_completion_and_abort_join() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("psx-vulkan-handoff-{}", std::process::id()));
    let relative = "pcsx2/GS/Renderers/Vulkan/VKLibretro.cpp";
    let caller =
        fs::read_to_string(root.join("vendor/armsx2/pcsx2/GS/Renderers/Vulkan/GSDeviceVK.cpp"))
            .unwrap();
    assert!(caller.contains("constexpr int kLibretroBackbuffers = 3;"));
    assert!(caller.contains("constexpr u64 kLibretroRetireFrames = 6;"));
    assert!(caller.contains("s_libretro_bb_idx = (s_libretro_bb_idx + 1) % kLibretroBackbuffers;\n\t\tVKLibretro::PublishFrame(frame);"));
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
    let frozen = compile(&dir, &actual, "handoff-frozen");
    unsafe {
        let library = Library::new(&frozen).unwrap();
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
    let patched = Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/Vulkan/VKLibretro.cpp",
        ])
        .arg(root.join("patches/armsx2/0011-vulkan-producer-overlap.patch"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        patched.status.success(),
        "{}",
        String::from_utf8_lossy(&patched.stderr)
    );
    let actual = fs::read_to_string(dir.join(relative)).unwrap();
    let overlap = compile(&dir, &actual, "handoff-overlap");
    unsafe {
        let library = Library::new(overlap).unwrap();
        let publish = *library
            .get::<unsafe extern "C" fn(usize)>(b"publish_image\0")
            .unwrap();
        let consume = *library
            .get::<unsafe extern "C" fn() -> bool>(b"consume\0")
            .unwrap();
        let image = *library
            .get::<unsafe extern "C" fn() -> usize>(b"current_image\0")
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
        let shutdown = *library
            .get::<unsafe extern "C" fn()>(b"shutdown\0")
            .unwrap();
        pacing(true);
        let (prepared, preparation) = mpsc::channel();
        let (published, publication) = mpsc::channel();
        let producer = std::thread::spawn(move || {
            publish(42);
            // Mirrors the retained caller's next, distinct backbuffer: its
            // CPU/GPU preparation may proceed before the prior image read ends.
            prepared.send(()).unwrap();
            publish(43);
            published.send(()).unwrap();
        });
        preparation
            .recv_timeout(Duration::from_secs(1))
            .expect("first publication still blocks next-frame preparation");
        assert!(consume());
        assert_eq!(image(), 42);
        stale();
        assert!(!consume());
        assert!(
            publication.recv_timeout(Duration::from_millis(30)).is_err(),
            "next publication overwrote an incomplete read"
        );
        assert_eq!(image(), 42);
        assert!(complete());
        publication.recv_timeout(Duration::from_secs(1)).unwrap();
        producer.join().unwrap();
        assert!(consume());
        assert_eq!(image(), 43);
        assert!(!complete_stale(), "old completion released the next image");
        // A failed read supplies no completion. Stop must release the pending
        // publication without waiting on the failed dependency or another owner.
        let (started, entered) = mpsc::channel();
        let (finished, stopped) = mpsc::channel();
        let producer = std::thread::spawn(move || {
            started.send(()).unwrap();
            publish(44);
            finished.send(()).unwrap();
        });
        entered.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(stopped.recv_timeout(Duration::from_millis(30)).is_err());
        assert!(!consume());
        pacing(false);
        stopped.recv_timeout(Duration::from_secs(1)).unwrap();
        producer.join().unwrap();
        assert!(consume());
        assert_eq!(image(), 44);
        shutdown();
        assert!(!consume());
        assert!(!complete());
    }
    fs::remove_dir_all(dir).unwrap();
}
