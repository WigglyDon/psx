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
#[test]
fn same_lease_request_wakes_owner_and_writes_bounded_signal_without_frontend_service() {
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
    while !root.join("first-stop.json").is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let signal = std::fs::read(root.join("first-stop.json")).unwrap();
    let signal = observer::operator::decode_stop(&signal, observer::mode()).unwrap();
    assert_eq!(signal["reason"], 7);
    assert!(observer::stop_requested());
    assert_eq!(owner.0.load(Ordering::SeqCst), 1);
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
    let deadline = Instant::now() + Duration::from_secs(2);
    while !root.join("first-stop.json").is_file() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    let signal = observer::operator::decode_stop(
        &std::fs::read(root.join("first-stop.json")).unwrap(),
        observer::mode(),
    )
    .unwrap();
    assert_eq!(signal["reason"], 2);
    assert_eq!(owner.0.load(Ordering::SeqCst), 1);
    observer::join_export();
    let export =
        observer::operator::decode_snapshot(&std::fs::read(root.join("observer-1.json")).unwrap())
            .unwrap();
    assert_eq!(export["first_terminal"]["stage"], 16);
    assert_eq!(export["first_terminal"]["detail"][7], -1);
    std::fs::remove_dir_all(root).unwrap();
}
