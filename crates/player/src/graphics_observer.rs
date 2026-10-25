//! Fixed development instrumentation. No media, paths or GL object names enter
//! exports. Ordinary builds neither sample GL nor create observation files.
use std::path::{Path, PathBuf};
pub const CAPACITY: usize = 256;
pub const PREFIX: usize = 32;
pub const EXPORT_LIMIT: usize = 256 * 1024;
pub const MAX_EXPORTS: usize = 3;
pub const SCRATCH: &str = "observer-scratch-v1";

#[cfg(feature = "graphics-observer")]
pub mod boundary;
#[cfg(feature = "graphics-observer")]
mod control;
#[cfg(feature = "graphics-observer")]
pub mod operator;
#[cfg(feature = "producer-local-output-sync")]
pub mod output_sync;
#[cfg(feature = "graphics-observer")]
pub use control::{join_export, request_stop, service, stop_requested, StopReason};

pub const fn mode() -> &'static str {
    if cfg!(feature = "producer-local-output-sync") {
        "producer-local-output-sync"
    } else if cfg!(feature = "no-game-image-reader") {
        "no-game-image-reader"
    } else {
        "normal"
    }
}
#[cfg(feature = "graphics-observer")]
mod enabled {
    use super::boundary::{self, Dispatch};
    use super::control::{request_stop, StopReason};
    use super::*;
    use crate::retro::ProcAddress;
    use glow::HasContext;
    use serde::Serialize;
    use std::{
        cell::RefCell,
        collections::VecDeque,
        ffi::{c_void, CString},
        sync::{
            atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
            Mutex, OnceLock,
        },
        time::Instant,
    };

    #[derive(Clone, Copy, Default, Debug, Serialize)]
    pub struct Event {
        pub order: u64,
        pub micros: u64,
        pub owner: u32,
        pub context: u32,
        pub surface: u32,
        pub group: u32,
        pub frame: u64,
        pub texture: u32,
        pub stage: u32,
        pub width: u32,
        pub height: u32,
        pub data: [i64; 32],
        pub detail: [i64; 8],
    }
    #[derive(Clone, Serialize)]
    pub struct Snapshot {
        pub version: u32,
        pub capacity: usize,
        pub dropped: u64,
        pub initialization: Vec<Event>,
        pub first_failure: Option<Event>,
        pub first_terminal: Option<Event>,
        pub preceding: Vec<Event>,
        pub tail: VecDeque<Event>,
        pub progress: Progress,
        pub streams: StreamProgress,
        #[cfg(feature = "producer-local-output-sync")]
        pub output_sync: output_sync::Progress,
    }
    #[derive(Clone, Default, Serialize)]
    pub struct StreamProgress {
        pub resources: Vec<Event>,
        pub first_failure: Option<Event>,
        pub resources_omitted: u64,
        pub selected: [u64; 4],
        pub maps: u64,
        pub creates: u64,
        pub null_creates: u64,
        pub waits: [u64; 5],
        pub unmaps: u64,
    }
    #[derive(Clone, Default, Serialize)]
    pub struct Progress {
        pub producer_frame: u64,
        pub published_frame: u64,
        pub consumed_frame: u64,
        pub copied_frame: u64,
        pub acknowledged_frame: u64,
        pub presented_frame: u64,
        pub swap_frame: u64,
    }
    pub struct Recorder {
        snapshot: Snapshot,
        keys: Vec<(u32, usize, u32)>,
        next: u32,
    }
    impl Default for Recorder {
        fn default() -> Self {
            Self {
                snapshot: Snapshot {
                    version: 2,
                    capacity: CAPACITY,
                    dropped: 0,
                    initialization: Vec::with_capacity(4),
                    first_failure: None,
                    first_terminal: None,
                    preceding: Vec::with_capacity(PREFIX),
                    tail: VecDeque::with_capacity(CAPACITY),
                    progress: Progress::default(),
                    streams: StreamProgress {
                        resources: Vec::with_capacity(16),
                        ..Default::default()
                    },
                    #[cfg(feature = "producer-local-output-sync")]
                    output_sync: output_sync::Progress::default(),
                },
                keys: Vec::with_capacity(128),
                next: 0,
            }
        }
    }
    impl Recorder {
        fn token(&mut self, kind: u32, key: usize, fresh: bool) -> u32 {
            if key == 0 {
                return 0;
            }
            if let Some(entry) = self
                .keys
                .iter_mut()
                .find(|entry| entry.0 == kind && entry.1 == key)
            {
                if fresh {
                    self.next += 1;
                    entry.2 = self.next;
                }
                return entry.2;
            }
            if self.keys.len() == 128 {
                self.snapshot.dropped += 1;
                return 0;
            }
            self.next += 1;
            self.keys.push((kind, key, self.next));
            self.next
        }
        pub fn push(&mut self, event: Event, failure: bool) {
            match event.stage {
                60 => {
                    if let Some(count) = self
                        .snapshot
                        .streams
                        .selected
                        .get_mut(event.detail[0] as usize)
                    {
                        *count += 1;
                    }
                    if self.snapshot.streams.resources.len() < 16 {
                        self.snapshot.streams.resources.push(event);
                    } else {
                        self.snapshot.streams.resources_omitted += 1;
                    }
                }
                61 => self.snapshot.streams.maps += 1,
                62 => {
                    self.snapshot.streams.creates += 1;
                    self.snapshot.streams.null_creates += u64::from(event.detail[6] == 0);
                }
                63 => {
                    let index = match event.detail[6] {
                        0x911a => 0,
                        0x911c => 1,
                        0x911b => 2,
                        0x911d => 3,
                        _ => 4,
                    };
                    self.snapshot.streams.waits[index] += 1;
                }
                64 => self.snapshot.streams.unmaps += 1,
                _ => {}
            }
            if failure
                && (60..=64).contains(&event.stage)
                && self.snapshot.streams.first_failure.is_none()
            {
                self.snapshot.streams.first_failure = Some(event);
            }
            let field = match event.stage {
                1 | 2 | 16 => Some(&mut self.snapshot.progress.producer_frame),
                3 => Some(&mut self.snapshot.progress.published_frame),
                4 => Some(&mut self.snapshot.progress.consumed_frame),
                6 => Some(&mut self.snapshot.progress.copied_frame),
                51 => Some(&mut self.snapshot.progress.acknowledged_frame),
                12 => Some(&mut self.snapshot.progress.presented_frame),
                14 => Some(&mut self.snapshot.progress.swap_frame),
                _ => None,
            };
            if let Some(field) = field {
                *field = (*field).max(event.frame);
            }
            if failure && super::control::stop_requested() && self.snapshot.first_terminal.is_none()
            {
                self.snapshot.first_terminal = Some(event);
            }
            let first_admission = event.stage == 11
                && event.detail[0] > 0
                && !self
                    .snapshot
                    .initialization
                    .iter()
                    .any(|old| old.stage == 11 && old.detail[0] > 0);
            if (first_admission
                || !self
                    .snapshot
                    .initialization
                    .iter()
                    .any(|old| old.owner == event.owner && old.context == event.context))
                && self.snapshot.initialization.len() < 4
            {
                self.snapshot.initialization.push(event);
            }
            if failure && self.snapshot.first_failure.is_none() {
                self.snapshot.first_failure = Some(event);
                let start = self.snapshot.tail.len().saturating_sub(PREFIX);
                self.snapshot
                    .preceding
                    .extend(self.snapshot.tail.iter().skip(start).copied());
            }
            if self.snapshot.tail.len() == CAPACITY {
                self.snapshot.tail.pop_front();
                self.snapshot.dropped += 1;
            }
            self.snapshot.tail.push_back(event);
        }
        pub fn snapshot(&self) -> Snapshot {
            #[allow(unused_mut)]
            let mut snapshot = self.snapshot.clone();
            #[cfg(feature = "producer-local-output-sync")]
            {
                snapshot.output_sync = output_sync::snapshot();
            }
            snapshot
        }
    }
    static RECORDER: OnceLock<Mutex<Recorder>> = OnceLock::new();
    static ORDER: AtomicU64 = AtomicU64::new(0);
    static FRAME: AtomicU64 = AtomicU64::new(0);
    static CONSUMED: AtomicU64 = AtomicU64::new(0);
    static DROPPED: AtomicU64 = AtomicU64::new(0);
    static FATAL: AtomicU32 = AtomicU32::new(0);
    static RETIRING: AtomicU32 = AtomicU32::new(0);
    static GROUP: AtomicU32 = AtomicU32::new(0);
    static PROC: AtomicUsize = AtomicUsize::new(0);
    static BINDING: AtomicUsize = AtomicUsize::new(0);
    static START: OnceLock<Instant> = OnceLock::new();
    pub fn produced() -> u64 {
        FRAME.load(Ordering::Acquire)
    }
    pub fn dropped() -> u64 {
        DROPPED.load(Ordering::Relaxed)
    }
    pub fn start(root: &Path, waker: std::task::Waker) -> std::io::Result<()> {
        let recorder = RECORDER
            .get()
            .ok_or_else(|| std::io::Error::other("Observer not initialized"))?;
        super::control::start(root, recorder, waker)
    }
    struct ThreadGl {
        gl: glow::Context,
        functions: [usize; 4],
    }
    thread_local! {static GL:RefCell<Option<ThreadGl>>=const {RefCell::new(None)};}

    /// Platform callback observes current context/surface on the calling owner.
    /// Values are transient private map keys, never export fields.
    pub fn initialize(proc: ProcAddress, binding: fn() -> (usize, usize)) {
        RECORDER.get_or_init(|| Mutex::new(Recorder::default()));
        START.get_or_init(Instant::now);
        PROC.store(proc as usize, Ordering::Release);
        BINDING.store(binding as usize, Ordering::Release);
    }
    fn retain_fatal(code: u32) {
        #[cfg(feature = "producer-local-output-sync")]
        if matches!(code, glow::OUT_OF_MEMORY | glow::CONTEXT_LOST) {
            output_sync::observed(0, 0, false, true);
        }
        if RETIRING.load(Ordering::Acquire) == 0
            && matches!(code, glow::OUT_OF_MEMORY | glow::CONTEXT_LOST)
        {
            let _ = FATAL.compare_exchange(0, code, Ordering::AcqRel, Ordering::Relaxed);
            request_stop(StopReason::Fatal);
        }
    }
    pub fn take_fatal() -> Option<u32> {
        match FATAL.swap(0, Ordering::AcqRel) {
            0 => None,
            code => Some(code),
        }
    }
    pub fn retiring() {
        RETIRING.store(1, Ordering::Release);
        event(39, [0; 8]);
    }
    pub fn session_ready() {
        RETIRING.store(0, Ordering::Release);
    }
    pub fn consumed() -> u64 {
        CONSUMED.load(Ordering::Acquire)
    }
    pub fn existing_error(code: u32) {
        if matches!(code, glow::OUT_OF_MEMORY | glow::CONTEXT_LOST) {
            #[cfg(feature = "producer-local-output-sync")]
            output_sync::observed(0, 0, false, code == glow::CONTEXT_LOST);
            request_stop(StopReason::Fatal);
        }
        emit(
            2,
            consumed(),
            0,
            30,
            0,
            0,
            [0; 32],
            [code as i64, 0, 0, 0, 0, 0, 0, 0],
            code != 0,
        );
    }
    pub fn event(stage: u32, detail: [i64; 8]) {
        if stage == 14 && detail[0] == 0 {
            request_stop(StopReason::Swap);
        }
        if stage == 43 {
            // Only a successful replacement of the actual frontend share group
            // retires its old pending error. Native close alone must not swallow
            // an unforwarded startup fault before typed recovery can see it.
            FATAL.store(0, Ordering::Release);
            session_ready();
            GROUP.fetch_add(1, Ordering::AcqRel);
        }
        emit(
            2,
            consumed(),
            0,
            stage,
            0,
            0,
            [0; 32],
            detail,
            (stage == 14 && detail[0] == 0)
                || (stage == 11 && detail[0] > 0 && detail[6] == 0 && detail[7] == 0),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        owner: u32,
        frame: u64,
        texture: u32,
        stage: u32,
        width: u32,
        height: u32,
        mut data: [i64; 32],
        detail: [i64; 8],
        failure: bool,
    ) {
        let Some(recorder) = RECORDER.get() else {
            return;
        };
        // Ordinary samples may drop under contention. The first failure must
        // survive: its bounded copy can wait for the short snapshot copy, never
        // for file I/O or formatting (both happen after the lock is released).
        let Some(mut recorder) = (if failure || (60..=64).contains(&stage) {
            recorder.lock().ok()
        } else {
            recorder.try_lock().ok()
        }) else {
            DROPPED.fetch_add(1, Ordering::Relaxed);
            return;
        };
        let binding = BINDING.load(Ordering::Acquire);
        let (context, surface) = if binding == 0 {
            (0, 0)
        } else {
            unsafe { std::mem::transmute::<usize, fn() -> (usize, usize)>(binding)() }
        };
        let context = recorder.token(1, context, stage == 43);
        let surface = recorder.token(2, surface, stage == 44);
        let group = GROUP.load(Ordering::Acquire);
        let texture = recorder.token(
            if (60..=64).contains(&stage) {
                0x60000 + group
            } else {
                0x20000 + group * 4
            },
            texture as usize,
            stage == 25 || stage == 60,
        );
        // FBOs are context-local; attachment object names are share-group names.
        for field in [12, 13] {
            data[field] = recorder.token(0x10000 + context, data[field] as usize, false) as i64;
        }
        for field in [10, 11] {
            let renderbuffer = u32::from(data[24 + field - 10] == glow::RENDERBUFFER as i64);
            data[field] = recorder.token(
                0x20000 + group * 4 + renderbuffer,
                data[field] as usize,
                false,
            ) as i64;
        }
        recorder.push(
            Event {
                order: ORDER.fetch_add(1, Ordering::Relaxed) + 1,
                micros: START
                    .get()
                    .map_or(0, |start| start.elapsed().as_micros() as u64),
                owner,
                context,
                surface,
                group,
                frame,
                texture,
                stage,
                width,
                height,
                data,
                detail,
            },
            failure,
        );
        drop(recorder);
        if failure {
            super::control::notify();
        }
    }
    pub fn sample(stage: u32, detail: [i64; 8]) {
        unsafe {
            native(
                stage,
                2,
                consumed(),
                0,
                0,
                0,
                std::ptr::null_mut(),
                detail,
                None,
            );
        }
    }
    /// Numeric call-site ABI; samples only the current valid owner. No binding
    /// change, readback, wait loop, formatting or file I/O occurs here.
    #[allow(clippy::too_many_arguments)]
    unsafe fn native(
        stage: u32,
        owner: u32,
        frame: u64,
        texture: u32,
        width: u32,
        height: u32,
        fence: *mut c_void,
        mut detail: [i64; 8],
        dispatch: Option<&Dispatch>,
    ) -> u64 {
        let frame = if stage == 1 {
            FRAME.fetch_add(1, Ordering::AcqRel) + 1
        } else {
            frame
        };
        if stage == 4 {
            CONSUMED.store(frame, Ordering::Release);
        }
        #[cfg(feature = "producer-local-output-sync")]
        if output_sync::context_unavailable() {
            // Preserve a post-flush/lifecycle record, without probing GL after
            // an already reported context fault solely for bookkeeping.
            let mut data = [0; 32];
            data[23] = -2;
            data[26] = i64::from(!fence.is_null());
            emit(
                owner,
                frame,
                texture,
                stage,
                width,
                height,
                data,
                [0, 0, 0, 0, -2, 0, 0, -3],
                false,
            );
            return frame;
        }
        let proc = PROC.load(Ordering::Acquire);
        if proc == 0 {
            return frame;
        }
        let binding = BINDING.load(Ordering::Acquire);
        let current = if binding == 0 {
            0
        } else {
            std::mem::transmute::<usize, fn() -> (usize, usize)>(binding)().0
        };
        if current == 0 {
            #[cfg(feature = "producer-local-output-sync")]
            output_sync::observed(frame, fence as usize, false, true);
            request_stop(StopReason::Context);
            emit(
                owner,
                frame,
                texture,
                stage,
                width,
                height,
                [0; 32],
                [0, 0, 0, 0, 0, 0, 0, -1],
                true,
            );
            return frame;
        }
        GL.with(|slot| {
            let mut slot = slot.borrow_mut();
            let observation = slot.get_or_insert_with(|| {
                let mut functions = [0; 4];
                let gl = glow::Context::from_loader_function(|name| {
                    let name_c = CString::new(name).unwrap();
                    let proc = std::mem::transmute::<usize, ProcAddress>(proc);
                    let address = proc(name_c.as_ptr()).map_or(std::ptr::null(), |function| function as *const _);
                    if let Some(index) = ["glFenceSync", "glIsSync", "glGetSynciv", "glGetError"].iter().position(|n| *n == name) {
                        functions[index] = address as usize;
                    }
                    address
                });
                ThreadGl { gl, functions }
            });
            let gl = &observation.gl;
            let mut data = [0i64; 32];
            // Each consumed fatal result is forwarded once to typed recovery.
            // Multiple GL flags are not assumed to arrive in causal order.
            for item in data.iter_mut().take(8) {
                let error = gl.get_error();
                *item = error as i64;
                retain_fatal(error);
                if error == 0 {
                    break;
                }
            }
            #[cfg(feature = "producer-local-output-sync")]
            if output_sync::context_unavailable() {
                data[23] = -2;
                data[26] = i64::from(!fence.is_null());
                emit(owner,frame,texture,stage,width,height,data,[0,0,0,0,-2,0,0,-3],true);
                return;
            }
            if !matches!(stage, 3 | 4 | 23 | 25 | 40 | 41 | 51 | 52 | 53) {
                data[12] = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING) as i64;
                data[13] = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as i64;
                // A surfaceless producer's unused default read FBO may be
                // undefined; completeness is meaningful only for its draw or
                // actual copy targets, never that unused read framebuffer.
                if matches!(stage, 5 | 6 | 7 | 11 | 12 | 15) {
                    data[8] = gl.check_framebuffer_status(glow::READ_FRAMEBUFFER) as i64;
                }
                if matches!(stage, 1 | 2 | 16 | 17 | 5 | 6 | 7 | 11 | 12 | 13 | 15) {
                    data[9] = gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER) as i64;
                }
                for (target, index, binding) in [
                    (glow::READ_FRAMEBUFFER, 10, data[12]),
                    (glow::DRAW_FRAMEBUFFER, 11, data[13]),
                ] {
                    if binding != 0 {
                        data[24 + index - 10] = gl.get_framebuffer_attachment_parameter_i32(
                            target,
                            glow::COLOR_ATTACHMENT0,
                            glow::FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE,
                        ) as i64;
                        if matches!(
                            data[24 + index - 10] as u32,
                            glow::TEXTURE | glow::RENDERBUFFER
                        ) {
                            data[index] = gl.get_framebuffer_attachment_parameter_i32(
                                target,
                                glow::COLOR_ATTACHMENT0,
                                glow::FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
                            ) as i64;
                        }
                    }
                }
                data[26] = i64::from(!fence.is_null());
                let mut values = [0; 4];
                gl.get_parameter_i32_slice(glow::VIEWPORT, &mut values);
                for i in 0..4 {
                    data[14 + i] = values[i] as i64;
                }
                gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut values);
                for i in 0..4 {
                    data[18 + i] = values[i] as i64;
                }
                data[22] = i64::from(gl.is_enabled(glow::SCISSOR_TEST));
            }
            // Metadata contains the native call site's actual GLAD function
            // addresses and current EGL context. They are compared, never exported.
            if let Some(dispatch) = dispatch {
                let abi = dispatch.version == 2
                    && dispatch.size as usize == std::mem::size_of::<Dispatch>();
                let context = abi && current == dispatch.context && current != 0;
                detail = [
                    i64::from(abi),
                    i64::from(context),
                    i64::from(dispatch.create != 0 && dispatch.create == observation.functions[0]),
                    i64::from(dispatch.valid != 0 && dispatch.valid == observation.functions[1]),
                    -2,
                    boundary::pair(stage, frame, fence as usize, *dispatch),
                    i64::from(dispatch.status != 0 && dispatch.status == observation.functions[2]),
                    i64::from(dispatch.error != 0 && dispatch.error == observation.functions[3]),
                ];
                if !context || dispatch.valid == 0 {
                    request_stop(StopReason::Context);
                } else if !fence.is_null() {
                    let sync = glow::NativeFence(fence.cast());
                    let (status, native_valid, mismatch) = boundary::classify(true,
                        || std::mem::transmute::<usize, unsafe extern "system" fn(*mut c_void) -> u8>(dispatch.valid)(fence) != 0,
                        || gl.is_sync(sync),
                        || gl.get_sync_parameter_i32(sync, glow::SYNC_STATUS) as i64);
                    data[23] = status;
                    detail[4] = native_valid;
                    if mismatch { request_stop(StopReason::Dispatch); }
                }
                if detail[5] == -1 {
                    request_stop(StopReason::Handle);
                }
            } else if !fence.is_null() {
                let sync = glow::NativeFence(fence.cast());
                data[23] = if gl.is_sync(sync) {
                    gl.get_sync_parameter_i32(sync, glow::SYNC_STATUS) as i64
                } else {
                    -1
                };
            }
            data[26] = i64::from(!fence.is_null());
            #[cfg(feature = "producer-local-output-sync")]
            if matches!(stage, 16 | 2) {
                output_sync::observed(frame, fence as usize,
                    detail[4] == 1 && data[23] != -1,
                    detail[1] == 0 || data[..8].contains(&(glow::CONTEXT_LOST as i64)));
            }
            if matches!(stage, 2 | 7 | 16 | 17 | 52) && (fence.is_null() || data[23] == -1) {
                request_stop(StopReason::Fence);
            }
            let bad = stage == 53 || (stage == 25 && texture == 0)
                || (stage == 11 && detail[0] > 0 && detail[6] == 0)
                || data[..8].iter().any(|v| *v != 0)
                || [data[8], data[9]]
                    .iter()
                    .any(|v| *v != 0 && *v != glow::FRAMEBUFFER_COMPLETE as i64)
                || (dispatch.is_some() && (detail[0] == 0 || detail[1] == 0 || detail[5] == -1))
                || (matches!(stage, 2 | 7 | 16 | 17 | 52) && fence.is_null())
                || data[23] == -1;
            emit(
                owner, frame, texture, stage, width, height, data, detail, bad,
            );
            let mut post = data;
            post[..8].fill(0);
            let mut any = false;
            for item in post.iter_mut().take(8) {
                let code = gl.get_error();
                *item = code as i64;
                retain_fatal(code);
                if code == 0 {
                    break;
                }
                any = true;
            }
            if any {
                let mut after = detail;
                after[7] = 1;
                emit(
                    owner, frame, texture, stage, width, height, post, after, true,
                );
            }
        });
        frame
    }
    /// # Safety
    /// GPU stages require the caller's current GL context. Stage 23 reports
    /// only a CPU handoff miss, with a null fence and no context requirement.
    /// Any non-null fence must still belong to its live share group; no callback
    /// may outlive native teardown.
    #[allow(clippy::too_many_arguments)]
    pub unsafe extern "C" fn callback(
        stage: u32,
        owner: u32,
        frame: u64,
        texture: u32,
        width: u32,
        height: u32,
        fence: *mut c_void,
        dispatch: *const Dispatch,
    ) -> u64 {
        #[cfg(feature = "producer-local-output-sync")]
        if owner == 2 && !fence.is_null() {
            output_sync::unexpected_frontend(frame);
            return frame;
        }
        if stage == 50 {
            return u64::from(!cfg!(feature = "no-game-image-reader"));
        }
        if stage == 23 {
            // ConsumeFrame's empty/duplicate/busy branch transfers no GPU
            // object. It can run before the GS owner loads native GLAD/EGL.
            // Retain the CPU event without requiring or querying GL dispatch.
            emit(
                owner,
                frame,
                texture,
                stage,
                width,
                height,
                [0; 32],
                [0, 0, 0, 0, 0, 0, 0, -2],
                false,
            );
            return frame;
        }
        if stage == 53 {
            request_stop(StopReason::Ownership);
        }
        native(
            stage,
            owner,
            frame,
            texture,
            width,
            height,
            fence,
            [0; 8],
            dispatch.as_ref(),
        )
    }
    /// Numeric producer operations; no GL error reads, fences, waits or payloads.
    #[allow(clippy::too_many_arguments)]
    pub extern "C" fn stream_callback(
        stage: u32,
        implementation: u32,
        target: u32,
        resource: u32,
        generation: u64,
        offset: u32,
        length: u32,
        capacity: u32,
        result: u32,
    ) {
        let failed =
            (stage == 62 && result == 0) || (stage == 63 && !matches!(result, 0x911a | 0x911c));
        if failed {
            request_stop(StopReason::Dependency);
        }
        emit(
            1,
            FRAME.load(Ordering::Acquire),
            resource,
            stage,
            0,
            0,
            [0; 32],
            [
                implementation as i64,
                target as i64,
                generation as i64,
                offset as i64,
                length as i64,
                capacity as i64,
                result as i64,
                0,
            ],
            failed,
        );
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn stream_counters_and_first_failure_survive_tail_eviction() {
            let mut recorder = super::Recorder::default();
            recorder.push(
                super::Event {
                    stage: 60,
                    texture: 9,
                    detail: [1, 0x8892, 1, 0, 256, 256, 1, 0],
                    ..Default::default()
                },
                false,
            );
            for index in 0..400 {
                recorder.push(
                    super::Event {
                        stage: 63,
                        detail: [
                            1,
                            0x8892,
                            1,
                            0,
                            16,
                            256,
                            if index == 0 { 0x911d } else { 0x911a },
                            0,
                        ],
                        ..Default::default()
                    },
                    index == 0,
                );
            }
            let snapshot = recorder.snapshot();
            assert_eq!(snapshot.streams.selected[1], 1);
            assert_eq!(snapshot.streams.resources.len(), 1);
            assert_eq!(snapshot.streams.waits, [399, 0, 0, 1, 0]);
            assert_eq!(snapshot.first_failure.unwrap().detail[6], 0x911d);
            assert_eq!(snapshot.streams.first_failure.unwrap().detail[6], 0x911d);
            assert_eq!(snapshot.tail.len(), super::CAPACITY);
        }
        use super::*;
        #[test]
        fn bounded_first_failure_preceding_and_owner_frame_correlation() {
            let mut recorder = Recorder::default();
            for order in 1..=1024 {
                let e = Event {
                    order,
                    owner: if order % 2 == 0 { 1 } else { 2 },
                    frame: order / 2,
                    stage: order as u32,
                    ..Event::default()
                };
                recorder.push(e, order == 40 || order == 900);
            }
            let snapshot = recorder.snapshot();
            assert_eq!(snapshot.tail.len(), CAPACITY);
            assert_eq!(snapshot.dropped, 1024 - CAPACITY as u64);
            assert_eq!(snapshot.first_failure.unwrap().order, 40);
            assert_eq!(snapshot.preceding.len(), PREFIX);
            assert_eq!(snapshot.preceding.last().unwrap().frame, 19);
            assert!(serde_json::to_vec(&snapshot).unwrap().len() < EXPORT_LIMIT);
        }
        #[cfg(target_os = "linux")]
        #[test]
        fn real_incomplete_framebuffer_is_retained_without_binding_changes() {
            use khronos_egl as egl;
            type Egl = egl::DynamicInstance<egl::EGL1_5>;
            static API: OnceLock<Egl> = OnceLock::new();
            unsafe extern "C" fn proc(
                name: *const std::ffi::c_char,
            ) -> Option<unsafe extern "C" fn()> {
                API.get()
                    .unwrap()
                    .get_proc_address(std::ffi::CStr::from_ptr(name).to_str().unwrap())
                    .map(|function| {
                        std::mem::transmute::<extern "system" fn(), unsafe extern "C" fn()>(
                            function,
                        )
                    })
            }
            fn binding() -> (usize, usize) {
                let api = API.get().unwrap();
                (
                    api.get_current_context()
                        .map_or(0, |context| context.as_ptr() as usize),
                    0,
                )
            }
            unsafe {
                let api = API.get_or_init(|| Egl::load_required().unwrap());
                let display = api
                    .get_platform_display(0x31dd, std::ptr::null_mut(), &[egl::ATTRIB_NONE])
                    .unwrap();
                api.initialize(display).unwrap();
                api.bind_api(egl::OPENGL_ES_API).unwrap();
                let config = api
                    .choose_first_config(
                        display,
                        &[
                            egl::SURFACE_TYPE,
                            egl::PBUFFER_BIT,
                            egl::RENDERABLE_TYPE,
                            egl::OPENGL_ES3_BIT,
                            egl::NONE,
                        ],
                    )
                    .unwrap()
                    .unwrap();
                let context = api
                    .create_context(
                        display,
                        config,
                        None,
                        &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
                    )
                    .unwrap();
                let surface = api
                    .create_pbuffer_surface(
                        display,
                        config,
                        &[egl::WIDTH, 2, egl::HEIGHT, 2, egl::NONE],
                    )
                    .unwrap();
                api.make_current(display, Some(surface), Some(surface), Some(context))
                    .unwrap();
                initialize(proc, binding);
                let gl = glow::Context::from_loader_function(|name| {
                    api.get_proc_address(name)
                        .map_or(std::ptr::null(), |function| function as *const _)
                });
                let fbo = gl.create_framebuffer().unwrap();
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
                callback(5, 2, 123, 0, 2, 2, std::ptr::null_mut(), std::ptr::null());
                assert_eq!(gl.get_error(), glow::NO_ERROR);
                assert_eq!(
                    gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING) as u32,
                    fbo.0.get()
                );
                let snapshot = RECORDER.get().unwrap().lock().unwrap().snapshot();
                let failure = snapshot.first_failure.unwrap();
                assert_eq!(failure.stage, 5);
                assert_eq!(failure.frame, 123);
                assert_eq!(
                    failure.data[9],
                    glow::FRAMEBUFFER_INCOMPLETE_MISSING_ATTACHMENT as i64
                );
                assert!(!snapshot.tail.iter().any(|event| event.detail[7] == 1));
                gl.delete_framebuffer(fbo);
                gl.bind_framebuffer(glow::FRAMEBUFFER, None);
                let address = |name| api.get_proc_address(name).unwrap() as usize;
                let dispatch = Dispatch {
                    version: 2,
                    size: std::mem::size_of::<Dispatch>() as u32,
                    context: binding().0,
                    create: address("glFenceSync"),
                    valid: address("glIsSync"),
                    status: address("glGetSynciv"),
                    error: address("glGetError"),
                };
                let fence = gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).unwrap();
                callback(16, 1, 456, 0, 2, 2, fence.0.cast(), &dispatch);
                gl.flush();
                callback(2, 1, 456, 0, 2, 2, fence.0.cast(), &dispatch);
                let observed = RECORDER.get().unwrap().lock().unwrap().snapshot();
                let post = observed
                    .tail
                    .iter()
                    .find(|e| e.stage == 2 && e.frame == 456)
                    .unwrap();
                assert_eq!(post.detail, [1, 1, 1, 1, 1, 1, 1, 1]);
                assert!(matches!(
                    post.data[23] as u32,
                    glow::SIGNALED | glow::UNSIGNALED
                ));
                assert_eq!(gl.get_error(), glow::NO_ERROR);
                gl.delete_sync(fence);
                callback(16, 1, 457, 0, 2, 2, fence.0.cast(), &dispatch);
                let observed = RECORDER.get().unwrap().lock().unwrap().snapshot();
                let invalid = observed
                    .tail
                    .iter()
                    .find(|e| e.stage == 16 && e.frame == 457)
                    .unwrap();
                assert_eq!(invalid.data[23], -1);
                assert_eq!(invalid.detail[4], 0);
                assert_eq!(gl.get_error(), glow::NO_ERROR);
                api.make_current(display, None, None, None).unwrap();
                api.destroy_context(display, context).unwrap();
                api.destroy_surface(display, surface).unwrap();
                api.terminate(display).unwrap();
            }
        }
        #[test]
        fn fatal_injection_forwarded_once_ordinary_and_late_errors_retained() {
            FATAL.store(0, Ordering::Release);
            retain_fatal(glow::INVALID_ENUM);
            assert_eq!(take_fatal(), None);
            retain_fatal(glow::OUT_OF_MEMORY);
            retain_fatal(glow::CONTEXT_LOST);
            assert_eq!(take_fatal(), Some(glow::OUT_OF_MEMORY));
            assert_eq!(take_fatal(), None);
            retiring();
            retain_fatal(glow::CONTEXT_LOST);
            assert_eq!(
                take_fatal(),
                None,
                "retirement observations cannot re-deliver a consumed fault"
            );
            session_ready();
            retain_fatal(glow::CONTEXT_LOST);
            assert_eq!(take_fatal(), Some(glow::CONTEXT_LOST));
            assert_eq!(take_fatal(), None);
            retain_fatal(glow::OUT_OF_MEMORY);
            retiring();
            session_ready();
            assert_eq!(
                take_fatal(),
                Some(glow::OUT_OF_MEMORY),
                "native cleanup must not erase an unforwarded startup fault"
            );
            let mut recorder = Recorder::default();
            recorder.push(
                Event {
                    stage: 14,
                    ..Event::default()
                },
                false,
            );
            recorder.push(
                Event {
                    stage: 41,
                    ..Event::default()
                },
                true,
            );
            assert_eq!(recorder.snapshot().first_failure.unwrap().stage, 41);
        }
    }
}
#[cfg(feature = "graphics-observer")]
pub use enabled::*;

/// Diagnostic builds refuse unprepared or aliased mutable roots. The host
/// prepares an independent library/profile and read-only authorized inputs.
pub fn storage_root(normal: &Path) -> std::io::Result<PathBuf> {
    #[cfg(feature = "graphics-observer")]
    {
        let root = normal.join(SCRATCH);
        if std::fs::symlink_metadata(&root)?.file_type().is_symlink()
            || std::fs::read(root.join("prepared-v1"))?
                != format!("PSX-OBSERVER-SCRATCH-2:{}\n", mode()).as_bytes()
        {
            return Err(std::io::Error::other("Diagnostic scratch is not prepared"));
        }
        let mut directories = vec![root.clone()];
        let mut visited = 0;
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory)? {
                let path = entry?.path();
                let metadata = std::fs::symlink_metadata(&path)?;
                visited += 1;
                if visited > 10000 {
                    return Err(std::io::Error::other(
                        "Diagnostic namespace exceeded its bound",
                    ));
                }
                if metadata.file_type().is_symlink() {
                    if !path.canonicalize()?.starts_with(root.canonicalize()?) {
                        return Err(std::io::Error::other(
                            "Diagnostic mutable state cannot alias normal progress",
                        ));
                    }
                } else if metadata.is_dir() {
                    directories.push(path)
                }
                #[cfg(unix)]
                if metadata.is_file() {
                    use std::os::unix::fs::MetadataExt;
                    if metadata.nlink() != 1 {
                        return Err(std::io::Error::other(
                            "Diagnostic state must not be hard linked",
                        ));
                    }
                }
            }
        }
        Ok(root)
    }
    #[cfg(not(feature = "graphics-observer"))]
    {
        Ok(normal.to_path_buf())
    }
}
#[cfg(all(test, not(feature = "graphics-observer")))]
mod disabled {
    #[test]
    fn ordinary_execution_needs_no_scratch_and_keeps_normal_root() {
        let root = std::path::Path::new("/absent-normal-root");
        assert_eq!(super::storage_root(root).unwrap(), root);
    }
}
#[cfg(all(test, feature = "graphics-observer", unix))]
mod isolation {
    use super::*;
    #[test]
    fn prepared_scratch_never_aliases_normal_cards_checkpoints_or_profiles() {
        let normal =
            std::env::temp_dir().join(format!("psx-observer-isolation-{}", std::process::id()));
        std::fs::create_dir(&normal).unwrap();
        let root = normal.join(SCRATCH);
        std::fs::create_dir(&root).unwrap();
        assert!(storage_root(&normal).is_err());
        std::fs::write(
            root.join("prepared-v1"),
            format!("PSX-OBSERVER-SCRATCH-2:{}\n", mode()).as_bytes(),
        )
        .unwrap();
        std::fs::write(normal.join("normal-card"), b"original progress").unwrap();
        assert_eq!(storage_root(&normal).unwrap(), root);
        std::fs::create_dir_all(root.join("ps2/game/pcsx2")).unwrap();
        let alias = root.join("ps2/game/pcsx2/memcards");
        std::os::unix::fs::symlink(&normal, &alias).unwrap();
        assert!(storage_root(&normal).is_err());
        std::fs::remove_file(alias).unwrap();
        let hard = root.join("controller.json");
        std::fs::hard_link(normal.join("normal-card"), &hard).unwrap();
        assert!(storage_root(&normal).is_err());
        std::fs::remove_file(hard).unwrap();
        assert_eq!(
            std::fs::read(normal.join("normal-card")).unwrap(),
            b"original progress"
        );
        std::fs::remove_dir_all(normal).unwrap();
    }
}
