//! Patched capture/pack ABI with mechanical VM/ZIP stubs. This tests
//! ownership and thread separation, not full serialization or device speed.
use super::*;
use std::{process::Command, sync::mpsc};

#[test]
fn frozen_checkpoint_packages_after_resume_and_releases_on_failure_or_cancel() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("psx-capture-pack-{}", std::process::id()));
    fs::create_dir_all(temp.join("pcsx2-libretro")).unwrap();
    let relative = "pcsx2-libretro/Main.cpp";
    fs::copy(
        repo.join("vendor/armsx2").join(relative),
        temp.join(relative),
    )
    .unwrap();
    let patch = Command::new("git")
        .args(["apply", "--include=pcsx2-libretro/Main.cpp"])
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
    let (_, capture) = source
        .split_once("// Split the donor's existing frozen entry list")
        .unwrap();
    let (capture, _) = capture
        .split_once("// psx adapter: expose the existing CPU-owner pause")
        .unwrap();
    let capture = capture.split_once("extern \"C\"").unwrap().1;
    // Mechanical types/VM/ZIP work only; actual exported functions below are
    // taken verbatim from the pinned donor and tracked patch in the package.
    let fixture = format!(
        r#"
#include <atomic>
#include <memory>
#include <vector>
#include <cstring>
#include <thread>
#include <chrono>
using u8 = unsigned char;
using u64 = unsigned long long;
#define RETRO_API
static constexpr size_t kSerializeSize = 68 * 1024 * 1024;
enum class VMState {{ Paused, Running }};
static std::atomic<VMState> state{{VMState::Running}};
static std::atomic<unsigned> destroyed{{0}}, cpu_calls{{0}}, live_byte{{17}};
static std::atomic<bool> started{{false}}, release_pack{{false}}, fail_pack{{false}};
namespace LibretroCore {{ static std::atomic<bool> s_vm_thread_running{{true}}; }}
namespace VMManager {{ VMState GetState() {{ return state.load(); }} }}
namespace Host {{ template<class F> void RunOnCPUThread(F fn, bool) {{ ++cpu_calls; fn(); }} }}
struct Error {{ const char* GetDescription() const {{ return "fixture"; }} }};
struct ConsoleType {{ template<class... T> void ErrorFmt(T...) {{}} }} Console;
struct ArchiveEntryList {{ std::vector<u8> bytes; ~ArchiveEntryList() {{ ++destroyed; }} }};
std::unique_ptr<ArchiveEntryList> SaveState_DownloadState(Error*) {{
    auto entries = std::make_unique<ArchiveEntryList>();
    entries->bytes = {{static_cast<u8>(live_byte.load()), 3, 5, 7}};
    return entries;
}}
bool SaveState_ZipToBuffer(std::unique_ptr<ArchiveEntryList> entries, std::nullptr_t,
    std::vector<u8>* out, Error*, bool store_only) {{
    started = true;
    while (!release_pack) std::this_thread::sleep_for(std::chrono::milliseconds(1));
    if (!store_only || fail_pack) return false;
    *out = entries->bytes;
    return true;
}}
extern "C"{capture}
extern "C" void fixture_state(bool paused) {{ state = paused ? VMState::Paused : VMState::Running; }}
extern "C" void fixture_live(unsigned value) {{ live_byte = value; }}
extern "C" unsigned fixture_destroyed() {{ return destroyed.load(); }}
extern "C" unsigned fixture_cpu_calls() {{ return cpu_calls.load(); }}
extern "C" bool fixture_started() {{ return started.load(); }}
extern "C" void fixture_release(bool fail) {{ fail_pack = fail; release_pack = true; }}
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
        let library = Arc::new(Library::new(temp.join("fixture.so")).unwrap());
        macro_rules! sym {
            ($name:literal, $ty:ty) => {
                *library.get::<$ty>(concat!($name, "\0").as_bytes()).unwrap()
            };
        }
        let capture = sym!(
            "retro_psx_capture_state",
            unsafe extern "C" fn() -> *mut c_void
        );
        let pack = sym!(
            "retro_psx_pack_state",
            unsafe extern "C" fn(*mut c_void, *mut c_void, usize) -> usize
        );
        let free = sym!("retro_psx_free_state", unsafe extern "C" fn(*mut c_void));
        let set_state = sym!("fixture_state", unsafe extern "C" fn(bool));
        let set_live = sym!("fixture_live", unsafe extern "C" fn(u32));
        let destroyed = sym!("fixture_destroyed", unsafe extern "C" fn() -> u32);
        let cpu_calls = sym!("fixture_cpu_calls", unsafe extern "C" fn() -> u32);
        let started = sym!("fixture_started", unsafe extern "C" fn() -> bool);
        let release = sym!("fixture_release", unsafe extern "C" fn(bool));
        assert!(capture().is_null());
        set_state(true);
        let captured = Checkpoint {
            captured: NonNull::new(capture()),
            pack,
            free,
            _library: library.clone(),
        };
        assert!(captured.captured.is_some());
        assert_eq!(cpu_calls(), 1);
        set_state(false); // Resume before the save worker packages the state.
        set_live(29);
        let (tx, rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || tx.send(captured.into_bytes()).unwrap());
        let deadline = Instant::now() + Duration::from_secs(2);
        while !started() && Instant::now() < deadline {
            std::thread::yield_now();
        }
        let packaging_started = started();
        let still_blocked = rx.try_recv().is_err();
        let queue_calls = cpu_calls();
        release(false); // Always release before assertions, including timeout.
        let bytes = rx.recv_timeout(Duration::from_secs(2)).unwrap().unwrap();
        worker.join().unwrap();
        assert!(packaging_started && still_blocked);
        assert_eq!(queue_calls, 1); // Packaging cannot re-enter the CPU queue.
        assert_eq!(bytes, [4, 0, 0, 0, 0, 0, 0, 0, 17, 3, 5, 7]);
        assert_eq!(destroyed(), 1);
        set_state(true);
        let cancelled = Checkpoint {
            captured: NonNull::new(capture()),
            pack,
            free,
            _library: library.clone(),
        };
        drop(cancelled);
        assert_eq!(destroyed(), 2);
        let failed = Checkpoint {
            captured: NonNull::new(capture()),
            pack,
            free,
            _library: library.clone(),
        };
        release(true);
        assert!(failed.into_bytes().is_err());
        assert_eq!(destroyed(), 3); // Exactly one release on failed packaging.
    }
    fs::remove_dir_all(temp).unwrap();
}
