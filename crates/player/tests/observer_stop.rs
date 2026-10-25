//! Separate process: exercise the actual writer/owner stop, without GL/device.
#![cfg(feature = "graphics-observer")]
use psx_player::graphics_observer as observer;
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    task::{Wake, Waker},
    time::{Duration, Instant},
};
struct Owner(AtomicUsize);
impl Wake for Owner {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
unsafe extern "C" fn proc(_: *const std::ffi::c_char) -> Option<unsafe extern "C" fn()> {
    None
}
fn signal(root: &std::path::Path) -> serde_json::Value {
    // Storage publication is not the owner-stop latency contract. Slow fsync
    // on a removable test workspace must not masquerade as a missed wakeup.
    let deadline = Instant::now() + Duration::from_secs(30);
    let path = root.join("first-stop.json");
    while !path.is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        path.is_file(),
        "bounded background signal publication did not complete"
    );
    observer::operator::decode_stop(&std::fs::read(path).unwrap(), observer::mode()).unwrap()
}
#[test]
fn same_lease_request_wakes_owner_and_writes_bounded_signal_without_frontend_service() {
    #[cfg(feature = "producer-local-output-sync")]
    assert!(std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "output_ownership_stop_fixture", "--ignored"])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "idle_consume_before_native_dispatch_fixture",
            "--ignored"
        ])
        .status()
        .unwrap()
        .success());
    assert!(std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "missing_context_callback_fixture", "--ignored"])
        .status()
        .unwrap()
        .success());
    let root = std::env::temp_dir().join(format!("psx-observer-stop-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("prepared-v1"), b"PSX-OBSERVER-SCRATCH-1\n").unwrap();
    observer::initialize(proc, || (0, 0));
    let owner = Arc::new(Owner(AtomicUsize::new(0)));
    observer::start(&root, Waker::from(owner.clone())).unwrap();
    // This is the exact operator payload, executed in an isolated host fixture.
    let files = root.join("fixture/files").join(observer::SCRATCH);
    std::fs::create_dir_all(files.parent().unwrap()).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, &files).unwrap();
    assert!(std::process::Command::new("sh")
        .args(["-c", &observer::operator::stop_shell()])
        .current_dir(root.join("fixture"))
        .status()
        .unwrap()
        .success());
    let deadline = Instant::now() + Duration::from_secs(2);
    while owner.0.load(Ordering::SeqCst) == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(observer::stop_requested());
    assert_eq!(owner.0.load(Ordering::SeqCst), 1);
    let signal = signal(&root);
    assert_eq!(signal["reason"], 7);
    observer::request_stop(observer::StopReason::Fence);
    assert_eq!(owner.0.load(Ordering::SeqCst), 1);
    observer::join_export();
    let after = observer::operator::decode_stop(
        &std::fs::read(root.join("first-stop.json")).unwrap(),
        observer::mode(),
    )
    .unwrap();
    assert_eq!(after, signal, "first stop is immutable");
    for entry in std::fs::read_dir(&root).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "json") {
            assert!(std::fs::metadata(path).unwrap().len() <= observer::EXPORT_LIMIT as u64);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(feature = "producer-local-output-sync")]
#[test]
#[ignore = "isolated output ownership stop process, invoked by the contract test"]
fn output_ownership_stop_fixture() {
    use observer::output_sync as output;
    let root = std::env::temp_dir().join(format!("psx-output-stop-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    observer::initialize(proc, || (0, 0));
    output::begin().unwrap();
    let owner = Arc::new(Owner(AtomicUsize::new(0)));
    observer::start(&root, Waker::from(owner.clone())).unwrap();
    let mut command = output::Command::new(output::REJECT, 2);
    assert_eq!(unsafe { output::callback(&mut command) }, 0);
    assert_eq!(owner.0.load(Ordering::SeqCst), 1);
    assert_eq!(signal(&root)["reason"], 5);
    observer::join_export();
    let snapshot =
        observer::operator::decode_snapshot(&std::fs::read(root.join("observer-1.json")).unwrap())
            .unwrap();
    assert!(snapshot["output_sync"]["first_ownership_failure"].is_object());
    assert_eq!(snapshot["output_sync"]["frontend_sync_operations"], 0);
    assert_eq!(snapshot["output_sync"]["live"], 0);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "isolated startup observer process, invoked by the contract test"]
fn idle_consume_before_native_dispatch_fixture() {
    unsafe extern "C" fn no_gl(_: *const std::ffi::c_char) -> Option<unsafe extern "C" fn()> {
        panic!("An empty CPU handoff must not load or query GL");
    }
    // ConsumeFrame may report its empty CPU slot before the GS thread loads
    // native EGL/GLAD dispatch. The callback carries no frame or GPU ownership.
    observer::initialize(no_gl, || (0, 0));
    let root = std::env::temp_dir().join(format!("psx-observer-idle-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    let owner = Arc::new(Owner(AtomicUsize::new(0)));
    observer::start(&root, Waker::from(owner.clone())).unwrap();
    let dispatch = observer::boundary::Dispatch {
        version: 2,
        size: std::mem::size_of::<observer::boundary::Dispatch>() as u32,
        ..Default::default()
    };
    unsafe {
        assert_eq!(
            observer::callback(23, 2, 0, 0, 0, 0, std::ptr::null_mut(), &dispatch),
            0
        );
    }
    assert!(
        !observer::stop_requested(),
        "No published frame is not a context failure"
    );
    assert_eq!(owner.0.load(Ordering::SeqCst), 0);
    observer::join_export();
    assert!(!root.join("first-stop.json").exists());
    let export =
        observer::operator::decode_snapshot(&std::fs::read(root.join("observer-1.json")).unwrap())
            .unwrap();
    assert!(export["first_failure"].is_null());
    assert!(export["first_terminal"].is_null());
    assert!(export["tail"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["stage"] == 23));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "isolated static observer process, invoked by the contract test"]
fn missing_context_callback_fixture() {
    let root =
        std::env::temp_dir().join(format!("psx-observer-callback-stop-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    observer::initialize(proc, || (0, 0));
    let owner = Arc::new(Owner(AtomicUsize::new(0)));
    observer::start(&root, Waker::from(owner.clone())).unwrap();
    unsafe {
        observer::callback(16, 1, 1, 0, 0, 0, std::ptr::null_mut(), std::ptr::null());
    }
    assert_eq!(owner.0.load(Ordering::SeqCst), 1);
    let signal = signal(&root);
    assert_eq!(signal["reason"], 2);
    observer::join_export();
    let export =
        observer::operator::decode_snapshot(&std::fs::read(root.join("observer-1.json")).unwrap())
            .unwrap();
    assert_eq!(export["first_terminal"]["stage"], 16);
    assert_eq!(export["first_terminal"]["detail"][7], -1);
    std::fs::remove_dir_all(root).unwrap();
}
