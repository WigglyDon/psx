//! Exercise the real Rust destructor with mechanical native callbacks. No
//! game, graphics driver or deliberate process fault is needed.
use super::*;

static EVENTS: Mutex<Vec<(&str, bool)>> = Mutex::new(Vec::new());

fn record(name: &'static str) {
    EVENTS
        .lock()
        .unwrap()
        .push((name, super::shared().is_some()));
}
unsafe extern "C" fn idle() {}
unsafe extern "C" fn unload() {
    record("unload");
}
unsafe extern "C" fn destroy() {
    record("context");
}
unsafe extern "C" fn deinit() {
    record("deinit");
}
unsafe extern "C" fn pause(_: bool) -> bool {
    record("pause");
    true
}
unsafe extern "C" fn frame_callback(callback: Option<unsafe extern "C" fn()>) -> bool {
    record(if callback.is_none() { "retire" } else { "set" });
    true
}
unsafe extern "C" fn yes() -> bool {
    true
}
unsafe extern "C" fn speed(_: f32) -> bool {
    true
}
unsafe extern "C" fn capture() -> *mut c_void {
    std::ptr::null_mut()
}
unsafe extern "C" fn pack(_: *mut c_void, _: *mut c_void, _: usize) -> usize {
    0
}
unsafe extern "C" fn free(_: *mut c_void) {}
unsafe extern "C" fn restore(_: *const c_void, _: usize) -> bool {
    false
}

#[test]
fn ps2_workers_close_before_retiring_the_shared_graphics_context() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "ps2::teardown_tests::teardown_fixture",
            "--nocapture",
        ])
        .env("PSX_TEARDOWN_FIXTURE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

// Native callback ownership and the core lease are process-wide. Isolate the
// fixture from the suite's concurrent callback and signal tests.
#[test]
#[ignore]
fn teardown_fixture() {
    assert_eq!(std::env::var("PSX_TEARDOWN_FIXTURE").as_deref(), Ok("1"));
    for (loaded, context) in [(true, true), (true, false), (false, true), (false, false)] {
        let library = Arc::new(unsafe { Library::new("libc.so.6") }.unwrap());
        let mapping = Arc::downgrade(&library);
        let shared = Arc::new(Shared {
            system: CString::new("fixture-system").unwrap(),
            save: CString::new("fixture-save").unwrap(),
            options: BTreeMap::new(),
            framebuffer: 0,
            proc_address: None,
            gles: [3, 2],
            audio: Arc::new(AudioQueue::new(128)),
            buttons: AtomicU16::new(0),
            axes: std::array::from_fn(|_| AtomicI16::new(0)),
            fast: AtomicBool::new(false),
            frames: FrameSignal::default(),
            output: Mutex::new(Output {
                hw: Some(HwRender {
                    context_type: 4,
                    reset: None,
                    framebuffer: None,
                    get_proc_address: None,
                    depth: false,
                    stencil: false,
                    bottom_left: true,
                    major: 3,
                    minor: 2,
                    cache_context: false,
                    destroy: Some(destroy),
                    debug_context: false,
                }),
                ..Output::default()
            }),
        });
        *ACTIVE.lock().unwrap() = Some(shared.clone());
        let core = Core {
            shared,
            run_fn: idle,
            unload,
            deinit,
            pause_fn: pause,
            flush_fn: yes,
            ready_fn: yes,
            speed_fn: speed,
            frame_callback_fn: frame_callback,
            speed: None,
            speed_changed: Instant::now(),
            capture_state: capture,
            pack_state: pack,
            free_state: free,
            unserialize: restore,
            library,
            native_signals: NativeSignals::capture(libc::abort as *const () as *const c_void)
                .unwrap(),
            loaded,
            context,
            paused: false,
            _owner: PhantomData,
            _lease: CoreLease::acquire().unwrap(),
        };
        drop(core);
        let mut expected = vec![("retire", true)];
        if loaded {
            expected.extend([("pause", true), ("unload", true)]);
        }
        if context {
            expected.push(("context", true));
        }
        expected.push(("deinit", true));
        assert_eq!(*EVENTS.lock().unwrap(), expected);
        EVENTS.lock().unwrap().clear();
        assert!(super::shared().is_none());
        assert!(mapping.upgrade().is_none());
        drop(CoreLease::acquire().unwrap());
    }
}
