//! Exercise the real Rust destructor with mechanical native callbacks. No
//! game, graphics driver or deliberate process fault is needed.
use super::*;

static EVENTS: Mutex<Vec<(&str, bool)>> = Mutex::new(Vec::new());
static FAIL_CAPTURE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static CAPTURE_OK: AtomicBool = AtomicBool::new(false);
static FLUSH_OK: AtomicBool = AtomicBool::new(true);

fn record(name: &'static str) {
    EVENTS
        .lock()
        .unwrap()
        .push((name, super::shared().is_some()));
}
unsafe extern "C" fn idle() {
    record("run");
}
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
unsafe extern "C" fn recorded_flush() -> bool {
    record("flush");
    FLUSH_OK.load(Ordering::Relaxed)
}
unsafe extern "C" fn speed(_: f32) -> bool {
    true
}
unsafe extern "C" fn capture() -> *mut c_void {
    record("capture");
    if FAIL_CAPTURE.load(Ordering::Relaxed) {
        std::thread::spawn(|| unsafe {
            let mut failure = crate::producer_failure::Failure {
                kind: 1,
                result: 0x911d,
            };
            assert!(environment(
                crate::producer_failure::ENVIRONMENT_COMMAND,
                (&mut failure as *mut crate::producer_failure::Failure).cast()
            ));
        })
        .join()
        .unwrap();
        return NonNull::<u8>::dangling().as_ptr().cast();
    }
    if CAPTURE_OK.load(Ordering::Relaxed) {
        // An opaque fixture token; the mechanical free callback never dereferences it.
        NonNull::<u8>::dangling().as_ptr().cast()
    } else {
        std::ptr::null_mut()
    }
}
unsafe extern "C" fn pack(_: *mut c_void, _: *mut c_void, _: usize) -> usize {
    record("pack");
    0
}
unsafe extern "C" fn free(_: *mut c_void) {
    record("free");
}
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
    for (fail_during_capture, vulkan) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        FAIL_CAPTURE.store(fail_during_capture, Ordering::Relaxed);
        for (loaded, context) in [(true, true), (true, false), (false, true), (false, false)] {
            let library = Arc::new(unsafe { Library::new("libc.so.6") }.unwrap());
            let mapping = Arc::downgrade(&library);
            let shared = Arc::new(Shared {
                vulkan: vulkan.then(|| Arc::new(crate::ps2_vulkan::Frontend::default())),
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
                producer_failure: crate::producer_failure::Latch::default(),
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
            let mut core = Core {
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
            if !fail_during_capture && !vulkan && loaded && context {
                let root = std::env::temp_dir().join(format!(
                    "psx-snapshot-flush-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_nanos()
                ));
                fs::create_dir(&root).unwrap();
                let card_path = root.join("Mcd001.ps2");
                fs::write(&card_path, b"synthetic current card").unwrap();
                let expected_card = card_identity(&root).unwrap();
                CAPTURE_OK.store(true, Ordering::Relaxed);
                core.flush_fn = recorded_flush;
                // Reproduce the actual former host sequence through retained
                // native callbacks: outer flush, capture, then card identity.
                core.flush().unwrap();
                let old_state = core.checkpoint().unwrap();
                core.flush().unwrap();
                assert_eq!(card_identity(&root).unwrap(), expected_card);
                drop(old_state);
                assert_eq!(
                    EVENTS
                        .lock()
                        .unwrap()
                        .iter()
                        .filter(|e| e.0 == "flush")
                        .count(),
                    3
                );
                core.pause(false).unwrap();
                EVENTS.lock().unwrap().clear();
                let mut engine = crate::engine::Core::Ps2(Box::new(core), root.clone());
                let crate::engine::Snapshot { state, card, .. } = engine.checkpoint(None).unwrap();
                let crate::engine::CapturedCards::Files(card) = card else {
                    panic!("PS2 checkpoint did not retain its card barrier")
                };
                assert_eq!(card.synchronize().unwrap(), expected_card);
                drop(state);
                assert_eq!(
                    *EVENTS.lock().unwrap(),
                    vec![
                        ("pause", true),
                        ("flush", true),
                        ("capture", true),
                        ("free", true)
                    ]
                );
                engine.pause(false).unwrap();
                EVENTS.lock().unwrap().clear();
                for directory_failure in [false, true] {
                    let mut snapshot = engine.checkpoint(None).unwrap();
                    snapshot.card = crate::engine::CapturedCards::Files(PendingCardIdentity {
                        identity: expected_card.clone(),
                        // Real failing sync descriptors exercise both required
                        // barriers without filesystem exhaustion/card mutation.
                        files: vec![fs::File::open(if directory_failure {
                            &card_path
                        } else {
                            Path::new("/dev/null")
                        })
                        .unwrap()],
                        directory: fs::File::open(if directory_failure {
                            Path::new("/dev/null")
                        } else {
                            &root
                        })
                        .unwrap(),
                    });
                    let error = snapshot.into_bytes().err().expect("Card sync must fail");
                    assert!(error.contains("could not be synchronized"));
                    assert_eq!(
                        *EVENTS.lock().unwrap(),
                        vec![
                            ("pause", true),
                            ("flush", true),
                            ("capture", true),
                            ("free", true)
                        ]
                    );
                    engine.pause(false).unwrap();
                    EVENTS.lock().unwrap().clear();
                }
                FLUSH_OK.store(false, Ordering::Relaxed);
                assert!(engine.checkpoint(None).is_err());
                assert_eq!(
                    *EVENTS.lock().unwrap(),
                    vec![("pause", true), ("flush", true)]
                );
                FLUSH_OK.store(true, Ordering::Relaxed);
                EVENTS.lock().unwrap().clear();
                // A failed authoritative-card read must free the captured list.
                fs::remove_file(card_path).unwrap();
                assert!(engine.checkpoint(None).is_err());
                assert_eq!(
                    *EVENTS.lock().unwrap(),
                    vec![("flush", true), ("capture", true), ("free", true)]
                );
                let crate::engine::Core::Ps2(restored, _) = engine else {
                    unreachable!()
                };
                core = *restored;
                core.pause(false).unwrap();
                core.flush_fn = yes;
                CAPTURE_OK.store(false, Ordering::Relaxed);
                EVENTS.lock().unwrap().clear();
                fs::remove_dir(root).unwrap();
            }
            // The actual environment callback may run on the GS worker. It must
            // only latch/wake, with no reentrant teardown or state capture.
            let mut failure = crate::producer_failure::Failure {
                kind: 1,
                result: 0x911d,
            };
            if fail_during_capture {
                assert!(core.checkpoint().is_err());
                assert_eq!(
                    *EVENTS.lock().unwrap(),
                    vec![("pause", true), ("capture", true), ("free", true)]
                );
                EVENTS.lock().unwrap().clear();
            } else {
                assert!(std::thread::spawn(move || unsafe {
                    environment(
                        crate::producer_failure::ENVIRONMENT_COMMAND,
                        (&mut failure as *mut crate::producer_failure::Failure).cast(),
                    )
                })
                .join()
                .unwrap());
            }
            assert!(EVENTS.lock().unwrap().is_empty());
            assert_eq!(core.producer_failure(), Some(failure));
            let mut later = crate::producer_failure::Failure { kind: 2, result: 0 };
            assert!(unsafe {
                environment(
                    crate::producer_failure::ENVIRONMENT_COMMAND,
                    (&mut later as *mut crate::producer_failure::Failure).cast(),
                )
            });
            assert_eq!(
                core.producer_failure(),
                Some(failure),
                "later notification replaced the first failure"
            );
            core.run();
            assert!(core.checkpoint().is_err());
            assert!(
                EVENTS.lock().unwrap().is_empty(),
                "failed producer ran or captured a checkpoint"
            );
            drop(core);
            let mut expected = vec![("retire", true)];
            if loaded {
                if !fail_during_capture && !vulkan {
                    expected.push(("pause", true));
                }
                expected.push(("unload", true));
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
}
