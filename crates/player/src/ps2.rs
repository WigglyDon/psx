//! ARMSX2 adapter. The donor retains its CPU/GS owners and EGL share group.
//! Callback state is shared, never a TLS mutable reference on native workers.
use crate::{
    audio::AudioQueue,
    retro::{AvInfo, CoreLease, Environment, Geometry, HostConfig, HwRender},
    saves::atomic_write,
    session::GameId,
    settings::Speed,
};
use libloading::Library;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    ffi::{c_char, c_void, CStr, CString},
    fs,
    marker::PhantomData,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, AtomicI16, AtomicU16, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, String>;
pub const REVISION: &str = "83fe4e293f71ab5587ef81b06143725bf8c6a30b";
pub fn revision() -> &'static str {
    use sha2::{Digest, Sha256};
    static BINDING: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    BINDING.get_or_init(|| {
        format!(
            "{REVISION}+{:x}",
            Sha256::digest(include_bytes!(
                "../../../patches/armsx2/0001-host-pause-and-card-flush.patch"
            ))
        )
    })
}
pub const MAX_STATE_SIZE: usize = 68 * 1024 * 1024;
#[derive(Clone, Default)]
pub struct Output {
    pub metrics: Option<[f32; 5]>,
    pub av: AvInfo,
    pub hw: Option<HwRender>,
    pub hardware_frames: u64,
    pub software_frames: u64,
    pub frame_width: u32,
    pub frame_height: u32,
    pub shutdown: bool,
    pub last_message: Option<String>,
}
struct Shared {
    system: CString,
    save: CString,
    options: BTreeMap<Vec<u8>, CString>,
    framebuffer: usize,
    proc_address: Option<crate::retro::ProcAddress>,
    gles: [u32; 2],
    audio: Arc<AudioQueue>,
    buttons: AtomicU16,
    axes: [AtomicI16; 4],
    fast: AtomicBool,
    output: Mutex<Output>,
}
static ACTIVE: Mutex<Option<Arc<Shared>>> = Mutex::new(None);
fn shared() -> Option<Arc<Shared>> {
    ACTIVE.lock().ok()?.clone()
}
unsafe extern "C" fn framebuffer() -> usize {
    shared().map_or(0, |s| s.framebuffer)
}
unsafe extern "C" fn video(data: *const c_void, w: u32, h: u32, _pitch: usize) {
    if let Some(s) = shared() {
        if let Ok(mut o) = s.output.lock() {
            if data as usize == usize::MAX {
                o.hardware_frames += 1;
                o.frame_width = w;
                o.frame_height = h;
            } else if !data.is_null() {
                o.software_frames += 1;
            }
        }
    }
}
unsafe extern "C" fn batch(data: *const i16, frames: usize) -> usize {
    if !data.is_null() && frames <= 65536 {
        if let Some(s) = shared() {
            s.audio.push(std::slice::from_raw_parts(data, frames * 2));
        }
    }
    frames
}
unsafe extern "C" fn sample(l: i16, r: i16) {
    if let Some(s) = shared() {
        s.audio.push(&[l, r]);
    }
}
unsafe extern "C" fn poll() {}
unsafe extern "C" fn input(port: u32, device: u32, index: u32, id: u32) -> i16 {
    let Some(s) = shared() else { return 0 };
    if port != 0 {
        return 0;
    }
    match device {
        1 => {
            let bits = s.buttons.load(Ordering::Relaxed);
            if id == 256 {
                bits as i16
            } else {
                i16::from(id < 16 && bits & (1 << id) != 0)
            }
        }
        5 if index < 2 && id < 2 => s.axes[(index * 2 + id) as usize].load(Ordering::Relaxed),
        _ => 0,
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
    if command == 7 {
        if let Ok(mut o) = s.output.lock() {
            o.shutdown = true;
        }
        return true;
    }
    if command == 0x10033 || command == 0x1002c {
        return true;
    } // bitmasks / EGL share group
    if data.is_null() {
        return false;
    }
    match command {
        0x20000 => {
            let metrics = *(data as *const [f32; 5]);
            if metrics.iter().all(|v| v.is_finite()) {
                if let Ok(mut o) = s.output.lock() {
                    o.metrics = Some(metrics);
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
            if requested > s.gles || s.proc_address.is_none() {
                return false;
            }
            hw.framebuffer = Some(framebuffer);
            hw.get_proc_address = s.proc_address;
            if let Ok(mut o) = s.output.lock() {
                o.hw = Some(*hw);
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
            var.value = s
                .options
                .get(CStr::from_ptr(var.key).to_bytes())
                .map_or(std::ptr::null(), |v| v.as_ptr());
            !var.value.is_null()
        }
        16 | 35 => true, // legacy options/input descriptors; Rust supplies explicit overrides
        17 => {
            *(data as *mut bool) = false;
            true
        }
        32 => {
            if let Ok(mut o) = s.output.lock() {
                o.av = *(data as *const AvInfo);
                true
            } else {
                false
            }
        }
        37 => {
            if let Ok(mut o) = s.output.lock() {
                o.av.geometry = *(data as *const Geometry);
                true
            } else {
                false
            }
        }
        39 => {
            *(data as *mut u32) = 0;
            true
        } // English
        56 => {
            *(data as *mut u32) = 4;
            true
        } // GLES3 preference
        49 => {
            *(data as *mut bool) = s.fast.load(Ordering::Relaxed);
            true
        }
        52 => {
            *(data as *mut bool) = false;
            true
        } // not an achievements host
        6 => {
            let m = &*(data as *const Message);
            if !m.text.is_null() {
                if let Ok(mut o) = s.output.lock() {
                    o.last_message = Some(
                        CStr::from_ptr(m.text)
                            .to_string_lossy()
                            .chars()
                            .take(300)
                            .collect(),
                    );
                }
            }
            true
        }
        _ => false,
    }
}
#[repr(C)]
struct GameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}
pub struct Core {
    shared: Arc<Shared>,
    run_fn: unsafe extern "C" fn(),
    unload: unsafe extern "C" fn(),
    deinit: unsafe extern "C" fn(),
    pause_fn: unsafe extern "C" fn(bool) -> bool,
    flush_fn: unsafe extern "C" fn() -> bool,
    ready_fn: unsafe extern "C" fn() -> bool,
    speed_fn: unsafe extern "C" fn(f32) -> bool,
    speed: Option<Speed>,
    speed_changed: Instant,
    serialize_size: unsafe extern "C" fn() -> usize,
    serialize: unsafe extern "C" fn(*mut c_void, usize) -> bool,
    unserialize: unsafe extern "C" fn(*const c_void, usize) -> bool,
    library: Library,
    loaded: bool,
    context: bool,
    paused: bool,
    _owner: PhantomData<Rc<()>>,
    _lease: CoreLease,
}
impl Core {
    /// # Safety
    /// Keep the negotiated GLES context current for each call and teardown.
    /// The library must be the bundled revision with the recorded adapter patch.
    pub unsafe fn load(
        path: &Path,
        game: &Path,
        config: HostConfig<'_>,
        gles: [u32; 2],
    ) -> Result<Self> {
        if gles < [3, 1] {
            return Err("PS2 needs an OpenGL ES 3.1 or newer device renderer.".into());
        }
        let lease = CoreLease::acquire()?;
        let library = Library::new(path)
            .map_err(|e| format!("The bundled PS2 engine could not load: {e}"))?;
        let mut options = BTreeMap::new();
        let firmware = firmware_binding(config.system)?;
        options.insert(
            b"armsx2_bios".to_vec(),
            CString::new(firmware.file).map_err(|e| e.to_string())?,
        );
        for (k, v) in [
            ("armsx2_renderer", "OpenGL"),
            ("armsx2_upscale", "1x"),
            ("armsx2_ee_cycle_rate", "100%"),
            ("armsx2_ee_cycle_skip", "disabled"),
            ("armsx2_fast_boot", "enabled"),
            ("armsx2_widescreen_patches", "disabled"),
            ("armsx2_no_interlacing_patches", "disabled"),
            ("armsx2_hw_download_mode", "Accurate"),
        ] {
            options.insert(k.as_bytes().to_vec(), CString::new(v).unwrap());
        }
        let shared = Arc::new(Shared {
            system: CString::new(config.system.as_os_str().as_encoded_bytes())
                .map_err(|e| e.to_string())?,
            save: CString::new(config.save.as_os_str().as_encoded_bytes())
                .map_err(|e| e.to_string())?,
            options,
            framebuffer: config.framebuffer,
            proc_address: config.proc_address,
            gles,
            audio: config.audio,
            buttons: AtomicU16::new(0),
            axes: std::array::from_fn(|_| AtomicI16::new(0)),
            fast: AtomicBool::new(false),
            output: Mutex::new(Output::default()),
        });
        macro_rules! symbol {
            ($name:literal,$ty:ty) => {
                *library
                    .get::<$ty>(concat!($name, "\0").as_bytes())
                    .map_err(|e| e.to_string())?
            };
        }
        let init = symbol!("retro_init", unsafe extern "C" fn());
        let api = symbol!("retro_api_version", unsafe extern "C" fn() -> u32);
        if api() != 1 {
            return Err("The PS2 engine interface is unsupported.".into());
        }
        let set_env = symbol!("retro_set_environment", unsafe extern "C" fn(Environment));
        let set_video = symbol!(
            "retro_set_video_refresh",
            unsafe extern "C" fn(unsafe extern "C" fn(*const c_void, u32, u32, usize))
        );
        let set_sample = symbol!(
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
        let load = symbol!(
            "retro_load_game",
            unsafe extern "C" fn(*const GameInfo) -> bool
        );
        let av = symbol!(
            "retro_get_system_av_info",
            unsafe extern "C" fn(*mut AvInfo)
        );
        let mut core = Self {
            shared: shared.clone(),
            run_fn: symbol!("retro_run", unsafe extern "C" fn()),
            unload: symbol!("retro_unload_game", unsafe extern "C" fn()),
            deinit: symbol!("retro_deinit", unsafe extern "C" fn()),
            pause_fn: symbol!("retro_psx_set_paused", unsafe extern "C" fn(bool) -> bool),
            flush_fn: symbol!("retro_psx_flush_cards", unsafe extern "C" fn() -> bool),
            ready_fn: symbol!("retro_psx_ready", unsafe extern "C" fn() -> bool),
            speed_fn: symbol!("retro_psx_set_speed", unsafe extern "C" fn(f32) -> bool),
            speed: None, // Apply Rust's requested time scale before the first live tick.
            speed_changed: Instant::now(),
            serialize_size: symbol!("retro_serialize_size", unsafe extern "C" fn() -> usize),
            serialize: symbol!(
                "retro_serialize",
                unsafe extern "C" fn(*mut c_void, usize) -> bool
            ),
            unserialize: symbol!(
                "retro_unserialize",
                unsafe extern "C" fn(*const c_void, usize) -> bool
            ),
            library,
            loaded: false,
            context: false,
            paused: false,
            _owner: PhantomData,
            _lease: lease,
        };
        *ACTIVE
            .lock()
            .map_err(|_| "PS2 callback ownership is unavailable.")? = Some(shared);
        set_env(environment);
        set_video(video);
        set_sample(sample);
        set_batch(batch);
        set_poll(poll);
        set_input(input);
        init();
        let game = CString::new(game.as_os_str().as_encoded_bytes()).map_err(|e| e.to_string())?;
        let info = GameInfo {
            path: game.as_ptr(),
            data: std::ptr::null(),
            size: 0,
            meta: std::ptr::null(),
        };
        if !load(&info) {
            return Err(
                "The PS2 game could not start. Check its firmware, disc and renderer.".into(),
            );
        }
        core.loaded = true;
        let mut info = AvInfo::default();
        av(&mut info);
        core.shared
            .output
            .lock()
            .map_err(|_| "PS2 output unavailable.")?
            .av = info;
        let hw = core
            .output()
            .hw
            .ok_or("The PS2 engine did not negotiate a hardware renderer.")?;
        if let Some(reset) = hw.reset {
            reset();
            core.context = true;
        } else {
            return Err("The PS2 renderer did not supply context setup.".into());
        }
        Ok(core)
    }
    pub fn output(&self) -> Output {
        self.shared
            .output
            .lock()
            .map(|o| o.clone())
            .unwrap_or_else(|_| Output {
                shutdown: true,
                ..Output::default()
            })
    }
    pub fn input(&self, buttons: u16, axes: [i16; 4], fast: bool) {
        self.shared.buttons.store(buttons, Ordering::Relaxed);
        for (a, v) in self.shared.axes.iter().zip(axes) {
            a.store(v, Ordering::Relaxed);
        }
        self.shared.fast.store(fast, Ordering::Relaxed);
    }
    pub fn run(&mut self) {
        unsafe {
            (self.run_fn)();
        }
    }
    pub fn ready(&self) -> bool {
        unsafe { (self.ready_fn)() }
    }
    pub fn set_speed(&mut self, speed: Speed) -> Result<Speed> {
        if speed.is_paused() {
            return Err("Zero time must pause the runtime.".into());
        }
        if self.speed != Some(speed) {
            // Coalesce a drag to at most 20 native deadline changes per second.
            // Return the applied scale so audio/host pacing stay in agreement.
            if let Some(applied) = self.speed {
                if self.speed_changed.elapsed() < Duration::from_millis(50) {
                    return Ok(applied);
                }
            }
            if !unsafe { (self.speed_fn)(speed.multiplier().unwrap_or(0.) as f32) } {
                return Err("The PS2 playback speed could not change.".into());
            }
            self.speed = Some(speed);
            self.speed_changed = Instant::now();
        }
        Ok(speed)
    }
    pub fn pause(&mut self, paused: bool) -> Result<()> {
        if self.paused != paused {
            if !unsafe { (self.pause_fn)(paused) } {
                return Err("The PS2 runtime could not change pause state.".into());
            }
            self.paused = paused;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> Result<()> {
        self.pause(true)?;
        if !unsafe { (self.flush_fn)() } {
            return Err("The PS2 memory card could not be flushed. Progress is still owned by this session.".into());
        }
        Ok(())
    }
    pub fn checkpoint(&mut self) -> Result<Vec<u8>> {
        self.flush()?;
        let size = unsafe { (self.serialize_size)() };
        if size == 0 || size > MAX_STATE_SIZE {
            return Err("The PS2 session is not ready for a bounded checkpoint.".into());
        }
        let mut bytes = vec![0; size];
        if !unsafe { (self.serialize)(bytes.as_mut_ptr().cast(), size) } {
            return Err("The PS2 session checkpoint failed.".into());
        }
        // ARMSX2's ABI reserves a fixed maximum but prefixes the actual ZIP
        // length. Its reader accepts that exact extent; padding is not state.
        let used = checkpoint_extent(&bytes)?;
        bytes.truncate(used);
        Ok(bytes)
    }
    pub fn restore(&mut self, bytes: &[u8]) -> Result<()> {
        self.flush()?;
        if bytes.is_empty()
            || bytes.len() > MAX_STATE_SIZE
            || !unsafe { (self.unserialize)(bytes.as_ptr().cast(), bytes.len()) }
        {
            return Err("This PS2 checkpoint could not resume. The current card is kept.".into());
        }
        Ok(())
    }
}

fn checkpoint_extent(bytes: &[u8]) -> Result<usize> {
    let header: [u8; 8] = bytes
        .get(..8)
        .and_then(|b| b.try_into().ok())
        .ok_or("The PS2 checkpoint has no length header.")?;
    let length = usize::try_from(u64::from_le_bytes(header))
        .map_err(|_| "The PS2 checkpoint length is invalid.")?;
    if length == 0 || length > bytes.len() - 8 {
        return Err("The PS2 checkpoint length exceeds its buffer.".into());
    }
    Ok(8 + length)
}
impl Drop for Core {
    fn drop(&mut self) {
        unsafe {
            if self.loaded {
                let _ = self.pause(true);
            }
            if self.context {
                if let Some(destroy) = self.output().hw.and_then(|hw| hw.destroy) {
                    destroy();
                }
                self.context = false;
            }
            if self.loaded {
                (self.unload)();
                self.loaded = false;
            }
            (self.deinit)();
        }
        if let Ok(mut active) = ACTIVE.lock() {
            *active = None;
        }
        let _ = &self.library;
    }
}

/// Private per-entry roots keep donor-owned card names independent. Only pinned
/// resources and user-supplied firmware are shared; cards never are.
pub fn prepare(root: &Path, id: &GameId, renderer: &str) -> Result<PathBuf> {
    let system = root.join("ps2").join(id.as_str());
    let pcsx2 = system.join("pcsx2");
    fs::create_dir_all(pcsx2.join("memcards")).map_err(|e| e.to_string())?;
    for (name, target) in [
        ("bios", root.join("system/ps2/bios")),
        ("resources", root.join("system/ps2/resources")),
    ] {
        fs::create_dir_all(&target).map_err(|e| e.to_string())?;
        let link = pcsx2.join(name);
        #[cfg(unix)]
        if !link.exists() {
            std::os::unix::fs::symlink(&target, &link).map_err(|e| e.to_string())?;
        }
    }
    let has_progress = root
        .join("resume")
        .join(format!("{}.state", id.as_str()))
        .exists()
        || fs::read_dir(pcsx2.join("memcards"))
            .map_err(|e| e.to_string())?
            .next()
            .is_some();
    prepare_firmware(&system, has_progress)?;
    let ini = pcsx2.join("inis/armsx2-libretro.ini");
    // Source-bound settings version. Retain donor defaults for unspecified
    // keys, and explicitly disable adaptive guest clocks for qualification.
    let previous = fs::read_to_string(&ini).unwrap_or_default();
    let contents = qualification_settings(&previous, needs_fetch_copy(renderer));
    atomic_write(&ini, contents.as_bytes()).map_err(|e| e.to_string())?;
    Ok(system)
}

#[derive(Serialize, Deserialize)]
struct Firmware {
    file: String,
    sha256: String,
}
fn firmware_binding(system: &Path) -> Result<Firmware> {
    let binding: Firmware =
        serde_json::from_slice(&fs::read(system.join("firmware.json")).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if binding.file.is_empty()
        || binding.file == "auto"
        || Path::new(&binding.file)
            .file_name()
            .and_then(|n| n.to_str())
            != Some(&binding.file)
        || binding.file.contains('\0')
        || binding.sha256.len() != 64
        || !binding.sha256.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err("This game's PS2 firmware binding is invalid. Its saves are kept.".into());
    }
    Ok(binding)
}
/// This identity is stable while a runtime owns the entry. The next prepare
/// verifies the actual file again before any new native runtime can start.
pub fn firmware_identity(system: &Path) -> Result<String> {
    Ok(firmware_binding(system)?.sha256)
}
fn prepare_firmware(system: &Path, has_progress: bool) -> Result<()> {
    use sha2::{Digest, Sha256};
    let bios = system.join("pcsx2/bios");
    let manifest = system.join("firmware.json");
    let binding = if manifest.exists() {
        firmware_binding(system)?
    } else {
        let mut names = Vec::new();
        for entry in fs::read_dir(&bios).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let metadata = entry.metadata().map_err(|e| e.to_string())?;
            if metadata.is_file()
                && metadata.len() == 4 * 1024 * 1024
                && has_rom_version(&fs::read(entry.path()).map_err(|e| e.to_string())?)
            {
                if let Some(name) = entry.file_name().to_str().filter(|name| *name != "auto") {
                    names.push(name.to_owned());
                }
            }
        }
        names.sort();
        if has_progress && names.len() > 1 {
            return Err("This older PS2 session has no firmware binding and multiple firmware files are available. Keep its original firmware as the only PS2 firmware file before reopening. Its checkpoint and cards are kept.".into());
        }
        let file = names
            .into_iter()
            .next()
            .ok_or("PS2 needs compatible firmware. Add PS2 firmware in Settings.")?;
        let sha256 = format!(
            "{:x}",
            Sha256::digest(fs::read(bios.join(&file)).map_err(|e| e.to_string())?)
        );
        Firmware { file, sha256 }
    };
    let path = bios.join(&binding.file);
    let file = fs::File::open(&path).map_err(|_| "This game's PS2 firmware is missing. Restore the same firmware file; its saves are kept.".to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() != 4 * 1024 * 1024
        || format!(
            "{:x}",
            Sha256::digest(fs::read(path).map_err(|e| e.to_string())?)
        ) != binding.sha256
    {
        return Err("This game's PS2 firmware changed. Restore the original firmware to resume; its checkpoint and cards are kept.".into());
    }
    if !manifest.exists() {
        atomic_write(
            &manifest,
            &serde_json::to_vec(&binding).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}
// Match the donor's ROMDIR/ROMVER discovery, without restricting firmware to
// Sony images or asserting compatibility from a header. Native load still
// validates and executes the selected image.
fn has_rom_version(bytes: &[u8]) -> bool {
    let Some(start) = bytes
        .chunks_exact(16)
        .position(|r| &r[..10] == b"RESET\0\0\0\0\0")
    else {
        return false;
    };
    let mut offset = 0usize;
    for entry in bytes[start * 16..].chunks_exact(16).take(4096) {
        if entry[0] == 0 {
            return false;
        }
        let size = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        if &entry[..10] == b"ROMVER\0\0\0\0" {
            return bytes
                .get(offset..offset.saturating_add(14))
                .is_some_and(|v| v[..4].iter().all(u8::is_ascii_digit));
        }
        let Some(next) = offset
            .checked_add(size)
            .and_then(|n| n.checked_add(15))
            .map(|n| n & !15)
        else {
            return false;
        };
        if next > bytes.len() {
            return false;
        }
        offset = next;
    }
    false
}
fn needs_fetch_copy(renderer: &str) -> bool {
    // Observed ANGLE/PowerVR EXT framebuffer-fetch shader compilation failure.
    // Use the donor's existing copy path; leave other drivers' defaults alone.
    renderer.contains("ANGLE") && (renderer.contains("PowerVR") || renderer.contains("Imagination"))
}
fn qualification_settings(previous: &str, fetch_copy: bool) -> String {
    let mut overrides = vec![
        ("UI", "SettingsVersion", "1"),
        ("EmuCore", "AutoTune", "false"),
        ("EmuCore", "EnableFastBootFastForward", "false"),
        // Native serialization runs on the CPU owner. Avoid compressing the
        // archive while the VM is stopped; the bounded storage worker owns
        // hashing/writing. Both compressed and stored ZIPs use the same reader.
        ("EmuCore", "SavestateCompressionType", "0"),
        ("EmuCore/Speedhacks", "EECycleRate", "0"),
        ("EmuCore/Speedhacks", "EECycleSkip", "0"),
        ("EmuCore/Speedhacks", "DynamicEECycleRate", "false"),
    ];
    if fetch_copy {
        overrides.push(("EmuCore/GS", "DisableFramebufferFetch", "true"));
    }
    let mut sections = BTreeMap::<&str, Vec<&str>>::new();
    let mut section = "";
    for line in previous.lines() {
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
            continue;
        }
        if overrides.iter().any(|(s, k, _)| {
            *s == section
                && line
                    .split_once('=')
                    .is_some_and(|(key, _)| key.trim() == *k)
        }) {
            continue;
        }
        if !line.trim().is_empty() {
            sections.entry(section).or_default().push(line);
        }
    }
    let mut added = BTreeMap::<&str, Vec<String>>::new();
    for (s, k, v) in overrides {
        added.entry(s).or_default().push(format!("{k} = {v}"));
        sections.entry(s).or_default();
    }
    let mut out = String::new();
    for (s, lines) in sections {
        if !s.is_empty() {
            out.push_str(&format!("[{s}]\n"));
        }
        for line in lines {
            out.push_str(line);
            out.push('\n');
        }
        if let Some(lines) = added.get(s) {
            for line in lines {
                out.push_str(line);
                out.push('\n');
            }
        }
        out.push('\n');
    }
    out
}

pub fn install_resource(root: &Path, name: &str, digest: &str, bytes: &[u8]) -> Result<()> {
    use sha2::{Digest, Sha256};
    if Path::new(name)
        .components()
        .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || bytes.len() > 32 * 1024 * 1024
        || format!("{:x}", Sha256::digest(bytes)) != digest
    {
        return Err("A bundled PS2 resource does not match this build.".into());
    }
    let path = root.join("system/ps2/resources").join(name);
    if fs::read(&path).is_ok_and(|b| format!("{:x}", Sha256::digest(b)) == digest) {
        return Ok(());
    }
    atomic_write(&path, bytes).map_err(|e| e.to_string())
}

/// Call only after the native flush acknowledgement or complete donor teardown,
/// while holding the game's Writer. This reads and syncs, never replaces cards.
pub fn card_identity(root: &Path) -> std::io::Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            return Err(std::io::Error::other(
                "Unsupported non-file PS2 memory card storage.",
            ));
        }
        files.push(entry.path());
    }
    files.sort();
    if files.is_empty() || files.len() > 8 {
        return Err(std::io::Error::other(
            "The PS2 memory card files are unavailable.",
        ));
    }
    let mut hash = Sha256::new();
    for path in files {
        let mut file = fs::File::open(&path)?;
        let len = file.metadata()?.len();
        if len == 0 || len > 70 * 1024 * 1024 {
            return Err(std::io::Error::other(
                "The PS2 memory card size is unsupported.",
            ));
        }
        file.sync_all()?;
        let name = path.file_name().unwrap().as_encoded_bytes();
        hash.update((name.len() as u32).to_le_bytes());
        hash.update(name);
        hash.update(len.to_le_bytes());
        let mut buffer = [0u8; 65536];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hash.update(&buffer[..read]);
        }
    }
    fs::File::open(root)?.sync_all()?;
    Ok(hash.finalize().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn firmware_selection_is_pinned_and_changed_bytes_or_ambiguous_legacy_inputs_fail_safely() {
        fn image(marker: u8) -> Vec<u8> {
            let mut bytes = vec![marker; 4 * 1024 * 1024];
            bytes[..128].fill(0);
            for (i, name, size) in [(0, "RESET", 64u32), (1, "ROMDIR", 64), (2, "ROMVER", 14)] {
                bytes[64 + i * 16..64 + i * 16 + name.len()].copy_from_slice(name.as_bytes());
                bytes[64 + i * 16 + 12..64 + i * 16 + 16].copy_from_slice(&size.to_le_bytes());
            }
            bytes[128..142].copy_from_slice(b"0002AD20261017");
            bytes
        }
        let root = std::env::temp_dir().join(format!("psx-ps2-firmware-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let bios = root.join("pcsx2/bios");
        fs::create_dir_all(&bios).unwrap();
        assert!(!has_rom_version(&vec![0; 4 * 1024 * 1024]));
        fs::write(bios.join("000-invalid.bin"), vec![0; 4 * 1024 * 1024]).unwrap();
        fs::write(bios.join("owner.bin"), image(1)).unwrap();
        prepare_firmware(&root, false).unwrap();
        let identity = firmware_identity(&root).unwrap();
        fs::write(bios.join("earlier.bin"), image(2)).unwrap();
        prepare_firmware(&root, true).unwrap();
        assert_eq!(firmware_binding(&root).unwrap().file, "owner.bin");
        assert_eq!(firmware_identity(&root).unwrap(), identity);
        fs::write(bios.join("owner.bin"), image(3)).unwrap();
        assert!(prepare_firmware(&root, true).is_err());
        assert_eq!(firmware_identity(&root).unwrap(), identity);
        fs::remove_file(bios.join("owner.bin")).unwrap();
        assert!(prepare_firmware(&root, true).is_err());
        fs::remove_file(root.join("firmware.json")).unwrap();
        fs::write(bios.join("owner.bin"), image(1)).unwrap();
        assert!(prepare_firmware(&root, true).is_err());
        assert!(!root.join("firmware.json").exists());
        // A legacy entry with only an ordinary card is still an existing
        // session. Missing a checkpoint must not authorize a firmware change.
        let legacy = root.join("legacy");
        fs::create_dir_all(legacy.join("system/ps2")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&bios, legacy.join("system/ps2/bios")).unwrap();
        #[cfg(not(unix))]
        {
            fs::create_dir_all(legacy.join("system/ps2/bios")).unwrap();
            for name in ["owner.bin", "earlier.bin"] {
                fs::copy(bios.join(name), legacy.join("system/ps2/bios").join(name)).unwrap();
            }
        }
        let cards = legacy.join("ps2/abc123/pcsx2/memcards");
        fs::create_dir_all(&cards).unwrap();
        fs::write(cards.join("Mcd001.ps2"), b"owner progress").unwrap();
        assert!(prepare(&legacy, &GameId::parse("abc123").unwrap(), "fixture").is_err());
        assert!(!legacy.join("ps2/abc123/firmware.json").exists());
        assert_eq!(
            fs::read(cards.join("Mcd001.ps2")).unwrap(),
            b"owner progress"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn callbacks_keep_their_binding_on_native_worker_threads() {
        let binding = Arc::new(Shared {
            system: CString::new("fixture-system").unwrap(),
            save: CString::new("fixture-save").unwrap(),
            options: BTreeMap::new(),
            framebuffer: 17,
            proc_address: None,
            gles: [3, 2],
            audio: Arc::new(AudioQueue::new(128)),
            buttons: AtomicU16::new(1),
            axes: std::array::from_fn(|i| AtomicI16::new(if i == 0 { 12345 } else { 0 })),
            fast: AtomicBool::new(false),
            output: Mutex::new(Output::default()),
        });
        *ACTIVE.lock().unwrap() = Some(binding.clone());
        std::thread::spawn(|| unsafe {
            let mut directory: *const c_char = std::ptr::null();
            assert!(environment(
                9,
                (&mut directory as *mut *const c_char).cast()
            ));
            assert_eq!(CStr::from_ptr(directory).to_bytes(), b"fixture-system");
            assert_eq!(input(0, 1, 0, 0), 1);
            assert_eq!(input(0, 5, 0, 0), 12345);
            assert_eq!(input(1, 1, 0, 0), 0);
            video(usize::MAX as *const c_void, 640, 448, 0);
        })
        .join()
        .unwrap();
        assert_eq!(binding.output.lock().unwrap().hardware_frames, 1);
        assert_eq!(binding.output.lock().unwrap().frame_width, 640);
        *ACTIVE.lock().unwrap() = None;
    }
    #[test]
    fn checkpoints_retain_the_exact_native_archive_and_reject_bad_extents() {
        let mut bytes = 4u64.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"PK\x03\x04");
        bytes.resize(128, 0);
        assert_eq!(checkpoint_extent(&bytes).unwrap(), 12);
        bytes.truncate(12);
        assert_eq!(checkpoint_extent(&bytes).unwrap(), 12);
        assert!(checkpoint_extent(&bytes[..7]).is_err());
        assert!(checkpoint_extent(&[0; 8]).is_err());
        bytes[..8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(checkpoint_extent(&bytes).is_err());
        bytes[..8].copy_from_slice(&5u64.to_le_bytes());
        assert!(checkpoint_extent(&bytes).is_err());
    }
    #[test]
    fn settings_keep_other_values_and_disable_adaptive_clocks() {
        let s = qualification_settings("[EmuCore]\nAutoTune=true\nOther=keep\n", false);
        assert!(!s.contains("AutoTune=true"));
        assert!(s.contains("Other=keep"));
        assert!(s.contains("AutoTune = false"));
        assert!(s.contains("SavestateCompressionType = 0"));
        assert_eq!(qualification_settings(&s, false), s);
        let renderer = "ANGLE (Imagination Technologies, PowerVR C-Series Vulkan Driver)";
        assert!(needs_fetch_copy(renderer));
        assert!(!needs_fetch_copy("Mali-G78"));
        assert!(!needs_fetch_copy("ANGLE (Qualcomm Adreno)"));
        let compatible = qualification_settings(&s, needs_fetch_copy(renderer));
        assert!(compatible.contains("DisableFramebufferFetch = true"));
        assert!(compatible.contains("AutoTune = false"));
        assert!(compatible.contains("EECycleRate = 0"));
        assert_eq!(qualification_settings(&compatible, true), compatible);
    }
}
