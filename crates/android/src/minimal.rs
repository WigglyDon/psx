//! Independent development NativeActivity; no `android::Player` or Graphics UI.
use android_activity::{AndroidApp, MainEvent, PollEvent, WindowManagerFlags};
use glow::HasContext;
use jni::{
    objects::{JObject, JString, JValue},
    JavaVM,
};
use khronos_egl as egl;
use minimal_host::serde_json;
use psx_player::{
    graphics_observer as observe,
    minimal_host::{self, Config, Host, Lifecycle, LifecycleEvent},
};
use std::{
    ffi::{c_char, CStr},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    task::{Wake, Waker},
    time::{Duration, Instant},
};
const API: egl::Instance<egl::Static> = egl::Instance::new(egl::Static);
type Result<T> = std::result::Result<T, String>;
struct AppWake(android_activity::AndroidAppWaker);
#[derive(Default)]
struct WindowSetup {
    done: AtomicBool,
    error: Mutex<Option<String>>,
}
fn configure_window(app: &AndroidApp, waker: Waker) -> Arc<WindowSetup> {
    let state = Arc::new(WindowSetup::default());
    let done = state.clone();
    let java_app = app.clone();
    app.run_on_java_main_thread(Box::new(move || {
        let result = unsafe { window_on_java_thread(&java_app) };
        if let Err(error) = result {
            *done.error.lock().unwrap() = Some(error);
        }
        done.done.store(true, Ordering::Release);
        waker.wake_by_ref();
    }));
    state
}
unsafe fn window_on_java_thread(app: &AndroidApp) -> Result<()> {
    let vm = JavaVM::from_raw(app.vm_as_ptr() as _).map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let result = env.with_local_frame(32, |env| -> jni::errors::Result<()> {
        let activity = JObject::from_raw(app.activity_as_ptr() as _);
        let result = (|| -> jni::errors::Result<()> {
            let window = env
                .call_method(&activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            // Exact modern frozen-player edge-to-edge/window-bars policy.
            env.call_method(
                &window,
                "setDecorFitsSystemWindows",
                "(Z)V",
                &[JValue::Bool(0)],
            )?;
            let view = env
                .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                .l()?;
            let controller = env
                .call_method(
                    &view,
                    "getWindowInsetsController",
                    "()Landroid/view/WindowInsetsController;",
                    &[],
                )?
                .l()?;
            if controller.is_null() {
                return Err(jni::errors::Error::NullPtr(
                    "attached window insets controller",
                ));
            }
            env.call_method(
                &controller,
                "setSystemBarsBehavior",
                "(I)V",
                &[JValue::Int(1)],
            )?;
            for kind in ["statusBars", "navigationBars"] {
                let mask = env
                    .call_static_method("android/view/WindowInsets$Type", kind, "()I", &[])?
                    .i()?;
                env.call_method(&controller, "hide", "(I)V", &[JValue::Int(mask)])?;
            }
            Ok(())
        })();
        let _ = activity.into_raw();
        result
    });
    if result.is_err() {
        let _ = env.exception_clear();
    }
    result.map_err(|e| e.to_string())
}
impl Wake for AppWake {
    fn wake(self: Arc<Self>) {
        self.0.wake();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.wake();
    }
}
fn binding() -> (usize, usize) {
    (
        API.get_current_context().map_or(0, |v| v.as_ptr() as usize),
        API.get_current_surface(egl::DRAW)
            .map_or(0, |v| v.as_ptr() as usize),
    )
}
unsafe extern "C" fn proc(name: *const c_char) -> Option<unsafe extern "C" fn()> {
    if name.is_null() {
        return None;
    }
    API.get_proc_address(CStr::from_ptr(name).to_str().ok()?)
        .map(|f| std::mem::transmute::<extern "system" fn(), unsafe extern "C" fn()>(f))
}
struct Graphics {
    display: egl::Display,
    context: egl::Context,
    pbuffer: egl::Surface,
    surface: Option<egl::Surface>,
    gl: glow::Context,
    fbo: Option<glow::Framebuffer>,
    texture: Option<glow::Texture>,
    depth: Option<glow::Renderbuffer>,
    geometry: [i32; 2],
    gles: [u32; 2],
    attributes: Vec<i32>,
    selection_matches_frozen_query: bool,
    // Hold a normal Android window throughout native worker retirement.
    _window: ndk::native_window::NativeWindow,
}
impl Graphics {
    unsafe fn new(window: ndk::native_window::NativeWindow) -> Result<Self> {
        observe::initialize(proc, binding);
        let display = API
            .get_display(egl::DEFAULT_DISPLAY)
            .ok_or("no EGL display")?;
        API.initialize(display).map_err(|e| e.to_string())?;
        API.bind_api(egl::OPENGL_ES_API)
            .map_err(|e| e.to_string())?;
        let attributes = [
            egl::SURFACE_TYPE,
            egl::WINDOW_BIT | egl::PBUFFER_BIT,
            egl::RENDERABLE_TYPE,
            egl::OPENGL_ES3_BIT,
            egl::RED_SIZE,
            8,
            egl::GREEN_SIZE,
            8,
            egl::BLUE_SIZE,
            8,
            egl::ALPHA_SIZE,
            8,
            egl::NONE,
        ];
        let config = API
            .choose_first_config(display, &attributes)
            .map_err(|e| e.to_string())?
            .ok_or("no matching EGL config")?;
        let context = API
            .create_context(
                display,
                config,
                None,
                &[egl::CONTEXT_CLIENT_VERSION, 3, egl::NONE],
            )
            .map_err(|e| e.to_string())?;
        let pbuffer = match API.create_pbuffer_surface(
            display,
            config,
            &[egl::WIDTH, 1, egl::HEIGHT, 1, egl::NONE],
        ) {
            Ok(v) => v,
            Err(e) => {
                let _ = API.destroy_context(display, context);
                return Err(e.to_string());
            }
        };
        if let Err(e) = API.make_current(display, Some(pbuffer), Some(pbuffer), Some(context)) {
            let _ = API.destroy_surface(display, pbuffer);
            let _ = API.destroy_context(display, context);
            return Err(e.to_string());
        }
        let gl = glow::Context::from_loader_function(|n| {
            API.get_proc_address(n)
                .map_or(std::ptr::null(), |p| p as *const _)
        });
        let gles = [
            gl.get_parameter_i32(glow::MAJOR_VERSION) as u32,
            gl.get_parameter_i32(glow::MINOR_VERSION) as u32,
        ];
        let mut g = Self {
            display,
            context,
            pbuffer,
            surface: None,
            gl,
            fbo: None,
            texture: None,
            depth: None,
            geometry: [0; 2],
            gles,
            attributes: Vec::new(),
            selection_matches_frozen_query: false,
            _window: window,
        };
        // Only the required frontend render target, matching the frozen player.
        // No program, UI/text atlas, vertex buffer, draw or game-image copy.
        g.fbo = Some(g.gl.create_framebuffer()?);
        g.texture = Some(g.gl.create_texture()?);
        g.depth = Some(g.gl.create_renderbuffer()?);
        g.gl.bind_texture(glow::TEXTURE_2D, g.texture);
        g.gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            1024,
            1024,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        g.gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::NEAREST as i32,
        );
        g.gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::NEAREST as i32,
        );
        g.gl.bind_renderbuffer(glow::RENDERBUFFER, g.depth);
        g.gl.renderbuffer_storage(glow::RENDERBUFFER, glow::DEPTH24_STENCIL8, 1024, 1024);
        g.gl.bind_framebuffer(glow::FRAMEBUFFER, g.fbo);
        g.gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            g.texture,
            0,
        );
        g.gl.framebuffer_renderbuffer(
            glow::FRAMEBUFFER,
            glow::DEPTH_STENCIL_ATTACHMENT,
            glow::RENDERBUFFER,
            g.depth,
        );
        if g.gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
            return Err("required framebuffer incomplete".into());
        }
        g.gl.clear_color(0.02, 0.08, 0.12, 1.);
        g.gl.clear(glow::COLOR_BUFFER_BIT);
        g.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        observe::event(43, [0; 8]);
        let surface = API
            .create_window_surface(display, config, g._window.ptr().as_ptr() as _, None)
            .map_err(|e| e.to_string())?;
        g.surface = Some(surface);
        API.make_current(display, Some(surface), Some(surface), Some(context))
            .map_err(|e| e.to_string())?;
        API.swap_interval(display, 0).map_err(|e| e.to_string())?;
        g.geometry = [
            API.query_surface(display, surface, egl::WIDTH)
                .map_err(|e| e.to_string())?,
            API.query_surface(display, surface, egl::HEIGHT)
                .map_err(|e| e.to_string())?,
        ];
        for k in [
            egl::CONFIG_ID,
            egl::RED_SIZE,
            egl::GREEN_SIZE,
            egl::BLUE_SIZE,
            egl::ALPHA_SIZE,
            egl::DEPTH_SIZE,
            egl::STENCIL_SIZE,
            egl::SAMPLES,
            egl::SURFACE_TYPE,
            egl::RENDERABLE_TYPE,
        ] {
            g.attributes.push(
                API.get_config_attrib(display, config, k)
                    .map_err(|e| e.to_string())?,
            );
        }
        g.attributes.push(
            API.query_context(display, context, egl::CONTEXT_CLIENT_VERSION)
                .map_err(|e| e.to_string())?,
        );
        // Execute the frozen player's exact selector on the same display,
        // without constructing any player objects or additional context.
        // Its literal request/source is separately pinned by focused fixtures.
        g.selection_matches_frozen_query = API
            .choose_first_config(display, &attributes)
            .map_err(|e| e.to_string())?
            == Some(config);
        if !g.selection_matches_frozen_query {
            return Err("frozen EGL selector disagreement".into());
        }
        observe::event(44, [0; 8]);
        Ok(g)
    }
    fn swap(&self) -> Result<()> {
        if observe::stop_requested() {
            return Err("observer stop before swap".into());
        }
        // Retain the player's bounded frontend fatal check before/after its
        // presentation work. These are existing handoff checks, not GS polls.
        unsafe {
            self.check_context()?;
            // Each Android backbuffer is otherwise undefined. A constant clear
            // prevents that from becoming misleading visual flicker. No game
            // texture, UI shader, geometry or compositing is read/drawn.
            self.gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            self.gl.disable(glow::SCISSOR_TEST);
            self.gl.color_mask(true, true, true, true);
            self.gl.clear_color(0.02, 0.08, 0.12, 1.);
            self.gl.clear(glow::COLOR_BUFFER_BIT);
            self.check_context()?;
        }
        // A supported window swap remains. No first-party game draw/readback.
        API.swap_buffers(self.display, self.surface.ok_or("window absent")?)
            .map_err(|e| {
                observe::request_stop(observe::StopReason::Swap);
                e.to_string()
            })
    }
    unsafe fn check_context(&self) -> Result<()> {
        if let Some(code) = observe::take_fatal() {
            return Err(format!("frontend fatal {code}"));
        }
        for _ in 0..8 {
            let error = self.gl.get_error();
            observe::existing_error(error);
            match error {
                glow::NO_ERROR => break,
                glow::CONTEXT_LOST | glow::OUT_OF_MEMORY => {
                    return Err(format!("frontend fatal {error}"))
                }
                _ => {}
            }
        }
        Ok(())
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe {
            let surface = self.surface.unwrap_or(self.pbuffer);
            let current = API
                .make_current(
                    self.display,
                    Some(surface),
                    Some(surface),
                    Some(self.context),
                )
                .is_ok();
            if current && !observe::output_sync::context_unavailable() {
                if let Some(v) = self.fbo.take() {
                    self.gl.delete_framebuffer(v);
                }
                if let Some(v) = self.texture.take() {
                    self.gl.delete_texture(v);
                }
                if let Some(v) = self.depth.take() {
                    self.gl.delete_renderbuffer(v);
                }
            }
            // Existing shutdown observation; known fatal suppresses ordinary GL cleanup.
            if current && !observe::output_sync::context_unavailable() {
                observe::sample(41, [0; 8]);
            }
            let _ = API.make_current(self.display, None, None, None);
            if let Some(v) = self.surface.take() {
                let _ = API.destroy_surface(self.display, v);
            }
            let _ = API.destroy_surface(self.display, self.pbuffer);
            let destroyed = API.destroy_context(self.display, self.context).is_ok();
            observe::common::frontend_destroyed(destroyed);
            if destroyed {
                observe::output_sync::share_group_destroyed();
            }
            let _ = API.terminate(self.display);
        }
    }
}
unsafe fn library(app: &AndroidApp) -> Result<PathBuf> {
    let vm = JavaVM::from_raw(app.vm_as_ptr() as _).map_err(|e| e.to_string())?;
    let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
    let activity = JObject::from_raw(app.activity_as_ptr() as _);
    let result = (|| -> jni::errors::Result<PathBuf> {
        let info = env
            .call_method(
                &activity,
                "getApplicationInfo",
                "()Landroid/content/pm/ApplicationInfo;",
                &[],
            )?
            .l()?;
        let value = JString::from(
            env.get_field(&info, "nativeLibraryDir", "Ljava/lang/String;")?
                .l()?,
        );
        let path = PathBuf::from(env.get_string(&value)?.to_string_lossy().into_owned());
        Ok(path.join("libarmsx2.so"))
    })();
    let _ = activity.into_raw();
    result.map_err(|e| e.to_string())
}
pub fn run(app: AndroidApp) -> Result<()> {
    if unsafe { libc::sysconf(libc::_SC_PAGESIZE) } != 4096 {
        return Err("frozen engine host-page contract differs".into());
    }
    app.set_window_flags(
        WindowManagerFlags::FULLSCREEN
            | WindowManagerFlags::LAYOUT_IN_SCREEN
            | WindowManagerFlags::KEEP_SCREEN_ON,
        WindowManagerFlags::empty(),
    );
    let root = app
        .internal_data_path()
        .ok_or("no app storage")?
        .join("reference-v1");
    let marker = fs::read_to_string(root.join("prepared-v1")).map_err(|e| e.to_string())?;
    if marker.trim() != minimal_host::NATIVE_SHA {
        return Err("independent scratch not prepared for frozen core".into());
    }
    let config = fs::read(root.join("input.json")).map_err(|e| e.to_string())?;
    if config.len() > 4096 {
        return Err("oversized input descriptor".into());
    }
    let config: serde_json::Value = serde_json::from_slice(&config).map_err(|e| e.to_string())?;
    let firmware = config["firmware"].as_str().ok_or("missing firmware name")?;
    let _ = minimal_host::options(firmware)?;
    let expected = [
        config["window"][0].as_i64().ok_or("window width")? as i32,
        config["window"][1].as_i64().ok_or("window height")? as i32,
    ];
    if !expected.iter().all(|v| (1..=16384).contains(v)) {
        return Err("invalid matched window".into());
    }
    let game = root.join("game.iso");
    for p in [
        &game,
        &root.join("system/pcsx2/bios").join(firmware),
        &root.join("system/pcsx2/inis/armsx2-libretro.ini"),
    ] {
        if !p.is_file() {
            return Err("missing admitted input".into());
        }
    }
    if root.join("result.json").exists() {
        return Err("fresh-process trial already consumed".into());
    }
    let waker = Waker::from(Arc::new(AppWake(app.create_waker())));
    let mut graphics: Option<Graphics> = None;
    let mut host: Option<Host> = None;
    let mut lifecycle = Lifecycle::default();
    let mut launch = None;
    let started = Instant::now();
    let mut next = Instant::now();
    let mut geometry = [0; 2];
    let mut attributes = Vec::new();
    let mut gles = [0; 2];
    let mut selection_matches_frozen_query = false;
    let mut setup: Option<Arc<WindowSetup>> = None;
    let mut setup_completed = None;
    let mut geometry_settled = None;
    let result = (|| -> Result<()> {
        loop {
            if observe::stop_requested() {
                return Err("observer/operator first stop".into());
            }
            if started.elapsed() > Duration::from_secs(26) {
                return Err("startup/whole-arm deadline".into());
            }
            let timeout = if host.as_ref().is_some_and(Host::frame_pending) {
                Duration::ZERO
            } else if host.is_some() {
                next.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(8))
            } else {
                Duration::from_millis(1)
            };
            app.poll_events(Some(timeout), |event| {
                if let PollEvent::Main(main) = event {
                    match main {
                        MainEvent::Resume { .. } => lifecycle.event(LifecycleEvent::Resume),
                        MainEvent::GainedFocus => lifecycle.event(LifecycleEvent::Focus),
                        MainEvent::InitWindow { .. } => lifecycle.event(LifecycleEvent::Window),
                        MainEvent::LostFocus => lifecycle.event(LifecycleEvent::Blur),
                        MainEvent::Pause | MainEvent::Stop => {
                            lifecycle.event(LifecycleEvent::Pause)
                        }
                        MainEvent::Destroy | MainEvent::LowMemory => {
                            lifecycle.event(LifecycleEvent::Destroy)
                        }
                        MainEvent::TerminateWindow { .. } => {
                            lifecycle.event(LifecycleEvent::WindowLost)
                        }
                        MainEvent::WindowResized { .. } | MainEvent::ConfigChanged { .. }
                            if host.is_some()
                                && app.native_window().map(|w| [w.width(), w.height()])
                                    != Some(geometry) =>
                        {
                            lifecycle.stop("geometry changed");
                        }
                        _ => {}
                    }
                }
            });
            if let Some(reason) = lifecycle.stopped() {
                return Err(reason.into());
            }
            if host.is_none() {
                if !lifecycle.can_launch() {
                    continue;
                }
                let Some(window) = app.native_window() else {
                    continue;
                };
                let state = setup.get_or_insert_with(|| configure_window(&app, waker.clone()));
                if !state.done.load(Ordering::Acquire) {
                    continue;
                }
                if let Some(error) = state
                    .error
                    .lock()
                    .map_err(|_| "window setup state")?
                    .clone()
                {
                    return Err(error);
                }
                let completed = *setup_completed.get_or_insert_with(Instant::now);
                let observed = [window.width(), window.height()];
                if observed != expected {
                    geometry_settled = None;
                    if completed.elapsed() > Duration::from_secs(2) {
                        geometry = observed;
                        return Err("window equivalence mismatch after layout".into());
                    }
                    continue;
                }
                let settled = *geometry_settled.get_or_insert_with(Instant::now);
                if settled.elapsed() < Duration::from_millis(32) {
                    continue;
                }
                let g = unsafe { Graphics::new(window) }?;
                geometry = g.geometry;
                gles = g.gles;
                attributes = g.attributes.clone();
                selection_matches_frozen_query = g.selection_matches_frozen_query;
                if geometry != expected {
                    return Err("window equivalence mismatch".into());
                }
                observe::start(&root, waker.clone()).map_err(|e| e.to_string())?;
                lifecycle.begin()?;
                launch = Some(Instant::now());
                let loaded = unsafe {
                    Host::load(
                        &library(&app)?,
                        &game,
                        Config {
                            system: &root.join("system"),
                            save: &root.join("saves"),
                            firmware,
                            framebuffer: g.fbo.unwrap().0.get() as usize,
                            proc_address: proc,
                            gles: g.gles,
                            waker: waker.clone(),
                        },
                    )
                };
                graphics = Some(g);
                host = Some(loaded?);
                continue;
            }
            let h = host.as_mut().unwrap();
            if !h.is_ready() && launch.unwrap().elapsed() >= Duration::from_secs(15) {
                return Err("guest startup deadline".into());
            }
            if h.frame_pending() || Instant::now() >= next {
                let serviced = h.service(true)?;
                next = Instant::now() + Duration::from_millis(if h.is_ready() { 8 } else { 1 });
                if serviced {
                    graphics.as_ref().unwrap().swap()?;
                }
            }
            if launch.unwrap().elapsed() >= Duration::from_secs(20) {
                break;
            }
        }
        Ok(())
    })();
    if result.is_err() {
        observe::request_stop(observe::StopReason::Dependency);
    }
    let evidence = host
        .as_mut()
        .map(Host::stop)
        .or_else(minimal_host::stopped_evidence);
    drop(host);
    drop(graphics);
    observe::service(&root, true);
    observe::join_export();
    let bytes=serde_json::to_vec(&serde_json::json!({
        "version":2,"minimal_host":true,
        "harness_source":env!("PSX_MINIMAL_SOURCE_REVISION"),
        "frozen_player":minimal_host::PLAYER_SOURCE,"native":minimal_host::NATIVE_SHA,"baseline_native":minimal_host::BASELINE_NATIVE_SHA,
        "result":result.as_ref().err(),"window":geometry,"expected_window":expected,
        "gles":gles,"egl_attributes":attributes,
        "egl_selection_matches_frozen_query":selection_matches_frozen_query,
        "window_setup_complete":setup.as_ref().is_some_and(|s|s.done.load(Ordering::Acquire)&&s.error.lock().is_ok_and(|e|e.is_none())),
        "host_page":unsafe{libc::sysconf(libc::_SC_PAGESIZE)},
        "elapsed_since_guest_launch":launch.map(|t|t.elapsed().as_secs_f64()),"evidence":evidence,
        "removed":["player UI","compositing","collection/session","checkpoint worker","audio output device"],
        "shared":["ABI","observer","creator output","native signals","failure latch","android-activity/NDK/EGL"]
    })).map_err(|e|e.to_string())?;
    if bytes.len() > 64 * 1024 {
        return Err("bounded result exceeded".into());
    }
    psx_player::saves::atomic_write(&root.join("result.json"), &bytes)
        .map_err(|e| e.to_string())?;
    result
}
