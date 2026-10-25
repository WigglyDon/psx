//! Calls the actual retained player callbacks; no native/product runtime.
use super::*;
use sha2::{Digest, Sha256};
#[test]
fn frozen_player_callback_answers_match_independent_host() {
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--ignored",
            "--exact",
            "ps2::minimal_equivalence::callback_fixture",
            "--nocapture",
        ])
        .env("PSX_CALLBACK_EQUIVALENCE", "1")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
#[test]
fn retained_player_implementation_and_egl_bootstrap_are_exact_frozen_source() {
    let source = include_str!("ps2.rs")
        .split("\n// Minimal-host callback equivalence test hook.")
        .next()
        .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(source)),
        "bb60a190b3e0e16b3bd2117b34ee606fe3d0adf146df8735bb63d6098caaf0a1"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("../../android/src/android/graphics.rs"))
        ),
        "79b206967129a7c2ccde8352ced33c8e6b4d42e96f1ebed693e2c24ab5441185"
    );
}
#[test]
#[ignore]
fn callback_fixture() {
    if std::env::var("PSX_CALLBACK_EQUIVALENCE").as_deref() != Ok("1") {
        return;
    }
    unsafe extern "C" fn proc(_: *const c_char) -> Option<unsafe extern "C" fn()> {
        None
    }
    let mut options = host_options("fixture.bin".into()).unwrap();
    // Execute the Android option value on this Linux host. All other values
    // come directly from the retained function; its source hash is pinned.
    options.insert(
        b"psx_performance_hints".to_vec(),
        CString::new("enabled").unwrap(),
    );
    assert_eq!(
        options,
        crate::minimal_host::options("fixture.bin").unwrap()
    );
    let shared = Arc::new(Shared {
        system: CString::new("fixture-system").unwrap(),
        save: CString::new("fixture-save").unwrap(),
        options,
        framebuffer: 7,
        proc_address: Some(proc),
        gles: [3, 2],
        audio: Arc::new(AudioQueue::new(16384)),
        buttons: AtomicU16::new(0),
        axes: std::array::from_fn(|_| AtomicI16::new(0)),
        fast: AtomicBool::new(false),
        frames: FrameSignal::default(),
        producer_failure: crate::producer_failure::Latch::default(),
        output: Mutex::new(Output::default()),
    });
    assert!(ACTIVE.lock().unwrap().replace(shared).is_none());
    unsafe {
        crate::minimal_host::compare_frozen_environment(environment, proc);
    }
    for port in 0..3 {
        for device in [1, 5] {
            for index in 0..3 {
                for id in [0, 1, 15, 16, 256] {
                    assert_eq!(unsafe { input(port, device, index, id) }, 0);
                }
            }
        }
    }
    *ACTIVE.lock().unwrap() = None;
}
