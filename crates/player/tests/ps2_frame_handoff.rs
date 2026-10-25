//! Exercise the patched donor's actual handoff with GL/context calls stubbed.
//! This checks notification and cross-context fence ownership, not GPU execution
//! or emulation speed.
#![cfg(target_os = "linux")]

use libloading::Library;
use std::{fs, process::Command, sync::mpsc, sync::Mutex, time::Duration};

static WAKE: Mutex<Option<mpsc::SyncSender<()>>> = Mutex::new(None);
#[cfg(feature = "graphics-observer")]
static OBSERVED: Mutex<Vec<(u32, u32, u64)>> = Mutex::new(Vec::new());
#[cfg(feature = "graphics-observer")]
unsafe extern "C" fn observe(
    stage: u32,
    owner: u32,
    frame: u64,
    _texture: u32,
    _w: u32,
    _h: u32,
    _fence: *mut std::ffi::c_void,
    dispatch: *const psx_player::graphics_observer::boundary::Dispatch,
) -> u64 {
    let dispatch = &*dispatch;
    assert_eq!(dispatch.version, 2);
    assert_eq!(dispatch.size as usize, std::mem::size_of_val(dispatch));
    if stage == 23 && dispatch.context == 0 {
        let result = psx_player::graphics_observer::callback(
            stage, owner, frame, _texture, _w, _h, _fence, dispatch,
        );
        assert!(!psx_player::graphics_observer::stop_requested());
        return result;
    }
    assert_eq!(dispatch.context, usize::MAX - 2048);
    if stage == 52 {
        assert!([usize::MAX - 4096, usize::MAX - 4095].contains(&(_fence as usize)));
    }
    OBSERVED.lock().unwrap().push((stage, owner, frame));
    frame
}
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
    fs::copy(
        repo.join("vendor/armsx2/pcsx2/GS/Renderers/OpenGL/GLLibretro.h"),
        temp.join("pcsx2/GS/Renderers/OpenGL/GLLibretro.h"),
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
    #[cfg(feature = "graphics-observer")]
    assert!(Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp"
        ])
        .arg(repo.join("patches/armsx2/0002-development-graphics-observer.patch"))
        .current_dir(&temp)
        .status()
        .unwrap()
        .success());
    #[cfg(feature = "graphics-observer")]
    assert!(Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp"
        ])
        .arg(repo.join("patches/armsx2/0003-development-matched-reader.patch"))
        .current_dir(&temp)
        .status()
        .unwrap()
        .success());
    #[cfg(feature = "producer-local-output-sync")]
    {
        let result = Command::new("git")
            .args([
                "apply",
                "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp",
            ])
            .arg(repo.join("patches/armsx2/0006-development-creator-output.patch"))
            .current_dir(&temp)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    #[cfg(feature = "producer-local-output-sync")]
    assert!(Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp"
        ])
        .arg(repo.join("patches/armsx2/0007-development-common-results.patch"))
        .current_dir(&temp)
        .status()
        .unwrap()
        .success());
    let source = fs::read_to_string(temp.join(relative)).unwrap();
    let observer = if cfg!(feature = "graphics-observer") {
        source
            .split_once("\t// Installed before CPU/GS boot")
            .unwrap()
            .1
            .split_once("\n\tnamespace\n\t{")
            .unwrap()
            .0
            .replacen(" and retired only after their join.", "", 1)
            .replacen(
                "GLsync fence, std::uintptr_t creator)",
                "GLsync fence, std::uintptr_t creator = 0)",
                1,
            )
    } else {
        "\nstatic Observer s_observer=nullptr;\nvoid SetObserver(Observer observer) {s_observer=observer;}\n".to_owned()
    };
    let (_, handoff) = source
        .split_once("// GS thread -> retro_run frame handoff")
        .unwrap();
    // Mechanical fixture types/ABI only. The mutex/CV/notification logic below
    // is extracted verbatim from pinned GPL donor source plus our tracked patch.
    let reference_fixture = if cfg!(feature = "graphics-observer") {
        r#"
extern "C" void publish_reference() { GLLibretro::Frame f; f.texture=1; f.observation=78; f.fence=reinterpret_cast<void*>(UINTPTR_MAX-4096); GLLibretro::PublishFrame(f); }
extern "C" void publish_second_reference() { GLLibretro::Frame f; f.texture=1; f.observation=79; f.fence=reinterpret_cast<void*>(UINTPTR_MAX-4095); GLLibretro::PublishFrame(f); }
extern "C" bool ack_reference() { return GLLibretro::AcknowledgeNoReader(&consumed); }
extern "C" void save_stale() { stale=consumed; }
extern "C" bool ack_stale() { return GLLibretro::AcknowledgeNoReader(&stale); }
extern "C" unsigned reference_deletes() { return producer_deletes; }
extern "C" unsigned reference_waits() { return producer_waits; }
extern "C" unsigned second_deletes() { return second_output_deletes; }
extern "C" bool has_frame() { return GLLibretro::HasFrame(); }
"#
    } else {
        ""
    };
    // Exercise the actual normal output-fence wait/retirement block as well
    // as the handoff. Only its surrounding platform declarations are stubbed.
    fs::create_dir_all(temp.join("pcsx2-libretro")).unwrap();
    fs::copy(
        repo.join("vendor/armsx2/pcsx2-libretro/Main.cpp"),
        temp.join("pcsx2-libretro/Main.cpp"),
    )
    .unwrap();
    let mut main_patches = vec![
        "0001-host-pause-and-card-flush.patch",
        "0004-stream-buffer-failure.patch",
    ];
    if cfg!(feature = "graphics-observer") {
        main_patches.extend([
            "0002-development-graphics-observer.patch",
            "0003-development-matched-reader.patch",
            "0005-development-stream-observer.patch",
        ]);
    }
    if cfg!(feature = "producer-local-output-sync") {
        main_patches.push("0006-development-creator-output.patch");
    }
    for patch in main_patches {
        let result = Command::new("git")
            .args([
                "apply",
                "--include=pcsx2-libretro/Main.cpp",
                "--include=pcsx2/GS/Renderers/OpenGL/GLLibretro.h",
            ])
            .arg(repo.join("patches/armsx2").join(patch))
            .current_dir(&temp)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}: {}",
            patch,
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let main = fs::read_to_string(temp.join("pcsx2-libretro/Main.cpp")).unwrap();
    let normal_output = main
        .split_once("// Sharing objects does not share ordering:")
        .unwrap()
        .1
        .split_once("if (s_gl_present_fbo == 0)")
        .unwrap()
        .0;
    let normal_output = format!("extern \"C\" void normal_output() {{ auto& frame=consumed; // Sharing objects does not share ordering:{normal_output}\n}}\n");
    let (output_abi, local_fixture) = if cfg!(feature = "producer-local-output-sync") {
        let header =
            fs::read_to_string(temp.join("pcsx2/GS/Renderers/OpenGL/GLLibretro.h")).unwrap();
        let abi = header
            .split_once("\tstruct OutputCommand")
            .unwrap()
            .1
            .split_once("\n\tvoid SetOutputCallback")
            .unwrap()
            .0;
        let selector = main
            .split_once("// Build-only Rust policy, never inferred from fence validity.")
            .unwrap()
            .1
            .split_once("// Sharing objects does not share ordering:")
            .unwrap()
            .0;
        (
            format!("struct OutputCommand{abi}\n"),
            format!(
                r#"
static u64 local_observation=1000;
extern "C" void set_output(GLLibretro::OutputCallback callback) {{GLLibretro::SetOutputCallback(callback);}}
extern "C" void actor(unsigned value) {{fixture_owner=value;}}
extern "C" bool publish_local_raw(std::uintptr_t raw) {{
    GLLibretro::RetireCreatorOutputs(); GLLibretro::Frame f; f.texture=1; f.observation=++local_observation;f.fence=reinterpret_cast<void*>(raw);
    if (!GLLibretro::RetainCreatorOutput(&f)) return false;
    GLLibretro::PublishFrame(f); return true;
}}
extern "C" bool publish_local() {{return publish_local_raw(UINTPTR_MAX-4096);}}
extern "C" bool publish_local_second() {{return publish_local_raw(UINTPTR_MAX-4095);}}
extern "C" void frontend_local() {{ auto& frame=consumed;
{selector}
    unexpected_normal++;
    }}
}}
extern "C" bool ack_local_stale() {{return GLLibretro::AcknowledgeCreatorOutput(&stale);}}
extern "C" void drain_local() {{GLLibretro::RetireCreatorOutputs();}}
extern "C" std::uintptr_t transferred_fence() {{return reinterpret_cast<std::uintptr_t>(consumed.fence);}}
extern "C" unsigned frontend_deletes() {{return output_frontend_deletes;}}
extern "C" unsigned creator_deletes() {{return output_creator_deletes;}}
extern "C" unsigned normal_errors() {{return unexpected_normal;}}
"#
            ),
        )
    } else {
        (String::new(), String::new())
    };
    let fixture = format!(
        r#"
// SPDX-FileCopyrightText: 2002-2026 PCSX2 Dev Team
// SPDX-License-Identifier: GPL-3.0+
#include <mutex>
#include <condition_variable>
#include <cstdint>
#include <atomic>
#include <vector>
using u64 = std::uint64_t;
using u32 = std::uint32_t;
using GLuint = unsigned;
using GLsync = void*;
static unsigned producer_deletes = 0, producer_waits = 0;
static unsigned second_output_deletes = 0;
static thread_local unsigned fixture_owner=1;
static unsigned output_frontend_deletes=0,output_creator_deletes=0,unexpected_normal=0;
void glFenceSync() {{}}
void glIsSync() {{}}
void glGetSynciv() {{}}
void glGetError() {{}}
static bool context_ready = false;
void* eglGetCurrentContext() {{ return context_ready ? reinterpret_cast<void*>(UINTPTR_MAX-2048) : nullptr; }}
extern "C" void enable_context() {{ context_ready = true; }}
constexpr auto GL_TIMEOUT_IGNORED = ~std::uint64_t(0);
static std::atomic<unsigned> read_waits = 0, read_deletes = 0, bad_order = 0;
void glWaitSync(GLsync fence, unsigned, std::uint64_t) {{
    if (fence == reinterpret_cast<void*>(UINTPTR_MAX-4096)) producer_waits++;
    if (fence == reinterpret_cast<void*>(1)) read_waits++;
}}
void glDeleteSync(GLsync fence) {{
    if (fence == reinterpret_cast<void*>(UINTPTR_MAX-4096) || fence == reinterpret_cast<void*>(UINTPTR_MAX-4095)) {{ if (fixture_owner==1) output_creator_deletes++; else output_frontend_deletes++; }}
    if (fence == reinterpret_cast<void*>(UINTPTR_MAX-4096)) producer_deletes++;
    if (fence == reinterpret_cast<void*>(UINTPTR_MAX-4095)) second_output_deletes++;
    if (fence == reinterpret_cast<void*>(1)) {{
        if (read_waits.load() <= read_deletes.load()) bad_order++;
        read_deletes++;
    }}
}}
namespace GLLibretro {{
bool Active = false;
void ReleaseFrontendContext() {{}}
struct Frame {{ unsigned texture = 0; GLsync fence = nullptr; unsigned width = 0, height = 0; u64 observation=0; u64 read_lease=0; u64 output_generation=0; }};
struct ObserverDispatch {{ u32 version,size; std::uintptr_t context,create,valid,status,error; }};
using Observer=u64 (*)(unsigned,unsigned,u64,unsigned,unsigned,unsigned,GLsync,const ObserverDispatch*);
{output_abi}
// The conditional body is extracted from the actual diagnostic patch.
// Comment prefix retained here for the extracted first line:
// Installed before CPU/GS boot{observer}
{handoff}
extern "C" void set_observer(GLLibretro::Observer observer) {{GLLibretro::SetObserver(observer);}}
extern "C" void set_callback(void (*cb)()) {{ GLLibretro::SetFrameReadyCallback(cb); }}
extern "C" void set_pacing(bool value) {{ GLLibretro::SetPacing(value); }}
extern "C" void publish() {{ GLLibretro::Frame frame; frame.texture = 1; frame.observation=77; GLLibretro::PublishFrame(frame); }}
static GLLibretro::Frame consumed, stale;
extern "C" bool consume() {{ return GLLibretro::ConsumeFrame(&consumed); }}
{reference_fixture}
{normal_output}
{local_fixture}
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
        #[cfg(feature = "graphics-observer")]
        {
            type Observer = psx_player::graphics_observer::boundary::Callback;
            let set = *library
                .get::<unsafe extern "C" fn(Option<Observer>)>(b"set_observer\0")
                .unwrap();
            set(Some(observe));
        }
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
        #[cfg(feature = "graphics-observer")]
        {
            unsafe extern "C" fn no_gl(
                _: *const std::ffi::c_char,
            ) -> Option<unsafe extern "C" fn()> {
                panic!("An empty native CPU handoff must not query GL");
            }
            psx_player::graphics_observer::initialize(no_gl, || (0, 0));
            // Execute the actual patched ConsumeFrame before any producer or
            // native current context exists, with the real Rust observer.
            assert!(!consume());
            assert!(!psx_player::graphics_observer::stop_requested());
        }
        library
            .get::<unsafe extern "C" fn()>(b"enable_context\0")
            .unwrap()();
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
        #[cfg(feature = "graphics-observer")]
        {
            let publish_ref = *library
                .get::<unsafe extern "C" fn()>(b"publish_reference\0")
                .unwrap();
            let ack = *library
                .get::<unsafe extern "C" fn() -> bool>(b"ack_reference\0")
                .unwrap();
            let save_stale = *library
                .get::<unsafe extern "C" fn()>(b"save_stale\0")
                .unwrap();
            let ack_stale = *library
                .get::<unsafe extern "C" fn() -> bool>(b"ack_stale\0")
                .unwrap();
            let retired = *library
                .get::<unsafe extern "C" fn() -> u32>(b"reference_deletes\0")
                .unwrap();
            let ref_waits = *library
                .get::<unsafe extern "C" fn() -> u32>(b"reference_waits\0")
                .unwrap();
            for iteration in 0..20 {
                let expected = iteration * 2 + 1;
                publish_ref();
                assert!(consume());
                save_stale();
                assert!(ack());
                assert!(!ack());
                assert!(!ack_stale());
                reuse(1);
                assert_eq!(retired(), expected);
                assert_eq!(ref_waits(), 0);
                shutdown(); // Context epochs cannot make an old lease valid again.
                publish_ref();
                assert!(consume());
                assert!(!ack_stale());
                assert!(ack());
                reuse(1);
                shutdown();
                assert_eq!(retired(), expected + 1);
                // Each iteration has two producer fences.
            }
            assert_eq!(waits(), 2);
            assert_eq!(deletes(), 2);
            // Distinct live generations must not be confused with the repeated
            // opaque handle above: replace an unconsumed frame on its creator,
            // then retire the new frame only through its validated reader lease.
            let publish_second = *library
                .get::<unsafe extern "C" fn()>(b"publish_second_reference\0")
                .unwrap();
            let second_deletes = *library
                .get::<unsafe extern "C" fn() -> u32>(b"second_deletes\0")
                .unwrap();
            let has_frame = *library
                .get::<unsafe extern "C" fn() -> bool>(b"has_frame\0")
                .unwrap();
            let before = retired();
            publish_ref();
            publish_second();
            assert_eq!(retired(), before + 1);
            assert!(consume());
            assert!(ack());
            assert_eq!(second_deletes(), 1);
            assert!(!ack());
            // Unread withdrawal before a backbuffer overwrite retires once.
            publish_ref();
            reuse(1);
            assert!(!consume());
            assert_eq!(retired(), before + 2);
            // Aborting pacing does not transfer or retire a consumed fence.
            // The still-live frontend completion remains its one owner.
            publish_ref();
            assert!(consume());
            pacing(false);
            assert_eq!(retired(), before + 2);
            assert!(ack());
            reuse(1);
            shutdown();
            assert_eq!(retired(), before + 3);
            // A parked publisher can be cancelled without a frontend tick;
            // owner-side backbuffer destruction then retires its unread fence.
            pacing(true);
            let producer = std::thread::spawn(move || publish_ref());
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while !has_frame() && std::time::Instant::now() < deadline {
                std::thread::yield_now();
            }
            let pending = has_frame();
            pacing(false);
            producer.join().unwrap();
            reuse(1);
            shutdown();
            assert!(pending);
            assert_eq!(retired(), before + 4);
            // Run the real normal-reader output wait/delete block with the
            // same recycled opaque value, followed by its actual reverse-fence
            // completion/creator reuse. It is a distinct live generation.
            let normal_output = *library
                .get::<unsafe extern "C" fn()>(b"normal_output\0")
                .unwrap();
            publish_ref();
            assert!(consume());
            normal_output();
            complete();
            reuse(1);
            shutdown();
            assert_eq!(retired(), before + 5);
            assert_eq!(ref_waits(), 1);
            assert_eq!(waits(), 3);
            assert_eq!(deletes(), 3);
            #[cfg(feature = "producer-local-output-sync")]
            {
                use psx_player::graphics_observer::output_sync as output;
                type Output = unsafe extern "C" fn(*mut output::Command) -> u32;
                let set = *library
                    .get::<unsafe extern "C" fn(Option<Output>)>(b"set_output\0")
                    .unwrap();
                let actor = *library
                    .get::<unsafe extern "C" fn(u32)>(b"actor\0")
                    .unwrap();
                let publish_local = *library
                    .get::<unsafe extern "C" fn() -> bool>(b"publish_local\0")
                    .unwrap();
                let publish_local_second = *library
                    .get::<unsafe extern "C" fn() -> bool>(b"publish_local_second\0")
                    .unwrap();
                let frontend = *library
                    .get::<unsafe extern "C" fn()>(b"frontend_local\0")
                    .unwrap();
                let drain = *library
                    .get::<unsafe extern "C" fn()>(b"drain_local\0")
                    .unwrap();
                let fence = *library
                    .get::<unsafe extern "C" fn() -> usize>(b"transferred_fence\0")
                    .unwrap();
                let front_deletes = *library
                    .get::<unsafe extern "C" fn() -> u32>(b"frontend_deletes\0")
                    .unwrap();
                let gs_deletes = *library
                    .get::<unsafe extern "C" fn() -> u32>(b"creator_deletes\0")
                    .unwrap();
                let errors = *library
                    .get::<unsafe extern "C" fn() -> u32>(b"normal_errors\0")
                    .unwrap();
                let stale_ack = *library
                    .get::<unsafe extern "C" fn() -> bool>(b"ack_local_stale\0")
                    .unwrap();
                output::begin().unwrap();
                set(Some(output::callback));
                let before_gs = gs_deletes();
                let before_front = front_deletes();
                let before_wait = ref_waits();
                for _ in 0..20 {
                    actor(1);
                    assert!(publish_local());
                    actor(2);
                    assert!(consume());
                    assert_eq!(fence(), 0);
                    frontend();
                    assert_eq!(front_deletes(), before_front);
                    actor(1);
                    drain();
                    assert_eq!(output::snapshot().live, 0);
                }
                // Different unconsumed generations sharing the same texture
                // are retired by creator-side drop/withdrawal, not consumption.
                // Distinct live handles are necessary until the first is dead.
                actor(1);
                assert!(publish_local());
                assert!(publish_local_second());
                drain();
                reuse(1);
                assert!(!consume());
                // Stop while a metadata lease is outstanding: cancellation
                // releases its CPU wait, then the creator retires its fence.
                assert!(publish_local());
                actor(2);
                assert!(consume());
                actor(1);
                pacing(false);
                drain();
                shutdown();
                assert_eq!(output::snapshot().live, 0);
                // Recycled handle, stale lease and duplicate ack through the
                // actual new handoff/selected Main branch must not retire the
                // current generation or perform any frontend sync operation.
                actor(1);
                assert!(publish_local());
                actor(2);
                assert!(consume());
                save_stale();
                frontend();
                actor(1);
                drain();
                assert!(publish_local());
                actor(2);
                assert!(consume());
                assert!(!stale_ack());
                assert_eq!(output::snapshot().live, 1);
                frontend();
                frontend();
                actor(1);
                drain();
                pacing(false);
                shutdown();
                output::native_joined();
                let progress = output::snapshot();
                assert_eq!(progress.live, 0);
                assert_eq!(progress.created, 25);
                assert_eq!(progress.retired_on_creator, 25);
                assert_eq!(gs_deletes() - before_gs, 25);
                assert_eq!(front_deletes(), before_front);
                assert_eq!(ref_waits(), before_wait);
                assert_eq!(progress.frontend_sync_operations, 0);
                assert!(progress.first_ownership_failure.is_some());
                assert!(progress.native_joined);
                assert_eq!(errors(), 0);
                set(None);
            }
            let observations = OBSERVED.lock().unwrap();
            for expected in [(3, 1, 77), (4, 2, 77), (8, 1, 77)] {
                assert!(observations.contains(&expected), "missing {expected:?}");
            }
            type Observer = psx_player::graphics_observer::boundary::Callback;
            let set = *library
                .get::<unsafe extern "C" fn(Option<Observer>)>(b"set_observer\0")
                .unwrap();
            set(None);
        }
    }
    fs::remove_dir_all(temp).unwrap();
}
