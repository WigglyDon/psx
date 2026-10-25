use super::*;
use std::{process::Command, task::Wake};
struct WakeCount(AtomicU64);
impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}
unsafe extern "C" fn noop() {}
unsafe extern "C" fn proc(_n: *const c_char) -> Option<unsafe extern "C" fn()> {
    Some(noop)
}
fn binding() -> (usize, usize) {
    (0, 0)
}

#[test]
fn real_owner_abi_fixtures() {
    let root = std::env::temp_dir().join(format!("psx-minimal-owner-{}", std::process::id()));
    assert!(!root.exists());
    std::fs::create_dir(&root).unwrap();
    let library = root.join("fixture.so");
    let status = Command::new("rustc")
        .args([
            "--edition=2021",
            "--crate-type=cdylib",
            "-Cpanic=abort",
            "-Copt-level=1",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/minimal_core.rs"))
        .arg("-o")
        .arg(&library)
        .status()
        .unwrap();
    assert!(status.success());
    for case in [
        "clean",
        "native-fault",
        "bad-audio",
        "startup-stop",
        "load-refuse",
        "bad-hardware",
        "missing-reset",
        "wrong-api",
        "no-notify",
        "bad-av",
    ] {
        let result = Command::new(std::env::current_exe().unwrap())
            .args([
                "--ignored",
                "--exact",
                "minimal_host::tests::owner_fixture",
                "--nocapture",
            ])
            .env("PSX_MINIMAL_CASE", case)
            .env("PSX_MINIMAL_LIBRARY", &library)
            .env("PSX_MINIMAL_TRACE", root.join(format!("{case}.trace")))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{case}: {}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
    // Failure evidence is retained on any assertion; success fixture is not media.
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
#[ignore]
fn owner_fixture() {
    let Ok(case) = std::env::var("PSX_MINIMAL_CASE") else {
        return;
    };
    observe::initialize(proc, binding);
    let waker = Waker::from(Arc::new(WakeCount(AtomicU64::new(0))));
    let host = unsafe {
        Host::load(
            Path::new(&std::env::var("PSX_MINIMAL_LIBRARY").unwrap()),
            Path::new("fixture.iso"),
            Config {
                system: Path::new("fixture-system"),
                save: Path::new("fixture-save"),
                firmware: "fixture.bin",
                framebuffer: 7,
                proc_address: proc,
                gles: [3, 2],
                waker,
            },
        )
    };
    if matches!(
        case.as_str(),
        "startup-stop"
            | "load-refuse"
            | "bad-hardware"
            | "missing-reset"
            | "wrong-api"
            | "no-notify"
            | "bad-av"
    ) {
        assert!(host.is_err());
    } else {
        let mut host = host.unwrap();
        for _ in 0..50 {
            if host.evidence().hardware_frames > 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        if case == "clean" {
            assert!(host.service(true).unwrap());
            assert!(host.service(true).unwrap());
            let o = host.evidence();
            assert!(o.ready);
            assert_eq!(o.dimensions, [640, 448]);
            assert_eq!(o.audio_frames, 64);
            assert_eq!(o.audio_rejected, 0);
            assert!(o.hardware_frames > 0);
            assert!(o.notifications > 0);
        } else {
            assert!(host.service(true).is_err());
        }
        let o = host.stop();
        assert!(
            o.unloaded
                && o.context_destroyed
                && o.deinitialized
                && o.callback_storage_released
                && o.native_signals_restored
        );
        if case == "native-fault" {
            assert_eq!(o.native_failure, Some([7, 9]));
            assert_eq!(
                o.first_failure.as_deref(),
                Some("native dependency failure")
            );
        }
        if case == "bad-audio" {
            assert_eq!(o.audio_rejected, 1);
            assert_eq!(o.first_failure.as_deref(), Some("invalid audio batch"));
        }
        let before = o.services;
        assert_eq!(host.stop().services, before);
        drop(host);
    }
    let trace = std::fs::read_to_string(std::env::var("PSX_MINIMAL_TRACE").unwrap()).unwrap();
    assert!(trace.ends_with("observer_off\n"));
    if trace.contains("reset\n") {
        let pos = |s: &str| trace.find(s).unwrap();
        assert!(pos("frame_off") < pos("unload"));
        assert!(pos("unload") < pos("worker_joinable_callbacks_alive"));
        assert!(pos("worker_joinable_callbacks_alive") < pos("destroy"));
        assert!(pos("destroy") < pos("deinit"));
        assert!(pos("deinit") < pos("observer_off"));
    }
    assert!(ACTIVE.lock().unwrap().is_none());
}

#[test]
fn bounded_input_settings_and_real_graphics_requirements() {
    assert!(options("../firmware").is_err());
    assert!(options("").is_err());
    assert!(options("bad\0file").is_err());
    let o = options("fixture.bin").unwrap();
    assert_eq!(o.len(), 12);
    assert_eq!(o[b"armsx2_renderer".as_slice()].to_bytes(), b"OpenGL");
    let src = include_str!("../../android/src/minimal.rs");
    let frozen = include_str!("../../android/src/android/graphics.rs");
    let selector = |s: &str| {
        s.split_once("let attributes = [")
            .unwrap()
            .1
            .split_once("];")
            .unwrap()
            .0
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    assert_eq!(selector(src), selector(frozen));
    let compact = src
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    assert!(compact.contains("&[egl::CONTEXT_CLIENT_VERSION,3,egl::NONE]"));
    for required in [
        "create_window_surface",
        "swap_buffers",
        "DEPTH24_STENCIL8",
        "framebuffer incomplete",
        "context_unavailable",
        "join_export",
        "from_secs(20)",
    ] {
        assert!(src.contains(required), "{required}");
    }
    for removed in [
        "Objects::new",
        "Text::new",
        "Sound::start",
        "Checkpoint",
        "gl.draw_",
        "blit_framebuffer",
        "read_pixels",
        "ps2::Core",
    ] {
        assert!(!src.contains(removed), "{removed}");
    }
}

#[test]
fn lifecycle_cancels_before_launch_and_never_restarts_after_loss() {
    let mut l = Lifecycle::default();
    assert!(!l.can_launch());
    assert!(l.begin().is_err());
    l.event(LifecycleEvent::Resume);
    l.event(LifecycleEvent::Window);
    l.event(LifecycleEvent::Focus);
    assert!(l.can_launch());
    l.event(LifecycleEvent::Blur);
    assert!(!l.can_launch());
    l.event(LifecycleEvent::Focus);
    l.begin().unwrap();
    assert!(!l.can_launch());
    l.event(LifecycleEvent::WindowLost);
    assert_eq!(l.stopped(), Some("window loss"));
    l.event(LifecycleEvent::Destroy);
    l.event(LifecycleEvent::Window);
    l.event(LifecycleEvent::Resume);
    assert_eq!(l.stopped(), Some("window loss"));
    assert!(l.begin().is_err());
    for e in [
        LifecycleEvent::Blur,
        LifecycleEvent::Pause,
        LifecycleEvent::Destroy,
    ] {
        let mut l = Lifecycle::default();
        l.event(LifecycleEvent::Resume);
        l.event(LifecycleEvent::Focus);
        l.event(LifecycleEvent::Window);
        l.begin().unwrap();
        l.event(e);
        assert!(l.stopped().is_some());
    }
}
