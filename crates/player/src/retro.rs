//! Narrow libretro ABI, transcribed from the pinned core's
//! dep/libretro-common/include/libretro.h (upstream notices retained there).
//! Only the owning thread may call a core or its callbacks. No Android handles.
use crate::{
    audio::AudioQueue,
    saves::{MemoryCard, CARD_SIZE},
};
use libloading::Library;
use std::{
    cell::Cell,
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    marker::PhantomData,
    path::Path,
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

pub type Result<T> = std::result::Result<T, String>;
pub type Environment = unsafe extern "C" fn(u32, *mut c_void) -> bool;
pub type ProcAddress = unsafe extern "C" fn(*const c_char) -> Option<unsafe extern "C" fn()>;

static CORE_ACTIVE: AtomicBool = AtomicBool::new(false);
struct CoreLease;
impl Drop for CoreLease {
    fn drop(&mut self) {
        CORE_ACTIVE.store(false, Ordering::Release);
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Geometry {
    pub width: u32,
    pub height: u32,
    pub max_width: u32,
    pub max_height: u32,
    pub aspect: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Timing {
    pub fps: f64,
    pub sample_rate: f64,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct AvInfo {
    pub geometry: Geometry,
    pub timing: Timing,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct HwRender {
    pub context_type: u32,
    pub reset: Option<unsafe extern "C" fn()>,
    pub framebuffer: Option<unsafe extern "C" fn() -> usize>,
    pub get_proc_address: Option<ProcAddress>,
    pub depth: bool,
    pub stencil: bool,
    pub bottom_left: bool,
    pub major: u32,
    pub minor: u32,
    pub cache_context: bool,
    pub destroy: Option<unsafe extern "C" fn()>,
    pub debug_context: bool,
}
#[repr(C)]
struct Variable {
    key: *const c_char,
    value: *const c_char,
}
#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct DiskControl {
    eject: Option<unsafe extern "C" fn(bool) -> bool>,
    ejected: Option<unsafe extern "C" fn() -> bool>,
    index: Option<unsafe extern "C" fn() -> u32>,
    set_index: Option<unsafe extern "C" fn(u32) -> bool>,
    count: Option<unsafe extern "C" fn() -> u32>,
    replace: Option<unsafe extern "C" fn(u32, *const GameInfo) -> bool>,
    add: Option<unsafe extern "C" fn() -> bool>,
}
#[repr(C)]
#[derive(Clone, Copy)]
struct DiskControlExt {
    base: DiskControl,
    initial: Option<unsafe extern "C" fn(u32, *const c_char) -> bool>,
    path: Option<unsafe extern "C" fn(u32, *mut c_char, usize) -> bool>,
    label: Option<unsafe extern "C" fn(u32, *mut c_char, usize) -> bool>,
}
#[repr(C)]
struct Message {
    text: *const c_char,
    frames: u32,
}

pub struct Callbacks {
    disk: DiskControl,
    initial_disk: Option<unsafe extern "C" fn(u32, *const c_char) -> bool>,
    system: CString,
    save: CString,
    options: BTreeMap<Vec<u8>, CString>,
    pub hw: Option<HwRender>,
    pub proc_address: Option<ProcAddress>,
    pub framebuffer: usize,
    pub av: AvInfo,
    pub buttons: u16,
    pub audio: Arc<AudioQueue>,
    pub fast_forward: bool,
    pub hardware_frames: u64,
    pub software_frames: u64,
    pub frame_width: u32,
    pub frame_height: u32,
    pub last_message: Option<String>,
    pub shutdown: bool,
    pub jit: bool,
}

thread_local! { static ACTIVE:Cell<*mut Callbacks> = const {Cell::new(std::ptr::null_mut())}; }

unsafe fn with_callbacks<T>(fallback: T, f: impl FnOnce(&mut Callbacks) -> T) -> T {
    ACTIVE.with(|active| {
        let pointer = active.get();
        if pointer.is_null() {
            fallback
        } else {
            f(&mut *pointer)
        }
    })
}

unsafe extern "C" fn current_framebuffer() -> usize {
    with_callbacks(0, |s| s.framebuffer)
}
unsafe extern "C" fn video(data: *const c_void, w: u32, h: u32, _pitch: usize) {
    with_callbacks((), |s| {
        if data as usize == usize::MAX {
            s.hardware_frames += 1;
            s.frame_width = w;
            s.frame_height = h;
        } else if !data.is_null() {
            s.software_frames += 1;
        }
        // NULL is a duplicate-frame request: preserve the previous output.
    });
}
unsafe extern "C" fn audio_batch(samples: *const i16, frames: usize) -> usize {
    if !samples.is_null() && frames <= 65536 {
        with_callbacks((), |s| {
            s.audio
                .push(std::slice::from_raw_parts(samples, frames * 2));
        });
    }
    frames
}
unsafe extern "C" fn audio_sample(left: i16, right: i16) {
    with_callbacks((), |s| {
        s.audio.push(&[left, right]);
    });
}
unsafe extern "C" fn input_poll() {}
unsafe extern "C" fn input_state(port: u32, device: u32, _index: u32, id: u32) -> i16 {
    with_callbacks(0, |s| {
        if port != 0 || device != 1 {
            return 0;
        }
        if id == 256 {
            return s.buttons as i16;
        }
        if id < 16 && s.buttons & (1 << id) != 0 {
            1
        } else {
            0
        }
    })
}

unsafe extern "C" fn environment(command: u32, data: *mut c_void) -> bool {
    with_callbacks(false, |s| {
        // Handle only the actual contract. Unsupported optional interfaces
        // return false, preserving the core's own fallback behavior.
        if command == 7 {
            s.shutdown = true;
            return true;
        }
        if command == 0x10033 {
            return true;
        }
        if data.is_null() {
            return false;
        }
        match command {
            13 => {
                s.disk = *(data as *const DiskControl);
                true
            }
            57 => {
                *(data as *mut u32) = 1;
                true
            }
            58 => {
                let disk = *(data as *const DiskControlExt);
                s.disk = disk.base;
                s.initial_disk = disk.initial;
                true
            }
            3 => {
                *(data as *mut bool) = true;
                true
            } // can duplicate frames
            7 => {
                s.shutdown = true;
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
            10 => *(data as *const u32) == 1, // XRGB8888 (boot may use software)
            14 => {
                let hw = &mut *(data as *mut HwRender);
                // Host creates GLES 3.0. Reject higher requests explicitly so
                // SwanStation negotiates down using its existing version loop.
                if hw.context_type != 4 && !(hw.context_type == 5 && hw.major == 3 && hw.minor == 0)
                {
                    return false;
                }
                if s.proc_address.is_none() {
                    return false;
                }
                hw.framebuffer = Some(current_framebuffer);
                hw.get_proc_address = s.proc_address;
                s.hw = Some(*hw);
                true
            }
            15 => {
                let var = &mut *(data as *mut Variable);
                if var.key.is_null() {
                    return false;
                }
                let key = CStr::from_ptr(var.key).to_bytes();
                var.value = s.options.get(key).map_or(std::ptr::null(), |v| v.as_ptr());
                !var.value.is_null()
            }
            16 => {
                let mut var = data as *const Variable;
                for _ in 0..1024 {
                    if (*var).key.is_null() {
                        break;
                    }
                    if !(*var).value.is_null() {
                        let key = CStr::from_ptr((*var).key).to_bytes().to_vec();
                        let value = CStr::from_ptr((*var).value).to_string_lossy();
                        if let Some((_, values)) = value.split_once(';') {
                            let default = values.trim_start().split('|').next().unwrap_or("");
                            if let Ok(default) = CString::new(default) {
                                s.options.entry(key).or_insert(default);
                            }
                        }
                    }
                    var = var.add(1);
                }
                true
            }
            17 => {
                *(data as *mut bool) = false;
                true
            }
            32 => {
                s.av = *(data as *const AvInfo);
                true
            }
            37 => {
                s.av.geometry = *(data as *const Geometry);
                true
            }
            39 | 52 | 59 => {
                *(data as *mut u32) = 0;
                true
            } // English, legacy options/messages
            56 => {
                *(data as *mut u32) = 5;
                true
            } // prefer GLES
            61 => {
                *(data as *mut u32) = 1;
                true
            }
            74 => {
                *(data as *mut bool) = s.jit;
                true
            }
            0x1002f => {
                if !data.is_null() {
                    *(data as *mut i32) = 3;
                }
                true
            } // video + audio, including pitched fast-forward output
            0x10031 => {
                *(data as *mut bool) = s.fast_forward;
                true
            } // fast forwarding
            0x10033 => true, // input bitmasks: query has no output
            6 => {
                let message = &*(data as *const Message);
                if !message.text.is_null() {
                    s.last_message = Some(
                        CStr::from_ptr(message.text)
                            .to_string_lossy()
                            .chars()
                            .take(300)
                            .collect(),
                    );
                }
                true
            }
            11 | 18 | 35 | 55 | 69 => true,
            _ => false,
        }
    })
}

pub struct Core {
    state: Box<Callbacks>,
    run_fn: unsafe extern "C" fn(),
    unload: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
    memory: unsafe extern "C" fn(u32) -> *mut c_void,
    memory_size: unsafe extern "C" fn(u32) -> usize,
    library: Library,
    loaded: bool,
    context_active: bool,
    // Prevent transfer to another thread and destruction without its GL context.
    _owner: PhantomData<Rc<()>>,
    _lease: CoreLease,
}

pub struct HostConfig<'a> {
    pub system: &'a Path,
    pub save: &'a Path,
    pub audio: Arc<AudioQueue>,
    pub framebuffer: usize,
    pub proc_address: Option<ProcAddress>,
    pub jit: bool,
    pub initial_disc: usize,
}

impl Core {
    /// Load the signed, bundled pinned core on its sole execution thread.
    ///
    /// # Safety
    /// The library must implement the pinned libretro ABI. The caller must keep
    /// its GLES context current on this thread for every call and destruction,
    /// and provide a valid framebuffer and procedure lookup for that context.
    pub unsafe fn load(library_path: &Path, game: &Path, config: HostConfig<'_>) -> Result<Self> {
        let HostConfig {
            system,
            save,
            audio,
            framebuffer,
            proc_address,
            jit,
            initial_disc,
        } = config;
        if ACTIVE.with(|a| !a.get().is_null()) {
            return Err("A game is already running.".into());
        }
        if CORE_ACTIVE
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err("A game session already owns the core.".into());
        }
        let lease = CoreLease;
        let library = Library::new(library_path)
            .map_err(|e| format!("The bundled PS1 core could not be loaded: {e}"))?;
        let mut options = BTreeMap::new();
        for (key, value) in [
            ("GPU_Renderer", "OpenGL"),
            ("GPU_ResolutionScale", "1"),
            ("CPU_FastmemMode", "LUT"),
            (
                "CPU_ExecutionMode",
                if jit {
                    "Recompiler"
                } else {
                    "CachedInterpreter"
                },
            ),
            ("MemoryCards_Card1Type", "Libretro"),
            ("MemoryCards_Card2Type", "None"),
            ("Display_ShowOSDMessages", "false"),
            ("BIOS_PatchFastBoot", "true"),
            ("Display_AspectRatio", "4:3"),
            ("GPU_PGXPEnable", "false"),
        ] {
            options.insert(
                format!("swanstation_{key}").into_bytes(),
                CString::new(value).unwrap(),
            );
        }
        let state = Box::new(Callbacks {
            disk: DiskControl::default(),
            initial_disk: None,
            system: CString::new(system.as_os_str().as_encoded_bytes())
                .map_err(|e| e.to_string())?,
            save: CString::new(save.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?,
            options,
            hw: None,
            proc_address,
            framebuffer,
            av: AvInfo::default(),
            buttons: 0,
            audio,
            fast_forward: false,
            hardware_frames: 0,
            software_frames: 0,
            frame_width: 0,
            frame_height: 0,
            last_message: None,
            shutdown: false,
            jit,
        });
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {
                *library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|e| e.to_string())?
            };
        }
        let set_env = symbol!("retro_set_environment", unsafe extern "C" fn(Environment));
        let set_video = symbol!(
            "retro_set_video_refresh",
            unsafe extern "C" fn(unsafe extern "C" fn(*const c_void, u32, u32, usize))
        );
        let set_audio = symbol!(
            "retro_set_audio_sample",
            unsafe extern "C" fn(unsafe extern "C" fn(i16, i16))
        );
        let set_batch = symbol!(
            "retro_set_audio_sample_batch",
            unsafe extern "C" fn(unsafe extern "C" fn(*const i16, usize) -> usize)
        );
        let set_poll = symbol!(
            "retro_set_input_poll",
            unsafe extern "C" fn(unsafe extern "C" fn())
        );
        let set_input = symbol!(
            "retro_set_input_state",
            unsafe extern "C" fn(unsafe extern "C" fn(u32, u32, u32, u32) -> i16)
        );
        let init = symbol!("retro_init", unsafe extern "C" fn());
        let api = symbol!("retro_api_version", unsafe extern "C" fn() -> u32);
        if api() != 1 {
            return Err("The bundled core has an unsupported interface version.".into());
        }
        let load_game = symbol!(
            "retro_load_game",
            unsafe extern "C" fn(*const GameInfo) -> bool
        );
        let set_controller = symbol!(
            "retro_set_controller_port_device",
            unsafe extern "C" fn(u32, u32)
        );
        let get_av = symbol!(
            "retro_get_system_av_info",
            unsafe extern "C" fn(*mut AvInfo)
        );
        let mut core = Self {
            run_fn: symbol!("retro_run", unsafe extern "C" fn()),
            unload: symbol!("retro_unload_game", unsafe extern "C" fn()),
            deinit: symbol!("retro_deinit", unsafe extern "C" fn()),
            memory: symbol!(
                "retro_get_memory_data",
                unsafe extern "C" fn(u32) -> *mut c_void
            ),
            memory_size: symbol!("retro_get_memory_size", unsafe extern "C" fn(u32) -> usize),
            state,
            library,
            loaded: false,
            context_active: false,
            _owner: PhantomData,
            _lease: lease,
        };
        ACTIVE.with(|a| a.set(&mut *core.state));
        set_env(environment);
        set_video(video);
        set_audio(audio_sample);
        set_batch(audio_batch);
        set_poll(input_poll);
        set_input(input_state);
        init();
        let game_path =
            CString::new(game.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        if let Some(initial) = core.state.initial_disk {
            if !initial(initial_disc as u32, game_path.as_ptr()) {
                return Err("The core could not select the starting disc.".into());
            }
        } else if initial_disc != 0 {
            return Err("The core cannot select a starting disc.".into());
        }
        let info = GameInfo {
            path: game_path.as_ptr(),
            data: std::ptr::null(),
            size: 0,
            meta: std::ptr::null(),
        };
        if !load_game(&info) {
            return Err(core.state.last_message.take().unwrap_or_else(||"The game could not start. Check the disc tracks and select compatible PS1 firmware.".into()));
        }
        core.loaded = true;
        set_controller(0, 1); // digital PS1 pad for the first playable
        get_av(&mut core.state.av);
        if let Some((index, count)) = core.disc_status() {
            if count == 0 || index as usize != initial_disc {
                return Err("The core did not load the requested starting disc.".into());
            }
        }
        if let Some(reset) = core.state.hw.and_then(|h| h.reset) {
            reset();
            core.context_active = true;
        }
        if core.state.hw.is_none() {
            return Err("The device could not provide the required OpenGL ES renderer.".into());
        }
        Ok(core)
    }

    pub fn state(&self) -> &Callbacks {
        &self.state
    }
    pub fn state_mut(&mut self) -> &mut Callbacks {
        &mut self.state
    }
    pub fn run(&mut self) {
        unsafe {
            (self.run_fn)();
        }
    }
    pub fn disc_status(&self) -> Option<(u32, u32)> {
        unsafe { Some(((self.state.disk.index?)(), (self.state.disk.count?)())) }
    }

    /// Call while paused on the sole core/GL thread, after flushing the card.
    /// Follow the donor ABI: open tray, select image, close tray. Try restoring
    /// the previous image if selection/insertion fails, never reset the machine.
    pub fn change_disc(&mut self, index: u32) -> Result<()> {
        let disk = self.state.disk;
        let (Some(eject), Some(current), Some(select), Some(count)) =
            (disk.eject, disk.index, disk.set_index, disk.count)
        else {
            return Err("This game does not expose disc switching.".into());
        };
        unsafe {
            if index >= count() {
                return Err(
                    "That disc is not in this game set. Add it to the library first.".into(),
                );
            }
            let old = current();
            if old == index && !disk.ejected.is_some_and(|get| get()) {
                return Ok(());
            }
            if !eject(true) {
                return Err("The game could not open its disc tray.".into());
            }
            if select(index) && eject(false) {
                return Ok(());
            }
            let restored = select(old) && eject(false);
            Err(if restored {
                "The disc could not be inserted. The previous disc is restored."
            } else {
                "The disc could not be inserted and the tray may be open. Choose a disc again."
            }
            .into())
        }
    }
    pub fn card_ram(&mut self) -> Result<&mut [u8]> {
        unsafe {
            let size = (self.memory_size)(0);
            let ptr = (self.memory)(0) as *mut u8;
            if size != CARD_SIZE || ptr.is_null() {
                return Err("The core did not expose memory-card storage.".into());
            }
            Ok(std::slice::from_raw_parts_mut(ptr, size))
        }
    }
    pub fn restore_card(&mut self, card: &MemoryCard) -> Result<()> {
        card.restore(self.card_ram()?).map_err(|e| e.to_string())
    }
    pub fn flush_card(&mut self, card: &mut MemoryCard) -> Result<bool> {
        card.flush(self.card_ram()?).map_err(|e| e.to_string())
    }

    pub fn destroy_context(&mut self) {
        if self.context_active {
            unsafe {
                if let Some(destroy) = self.state.hw.and_then(|h| h.destroy) {
                    destroy();
                }
            }
            self.context_active = false;
        }
    }
}

impl Drop for Core {
    fn drop(&mut self) {
        // Core callbacks and library remain live throughout teardown.
        self.destroy_context();
        unsafe {
            if self.loaded {
                (self.unload)();
            }
            (self.deinit)();
        }
        ACTIVE.with(|a| a.set(std::ptr::null_mut()));
        let _ = &self.library;
    }
}
