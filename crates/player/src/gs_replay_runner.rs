//! Rust lifecycle for the retained native GS runner entry. The same minimal
//! Android graphics owner services metadata; no product runtime is involved.
use crate::{graphics_observer as observe, native_signals::NativeSignals};
use libloading::Library;
use serde::Serialize;
use std::{
    ffi::{c_char, c_void, CString},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::Waker,
    thread::JoinHandle,
};
type Result<T> = std::result::Result<T, String>;
static WAKE: Mutex<Option<Waker>> = Mutex::new(None);
static NOTIFICATIONS: AtomicU64 = AtomicU64::new(0);
static FAILURE: AtomicU64 = AtomicU64::new(0);
extern "C" fn dependency_failure(kind: u32, result: u32) {
    let _ = FAILURE.compare_exchange(
        0,
        (u64::from(kind) << 32) | u64::from(result),
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    observe::request_stop(observe::StopReason::Dependency);
}
extern "C" fn available() {
    NOTIFICATIONS.fetch_add(1, Ordering::Relaxed);
    if let Ok(w) = WAKE.lock() {
        if let Some(w) = w.as_ref() {
            w.wake_by_ref();
        }
    }
}
#[derive(Default, Clone, Serialize)]
pub struct Evidence {
    pub native_failure: Option<[u32; 2]>,
    pub services: u64,
    pub outputs: u64,
    pub notifications: u64,
    pub entry_return: Option<i32>,
    pub joined: bool,
    pub signals_restored: bool,
    pub callback_storage_released: bool,
}
pub struct Runner {
    library: Arc<Library>,
    signals: NativeSignals,
    worker: Option<JoinHandle<i32>>,
    service: unsafe extern "C" fn() -> u32,
    request_stop: unsafe extern "C" fn(),
    retired: unsafe extern "C" fn(),
    frame: unsafe extern "C" fn(Option<extern "C" fn()>) -> bool,
    ready: unsafe extern "C" fn() -> bool,
    evidence: Evidence,
}
impl Runner {
    /// # Safety
    /// Source-bound runner DSO, current matching EGL frontend owner, existing GS
    /// stage callback installed. Caller must stop/join before destroying graphics.
    pub unsafe fn load(path: &Path, game: &Path, waker: Waker) -> Result<Self> {
        let library = Arc::new(Library::new(path).map_err(|e| e.to_string())?);
        macro_rules! symbol {
            ($n:literal,$ty:ty) => {
                *library
                    .get::<$ty>(concat!($n, "\0").as_bytes())
                    .map_err(|e| e.to_string())?
            };
        }
        let entry = symbol!(
            "retro_psx_gsrunner_entry",
            unsafe extern "C" fn(i32, *mut *mut c_char) -> i32
        );
        let signals = NativeSignals::capture_runner(entry as *const () as *const c_void)
            .map_err(|e| e.to_string())?;
        observe::session_ready();
        observe::output_sync::begin().map_err(str::to_owned)?;
        observe::common::begin();
        symbol!(
            "retro_psx_observer_set_callback",
            unsafe extern "C" fn(Option<observe::boundary::Callback>)
        )(Some(observe::callback));
        type Stream = extern "C" fn(u32, u32, u32, u32, u64, u32, u32, u32, u32);
        symbol!(
            "retro_psx_observer_set_stream_callback",
            unsafe extern "C" fn(Option<Stream>)
        )(Some(observe::stream_callback));
        type Output = unsafe extern "C" fn(*mut observe::output_sync::Command) -> u32;
        symbol!(
            "retro_psx_observer_set_output_callback",
            unsafe extern "C" fn(Option<Output>)
        )(Some(observe::output_sync::callback));
        type NativeResult = extern "C" fn(u32, u32, usize, u64, u64, u64, u64, u64) -> u32;
        symbol!(
            "retro_psx_observer_set_result_callback",
            unsafe extern "C" fn(Option<NativeResult>)
        )(Some(observe::common::callback));
        FAILURE.store(0, Ordering::Release);
        symbol!(
            "retro_psx_runner_set_failure",
            unsafe extern "C" fn(Option<extern "C" fn(u32, u32)>)
        )(Some(dependency_failure));
        if !symbol!("retro_psx_runner_prepare", unsafe extern "C" fn() -> bool)() {
            return Err("runner shared context prepare refused".into());
        }
        let mut runner = Self {
            library: library.clone(),
            signals,
            worker: None,
            service: symbol!("retro_psx_runner_service", unsafe extern "C" fn() -> u32),
            request_stop: symbol!("retro_psx_runner_stop", unsafe extern "C" fn()),
            retired: symbol!("retro_psx_runner_retired", unsafe extern "C" fn()),
            frame: symbol!(
                "retro_psx_set_frame_callback",
                unsafe extern "C" fn(Option<extern "C" fn()>) -> bool
            ),
            ready: symbol!("retro_psx_runner_ready", unsafe extern "C" fn() -> bool),
            evidence: Evidence::default(),
        };
        let mut slot = WAKE.lock().map_err(|_| "runner wake lock")?;
        if slot.is_some() {
            return Err("runner callback already owned".into());
        }
        *slot = Some(waker);
        drop(slot);
        NOTIFICATIONS.store(0, Ordering::Relaxed);
        if !(runner.frame)(Some(available)) {
            return Err("runner notifications refused".into());
        }
        let args = [
            "psx-gsrunner",
            "-renderer",
            "gl",
            "-upscale",
            "1",
            "-surfaceless",
            "-loop",
            "0",
        ];
        let mut args: Vec<CString> = args.into_iter().map(|s| CString::new(s).unwrap()).collect();
        args.push(CString::new(game.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?);
        runner.worker = Some(
            std::thread::Builder::new()
                .name("gs-runner-entry".into())
                .spawn(move || {
                    let _keep_code = library;
                    let mut argv: Vec<_> = args.iter().map(|s| s.as_ptr() as *mut c_char).collect();
                    let argc = argv.len() as i32;
                    argv.push(std::ptr::null_mut());
                    entry(argc, argv.as_mut_ptr())
                })
                .map_err(|e| e.to_string())?,
        );
        Ok(runner)
    }
    pub fn is_ready(&self) -> bool {
        unsafe { (self.ready)() }
    }
    pub fn frame_pending(&self) -> bool {
        NOTIFICATIONS.load(Ordering::Relaxed) > self.evidence.notifications
    }
    pub fn service(&mut self, _force: bool) -> Result<bool> {
        if observe::stop_requested() {
            return Err("runner first stop".into());
        }
        if self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return Err("runner entry ended; review packet/output/closure evidence".into());
        }
        let status = unsafe { (self.service)() };
        self.evidence.services += 1;
        self.evidence.notifications = NOTIFICATIONS.load(Ordering::Relaxed);
        match status {
            0 => Ok(false),
            1 => {
                self.evidence.outputs += 1;
                Ok(true)
            }
            _ => {
                observe::request_stop(observe::StopReason::Ownership);
                Err("runner metadata acknowledgment failed".into())
            }
        }
    }
    pub fn stop(&mut self) -> Evidence {
        if !self.evidence.joined {
            observe::retiring();
            unsafe {
                (self.frame)(None);
                (self.request_stop)();
            }
            if let Some(worker) = self.worker.take() {
                match worker.join() {
                    Ok(code) => self.evidence.entry_return = Some(code),
                    Err(_) => {
                        observe::request_stop(observe::StopReason::Fatal);
                    }
                }
            }
            unsafe {
                (self.retired)();
            }
            observe::output_sync::native_joined();
            observe::event(40, [0; 8]);
            self.evidence.joined = true;
            let failure = FAILURE.load(Ordering::Acquire);
            self.evidence.native_failure =
                (failure != 0).then_some([(failure >> 32) as u32, failure as u32]);
            self.evidence.signals_restored = self.signals.restore().is_ok();
            if !self.evidence.signals_restored {
                std::mem::forget(self.library.clone());
            }
            if let Ok(mut slot) = WAKE.lock() {
                *slot = None;
                self.evidence.callback_storage_released = true;
            }
        }
        self.evidence.clone()
    }
}
impl Drop for Runner {
    fn drop(&mut self) {
        self.stop();
    }
}
