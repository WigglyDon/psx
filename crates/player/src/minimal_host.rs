//! Development-only independent libretro owner. No product Core, session,
//! controller, checkpoint, library, importer or presentation implementation.
//! Shared code is limited to ABI types, signal retirement, failure latch and
//! the exact existing observer/creator-output contract.
use crate::{
    graphics_observer as observe,
    native_signals::NativeSignals,
    producer_failure,
    retro::{AvInfo, Environment, Geometry, HwRender, ProcAddress},
};
use libloading::Library;
use serde::Serialize;
pub use serde_json;
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    marker::PhantomData,
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::Waker,
};
type Result<T> = std::result::Result<T, String>;

pub const PLAYER_SOURCE: &str = "8e61eb2ccdf364cc4aa80e33dfc0796021c9dac1";
pub const BASELINE_NATIVE_SHA: &str =
    "e9e648d8658c767c182d34ebccbfa52aa0672a3c3e7182ee77653ac201acf467";
pub const NATIVE_SHA: &str = "b6bff788bc7676d67417beb89c5880590cd47038f6d3d3b2d235220addf5d9cf";
pub const SETTINGS: [(&str, &str); 11] = [
    ("armsx2_renderer", "OpenGL"),
    ("armsx2_upscale", "1x"),
    ("armsx2_ee_cycle_rate", "100%"),
    ("armsx2_ee_cycle_skip", "disabled"),
    ("armsx2_mtvu", "disabled"),
    ("armsx2_fast_boot", "enabled"),
    ("armsx2_widescreen_patches", "disabled"),
    ("armsx2_aspect_ratio", "Stretch"),
    ("armsx2_no_interlacing_patches", "disabled"),
    ("armsx2_hw_download_mode", "Accurate"),
    ("psx_performance_hints", "enabled"),
];

#[derive(Default)]
pub struct Lifecycle {
    resumed: bool,
    focused: bool,
    window: bool,
    launched: bool,
    first_stop: Option<&'static str>,
}
pub enum LifecycleEvent {
    Resume,
    Focus,
    Window,
    Pause,
    Blur,
    WindowLost,
    Destroy,
}
impl Lifecycle {
    pub fn event(&mut self, e: LifecycleEvent) {
        match e {
            LifecycleEvent::Resume => self.resumed = true,
            LifecycleEvent::Focus => self.focused = true,
            LifecycleEvent::Window => self.window = true,
            LifecycleEvent::Pause => {
                self.resumed = false;
                if self.launched {
                    self.stop("background loss");
                }
            }
            LifecycleEvent::Blur => {
                self.focused = false;
                if self.launched {
                    self.stop("focus loss");
                }
            }
            LifecycleEvent::WindowLost => {
                self.window = false;
                if self.launched {
                    self.stop("window loss");
                }
            }
            LifecycleEvent::Destroy => self.stop("lifecycle termination"),
        }
    }
    pub fn can_launch(&self) -> bool {
        self.resumed && self.focused && self.window && !self.launched && self.first_stop.is_none()
    }
    pub fn begin(&mut self) -> Result<()> {
        if !self.can_launch() {
            return Err("startup cancelled or already consumed".into());
        }
        self.launched = true;
        Ok(())
    }
    pub fn stop(&mut self, reason: &'static str) {
        self.first_stop.get_or_insert(reason);
    }
    pub fn stopped(&self) -> Option<&'static str> {
        self.first_stop
    }
}

#[derive(Clone, Default, Serialize)]
pub struct Evidence {
    pub callback_calls: BTreeMap<u32, u64>,
    pub callback_refusals: BTreeMap<u32, u64>,
    pub hardware_request: Option<[u32; 7]>,
    pub av: [f64; 6],
    pub hardware_frames: u64,
    pub software_frames: u64,
    pub dimensions: [u32; 2],
    pub notifications: u64,
    pub services: u64,
    pub audio_frames: u64,
    pub audio_rejected: u64,
    pub inputs: u64,
    pub settings_queries: BTreeMap<String, String>,
    pub metrics: Option<[f32; 5]>,
    pub render_costs: Option<[f32; 9]>,
    pub init: bool,
    pub loaded: bool,
    pub reset: bool,
    pub ready: bool,
    pub normal_speed_applied: bool,
    pub unloaded: bool,
    pub context_destroyed: bool,
    pub deinitialized: bool,
    pub callback_storage_released: bool,
    pub native_signals_restored: bool,
    pub first_failure: Option<String>,
    pub native_failure: Option<[u32; 2]>,
    pub unexpected_software: bool,
}
struct Shared {
    system: CString,
    save: CString,
    options: BTreeMap<Vec<u8>, CString>,
    framebuffer: usize,
    proc_address: ProcAddress,
    gles: [u32; 2],
    pending: AtomicBool,
    notifications: AtomicU64,
    audio: AtomicU64,
    rejected: AtomicU64,
    inputs: AtomicU64,
    waker: Waker,
    failure: producer_failure::Latch,
    output: Mutex<Evidence>,
    hw: Mutex<Option<HwRender>>,
}
static ACTIVE: Mutex<Option<Arc<Shared>>> = Mutex::new(None);
static LAST: Mutex<Option<Evidence>> = Mutex::new(None);
pub fn stopped_evidence() -> Option<Evidence> {
    LAST.lock().ok()?.clone()
}

#[cfg(test)]
pub(crate) unsafe fn compare_frozen_environment(other: Environment, proc_address: ProcAddress) {
    let state = Arc::new(Shared {
        system: CString::new("fixture-system").unwrap(),
        save: CString::new("fixture-save").unwrap(),
        options: options("fixture.bin").unwrap(),
        framebuffer: 7,
        proc_address,
        gles: [3, 2],
        pending: AtomicBool::new(false),
        notifications: AtomicU64::new(0),
        audio: AtomicU64::new(0),
        rejected: AtomicU64::new(0),
        inputs: AtomicU64::new(0),
        waker: Waker::noop().clone(),
        failure: producer_failure::Latch::default(),
        output: Mutex::new(Evidence::default()),
        hw: Mutex::new(None),
    });
    assert!(ACTIVE.lock().unwrap().replace(state).is_none());
    for command in [
        0x10033, 0x1002c, 3, 6, 9, 10, 14, 15, 16, 17, 30, 31, 32, 35, 37, 39, 49, 52, 56, 1234567,
    ] {
        assert_eq!(
            environment(command, std::ptr::null_mut()),
            other(command, std::ptr::null_mut()),
            "null command{command}"
        );
    }
    for command in [3, 17, 49, 52] {
        let (mut a, mut b) = (false, false);
        assert_eq!(
            environment(command, (&mut a as *mut bool).cast()),
            other(command, (&mut b as *mut bool).cast())
        );
        assert_eq!(a, b);
    }
    for command in [39, 56] {
        let (mut a, mut b) = (99u32, 99u32);
        assert_eq!(
            environment(command, (&mut a as *mut u32).cast()),
            other(command, (&mut b as *mut u32).cast())
        );
        assert_eq!(a, b);
    }
    for format in [1u32, 2] {
        let mut a = format;
        let mut b = format;
        assert_eq!(
            environment(10, (&mut a as *mut u32).cast()),
            other(10, (&mut b as *mut u32).cast())
        );
    }
    for command in [9, 30, 31] {
        let (mut a, mut b) = (std::ptr::null::<c_char>(), std::ptr::null::<c_char>());
        assert_eq!(
            environment(command, (&mut a as *mut *const c_char).cast()),
            other(command, (&mut b as *mut *const c_char).cast())
        );
        assert_eq!(CStr::from_ptr(a), CStr::from_ptr(b));
    }
    for key in SETTINGS
        .iter()
        .map(|(k, _)| *k)
        .chain(["armsx2_bios", "not-a-supported-setting"])
    {
        let key = CString::new(key).unwrap();
        let mut a = Variable {
            key: key.as_ptr(),
            value: std::ptr::null(),
        };
        let mut b = Variable {
            key: key.as_ptr(),
            value: std::ptr::null(),
        };
        let yes = environment(15, (&mut a as *mut Variable).cast());
        assert_eq!(yes, other(15, (&mut b as *mut Variable).cast()));
        if yes {
            assert_eq!(CStr::from_ptr(a.value), CStr::from_ptr(b.value));
        } else {
            assert!(a.value.is_null() && b.value.is_null());
        }
    }
    for (context_type, major, minor) in [(4, 3, 0), (5, 3, 2), (5, 4, 0), (1, 3, 0)] {
        let mut a = HwRender {
            context_type,
            major,
            minor,
            reset: None,
            framebuffer: None,
            get_proc_address: None,
            depth: true,
            stencil: true,
            bottom_left: true,
            cache_context: false,
            destroy: None,
            debug_context: false,
        };
        let mut b = a;
        let yes = environment(14, (&mut a as *mut HwRender).cast());
        assert_eq!(yes, other(14, (&mut b as *mut HwRender).cast()));
        if yes {
            assert_eq!(a.framebuffer.unwrap()(), b.framebuffer.unwrap()());
            assert_eq!(a.framebuffer.unwrap()(), 7);
            assert_eq!(
                a.get_proc_address.unwrap()(c"glFenceSync".as_ptr()).is_some(),
                b.get_proc_address.unwrap()(c"glFenceSync".as_ptr()).is_some()
            );
        }
    }
    let mut unknown = 0u32;
    assert!(!environment(1234567, (&mut unknown as *mut u32).cast()));
    assert!(!other(1234567, (&mut unknown as *mut u32).cast()));
    *ACTIVE.lock().unwrap() = None;
}
fn shared() -> Option<Arc<Shared>> {
    ACTIVE.lock().ok()?.clone()
}
fn fail(s: &Shared, reason: &str) {
    if let Ok(mut o) = s.output.lock() {
        if o.first_failure.is_none() {
            o.first_failure = Some(reason.into());
        }
    }
    observe::request_stop(observe::StopReason::Dependency);
    s.waker.wake_by_ref();
}
fn counted(s: &Shared, command: u32, result: bool) -> bool {
    if let Ok(mut o) = s.output.lock() {
        // Fixed command vocabulary; unknown commands share a bounded sentinel.
        let key = if matches!(
            command,
            3 | 6
                | 7
                | 9
                | 10
                | 14
                | 15
                | 16
                | 17
                | 30
                | 31
                | 32
                | 35
                | 37
                | 39
                | 49
                | 52
                | 56
                | 0x10033
                | 0x1002c
                | 0x20000
                | 0x20001
                | 0x20002
        ) {
            command
        } else {
            u32::MAX
        };
        *o.callback_calls.entry(key).or_default() += 1;
        if !result {
            *o.callback_refusals.entry(key).or_default() += 1;
        }
    }
    result
}
pub fn options(firmware: &str) -> Result<BTreeMap<Vec<u8>, CString>> {
    if firmware.is_empty()
        || Path::new(firmware).file_name().and_then(|v| v.to_str()) != Some(firmware)
    {
        return Err("invalid firmware name".into());
    }
    let mut v = BTreeMap::new();
    v.insert(
        b"armsx2_bios".to_vec(),
        CString::new(firmware).map_err(|e| e.to_string())?,
    );
    for (k, value) in SETTINGS {
        v.insert(k.as_bytes().to_vec(), CString::new(value).unwrap());
    }
    Ok(v)
}
unsafe extern "C" fn framebuffer() -> usize {
    shared().map_or(0, |s| s.framebuffer)
}
unsafe extern "C" fn available() {
    if let Some(s) = shared() {
        s.notifications.fetch_add(1, Ordering::Relaxed);
        if !s.pending.swap(true, Ordering::AcqRel) {
            s.waker.wake_by_ref();
        }
    }
}
unsafe extern "C" fn poll() {}
unsafe extern "C" fn input(_port: u32, _device: u32, _index: u32, _id: u32) -> i16 {
    if let Some(s) = shared() {
        s.inputs.fetch_add(1, Ordering::Relaxed);
    }
    0
}
unsafe extern "C" fn batch(data: *const i16, frames: usize) -> usize {
    if frames == 0 {
        return 0;
    }
    if let Some(s) = shared() {
        if !data.is_null() && frames <= 65536 {
            s.audio.fetch_add(frames as u64, Ordering::Relaxed);
        } else {
            s.rejected.fetch_add(1, Ordering::Relaxed);
            fail(&s, "invalid audio batch");
        }
    }
    // The donor generated these frames. Consume without retaining or changing
    // guest pacing; this host has no audio device or growing queue.
    frames
}
unsafe extern "C" fn sample(_l: i16, _r: i16) {
    if let Some(s) = shared() {
        s.audio.fetch_add(1, Ordering::Relaxed);
    }
}
unsafe extern "C" fn video(data: *const c_void, w: u32, h: u32, _pitch: usize) {
    if let Some(s) = shared() {
        if let Ok(mut o) = s.output.lock() {
            if data as usize == usize::MAX {
                o.hardware_frames += 1;
                o.dimensions = [w, h];
            } else if !data.is_null() {
                o.software_frames += 1;
                o.unexpected_software = true;
            }
        }
        if !data.is_null() && data as usize != usize::MAX {
            fail(&s, "software output fallback");
        }
        if w > 1024 || h > 1024 {
            fail(&s, "native output exceeds matching target");
        }
    }
}
#[repr(C)]
struct Variable {
    key: *const c_char,
    value: *const c_char,
}
#[repr(C)]
struct Message {
    text: *const c_char,
    frames: u32,
}
unsafe extern "C" fn environment(command: u32, data: *mut c_void) -> bool {
    let Some(s) = shared() else { return false };
    let result = environment_answer(&s, command, data);
    counted(&s, command, result)
}
unsafe fn environment_answer(s: &Shared, command: u32, data: *mut c_void) -> bool {
    if command == producer_failure::ENVIRONMENT_COMMAND && !data.is_null() {
        if s.failure
            .record(*(data as *const producer_failure::Failure))
        {
            fail(s, "native dependency failure");
        }
        return true;
    }
    if command == 7 {
        fail(s, "native shutdown");
        return true;
    }
    if command == 0x10033 || command == 0x1002c {
        return true;
    }
    if data.is_null() {
        return false;
    }
    match command {
        0x20000 => {
            let v = *(data as *const [f32; 5]);
            if v.iter().all(|v| v.is_finite()) {
                if let Ok(mut o) = s.output.lock() {
                    o.metrics = Some(v);
                    return true;
                }
            }
            false
        }
        0x20001 => {
            let v = *(data as *const [f32; 9]);
            if v.iter().all(|v| v.is_finite() && *v >= 0.) {
                if let Ok(mut o) = s.output.lock() {
                    o.render_costs = Some(v);
                    return true;
                }
            }
            false
        }
        3 => {
            *(data as *mut bool) = true;
            true
        }
        9 | 30 => {
            *(data as *mut *const c_char) = s.system.as_ptr();
            true
        }
        31 => {
            *(data as *mut *const c_char) = s.save.as_ptr();
            true
        }
        10 => *(data as *const u32) == 1,
        14 => {
            let hw = &mut *(data as *mut HwRender);
            let requested = match hw.context_type {
                4 => [3, 0],
                5 => [hw.major, hw.minor],
                _ => return false,
            };
            if requested > s.gles {
                return false;
            }
            hw.framebuffer = Some(framebuffer);
            hw.get_proc_address = Some(s.proc_address);
            if let Ok(mut o) = s.output.lock() {
                o.hardware_request = Some([
                    hw.context_type,
                    hw.major,
                    hw.minor,
                    u32::from(hw.depth),
                    u32::from(hw.stencil),
                    u32::from(hw.bottom_left),
                    u32::from(hw.cache_context),
                ]);
            }
            if let Ok(mut slot) = s.hw.lock() {
                *slot = Some(*hw);
                true
            } else {
                false
            }
        }
        15 => {
            let var = &mut *(data as *mut Variable);
            if var.key.is_null() {
                return false;
            }
            let key = CStr::from_ptr(var.key).to_bytes();
            var.value = s.options.get(key).map_or(std::ptr::null(), |v| v.as_ptr());
            if let Some(value) = s.options.get(key) {
                if key != b"armsx2_bios" {
                    if let Ok(mut o) = s.output.lock() {
                        o.settings_queries.insert(
                            String::from_utf8_lossy(key).into_owned(),
                            value.to_string_lossy().into_owned(),
                        );
                    }
                }
            }
            !var.value.is_null()
        }
        16 | 35 => true,
        17 => {
            *(data as *mut bool) = false;
            true
        }
        32 => {
            set_av(s, *(data as *const AvInfo));
            true
        }
        37 => {
            let g = *(data as *const Geometry);
            if let Ok(mut o) = s.output.lock() {
                o.av[..4].copy_from_slice(&[
                    g.width as f64,
                    g.height as f64,
                    g.max_width as f64,
                    g.max_height as f64,
                ]);
            }
            true
        }
        39 => {
            *(data as *mut u32) = 0;
            true
        }
        56 => {
            *(data as *mut u32) = 4;
            true
        }
        49 | 52 => {
            *(data as *mut bool) = false;
            true
        }
        6 => {
            let _ = &*(data as *const Message);
            true
        }
        _ => false,
    }
}
fn set_av(s: &Shared, v: AvInfo) {
    if let Ok(mut o) = s.output.lock() {
        o.av = [
            v.geometry.width as f64,
            v.geometry.height as f64,
            v.geometry.max_width as f64,
            v.geometry.max_height as f64,
            v.timing.fps,
            v.timing.sample_rate,
        ];
    }
}

#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}
type SetFrame = unsafe extern "C" fn(Option<unsafe extern "C" fn()>) -> bool;
type Boundary = unsafe extern "C" fn(Option<observe::boundary::Callback>);
type Stream = extern "C" fn(u32, u32, u32, u32, u64, u32, u32, u32, u32);
type SetStream = unsafe extern "C" fn(Option<Stream>);
type Output = unsafe extern "C" fn(*mut observe::output_sync::Command) -> u32;
type SetOutput = unsafe extern "C" fn(Option<Output>);
type NativeResult = extern "C" fn(u32, u32, usize, u64, u64, u64, u64, u64) -> u32;
type SetResult = unsafe extern "C" fn(Option<NativeResult>);
pub struct Config<'a> {
    pub system: &'a Path,
    pub save: &'a Path,
    pub firmware: &'a str,
    pub framebuffer: usize,
    pub proc_address: ProcAddress,
    pub gles: [u32; 2],
    pub waker: Waker,
}
pub struct Host {
    library: Arc<Library>,
    signals: NativeSignals,
    shared: Arc<Shared>,
    run: unsafe extern "C" fn(),
    unload: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
    pause: unsafe extern "C" fn(bool) -> bool,
    ready: unsafe extern "C" fn() -> bool,
    set_frame: SetFrame,
    boundary: Boundary,
    stream: SetStream,
    output: SetOutput,
    result: SetResult,
    loaded: bool,
    reset: bool,
    initialized: bool,
    stopped: bool,
    started: bool,
    speed: unsafe extern "C" fn(f32) -> bool,
    _owner: PhantomData<Rc<()>>,
}
impl Host {
    /// # Safety
    /// Only the owner calls this API; keep the matching GLES context current
    /// until stop has joined native workers, then destroy frontend graphics.
    pub unsafe fn load(path: &Path, game: &Path, c: Config<'_>) -> Result<Self> {
        if c.gles < [3, 1] || c.framebuffer == 0 {
            return Err("required GLES/framebuffer missing".into());
        }
        let shared = Arc::new(Shared {
            system: CString::new(c.system.as_os_str().as_encoded_bytes())
                .map_err(|e| e.to_string())?,
            save: CString::new(c.save.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?,
            options: options(c.firmware)?,
            framebuffer: c.framebuffer,
            proc_address: c.proc_address,
            gles: c.gles,
            pending: AtomicBool::new(false),
            notifications: AtomicU64::new(0),
            audio: AtomicU64::new(0),
            rejected: AtomicU64::new(0),
            inputs: AtomicU64::new(0),
            waker: c.waker,
            failure: producer_failure::Latch::default(),
            output: Mutex::new(Evidence::default()),
            hw: Mutex::new(None),
        });
        let library = Arc::new(Library::new(path).map_err(|e| e.to_string())?);
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {
                *library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|e| e.to_string())?
            };
        }
        let run = symbol!("retro_run", unsafe extern "C" fn());
        let signals =
            NativeSignals::capture(run as *const () as *const c_void).map_err(|e| e.to_string())?;
        let mut host = Self {
            run,
            unload: symbol!("retro_unload_game", unsafe extern "C" fn()),
            deinit: symbol!("retro_deinit", unsafe extern "C" fn()),
            pause: symbol!("retro_psx_set_paused", unsafe extern "C" fn(bool) -> bool),
            ready: symbol!("retro_psx_ready", unsafe extern "C" fn() -> bool),
            speed: symbol!("retro_psx_set_speed", unsafe extern "C" fn(f32) -> bool),
            set_frame: symbol!("retro_psx_set_frame_callback", SetFrame),
            boundary: symbol!("retro_psx_observer_set_callback", Boundary),
            stream: symbol!("retro_psx_observer_set_stream_callback", SetStream),
            output: symbol!("retro_psx_observer_set_output_callback", SetOutput),
            result: symbol!("retro_psx_observer_set_result_callback", SetResult),
            library: library.clone(),
            signals,
            shared: shared.clone(),
            loaded: false,
            reset: false,
            initialized: false,
            stopped: false,
            started: false,
            _owner: PhantomData,
        };
        let mut slot = ACTIVE.lock().map_err(|_| "callback lock")?;
        if slot.is_some() {
            host.stopped = true;
            return Err("another minimal host owns callbacks".into());
        }
        *slot = Some(shared);
        drop(slot);
        if symbol!("retro_api_version", unsafe extern "C" fn() -> u32)() != 1 {
            return Err("unsupported core API".into());
        }
        observe::session_ready();
        observe::output_sync::begin().map_err(str::to_owned)?;
        observe::common::begin();
        (host.boundary)(Some(observe::callback));
        (host.stream)(Some(observe::stream_callback));
        (host.output)(Some(observe::output_sync::callback));
        (host.result)(Some(observe::common::callback));
        symbol!("retro_set_environment", unsafe extern "C" fn(Environment))(environment);
        symbol!(
            "retro_set_video_refresh",
            unsafe extern "C" fn(unsafe extern "C" fn(*const c_void, u32, u32, usize))
        )(video);
        symbol!(
            "retro_set_audio_sample",
            unsafe extern "C" fn(unsafe extern "C" fn(i16, i16))
        )(sample);
        symbol!(
            "retro_set_audio_sample_batch",
            unsafe extern "C" fn(unsafe extern "C" fn(*const i16, usize) -> usize)
        )(batch);
        symbol!(
            "retro_set_input_poll",
            unsafe extern "C" fn(unsafe extern "C" fn())
        )(poll);
        symbol!(
            "retro_set_input_state",
            unsafe extern "C" fn(unsafe extern "C" fn(u32, u32, u32, u32) -> i16)
        )(input);
        symbol!("retro_init", unsafe extern "C" fn())();
        host.initialized = true;
        host.shared.output.lock().unwrap().init = true;
        if !(host.set_frame)(Some(available)) {
            return Err("frame callback refused".into());
        }
        if observe::stop_requested() {
            return Err("startup cancelled".into());
        }
        let game = CString::new(game.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        if !symbol!(
            "retro_load_game",
            unsafe extern "C" fn(*const GameInfo) -> bool
        )(&GameInfo {
            path: game.as_ptr(),
            data: std::ptr::null(),
            size: 0,
            meta: std::ptr::null(),
        }) {
            return Err("guest boot refused".into());
        }
        host.loaded = true;
        host.shared.output.lock().unwrap().loaded = true;
        let mut av = AvInfo::default();
        symbol!(
            "retro_get_system_av_info",
            unsafe extern "C" fn(*mut AvInfo)
        )(&mut av);
        set_av(&host.shared, av);
        if !(45.0..=65.0).contains(&av.timing.fps)
            || !matches!(av.timing.sample_rate, 44100.0 | 48000.0)
        {
            return Err("unsupported frame/audio timing".into());
        }
        let hw = host
            .shared
            .hw
            .lock()
            .map_err(|_| "hardware callback lock")?
            .ok_or("no hardware negotiation")?;
        let reset = hw.reset.ok_or("no context reset callback")?;
        if observe::stop_requested() {
            return Err("startup cancelled".into());
        }
        reset();
        host.reset = true;
        host.shared.output.lock().unwrap().reset = true;
        Ok(host)
    }
    pub fn frame_pending(&self) -> bool {
        self.shared.pending.load(Ordering::Acquire)
    }
    pub fn is_ready(&self) -> bool {
        unsafe { (self.ready)() }
    }
    pub fn service(&mut self, force: bool) -> Result<bool> {
        if observe::stop_requested() || self.shared.failure.get().is_some() {
            return Err("observer/native stop".into());
        }
        let ready = unsafe { (self.ready)() };
        if ready && !self.started {
            // Same ready -> pause barrier -> normal scale -> resume ordering
            // as the frozen player, without its session/checkpoint machinery.
            if !unsafe { (self.pause)(true) && (self.speed)(1.) && (self.pause)(false) } {
                return Err("ready/normal-speed transition refused".into());
            }
            self.started = true;
            self.shared.output.lock().unwrap().normal_speed_applied = true;
        }
        if !self.shared.pending.swap(false, Ordering::AcqRel) && !force && ready {
            return Ok(false);
        }
        unsafe {
            (self.run)();
        }
        if let Ok(mut o) = self.shared.output.lock() {
            o.services += 1;
            o.ready = unsafe { (self.ready)() };
        }
        if observe::stop_requested() || self.shared.failure.get().is_some() {
            return Err("observer/native stop".into());
        }
        Ok(true)
    }
    pub fn evidence(&self) -> Evidence {
        let mut o = self.shared.output.lock().unwrap().clone();
        o.notifications = self.shared.notifications.load(Ordering::Relaxed);
        o.audio_frames = self.shared.audio.load(Ordering::Relaxed);
        o.audio_rejected = self.shared.rejected.load(Ordering::Relaxed);
        o.inputs = self.shared.inputs.load(Ordering::Relaxed);
        o.native_failure = self.shared.failure.get().map(|f| [f.kind, f.result]);
        o
    }
    pub fn stop(&mut self) -> Evidence {
        if !self.stopped {
            self.stopped = true;
            observe::retiring();
            unsafe {
                (self.set_frame)(None);
                if self.loaded {
                    // Finite dump replay can already have ended its CPU queue.
                    // Its unload contract aborts pacing, requests stop and joins;
                    // do not enqueue an additional blocking pause into that
                    // retiring queue. The earlier minimal operand is unchanged.
                    #[cfg(not(feature="gs-replay"))]
                    let _ = (self.pause)(true);
                    (self.unload)();
                    self.loaded = false;
                    self.shared.output.lock().unwrap().unloaded = true;
                }
                if self.reset {
                    if let Some(destroy) = self
                        .shared
                        .hw
                        .lock()
                        .ok()
                        .and_then(|h| *h)
                        .and_then(|h| h.destroy)
                    {
                        destroy();
                        self.shared.output.lock().unwrap().context_destroyed = true;
                    }
                    self.reset = false;
                }
                if self.initialized {
                    (self.deinit)();
                    self.initialized = false;
                    self.shared.output.lock().unwrap().deinitialized = true;
                }
                observe::output_sync::native_joined();
                (self.result)(None);
                (self.output)(None);
                (self.boundary)(None);
                (self.stream)(None);
                observe::event(40, [0; 8]);
                observe::session_ready();
            }
            let restored = self.signals.restore().is_ok();
            if !restored {
                std::mem::forget(self.library.clone());
            }
            if let Ok(mut o) = self.shared.output.lock() {
                o.native_signals_restored = restored;
            }
            if let Ok(mut slot) = ACTIVE.lock() {
                *slot = None;
                self.shared.output.lock().unwrap().callback_storage_released = true;
            }
        }
        let evidence = self.evidence();
        if let Ok(mut last) = LAST.lock() {
            *last = Some(evidence.clone());
        }
        evidence
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop();
    }
}

#[path = "minimal_qualification.rs"]
pub mod qualification;
#[cfg(test)]
#[path = "minimal_host_test.rs"]
mod tests;
