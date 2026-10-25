//! Generated-workload ABI fixture, never an engine or APK input.
use std::{
    ffi::{c_char, c_void, CStr},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    thread::JoinHandle,
};
type Env = unsafe extern "C" fn(u32, *mut c_void) -> bool;
static ENV: AtomicUsize = AtomicUsize::new(0);
static VIDEO: AtomicUsize = AtomicUsize::new(0);
static BATCH: AtomicUsize = AtomicUsize::new(0);
static INPUT: AtomicUsize = AtomicUsize::new(0);
static FRAME: AtomicUsize = AtomicUsize::new(0);
static UNLOAD: AtomicBool = AtomicBool::new(false);
static READY: AtomicBool = AtomicBool::new(false);
static WORKER: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);
fn mode() -> String {
    std::env::var("PSX_MINIMAL_CASE").unwrap()
}
fn note(s: &str) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("PSX_MINIMAL_TRACE").unwrap())
        .unwrap();
    writeln!(f, "{s}").unwrap();
}
unsafe fn env(c: u32, p: *mut c_void) -> bool {
    std::mem::transmute::<usize, Env>(ENV.load(Ordering::Acquire))(c, p)
}
#[repr(C)]
struct Variable {
    key: *const c_char,
    value: *const c_char,
}
#[repr(C)]
struct Geometry {
    width: u32,
    height: u32,
    max_width: u32,
    max_height: u32,
    aspect: f32,
}
#[repr(C)]
struct Timing {
    fps: f64,
    sample_rate: f64,
}
#[repr(C)]
pub struct Av {
    geometry: Geometry,
    timing: Timing,
}
#[repr(C)]
struct Hardware {
    context_type: u32,
    reset: Option<unsafe extern "C" fn()>,
    framebuffer: Option<unsafe extern "C" fn() -> usize>,
    proc: Option<unsafe extern "C" fn(*const c_char) -> Option<unsafe extern "C" fn()>>,
    depth: bool,
    stencil: bool,
    bottom_left: bool,
    major: u32,
    minor: u32,
    cache: bool,
    destroy: Option<unsafe extern "C" fn()>,
    debug: bool,
}
unsafe extern "C" fn destroy() {
    note("destroy");
}
unsafe extern "C" fn reset() {
    note("reset");
    *WORKER.lock().unwrap() = Some(std::thread::spawn(|| unsafe {
        let f = FRAME.load(Ordering::Acquire);
        if f != 0 {
            std::mem::transmute::<usize, unsafe extern "C" fn()>(f)();
        }
        let mut path = std::ptr::null::<c_char>();
        assert!(env(9, (&mut path as *mut *const c_char).cast()));
        assert!(!path.is_null());
        let video = std::mem::transmute::<
            usize,
            unsafe extern "C" fn(*const c_void, u32, u32, usize),
        >(VIDEO.load(Ordering::Acquire));
        video(usize::MAX as *const c_void, 640, 448, 0);
        let audio = std::mem::transmute::<usize, unsafe extern "C" fn(*const i16, usize) -> usize>(
            BATCH.load(Ordering::Acquire),
        );
        let frames = [0i16; 128];
        assert_eq!(audio(frames.as_ptr(), 64), 64);
        if mode() == "bad-audio" {
            audio(std::ptr::null(), 1);
        }
        if mode() == "native-fault" {
            let mut f = [7u32, 9];
            assert!(env(0x50535801, f.as_mut_ptr().cast()));
        }
        while !UNLOAD.load(Ordering::Acquire) {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert!(env(30, (&mut path as *mut *const c_char).cast()));
        assert!(!path.is_null());
        let input = std::mem::transmute::<usize, unsafe extern "C" fn(u32, u32, u32, u32) -> i16>(
            INPUT.load(Ordering::Acquire),
        );
        assert_eq!(input(0, 1, 0, 256), 0);
        note("worker_joinable_callbacks_alive");
    }));
}
#[no_mangle]
pub unsafe extern "C" fn retro_api_version() -> u32 {
    if mode() == "wrong-api" {
        2
    } else {
        1
    }
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_environment(e: Env) {
    ENV.store(e as usize, Ordering::Release);
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_video_refresh(
    v: unsafe extern "C" fn(*const c_void, u32, u32, usize),
) {
    VIDEO.store(v as usize, Ordering::Release);
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_audio_sample(_v: unsafe extern "C" fn(i16, i16)) {}
#[no_mangle]
pub unsafe extern "C" fn retro_set_audio_sample_batch(
    v: unsafe extern "C" fn(*const i16, usize) -> usize,
) {
    BATCH.store(v as usize, Ordering::Release);
}
#[no_mangle]
pub unsafe extern "C" fn retro_set_input_poll(_v: unsafe extern "C" fn()) {}
#[no_mangle]
pub unsafe extern "C" fn retro_set_input_state(v: unsafe extern "C" fn(u32, u32, u32, u32) -> i16) {
    INPUT.store(v as usize, Ordering::Release);
}
#[no_mangle]
pub unsafe extern "C" fn retro_init() {
    note("init");
    let mut b = false;
    assert!(env(3, (&mut b as *mut bool).cast()) && b);
    let mut preferred = 0u32;
    assert!(env(56, (&mut preferred as *mut u32).cast()) && preferred == 4);
    assert!(!env(1234567, (&mut b as *mut bool).cast()));
    if mode() == "startup-stop" {
        let mut f = [3u32, 4];
        env(0x50535801, f.as_mut_ptr().cast());
    }
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_set_frame_callback(f: Option<unsafe extern "C" fn()>) -> bool {
    FRAME.store(f.map_or(0, |v| v as usize), Ordering::Release);
    note(if f.is_some() { "frame_on" } else { "frame_off" });
    !(mode() == "no-notify" && f.is_some())
}
#[no_mangle]
pub unsafe extern "C" fn retro_load_game(_g: *const c_void) -> bool {
    note("load");
    if mode() == "load-refuse" {
        return false;
    }
    for (key, expected) in [
        (c"armsx2_renderer", c"OpenGL"),
        (c"armsx2_ee_cycle_rate", c"100%"),
        (c"armsx2_mtvu", c"disabled"),
        (c"psx_performance_hints", c"enabled"),
    ] {
        let mut v = Variable {
            key: key.as_ptr(),
            value: std::ptr::null(),
        };
        assert!(env(15, (&mut v as *mut Variable).cast()));
        assert_eq!(CStr::from_ptr(v.value), expected);
    }
    let mut hw = Hardware {
        context_type: if mode() == "bad-hardware" { 1 } else { 4 },
        reset: if mode() == "missing-reset" {
            None
        } else {
            Some(reset)
        },
        framebuffer: None,
        proc: None,
        depth: true,
        stencil: true,
        bottom_left: true,
        major: 3,
        minor: 0,
        cache: false,
        destroy: Some(destroy),
        debug: false,
    };
    if !env(14, (&mut hw as *mut Hardware).cast()) {
        return false;
    }
    assert_eq!(hw.framebuffer.unwrap()(), 7);
    assert!(hw.proc.unwrap()(c"glFenceSync".as_ptr()).is_some());
    true
}
#[no_mangle]
pub unsafe extern "C" fn retro_get_system_av_info(out: *mut Av) {
    *out = Av {
        geometry: Geometry {
            width: 640,
            height: 448,
            max_width: 1024,
            max_height: 1024,
            aspect: 4. / 3.,
        },
        timing: Timing {
            fps: if mode() == "bad-av" { 0.0 } else { 59.94 },
            sample_rate: 48000.,
        },
    };
}
#[no_mangle]
pub unsafe extern "C" fn retro_run() {
    note("service");
    READY.store(true, Ordering::Release);
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_ready() -> bool {
    READY.load(Ordering::Acquire)
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_set_speed(v: f32) -> bool {
    note("normal_speed");
    READY.load(Ordering::Acquire) && v == 1.
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_set_paused(v: bool) -> bool {
    note(if v { "pause" } else { "resume" });
    true
}
#[no_mangle]
pub unsafe extern "C" fn retro_unload_game() {
    note("unload");
    UNLOAD.store(true, Ordering::Release);
    if let Some(w) = WORKER.lock().unwrap().take() {
        w.join().unwrap();
    }
    if mode() == "native-fault" {
        let mut f = [8u32, 1285];
        env(0x50535801, f.as_mut_ptr().cast());
    }
}
#[no_mangle]
pub unsafe extern "C" fn retro_deinit() {
    note("deinit");
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_observer_set_callback(_f: Option<unsafe extern "C" fn()>) {
    note(if _f.is_some() {
        "observer_on"
    } else {
        "observer_off"
    });
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_observer_set_stream_callback(
    _f: Option<unsafe extern "C" fn()>,
) {
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_observer_set_output_callback(
    _f: Option<unsafe extern "C" fn()>,
) {
}
#[no_mangle]
pub unsafe extern "C" fn retro_psx_observer_set_result_callback(
    _f: Option<unsafe extern "C" fn()>,
) {
}
