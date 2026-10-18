mod graphics;
mod platform;
mod sound;

use android_activity::{
    input::{InputEvent, KeyAction, Keycode, MotionAction},
    AndroidApp, InputStatus, MainEvent, PollEvent,
};
use graphics::Graphics;
use platform::{Event, Platform, PICK_FIRMWARE, PICK_GAME};
use psx_player::{
    audio::AudioQueue,
    files::{self, Game},
    input::{Layout, Touches},
    retro::{Core, HostConfig},
    saves::MemoryCard,
    Activity,
};
use sound::Sound;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, mpsc, Arc},
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
enum Action {
    Open,
    Recent,
    Resume,
    Firmware,
    Close,
    About,
    Exit,
}
struct Player {
    root: PathBuf,
    platform: Platform,
    core: Option<Core>,
    card: Option<MemoryCard>,
    game: Option<Game>,
    recent: Option<Game>,
    queue: Arc<AudioQueue>,
    sound: Option<Sound>,
    activity: Activity,
    touches: Touches,
    layout: Layout,
    density: f32,
    menu: bool,
    notice: bool,
    picker: bool,
    actions: Vec<Action>,
    import: Option<mpsc::Receiver<Result<Option<Game>, String>>>,
    message: String,
    destroyed: bool,
    dirty: bool,
    next_frame: Instant,
    last_flush: Instant,
    last_report: Instant,
    frames: u64,
    run_time: Duration,
    // Last: GL outlives every core teardown, including early-return paths.
    graphics: Graphics,
}

pub fn run(app: AndroidApp) -> Result<(), String> {
    let root = app
        .internal_data_path()
        .ok_or("App storage is unavailable.")?;
    fs::create_dir_all(root.join("system")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("saves")).map_err(|e| e.to_string())?;
    let platform = unsafe { Platform::new(&app)? };
    platform.configure(&app);
    let graphics = unsafe { Graphics::new()? };
    let now = Instant::now();
    let mut player=Player {
        recent:files::recent(&root),root,platform,graphics,
        core:None,card:None,game:None,queue:Arc::new(AudioQueue::new(4096)),sound:None,
        activity:Activity::default(),touches:Touches::default(),layout:Layout::new(360.,800.),density:1.,
        menu:false,notice:false,picker:false,actions:Vec::new(),import:None,
        message:"Open a local PS1 game. Select one CUE with all its BIN tracks, or a CHD, PBP, ISO or PS-EXE.".into(),
        destroyed:false,dirty:true,next_frame:now,last_flush:now,last_report:now,frames:0,run_time:Duration::ZERO,
    };
    while !player.destroyed {
        let timeout = if player.activity.can_run() && player.core.is_some() {
            player
                .next_frame
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(8))
        } else {
            Duration::from_millis(33)
        };
        app.poll_events(Some(timeout), |event| {
            if let PollEvent::Main(event) = event {
                if let Err(error) = player.lifecycle(&app, event) {
                    player.fail(error);
                }
            }
        });
        player.input(&app);
        for event in platform::events() {
            player.platform_event(&app, event);
        }
        if let Some(receiver) = &player.import {
            if let Ok(result) = receiver.try_recv() {
                player.import = None;
                player.activity.loading = false;
                match result {
                    Ok(Some(game)) => {
                        if let Err(e) = player.start(game) {
                            player.fail(e);
                        }
                    }
                    Ok(None) => {
                        player.message = "Firmware added. Open your game to try it.".into();
                    }
                    Err(e) => player.fail(e),
                }
            }
        }
        if player.destroyed {
            break;
        }
        if !player.activity.can_run() {
            player.stop_sound();
            player.next_frame = Instant::now();
        } else if player.core.is_some() && Instant::now() >= player.next_frame {
            if let Err(error) = player.tick() {
                player.fail(error);
            }
        }
        if let Some(sound) = &player.sound {
            if sound.failed.load(Ordering::Acquire) {
                player.fail("The audio device changed. Resume the game to reconnect sound.".into());
            }
        }
        if !player.menu
            && !player.picker
            && player.import.is_none()
            && player.activity.foreground
            && player.activity.surface
            && (player.core.is_none() || player.activity.user_paused)
        {
            player.show_menu(&app);
        }
        let frame = player.core.as_ref().and_then(|core| {
            let state = core.state();
            (state.hardware_frames > 0).then_some((
                state.frame_width,
                state.frame_height,
                state.hw.map(|h| h.bottom_left).unwrap_or(true),
            ))
        });
        if player.dirty && player.activity.surface && player.activity.foreground {
            if let Err(error) = unsafe {
                player.graphics.render(
                    player.density,
                    &player.layout,
                    frame,
                    player.touches.mask(),
                    &player.message,
                )
            } {
                player.fail(error);
                player.activity.surface = false;
            }
            player.dirty = false;
        }
    }
    player.stop_sound();
    let result = player.flush();
    player.core.take();
    player.platform.focus(&app, false);
    result
}

impl Player {
    fn lifecycle(&mut self, app: &AndroidApp, event: MainEvent) -> Result<(), String> {
        self.dirty = true;
        match event {
            MainEvent::InitWindow { .. } => {
                if let Some(window) = app.native_window() {
                    unsafe {
                        self.graphics.attach(&window)?;
                    }
                }
                self.activity.surface = true;
                self.resize(app)?;
            }
            MainEvent::TerminateWindow { .. } => {
                self.interrupt()?;
                self.activity.surface = false;
                self.graphics.detach()?;
            }
            MainEvent::WindowResized { .. }
            | MainEvent::ContentRectChanged { .. }
            | MainEvent::ConfigChanged { .. }
            | MainEvent::InsetsChanged { .. } => {
                self.touches.cancel();
                self.resize(app)?;
            }
            MainEvent::GainedFocus => {
                self.activity.focused = true;
            }
            MainEvent::LostFocus => {
                self.activity.focused = false;
                self.interrupt()?;
            }
            MainEvent::Resume { .. } => {
                self.activity.foreground = true;
                self.platform.focus(app, true);
            }
            MainEvent::Pause | MainEvent::Stop => {
                self.activity.foreground = false;
                self.activity.audio_focus = false;
                self.interrupt()?;
                self.platform.focus(app, false);
            }
            MainEvent::SaveState { .. } => {
                self.flush()?;
            }
            MainEvent::LowMemory => {
                self.flush()?;
            }
            MainEvent::Destroy => {
                self.destroyed = true;
                self.interrupt()?;
            }
            _ => {}
        }
        Ok(())
    }
    fn resize(&mut self, app: &AndroidApp) -> Result<(), String> {
        self.graphics.resize()?;
        let content = app.content_rect();
        self.graphics
            .content_rect(content.left, content.top, content.right, content.bottom);
        self.density = app
            .config()
            .density()
            .map(|d| d as f32 / 160.)
            .filter(|d| *d >= 0.5 && *d <= 6.)
            .unwrap_or(1.);
        self.layout = Layout::new(
            self.graphics.content.w / self.density,
            self.graphics.content.h / self.density,
        );
        Ok(())
    }
    fn input(&mut self, app: &AndroidApp) {
        let Ok(mut events) = app.input_events_iter() else {
            return;
        };
        while events.next(|event| match event {
            InputEvent::MotionEvent(motion) => {
                self.dirty = true;
                match motion.action() {
                    MotionAction::Cancel => self.touches.cancel(),
                    MotionAction::Up | MotionAction::PointerUp => {
                        self.touches
                            .release(motion.pointer_at_index(motion.pointer_index()).pointer_id());
                    }
                    MotionAction::Down | MotionAction::PointerDown | MotionAction::Move => {
                        for pointer in motion.pointers() {
                            let x = (pointer.x() - self.graphics.content.x) / self.density;
                            let y = (pointer.y() - self.graphics.content.y) / self.density;
                            if motion.action() == MotionAction::Down
                                && self.layout.pause.contains(x, y)
                                && self.core.is_some()
                            {
                                self.activity.user_paused = true;
                                self.touches.cancel();
                                if let Err(error) = self.interrupt() {
                                    self.fail(error);
                                }
                            } else if self.activity.can_run() {
                                self.touches
                                    .update(pointer.pointer_id(), self.layout.mask_at(x, y));
                            }
                        }
                    }
                    _ => {}
                }
                InputStatus::Handled
            }
            InputEvent::KeyEvent(key) if key.key_code() == Keycode::Back => {
                if key.action() == KeyAction::Up {
                    self.activity.user_paused = true;
                    if let Err(error) = self.interrupt() {
                        self.fail(error);
                    }
                }
                InputStatus::Handled
            }
            _ => InputStatus::Unhandled,
        }) {}
    }
    fn platform_event(&mut self, app: &AndroidApp, event: Event) {
        self.dirty = true;
        match event {
            Event::Menu(index) => {
                self.menu = false;
                if self.notice {
                    self.notice = false;
                    return;
                }
                if index < 0 {
                    if self.core.is_some() {
                        self.activity.user_paused = false;
                        self.platform.focus(app, true);
                    } else {
                        self.platform.finish(app);
                    }
                    return;
                }
                match self.actions.get(index as usize).copied() {
                    Some(Action::Open) => {
                        self.picker = true;
                        self.platform.pick(app, PICK_GAME);
                    }
                    Some(Action::Firmware) => {
                        self.picker = true;
                        self.platform.pick(app, PICK_FIRMWARE);
                    }
                    Some(Action::Recent) => {
                        if let Some(game) = self.recent.clone() {
                            if let Err(error) = self.start(game) {
                                self.fail(error);
                            }
                        }
                    }
                    Some(Action::Resume) => {
                        self.activity.user_paused = false;
                        self.platform.focus(app, true);
                    }
                    Some(Action::Close) => match self.close() {
                        Err(e) => self.fail(e),
                        Ok(()) => {
                            self.message = "Game closed. Your memory card has been saved.".into()
                        }
                    },
                    Some(Action::About) => {
                        let mut body=String::from("psx keeps games, firmware and saved progress on this device. No accounts, analytics or uploads.\n\nSource and build instructions: https://github.com/WigglyDon/psx\n\n");
                        for path in [
                            c"notices/INDEX.txt",
                            c"notices/host/LICENSE",
                            c"notices/swanstation/LICENSE",
                            c"notices/swanstation/dep/openbios/LICENSE.txt",
                        ] {
                            if let Some(mut asset) = app.asset_manager().open(path) {
                                if let Ok(bytes) = asset.buffer() {
                                    body.push_str(&String::from_utf8_lossy(bytes));
                                    body.push_str("\n\n");
                                }
                            }
                        }
                        self.notice = true;
                        self.menu = true;
                        self.platform.notices(app, body);
                    }
                    Some(Action::Exit) => {
                        if let Err(e) = self.close() {
                            self.fail(e);
                        } else {
                            self.platform.finish(app);
                        }
                    }
                    _ => {}
                }
            }
            Event::Documents(request, intent) => {
                self.picker = false;
                if let Some(intent) = intent {
                    self.activity.loading = true;
                    self.import = Some(self.platform.import(self.root.clone(), request, intent));
                    self.message="Importing the selected files into private storage. Large discs can take a moment.".into();
                }
            }
            Event::Focus(focus) => {
                self.activity.audio_focus = focus == 1;
                if focus != 1 {
                    if focus == -1 && self.activity.foreground && !self.picker && !self.menu {
                        self.activity.user_paused = true;
                    }
                    if let Err(e) = self.interrupt() {
                        self.fail(e);
                    }
                }
            }
            Event::Error(error) => {
                self.picker = false;
                self.menu = false;
                self.fail(error);
            }
        }
    }
    fn show_menu(&mut self, app: &AndroidApp) {
        self.activity.user_paused = self.core.is_some();
        self.menu = true;
        self.actions.clear();
        let mut items = Vec::new();
        let mut add = |action, label: String| {
            self.actions.push(action);
            items.push(label);
        };
        if self.core.is_some() {
            add(Action::Resume, "Resume game".into());
        }
        add(Action::Open, "Open game…".into());
        if self.core.is_none() {
            if let Some(game) = &self.recent {
                add(Action::Recent, format!("Continue {}", game.title));
            }
        }
        add(Action::Firmware, "Select PS1 firmware…".into());
        if self.core.is_some() {
            add(Action::Close, "Close game and save memory card".into());
        }
        add(Action::About, "Source, licenses and privacy".into());
        add(Action::Exit, "Exit".into());
        self.platform.menu(
            app,
            if self.core.is_some() {
                "psx · Paused".into()
            } else {
                "psx".into()
            },
            self.message.clone(),
            items,
        );
    }
    fn start(&mut self, game: Game) -> Result<(), String> {
        self.close()?;
        let library = Path::new("libswanstation.so");
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let jit = page_size == 4096;
        // Existing cached interpreter avoids 4KB JIT guard/mprotect assumptions
        // on 16KB kernels. Runtime performance there is still unqualified.
        let mut core = unsafe {
            Core::load(
                library,
                &game.path,
                HostConfig {
                    system: &self.root.join("system"),
                    save: &self.root.join("saves"),
                    audio: self.queue.clone(),
                    framebuffer: self.graphics.framebuffer(),
                    proc_address: self.graphics.proc_address(),
                    jit,
                },
            )?
        };
        let card = MemoryCard::open(self.root.join("saves").join(format!("{}.srm", game.id)))
            .map_err(|e| e.to_string())?;
        core.restore_card(&card)?;
        let av = core.state().av;
        if !(45.0..=65.0).contains(&av.timing.fps) || av.timing.sample_rate != 44100. {
            return Err("The core requested unsupported audio or frame timing.".into());
        }
        log::info!(
            "Session started: fps={:.4}, sample_rate={}, host_page={}, jit={}",
            av.timing.fps,
            av.timing.sample_rate,
            page_size,
            jit
        );
        self.core = Some(core);
        self.card = Some(card);
        self.game = Some(game.clone());
        self.recent = Some(game.clone());
        files::remember(&self.root, &game).map_err(|e| e.to_string())?;
        self.activity.user_paused = false;
        self.next_frame = Instant::now();
        self.last_flush = Instant::now();
        self.last_report = Instant::now();
        self.frames = 0;
        self.run_time = Duration::ZERO;
        self.message="Use the controls to play. Pause or Android Back opens the menu. Completed card changes are saved each second and on interruption. The bundled OpenBIOS fallback may have compatibility limits; select your own firmware if needed.".into();
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        self.dirty = true;
        let core = self.core.as_mut().unwrap();
        core.state_mut().buttons = self.touches.mask();
        let before = Instant::now();
        core.run();
        self.run_time += before.elapsed();
        self.frames += 1;
        if core.state().shutdown {
            return Err("The game session stopped.".into());
        }
        if core.state().hardware_frames > 0 && core.state().software_frames > 0 {
            return Err("The hardware renderer stopped. Close and reopen the game.".into());
        }
        if core.state().hardware_frames == 0 && self.frames > 600 {
            return Err("The core has not produced a hardware-rendered picture. Try compatible PS1 firmware.".into());
        }
        if core.state().frame_width > 1024 || core.state().frame_height > 1024 {
            return Err("The core output exceeded the native-resolution buffer.".into());
        }
        let fps = core.state().av.timing.fps;
        let error = (self.queue.queued() as f64 - 1100.) / 1100.;
        let period =
            Duration::from_secs_f64((1. / fps) * (1. + (error * 0.003).clamp(-0.005, 0.005)));
        self.next_frame += period;
        if Instant::now().saturating_duration_since(self.next_frame) > Duration::from_millis(100) {
            self.next_frame = Instant::now();
        }
        if self.sound.is_none() {
            self.sound = Some(Sound::start(self.queue.clone())?);
        }
        if self.last_flush.elapsed() >= Duration::from_secs(1) {
            self.flush()?;
            self.last_flush = Instant::now();
        }
        if self.last_report.elapsed() >= Duration::from_secs(10) {
            let elapsed = self.last_report.elapsed().as_secs_f64();
            let core = self.core.as_ref().unwrap();
            log::info!("playback fps={:.2} speed={:.1}% core_ms={:.2} hw_frames={} sw_frames={} audio_queued={} underruns={} dropped={}",
                self.frames as f64/elapsed,self.frames as f64/elapsed/fps*100.,self.run_time.as_secs_f64()*1000./self.frames as f64,
                core.state().hardware_frames,core.state().software_frames,self.queue.queued(),self.queue.underruns.load(Ordering::Relaxed),self.queue.dropped.load(Ordering::Relaxed));
            self.last_report = Instant::now();
            self.frames = 0;
            self.run_time = Duration::ZERO;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        if let (Some(core), Some(card)) = (&mut self.core, &mut self.card) {
            core.flush_card(card)?;
        }
        Ok(())
    }
    fn stop_sound(&mut self) {
        self.sound.take();
        self.queue.clear_stopped();
    }
    fn interrupt(&mut self) -> Result<(), String> {
        self.frames = 0;
        self.run_time = Duration::ZERO;
        self.last_report = Instant::now();
        self.touches.cancel();
        if let Some(core) = &mut self.core {
            core.state_mut().buttons = 0;
        }
        self.stop_sound();
        self.flush()
    }
    fn close(&mut self) -> Result<(), String> {
        self.interrupt()?;
        self.core.take();
        self.card = None;
        self.game = None;
        self.queue.clear_stopped();
        Ok(())
    }
    fn fail(&mut self, error: String) {
        self.dirty = true;
        log::error!("{error}");
        self.message = error;
        self.activity.user_paused = true;
        self.touches.cancel();
        self.stop_sound();
    }
}
