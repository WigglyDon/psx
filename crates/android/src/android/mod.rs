mod background;
mod graphics;
mod platform;
mod shortcuts;
mod sound;

use android_activity::{
    input::{InputEvent, KeyAction, Keycode, MotionAction},
    AndroidApp, InputStatus, MainEvent, PollEvent,
};
use graphics::{Graphics, RenderError};
use platform::{Event, Platform, PICK_FIRMWARE, PICK_GAME, PICK_PS2_FIRMWARE, PICK_STORAGE};
use psx_player::{
    artwork::Artwork,
    audio::AudioQueue,
    controller_profiles::Settings as ControllerProfiles,
    engine::Core,
    files::{Game, System},
    input::{safe_content, ControllerSettings, Layout, PlayerViewport, Preset, Rect, Touches},
    library::{self, Library},
    phone_buttons::{Bindings, Button, PhoneInput, PhoneKey},
    resume,
    retro::HostConfig,
    saves::MemoryCard,
    session::{GameId, Launch, Runtime, SavedSession, State as SessionState},
    settings::PlaybackSettings,
    ui::{EditResult, Editor, HoldPress, Menu, PressAction, Row, Visuals},
    Activity,
};
use sound::Sound;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
enum Action {
    Browse,
    Scan,
    GrantStorage,
    ChooseGame(usize),
    GameActions(usize),
    PinGame(GameId),
    Relink(usize),
    ChooseDisc(usize),
    ChooseSystem(usize, System),
    ToggleGame(usize),
    SelectAll,
    AddSelected,
    CancelWork,
    Settings,
    Home,
    Discs,
    Controllers,
    ToggleGameController,
    PhoneButtons,
    BindPhoneKey(PhoneKey),
    SetPhoneKey(PhoneKey, Option<Button>),
    ResetPhoneKeys,
    Sound,
    Timescale,
    ToggleMute,
    SetVolume(u8),
    ToggleTimescale,
    SetTimescale(u8),
    TogglePause,
    ToggleOverlap,
    ToggleFollowThumb,
    ToggleStickClick,
    EditControls,
    ResetControls,
    SaveControls,
    CancelEdit,
    License(&'static str),
    Source,
    Back,
    Resume,
    Firmware,
    Ps2Firmware,
    Close,
    Restart,
    ToggleBackground,
    About,
}
#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Home,
    Settings,
    Access,
    Scanning,
    Results,
    Adding,
    Discs(usize),
    Console(usize),
    GameActions(usize),
    Recover(Option<usize>),
    About,
    License(&'static str),
    Controllers,
    PhoneButtons,
    PhoneBinding(PhoneKey),
    Sound,
    Timescale,
    Editing,
    Error,
}
enum Work {
    Scanned(library::Scan),
    Added(Library),
}
struct Player {
    frame_waker: std::task::Waker,
    root: PathBuf,
    platform: Platform,
    core: Option<Core>,
    session: Option<Runtime>,
    card: Option<MemoryCard>,
    game: Option<Game>,
    library: Library,
    controller_settings: ControllerSettings,
    controller_profiles: ControllerProfiles,
    phone_bindings: Bindings,
    phone_input: PhoneInput,
    playback: PlaybackSettings,
    queue: Arc<AudioQueue>,
    sound: Option<Sound>,
    activity: Activity,
    touches: Touches,
    speaker_press: HoldPress,
    time_press: HoldPress,
    layout: Layout,
    density: f32,
    insets: [i32; 4],
    fullscreen_insets: Option<[i32; 4]>,
    picture_insets: Option<[i32; 4]>,
    status_bar_top: i32,
    inset_size: [i32; 2],
    last_insets_request: Instant,
    rehide_bars_at: Option<Instant>,
    playback_changed: Option<Instant>,
    cutouts: Vec<[i32; 4]>,
    screen: Screen,
    menu: Option<Menu>,
    closing_menu: Option<Menu>,
    visuals: Visuals,
    visual_animating: bool,
    last_visual: Instant,
    editor: Option<Editor>,
    picker: bool,
    relink: Option<(Game, usize)>,
    scan_available: bool,
    actions: Vec<Action>,
    back_action: Action,
    scan_games: Vec<(library::FoundGame, bool)>,
    work: Option<mpsc::Receiver<Result<Work, String>>>,
    cancel_work: Arc<AtomicBool>,
    import: Option<mpsc::Receiver<Result<Option<Game>, String>>>,
    message: String,
    destroyed: bool,
    dirty: bool,
    next_frame: Instant,
    next_present: Instant,
    last_flush: Instant,
    last_checkpoint: Instant,
    checkpoint_dirty: bool,
    checkpoint_valid: bool,
    checkpoint_after_present: bool,
    checkpoint_work: Option<mpsc::Receiver<Result<(), String>>>,
    restore_pending: bool,
    background_requested: bool,
    background_ready: bool,
    last_report: Instant,
    frames: u64,
    run_time: Duration,
    presents: u64,
    present_time: Duration,
    max_present_time: Duration,
    // Last: GL outlives every core teardown, including early-return paths.
    graphics: Graphics,
}

pub fn run(app: AndroidApp) -> Result<(), String> {
    let root = app
        .internal_data_path()
        .ok_or("App storage is unavailable.")?;
    let root = psx_player::acceptance::storage_root(&root).map_err(|e| e.to_string())?;
    let root = psx_player::graphics_observer::storage_root(&root).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("system")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("saves")).map_err(|e| e.to_string())?;
    let platform = unsafe { Platform::new(&app)? };
    platform.configure(&app);
    let graphics = unsafe { Graphics::new()? };
    let now = Instant::now();
    let playback = PlaybackSettings::load(&root).map_err(|e| e.to_string())?;
    let controller_profiles = ControllerProfiles::load(&root).map_err(|e| e.to_string())?;
    let queue = Arc::new(AudioQueue::new(16384));
    queue.set_volume(playback.output_volume());
    queue.set_speed(playback.effective_speed());
    let mut player = Player {
        frame_waker: platform::frame_waker(&app),
        library: Library::load(&root)
            .map_err(|e| format!("The saved library could not be read: {e}"))?,
        controller_settings: controller_profiles
            .controls(&root, None, false)
            .map_err(|e| e.to_string())?,
        phone_bindings: controller_profiles.bindings(None, false),
        controller_profiles,
        phone_input: PhoneInput::default(),
        playback,
        root,
        platform,
        graphics,
        core: None,
        session: None,
        card: None,
        game: None,
        queue,
        sound: None,
        activity: Activity::default(),
        touches: Touches::default(),
        speaker_press: HoldPress::default(),
        time_press: HoldPress::default(),
        layout: Layout::new(360., 800.),
        density: 1.,
        insets: [0; 4],
        fullscreen_insets: None,
        picture_insets: None,
        status_bar_top: 0,
        inset_size: [0; 2],
        last_insets_request: now,
        rehide_bars_at: None,
        playback_changed: None,
        cutouts: Vec::new(),
        screen: Screen::Home,
        menu: None,
        closing_menu: None,
        visuals: Visuals::default(),
        visual_animating: true,
        last_visual: now,
        editor: None,
        picker: false,
        relink: None,
        scan_available: cfg!(feature = "storage-scan"),
        actions: Vec::new(),
        back_action: Action::Scan,
        import: None,
        scan_games: Vec::new(),
        work: None,
        cancel_work: Arc::new(AtomicBool::new(false)),
        message: String::new(),
        destroyed: false,
        dirty: true,
        next_frame: now,
        next_present: now,
        last_flush: now,
        last_checkpoint: now,
        checkpoint_dirty: false,
        checkpoint_valid: false,
        checkpoint_after_present: false,
        checkpoint_work: None,
        restore_pending: true,
        background_requested: false,
        background_ready: false,
        last_report: now,
        frames: 0,
        run_time: Duration::ZERO,
        presents: 0,
        present_time: Duration::ZERO,
        max_present_time: Duration::ZERO,
    };
    #[cfg(feature = "graphics-observer")]
    psx_player::graphics_observer::start(&player.root, player.frame_waker.clone())
        .map_err(|e| e.to_string())?;
    player.refresh_menu(&app);
    while !player.destroyed {
        let timeout = if player.activity.can_run() && player.core.is_some() {
            if player.core.as_ref().is_some_and(Core::frame_pending) {
                Duration::ZERO
            } else {
                player
                    .next_frame
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(8))
            }
        } else if player.visual_animating {
            Duration::from_millis(16)
        } else {
            Duration::from_millis(33)
        };
        let timeout = if player.dirty
            && player.activity.surface
            && player.activity.foreground
            && (player.playback.effective_speed().is_fast()
                || player.core.as_ref().is_some_and(Core::owns_clock))
        {
            timeout.min(
                player
                    .next_present
                    .saturating_duration_since(Instant::now()),
            )
        } else {
            timeout
        };
        app.poll_events(Some(timeout), |event| {
            if let PollEvent::Main(event) = event {
                if let Err(error) = player.lifecycle(&app, event) {
                    player.fail(error);
                }
            }
        });
        #[cfg(feature = "graphics-observer")]
        if psx_player::graphics_observer::stop_requested() {
            break;
        }
        for event in platform::events() {
            player.platform_event(&app, event);
        }
        // EGL can adopt the new buffer size on swap, after NativeActivity's
        // final resize notification. Reconcile it before hit testing, even if
        // Android sends no further lifecycle event.
        if player.activity.surface {
            let before = [player.graphics.width, player.graphics.height];
            if let Err(error) = player.graphics.resize() {
                player.fail(error);
            } else if before != [player.graphics.width, player.graphics.height] {
                player.touches.cancel();
                player.phone_input.cancel();
                if let Err(error) = player.resize(&app) {
                    player.fail(error);
                }
                if player.activity.foreground && player.activity.focused {
                    player.platform.fullscreen(&app);
                }
                player.dirty = true;
            }
            // Revealed bars may overlay the window without a resize callback.
            // Observe visibility at a bounded rate, including while paused.
            if (player.inset_size != [player.graphics.width, player.graphics.height]
                && player.last_insets_request.elapsed() >= Duration::from_millis(100))
                || (player.activity.foreground
                    && player.last_insets_request.elapsed() >= Duration::from_millis(500))
            {
                player.platform.insets(&app);
                player.last_insets_request = Instant::now();
            }
        }
        if player
            .rehide_bars_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            player.rehide_bars_at = None;
            if player.activity.foreground
                && player.activity.focused
                && !player.picker
                && (player.core.is_none() || player.activity.audio_focus)
            {
                player.platform.fullscreen(&app);
                player.platform.insets(&app);
            }
        }
        if player
            .playback_changed
            .is_some_and(|t| t.elapsed() >= Duration::from_millis(500))
        {
            if let Err(error) = player.save_playback() {
                player.fail(error);
            }
        }
        if player.activity.surface && player.activity.foreground && !player.activity.loading {
            if let Some(launch) = platform::next_launch() {
                player.restore_pending = false;
                if let Err(error) = player.open_game(&app, launch) {
                    player.fail(error);
                }
            }
        }
        if player.restore_pending && player.activity.surface && player.activity.foreground {
            player.restore_pending = false;
            match resume::active(&player.root) {
                Ok(Some(id)) => {
                    let result = GameId::parse(&id)
                        .map_err(String::from)
                        .and_then(|id| player.open_game(&app, Launch::OpenGame(id)));
                    if let Err(error) = result {
                        player.fail(error);
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    player.fail(format!("The previous session could not be read: {error}"))
                }
            }
        }
        player.input(&app);
        if player.speaker_press.held(Instant::now()) {
            player.platform.haptic(&app);
            if let Err(error) = player.action(&app, Action::Sound) {
                player.fail(error);
            }
        }
        if player.time_press.held(Instant::now()) {
            player.platform.haptic(&app);
            if let Err(error) = player.action(&app, Action::Timescale) {
                player.fail(error);
            }
        }
        if let Some(failure) = player.core.as_ref().and_then(Core::producer_failure) {
            player.render_failed(RenderError::Producer(failure));
        }
        player.activity.time_paused = player.playback.effective_speed().is_paused();
        player.sync_background(&app);
        if let Some(core) = &mut player.core {
            if let Err(error) = core.pause(!player.activity.can_run()) {
                player.fail(error);
            }
        }
        player
            .queue
            .set_paused(player.activity.user_paused || player.activity.time_paused);
        player
            .queue
            .set_background(player.activity.background_playback && !player.activity.foreground);
        if let Some(work) = &player.checkpoint_work {
            if let Ok(result) = work.try_recv() {
                player.checkpoint_work = None;
                player.dirty |= player.checkpoint_after_present;
                if let Err(error) = result {
                    player.fail(error);
                } else {
                    player.checkpoint_valid = true;
                }
            }
        }
        if let Some(receiver) = &player.import {
            if let Ok(result) = receiver.try_recv() {
                player.import = None;
                player.activity.loading = false;
                match result {
                    Ok(Some(game)) => {
                        let result = GameId::parse(&game.id)
                            .map_err(String::from)
                            .and_then(|id| {
                                player.library.put(game);
                                player.open_game(&app, Launch::OpenGame(id))
                            });
                        if let Err(e) = result {
                            player.fail(e);
                        }
                    }
                    Ok(None) => {
                        player.message = "Firmware added. Open your game to try it.".into();
                        player.navigate(&app, Screen::Settings);
                    }
                    Err(e) => player.fail(e),
                }
            }
        }
        if let Some(receiver) = &player.work {
            if let Ok(result) = receiver.try_recv() {
                player.work = None;
                player.activity.loading = false;
                if player.cancel_work.load(Ordering::Relaxed)
                    && !matches!(&result, Ok(Work::Added(_)))
                {
                    player.message = "Cancelled. Your saved collection is unchanged.".into();
                    player.navigate(&app, Screen::Home);
                } else {
                    match result {
                        Ok(Work::Scanned(scan)) => {
                            player.message = format!("{} games found. {} unreadable folders; {} rejected images; {} archives need extraction.{}", scan.games.len(), scan.unreadable, scan.rejected, scan.archives, if scan.limited { " Scan limit reached." } else { "" });
                            player.scan_games =
                                scan.games.into_iter().map(|g| (g, false)).collect();
                            player.navigate(&app, Screen::Results);
                        }
                        Ok(Work::Added(library)) => {
                            player.library = library;
                            player.message = "Games added. Choose a game to play; adding more games scans storage again.".into();
                            player.navigate(&app, Screen::Home);
                        }
                        Err(e) => player.fail(e),
                    }
                }
            }
        }
        if player.destroyed {
            break;
        }
        if let Some(session) = &mut player.session {
            session.update(player.activity.can_run());
        }
        if !player.activity.can_run() {
            if !((player.activity.user_paused || player.activity.time_paused)
                && player.activity.foreground
                && player.activity.focused
                && player.activity.surface
                && player.activity.audio_focus)
            {
                player.stop_sound();
            }
            player.next_frame = Instant::now();
        } else if player
            .core
            .as_ref()
            .is_some_and(|core| core.frame_pending() || Instant::now() >= player.next_frame)
        {
            if let Err(error) = player.tick() {
                player.fail(error);
            }
        }
        if let Some(sound) = &player.sound {
            if sound.failed.load(Ordering::Acquire) {
                player.fail("The audio device changed. Resume the game to reconnect sound.".into());
            }
        }
        if player.menu.is_none()
            && player.editor.is_none()
            && player.screen != Screen::Error
            && !player.picker
            && player.import.is_none()
            && player.activity.foreground
            && player.activity.surface
            && player.core.is_none()
        {
            player.navigate(&app, Screen::Home);
        }
        if player.screen == Screen::Error && player.menu.is_none() {
            player.refresh_menu(&app);
        }
        let now = Instant::now();
        let animated_icons = player.menu.as_ref().is_some_and(|m| {
            m.rows.iter().any(|r| {
                r.artwork
                    .as_ref()
                    .and_then(|a| a.icon.as_ref())
                    .is_some_and(|i| i.frames.len() > 1)
            })
        });
        player.visuals.has_game = player.core.is_some();
        player.visuals.axes = player.touches.axes();
        player.visuals.audio_blocked = !player.activity.audio_focus;
        player.visuals.playback_paused = player.activity.user_paused
            || player.activity.time_paused
            || !player.activity.audio_focus;
        player.visuals.playback_speed = player.playback.effective_speed();
        player.visuals.timescale_enabled = player.playback.timescale_enabled;
        player.visuals.muted = player.playback.muted || player.playback.volume == 0;
        player.visual_animating = player.visuals.advance(
            (now - player.last_visual).as_secs_f32(),
            player.touches.mask() | player.phone_input.mask(),
            player.menu.is_some() || player.editor.is_some(),
            animated_icons,
            player.touches.dpad_feedback(),
        );
        player.last_visual = now;
        player.dirty |= player.visual_animating;
        if player.visuals.overlay == 0. && player.menu.is_none() {
            player.closing_menu = None;
        }
        let frame = player.core.as_ref().and_then(|core| {
            let state = core.state();
            (state.hardware_frames > 0).then_some((
                state.frame_width,
                state.frame_height,
                state.hw.map(|h| h.bottom_left).unwrap_or(true),
            ))
        });
        // A threaded engine can wake us faster than the display. Always drain
        // its handoff, but bound composition to60 Hz and present the newest frame.
        let paced_present = player.playback.effective_speed().is_fast()
            || player.core.as_ref().is_some_and(Core::owns_clock);
        let present_due = !paced_present || now >= player.next_present;
        #[cfg(feature = "graphics-observer")]
        if psx_player::graphics_observer::stop_requested() {
            break;
        }
        if player.dirty && player.activity.surface && player.activity.foreground && present_due {
            player.next_present = now + Duration::from_secs_f64(1. / 60.);
            let before = Instant::now();
            // A failed presentation sets dirty again for its recovery/error
            // screen. Do not erase that request after handling the failure.
            player.dirty = false;
            let rendered = unsafe {
                player.graphics.render(
                    player.density,
                    &player.layout,
                    frame,
                    player.menu.as_ref().or(player.closing_menu.as_ref()),
                    player.editor.as_ref(),
                    &player.visuals,
                )
            };
            let elapsed = before.elapsed();
            match rendered {
                Ok(()) => {
                    player.presents += 1;
                    player.present_time += elapsed;
                    player.max_present_time = player.max_present_time.max(elapsed);
                    // One save worker at a time; rapid toggles coalesce to the
                    // latest intent. Pause/resume never waits for that worker.
                    if player.checkpoint_after_present && player.checkpoint_work.is_none() {
                        player.checkpoint_after_present = false;
                        if let Err(error) = player.queue_checkpoint() {
                            player.fail(error);
                        }
                    }
                }
                Err(error) => player.render_failed(error),
            }
        }
        #[cfg(feature = "graphics-observer")]
        psx_player::graphics_observer::service(&player.root, false);
    }
    player.stop_sound();
    player.cancel_work.store(true, Ordering::Relaxed);
    #[cfg(feature = "graphics-observer")]
    if psx_player::graphics_observer::stop_requested() {
        // Test retirement on the actual frontend owner, never the GL callback.
        // Keep existing completed writers, but do not make a new fault checkpoint.
        let _ = player.finish_checkpoint();
        player.checkpoint_dirty = false;
        player.checkpoint_valid = false;
        player.checkpoint_after_present = false;
        // Retire through Core's existing pause/unload/join on this GL owner.
        // Scratch is disposable; starting a new flush/checkpoint after a GPU
        // failure would add work and can delay the bounded test stop.
        player.core.take();
    }
    let result = player.unload(false);
    #[cfg(all(
        feature = "graphics-observer",
        not(feature = "producer-local-output-sync")
    ))]
    {
        psx_player::graphics_observer::service(&player.root, true);
        psx_player::graphics_observer::join_export();
    }
    player.platform.focus(&app, false);
    player.platform.background(&app, false);
    #[cfg(feature = "producer-local-output-sync")]
    {
        // Observe the actual final share-group destruction after native join,
        // rather than treating CPU acknowledgment or donor closure as reclaim.
        let root = player.root.clone();
        drop(player);
        psx_player::graphics_observer::service(&root, true);
        psx_player::graphics_observer::join_export();
    }
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
                self.platform.insets(app);
            }
            MainEvent::TerminateWindow { .. } => {
                if self.activity.background_playback {
                    self.cancel_input();
                } else {
                    self.interrupt()?;
                }
                self.activity.surface = false;
                self.graphics.detach()?;
            }
            MainEvent::WindowResized { .. }
            | MainEvent::ContentRectChanged { .. }
            | MainEvent::ConfigChanged { .. }
            | MainEvent::InsetsChanged { .. } => {
                let surface_size = [self.graphics.width, self.graphics.height];
                self.touches.cancel();
                self.phone_input.cancel();
                self.resize(app)?;
                self.platform.insets(app);
                // Bar reveals can send resize callbacks without changing the
                // actual surface. Don't hide bars again during that gesture.
                if surface_size != [self.graphics.width, self.graphics.height]
                    && self.activity.foreground
                    && self.activity.focused
                {
                    self.platform.fullscreen(app);
                }
            }
            MainEvent::GainedFocus => {
                self.activity.focused = true;
                self.platform.fullscreen(app);
                self.platform.insets(app);
            }
            MainEvent::LostFocus => {
                self.activity.focused = false;
                self.rehide_bars_at = None;
                if self.activity.background_playback {
                    self.cancel_input();
                } else {
                    self.interrupt()?;
                }
            }
            MainEvent::Resume { .. } => {
                self.activity.foreground = true;
                self.platform.focus(app, true);
                if self.activity.surface {
                    self.resize(app)?;
                }
            }
            MainEvent::Pause | MainEvent::Stop => {
                self.speaker_press.cancel();
                self.time_press.cancel();
                self.activity.foreground = false;
                self.touches.cancel();
                self.phone_input.cancel();
                if !self.activity.background_playback {
                    self.activity.audio_focus = false;
                    self.interrupt()?;
                    self.platform.focus(app, false);
                }
                self.checkpoint_interruption()?;
                #[cfg(feature = "graphics-observer")]
                psx_player::graphics_observer::service(&self.root, true);
            }
            MainEvent::SaveState { .. } => {
                self.checkpoint_interruption()?;
            }
            MainEvent::LowMemory => {
                self.flush()?;
            }
            MainEvent::Destroy => {
                self.destroyed = true;
                self.interrupt()?;
                self.checkpoint()?;
            }
            _ => {}
        }
        Ok(())
    }
    fn resize(&mut self, app: &AndroidApp) -> Result<(), String> {
        self.speaker_press.cancel();
        self.time_press.cancel();
        let previous_game = self.layout.game;
        let previous = (
            self.graphics.width,
            self.graphics.height,
            self.graphics.content.x,
            self.graphics.content.y,
            self.graphics.content.w,
            self.graphics.content.h,
            self.density,
        );
        self.graphics.resize()?;
        let fullscreen = self.fullscreen_insets.filter(|_| {
            self.activity.foreground
                && !self.picker
                && (self.core.is_none() || self.activity.audio_focus)
        });
        let content = safe_content(
            [self.graphics.width, self.graphics.height],
            self.inset_size,
            fullscreen.unwrap_or(self.insets),
        );
        self.graphics.content_rect(
            content.x as i32,
            content.y as i32,
            (content.x + content.w) as i32,
            (content.y + content.h) as i32,
        );
        let (w, h) = self.dimensions();
        if self
            .editor
            .as_ref()
            .is_some_and(|e| e.width != w || e.height != h)
        {
            self.editor = None;
            self.screen = Screen::Controllers;
            self.refresh_menu(app);
        }
        let preset = self.controller_settings.preset(w, h);
        let cutouts = self.cutout_rects();
        let matching = self.inset_size == [self.graphics.width, self.graphics.height];
        let picture = safe_content(
            [self.graphics.width, self.graphics.height],
            self.inset_size,
            self.picture_insets
                .filter(|_| fullscreen.is_some())
                .unwrap_or(fullscreen.unwrap_or(self.insets)),
        );
        let picture = Rect::new(
            (picture.x - content.x) / self.density,
            (picture.y - content.y) / self.density,
            picture.w / self.density,
            picture.h / self.density,
        );
        let reusable = (-picture.y).max(0.);
        let covered = if matching {
            ((self.status_bar_top as f32 - content.y) / self.density - picture.y).max(0.)
        } else {
            0.
        };
        self.layout = PlayerViewport {
            controls: [w, h],
            picture,
            reusable_top: reusable,
            covered_top: covered,
        }
        .layout(&preset, &cutouts);
        if let Some(editor) = &mut self.editor {
            editor.set_picture_viewport(reusable, covered, picture);
        }
        if let Some(menu) = &mut self.menu {
            menu.cutouts = cutouts;
            if menu.anchor.is_some() {
                menu.anchor = Some(if self.screen == Screen::Timescale {
                    self.layout.speed
                } else {
                    self.layout.mute
                });
            }
            let panel = menu.surface(w, h);
            menu.description_lines = self
                .graphics
                .menu_description(&menu.description, Menu::column(panel.w).w);
            menu.cancel();
            let panel = menu.surface(w, h);
            menu.clamp_scroll(panel.w, panel.h);
        }
        if matches!(self.screen, Screen::License(_)) {
            self.refresh_menu(app);
        }
        let content = self.graphics.content;
        if !previous_game.same_bounds(self.layout.game)
            || previous
                != (
                    self.graphics.width,
                    self.graphics.height,
                    content.x,
                    content.y,
                    content.w,
                    content.h,
                    self.density,
                )
        {
            log::info!(
                "Display resized: surface={}x{} content={}x{} at {},{} density={} picture={:.1}x{:.1} at {:.1},{:.1}",
                self.graphics.width,
                self.graphics.height,
                content.w,
                content.h,
                content.x,
                content.y,
                self.density,
                self.layout.game.w * self.density,
                self.layout.game.h * self.density,
                self.graphics.content.x + self.layout.game.x * self.density,
                self.graphics.content.y + self.layout.game.y * self.density
            );
        }
        Ok(())
    }
    fn input(&mut self, app: &AndroidApp) {
        let Ok(mut events) = app.input_events_iter() else {
            return;
        };
        let mut action = None;
        while events.next(|event| match event {
            InputEvent::MotionEvent(motion) => {
                self.dirty = true;
                let pointer = motion.pointer_at_index(motion.pointer_index());
                let x = (pointer.x() - self.graphics.content.x) / self.density;
                let y = (pointer.y() - self.graphics.content.y) / self.density;
                if motion.action() == MotionAction::Cancel {
                    self.speaker_press.cancel();
                    self.time_press.cancel();
                }
                if motion.action() == MotionAction::Move {
                    for p in motion.pointers() {
                        self.speaker_press.motion(
                            p.pointer_id(),
                            (p.x() - self.graphics.content.x) / self.density,
                            (p.y() - self.graphics.content.y) / self.density,
                        );
                        self.time_press.motion(
                            p.pointer_id(),
                            (p.x() - self.graphics.content.x) / self.density,
                            (p.y() - self.graphics.content.y) / self.density,
                        );
                    }
                }
                if matches!(motion.action(), MotionAction::Up | MotionAction::PointerUp)
                    && self.speaker_press.owns(pointer.pointer_id())
                {
                    if let Some(press) =
                        self.speaker_press
                            .release(pointer.pointer_id(), x, y, Instant::now())
                    {
                        action = Some(match press {
                            PressAction::Tap => Action::ToggleMute,
                            PressAction::Hold => Action::Sound,
                        });
                        self.platform.haptic(app);
                    }
                    return InputStatus::Handled;
                }
                if matches!(motion.action(), MotionAction::Up | MotionAction::PointerUp)
                    && self.time_press.owns(pointer.pointer_id())
                {
                    if let Some(press) =
                        self.time_press
                            .release(pointer.pointer_id(), x, y, Instant::now())
                    {
                        action = Some(match press {
                            PressAction::Tap => Action::ToggleTimescale,
                            PressAction::Hold => Action::Timescale,
                        });
                        self.platform.haptic(app);
                    }
                    return InputStatus::Handled;
                }
                if self.editor.is_none()
                    && self.core.is_some()
                    && matches!(
                        motion.action(),
                        MotionAction::Down | MotionAction::PointerDown
                    )
                {
                    let pointer = motion.pointer_at_index(motion.pointer_index());
                    let x = (pointer.x() - self.graphics.content.x) / self.density;
                    let y = (pointer.y() - self.graphics.content.y) / self.density;
                    let toolbar = if self.layout.pause.contains(x, y) {
                        Some(if self.menu.is_some() && self.screen == Screen::Settings {
                            Action::Resume
                        } else {
                            Action::Settings
                        })
                    } else if self.layout.playback.contains(x, y) {
                        Some(Action::TogglePause)
                    } else if self.layout.speed.contains(x, y) {
                        self.time_press
                            .begin(pointer.pointer_id(), x, y, Instant::now());
                        return InputStatus::Handled;
                    } else if self.layout.mute.contains(x, y) {
                        self.speaker_press
                            .begin(pointer.pointer_id(), x, y, Instant::now());
                        return InputStatus::Handled;
                    } else {
                        None
                    };
                    if let Some(toolbar) = toolbar {
                        action = Some(toolbar);
                        self.platform.haptic(app);
                        return InputStatus::Handled;
                    }
                }
                let mut feedback = false;
                if let Some(editor) = &mut self.editor {
                    let pointer = motion.pointer_at_index(motion.pointer_index());
                    let x = (pointer.x() - self.graphics.content.x) / self.density;
                    let y = (pointer.y() - self.graphics.content.y) / self.density;
                    match motion.action() {
                        MotionAction::Down => editor.down(pointer.pointer_id(), x, y),
                        MotionAction::Move => {
                            for pointer in motion.pointers() {
                                editor.motion(
                                    pointer.pointer_id(),
                                    (pointer.x() - self.graphics.content.x) / self.density,
                                    (pointer.y() - self.graphics.content.y) / self.density,
                                );
                            }
                        }
                        MotionAction::Up | MotionAction::PointerUp => {
                            action =
                                editor
                                    .up(pointer.pointer_id(), x, y)
                                    .map(|result| match result {
                                        EditResult::Done => Action::SaveControls,
                                        EditResult::Cancel => Action::CancelEdit,
                                    });
                        }
                        MotionAction::Cancel => editor.cancel_touch(),
                        _ => {}
                    }
                } else if let Some(menu) = &mut self.menu {
                    let pointer = motion.pointer_at_index(motion.pointer_index());
                    let panel = menu.surface(
                        self.graphics.content.w / self.density,
                        self.graphics.content.h / self.density,
                    );
                    let x = (pointer.x() - self.graphics.content.x) / self.density - panel.x;
                    let y = (pointer.y() - self.graphics.content.y) / self.density - panel.y;
                    let (w, h) = (panel.w, panel.h);
                    match motion.action() {
                        MotionAction::Down => menu.down(pointer.pointer_id(), x, y, w, h),
                        MotionAction::Move => {
                            for pointer in motion.pointers() {
                                menu.motion(
                                    pointer.pointer_id(),
                                    (pointer.x() - self.graphics.content.x) / self.density
                                        - panel.x,
                                    (pointer.y() - self.graphics.content.y) / self.density
                                        - panel.y,
                                    w,
                                    h,
                                );
                            }
                        }
                        MotionAction::Up | MotionAction::PointerUp => {
                            if let Some(index) = menu.up(pointer.pointer_id(), x, y, w, h) {
                                action = if index == usize::MAX {
                                    Some(self.back_action.clone())
                                } else if index == usize::MAX - 1 {
                                    Some(Action::Back)
                                } else if index == usize::MAX - 2 {
                                    Some(Action::Resume)
                                } else if index >= Menu::SECONDARY {
                                    Some(Action::GameActions(index - Menu::SECONDARY))
                                } else {
                                    self.actions.get(index).cloned()
                                };
                                feedback = action.is_some();
                            }
                        }
                        MotionAction::Cancel => menu.cancel(),
                        _ => {}
                    }
                    if let Some((_, value)) = menu.take_slider_change() {
                        action = Some(if self.screen == Screen::Timescale {
                            Action::SetTimescale(value)
                        } else {
                            Action::SetVolume(value)
                        });
                    }
                } else {
                    match motion.action() {
                        MotionAction::Cancel => self.touches.cancel(),
                        MotionAction::Up | MotionAction::PointerUp => self
                            .touches
                            .release(motion.pointer_at_index(motion.pointer_index()).pointer_id()),
                        MotionAction::Down | MotionAction::PointerDown | MotionAction::Move => {
                            let new_pointer = matches!(
                                motion.action(),
                                MotionAction::Down | MotionAction::PointerDown
                            )
                            .then(|| motion.pointer_at_index(motion.pointer_index()).pointer_id());
                            for pointer in motion.pointers() {
                                if self.speaker_press.owns(pointer.pointer_id())
                                    || self.time_press.owns(pointer.pointer_id())
                                {
                                    continue;
                                }
                                let x = (pointer.x() - self.graphics.content.x) / self.density;
                                let y = (pointer.y() - self.graphics.content.y) / self.density;
                                if self.activity.can_run() {
                                    let pressed = if new_pointer == Some(pointer.pointer_id()) {
                                        let (w, h) = self.dimensions();
                                        let preset = self.controller_settings.preset(w, h);
                                        self.touches.down_with_stick_click(
                                            pointer.pointer_id(),
                                            x,
                                            y,
                                            &self.layout,
                                            preset.follow_thumb,
                                            preset.stick_click_gesture,
                                        )
                                    } else {
                                        self.touches.motion(
                                            pointer.pointer_id(),
                                            x,
                                            y,
                                            &self.layout,
                                        )
                                    };
                                    let pressed = pressed & !self.phone_input.mask();
                                    self.visuals.trigger(pressed);
                                    feedback |= pressed != 0;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if feedback {
                    self.platform.haptic(app);
                }
                InputStatus::Handled
            }
            InputEvent::KeyEvent(key) if key.key_code() == Keycode::Back => {
                if key.action() == KeyAction::Up {
                    action = Some(Action::Back);
                }
                InputStatus::Handled
            }
            InputEvent::KeyEvent(key) => {
                let phone_key = match key.key_code() {
                    Keycode::VolumeUp => PhoneKey::VolumeUp,
                    Keycode::VolumeDown => PhoneKey::VolumeDown,
                    _ => return InputStatus::Unhandled,
                };
                let handled = match key.action() {
                    KeyAction::Down => {
                        let playing = self.core.is_some()
                            && self.activity.can_run()
                            && self.activity.foreground
                            && self.activity.focused
                            && self.menu.is_none()
                            && self.editor.is_none()
                            && !self.picker
                            && self.import.is_none()
                            && self.work.is_none();
                        self.phone_input
                            .down(
                                key.device_id(),
                                phone_key,
                                key.repeat_count() != 0,
                                self.phone_bindings,
                                playing,
                            )
                            .map(|pressed| {
                                let pressed = pressed & !self.touches.mask();
                                self.visuals.trigger(pressed);
                                if pressed != 0 {
                                    self.platform.haptic(app);
                                }
                            })
                            .is_some()
                    }
                    // Multiple/unknown actions release our own source too.
                    _ => self.phone_input.up(key.device_id(), phone_key),
                };
                if handled {
                    self.dirty = true;
                    InputStatus::Handled
                } else {
                    InputStatus::Unhandled
                }
            }
            _ => InputStatus::Unhandled,
        }) {}
        if let Some(action) = action {
            if let Err(error) = self.action(app, action) {
                self.fail(error);
            }
        }
    }

    fn platform_event(&mut self, app: &AndroidApp, event: Event) {
        self.dirty = true;
        match event {
            Event::RefreshInsets => self.platform.insets(app),
            Event::Documents(request, intent) => {
                self.picker = false;
                if request == PICK_STORAGE {
                    self.platform.storage(app, false);
                } else if let Some(intent) = intent {
                    self.activity.loading = true;
                    self.import = Some(self.platform.import(
                        self.root.clone(),
                        request,
                        intent,
                        self.relink.take(),
                    ));
                    self.message = "Copying your selected files into private storage. Large discs can take a moment.".into();
                    self.refresh_menu(app);
                } else {
                    self.relink = None;
                }
            }
            Event::Storage(roots) => {
                self.picker = false;
                if let Some(roots) = roots {
                    self.activity.loading = true;
                    self.cancel_work = Arc::new(AtomicBool::new(false));
                    let cancel = self.cancel_work.clone();
                    let (tx, rx) = mpsc::sync_channel(1);
                    self.work = Some(rx);
                    std::thread::spawn(move || {
                        let _ = tx.send(Ok(Work::Scanned(library::scan(roots, &cancel))));
                    });
                    self.navigate(app, Screen::Scanning);
                } else {
                    self.navigate(app, Screen::Access);
                }
            }
            Event::ScanUnavailable => {
                self.picker = false;
                self.scan_available = false;
                self.navigate(app, Screen::Access);
            }
            Event::Focus(focus) => {
                let changed = self.activity.audio_focus != (focus == 1);
                self.activity.audio_focus = focus == 1;
                if focus != 1 {
                    if focus == -1
                        && self.activity.foreground
                        && !self.picker
                        && self.menu.is_none()
                    {
                        self.activity.user_paused = true;
                    }
                    if let Err(error) = self.interrupt() {
                        self.fail(error);
                    }
                }
                // Persistent call indicators can remain over the camera
                // margin despite hidden-bar reports. Keep quick controls in
                // safe content throughout an audio interruption.
                if changed {
                    self.touches.cancel();
                    if let Err(error) = self.resize(app) {
                        self.fail(error);
                    }
                }
            }
            Event::LaunchError(message) => {
                self.restore_pending = false;
                self.message = message;
                self.navigate(app, Screen::Recover(None));
            }
            Event::Notice(message) => {
                self.message = message;
                self.dirty = true;
                self.refresh_menu(app);
            }
            Event::Error(error) => {
                self.picker = false;
                self.fail(error);
            }
            Event::Background(ready) => {
                self.background_ready = ready;
                self.activity.background_playback = ready && self.playback.run_in_background;
            }
            Event::PauseBackground => {
                self.activity.user_paused = true;
                self.checkpoint_dirty = self.core.is_some();
                if let Err(error) = self.interrupt().and_then(|_| self.checkpoint()) {
                    self.fail(error);
                }
            }
            Event::Insets {
                size,
                density,
                edges,
                fullscreen_edges,
                picture_edges,
                status_bar_top,
                cutouts,
                hidden_bars_visible,
            } => {
                if hidden_bars_visible
                    && self.activity.foreground
                    && self.activity.focused
                    && !self.picker
                {
                    self.rehide_bars_at
                        .get_or_insert_with(|| Instant::now() + Duration::from_millis(1200));
                } else {
                    self.rehide_bars_at = None;
                }
                if edges != self.insets
                    || fullscreen_edges != self.fullscreen_insets
                    || picture_edges != self.picture_insets
                    || status_bar_top != self.status_bar_top
                    || size != self.inset_size
                    || density != self.density
                    || cutouts != self.cutouts
                {
                    self.insets = edges;
                    self.fullscreen_insets = fullscreen_edges;
                    self.picture_insets = picture_edges;
                    self.status_bar_top = status_bar_top;
                    self.inset_size = size;
                    if (0.5..=6.).contains(&density) {
                        self.density = density;
                    }
                    self.cutouts = cutouts;
                    self.touches.cancel();
                    if let Err(error) = self.resize(app) {
                        self.fail(error);
                    }
                }
            }
        }
    }

    fn navigate(&mut self, app: &AndroidApp, screen: Screen) {
        self.speaker_press.cancel();
        self.time_press.cancel();
        self.closing_menu = None;
        self.screen = screen;
        self.touches.cancel();
        self.phone_input.cancel();
        if let Some(core) = &mut self.core {
            core.input(0, [0; 4], false);
        }
        if let Err(error) = self.flush() {
            self.fail(error);
        }
        self.refresh_menu(app);
    }

    fn action(&mut self, app: &AndroidApp, action: Action) -> Result<(), String> {
        if self.work.is_some() && !matches!(action, Action::CancelWork | Action::Back) {
            return Ok(());
        }
        if self.import.is_some() {
            return Ok(());
        }
        if matches!(action, Action::Resume | Action::TogglePause) {
            self.ensure_session_available()?;
        }
        match action {
            Action::Scan => {
                if self.work.is_some() || self.import.is_some() {
                    return Ok(());
                }
                self.platform.storage(app, false);
            }
            Action::GrantStorage => {
                self.picker = true;
                self.platform.storage(app, true);
            }
            Action::Browse | Action::Firmware | Action::Ps2Firmware => {
                self.picker = true;
                self.platform.pick(
                    app,
                    if matches!(action, Action::Browse) {
                        PICK_GAME
                    } else if matches!(action, Action::Ps2Firmware) {
                        PICK_PS2_FIRMWARE
                    } else {
                        PICK_FIRMWARE
                    },
                );
            }
            Action::ChooseGame(index) => {
                let game = self
                    .library
                    .games
                    .get(index)
                    .ok_or("The game is no longer in the library.")?
                    .clone();
                self.open_game(app, Launch::OpenGame(GameId::parse(&game.id)?))?;
            }
            Action::GameActions(index) => self.navigate(app, Screen::GameActions(index)),
            Action::Relink(index) => {
                let game = self
                    .library
                    .games
                    .get(index)
                    .ok_or("This library entry is unavailable.")?
                    .clone();
                let missing = game
                    .disc_list()
                    .iter()
                    .position(|d| fs::File::open(&d.path).is_err())
                    .unwrap_or(game.last_disc);
                self.relink = Some((game, missing));
                self.picker = true;
                self.platform.pick(app, PICK_GAME);
            }
            Action::PinGame(id) => {
                let game = self
                    .library
                    .games
                    .iter()
                    .find(|g| g.id == id.as_str())
                    .ok_or("The game is no longer in the library.")?
                    .clone();
                self.platform.pin_game(app, game, &self.root);
            }
            Action::ChooseDisc(index) => {
                let Screen::Discs(game_index) = self.screen else {
                    return Ok(());
                };
                let mut game = self
                    .library
                    .games
                    .get(game_index)
                    .ok_or("The game is no longer in the library.")?
                    .clone();
                if self
                    .game
                    .as_ref()
                    .is_some_and(|active| active.id == game.id)
                {
                    self.interrupt()?;
                    self.core
                        .as_mut()
                        .ok_or("The game is closed.")?
                        .change_disc(index as u32)?;
                    game.last_disc = index;
                    self.game = Some(game.clone());
                    self.library.put(game.clone());
                    self.library.save(&self.root).map_err(|e| e.to_string())?;
                    self.message = format!("Disc {} inserted. Resume when ready.", index + 1);
                    self.navigate(app, Screen::Settings);
                } else {
                    game.last_disc = index;
                    let id = GameId::parse(&game.id)?;
                    self.library.put(game);
                    self.open_game(app, Launch::OpenGame(id))?;
                }
            }
            Action::ChooseSystem(index, system) => {
                let game = self
                    .library
                    .games
                    .get_mut(index)
                    .ok_or("The game is no longer in the library.")?;
                game.system = system;
                let id = GameId::parse(&game.id)?;
                self.library.save(&self.root).map_err(|e| e.to_string())?;
                self.open_game(app, Launch::OpenGame(id))?;
            }
            Action::ToggleGame(index) => {
                if let Some((_, checked)) = self.scan_games.get_mut(index) {
                    *checked = !*checked;
                }
                let scroll = self.menu.as_ref().map_or(0., |m| m.scroll);
                self.refresh_menu(app);
                if let Some(menu) = &mut self.menu {
                    menu.scroll = scroll;
                }
            }
            Action::SelectAll => {
                let checked = !self.scan_games.iter().all(|(_, c)| *c);
                for (_, value) in &mut self.scan_games {
                    *value = checked;
                }
                self.refresh_menu(app);
            }
            Action::AddSelected => {
                let selected: Vec<_> = self
                    .scan_games
                    .iter()
                    .filter(|(_, c)| *c)
                    .map(|(g, _)| g.clone())
                    .collect();
                if selected.is_empty() {
                    return Ok(());
                }
                self.activity.loading = true;
                self.cancel_work = Arc::new(AtomicBool::new(false));
                let cancel = self.cancel_work.clone();
                let root = self.root.clone();
                let (tx, rx) = mpsc::sync_channel(1);
                self.work = Some(rx);
                std::thread::spawn(move || {
                    let result = library::add(&root, selected, &cancel)
                        .map(Work::Added)
                        .map_err(|e| e.to_string());
                    let _ = tx.send(result);
                });
                self.navigate(app, Screen::Adding);
            }
            Action::CancelWork => {
                self.cancel_work.store(true, Ordering::Relaxed);
                self.message = "Stopping...".into();
                self.refresh_menu(app);
            }
            Action::Resume => {
                self.closing_menu = self.menu.take();
                self.platform.focus(app, true);
                self.dirty = true;
            }
            Action::Settings => self.navigate(app, Screen::Settings),
            Action::Home => {
                self.message.clear();
                self.navigate(app, Screen::Home);
            }
            Action::Discs => {
                if let Some(game) = &self.game {
                    if let Some(index) = self.library.games.iter().position(|g| g.id == game.id) {
                        self.navigate(app, Screen::Discs(index));
                    }
                }
            }
            Action::Controllers => self.navigate(app, Screen::Controllers),
            Action::ToggleGameController => {
                let id = self
                    .controller_id()?
                    .ok_or("Open a game to customize its controller.")?;
                self.controller_profiles
                    .set_custom(
                        &self.root,
                        &id,
                        !self.controller_profiles.custom(&id),
                        &self.controller_settings,
                    )
                    .map_err(|e| format!("Controller profile could not be saved: {e}"))?;
                self.reload_controls(app)?;
                self.refresh_menu(app);
            }
            Action::PhoneButtons => self.navigate(app, Screen::PhoneButtons),
            Action::BindPhoneKey(key) => self.navigate(app, Screen::PhoneBinding(key)),
            Action::SetPhoneKey(key, button) => {
                let id = self.controller_id()?;
                self.controller_profiles
                    .assign(&self.root, id.as_ref(), key, button)
                    .map_err(|e| format!("Phone buttons could not be saved: {e}"))?;
                self.phone_bindings = self
                    .controller_profiles
                    .bindings(id.as_ref(), self.controller_settings.analog());
                self.navigate(app, Screen::PhoneButtons);
            }
            Action::ResetPhoneKeys => {
                let id = self.controller_id()?;
                self.controller_profiles
                    .set_bindings(&self.root, id.as_ref(), Bindings::default())
                    .map_err(|e| format!("Phone buttons could not be saved: {e}"))?;
                self.phone_bindings = Bindings::default();
                self.navigate(app, Screen::PhoneButtons);
            }
            Action::Sound => {
                if self.screen == Screen::Sound && self.menu.is_some() {
                    self.action(app, Action::Resume)?;
                } else {
                    self.navigate(app, Screen::Sound);
                }
            }
            Action::Timescale => {
                if self.screen == Screen::Timescale && self.menu.is_some() {
                    self.action(app, Action::Resume)?;
                } else {
                    self.navigate(app, Screen::Timescale);
                }
            }
            Action::TogglePause => {
                self.frames = 0;
                self.run_time = Duration::ZERO;
                self.last_report = Instant::now();
                if self.activity.user_paused
                    || self.activity.time_paused
                    || !self.activity.audio_focus
                {
                    self.activity.user_paused = false;
                    if self.activity.time_paused {
                        self.playback.timescale_enabled = false;
                        self.timescale_changed();
                    }
                    if !self.activity.audio_focus {
                        self.platform.focus(app, true);
                    }
                    self.next_frame = Instant::now();
                } else {
                    self.activity.user_paused = true;
                    self.cancel_input();
                }
                self.queue
                    .set_paused(self.activity.user_paused || self.activity.time_paused);
                self.checkpoint_dirty = self.core.is_some();
                self.checkpoint_after_present = true;
                self.next_present = Instant::now();
                self.dirty = true;
                if self.menu.is_some() {
                    self.refresh_menu(app);
                }
            }
            Action::ToggleMute => {
                self.playback.toggle_mute();
                self.save_playback()?;
                if self.menu.is_some() {
                    self.refresh_menu(app);
                }
                self.dirty = true;
            }
            Action::SetVolume(value) => {
                self.playback.set_volume(value);
                self.queue.set_volume(self.playback.output_volume());
                self.playback_changed = Some(Instant::now());
                // Keep the captured slider and its pointer alive across moves.
                if let Some(menu) = &mut self.menu {
                    if let Some(row) = menu.rows.get_mut(1) {
                        row.title = if value == 0 { "Unmute" } else { "Mute" }.into();
                    }
                }
                self.dirty = true;
            }
            Action::ToggleTimescale => {
                self.playback.toggle_timescale();
                self.timescale_changed();
                if self.menu.is_some() {
                    self.refresh_menu(app);
                }
            }
            Action::SetTimescale(value) => {
                self.playback.set_timescale(value);
                self.timescale_changed();
                // Retain the slider pointer, updating only its displayed value.
                if let Some(menu) = &mut self.menu {
                    if let Some(row) = menu.rows.get_mut(0) {
                        row.slider_label = Some(self.playback.speed.label());
                    }
                    if let Some(row) = menu.rows.get_mut(1) {
                        row.title = "Normal time".into();
                    }
                }
            }
            Action::ToggleFollowThumb => {
                let (w, h) = self.dimensions();
                let mut preset = self.controller_settings.preset(w, h);
                preset.follow_thumb = !preset.follow_thumb;
                self.save_control_preset(w, h, preset)?;
                self.resize(app)?;
                self.refresh_menu(app);
            }
            Action::ToggleStickClick => {
                let (w, h) = self.dimensions();
                let mut preset = self.controller_settings.preset(w, h);
                preset.stick_click_gesture = !preset.stick_click_gesture;
                self.save_control_preset(w, h, preset)?;
                self.touches.cancel();
                self.refresh_menu(app);
            }
            Action::ToggleOverlap => {
                let (w, h) = self.dimensions();
                let mut preset = self.controller_settings.preset(w, h);
                preset.allow_overlap = !preset.allow_overlap;
                self.save_control_preset(w, h, preset)?;
                self.resize(app)?;
                self.refresh_menu(app);
            }
            Action::ResetControls => {
                let (w, h) = self.dimensions();
                self.save_control_preset(w, h, Default::default())?;
                self.resize(app)?;
                self.refresh_menu(app);
            }
            Action::EditControls => {
                self.screen = Screen::Editing;
                let (w, h) = self.dimensions();
                self.editor = Some(Editor::with_cutouts(
                    self.controller_settings.preset(w, h),
                    w,
                    h,
                    self.cutout_rects(),
                ));
                self.menu = None;
                self.closing_menu = None;
                self.resize(app)?;
                self.dirty = true;
            }
            Action::SaveControls => {
                if let Some((w, h, preset)) = self
                    .editor
                    .as_ref()
                    .map(|e| (e.width, e.height, e.preset.clone()))
                {
                    self.save_control_preset(w, h, preset)?;
                    self.editor = None;
                    self.resize(app)?;
                }
                self.navigate(app, Screen::Controllers);
            }
            Action::CancelEdit => {
                self.editor = None;
                self.navigate(app, Screen::Controllers);
            }
            Action::Close => {
                self.close()?;
                self.message = "Session saved. Choose the game to resume.".into();
                self.navigate(app, Screen::Home);
            }
            Action::Restart => {
                if let Some(game) = self.game.clone() {
                    self.close()?;
                    resume::discard(&self.root, &game).map_err(|e| e.to_string())?;
                    self.open_game(app, Launch::OpenGame(GameId::parse(&game.id)?))?;
                }
            }
            Action::ToggleBackground => {
                self.playback.run_in_background = !self.playback.run_in_background;
                self.save_playback()?;
                if self.playback.run_in_background {
                    self.platform.background_permission(app);
                }
                self.refresh_menu(app);
            }
            Action::About => self.navigate(app, Screen::About),
            Action::License(path) => self.navigate(app, Screen::License(path)),
            Action::Source => self.platform.open_source(app),
            Action::Back => match self.screen {
                Screen::Scanning | Screen::Adding => return self.action(app, Action::CancelWork),
                Screen::License(_) => self.navigate(app, Screen::About),
                Screen::Editing => return self.action(app, Action::CancelEdit),
                Screen::Controllers => self.navigate(app, Screen::Settings),
                Screen::PhoneButtons => self.navigate(app, Screen::Controllers),
                Screen::PhoneBinding(_) => self.navigate(app, Screen::PhoneButtons),
                Screen::Sound | Screen::Timescale => return self.action(app, Action::Resume),
                Screen::Settings if self.core.is_some() => return self.action(app, Action::Resume),
                Screen::Home if self.core.is_none() => self.platform.finish(app),
                _ if self.core.is_some() && self.menu.is_none() => {
                    self.navigate(app, Screen::Settings)
                }
                _ if self.core.is_some() => self.navigate(app, Screen::Settings),
                _ => self.navigate(app, Screen::Home),
            },
        }
        Ok(())
    }

    fn dimensions(&self) -> (f32, f32) {
        (
            self.graphics.content.w / self.density,
            self.graphics.content.h / self.density,
        )
    }
    fn menu_text_width(&self) -> f32 {
        let (w, h) = self.dimensions();
        let mut menu = Menu::new(String::new(), String::new(), Vec::new(), "Back");
        menu.over_game = self.core.is_some();
        Menu::column(menu.surface(w, h).w).w - 8.
    }
    fn controller_id(&self) -> Result<Option<GameId>, String> {
        self.game
            .as_ref()
            .map(|g| GameId::parse(&g.id))
            .transpose()
            .map_err(|e| e.to_string())
    }
    fn custom_controller(&self) -> bool {
        self.controller_id()
            .ok()
            .flatten()
            .is_some_and(|id| self.controller_profiles.custom(&id))
    }
    fn reload_controls(&mut self, app: &AndroidApp) -> Result<(), String> {
        let id = self.controller_id()?;
        let analog = self.controller_settings.analog();
        self.controller_settings = self
            .controller_profiles
            .controls(&self.root, id.as_ref(), analog)
            .map_err(|e| e.to_string())?;
        self.phone_bindings = self.controller_profiles.bindings(id.as_ref(), analog);
        self.cancel_input();
        self.resize(app)
    }
    fn save_control_preset(&mut self, w: f32, h: f32, preset: Preset) -> Result<(), String> {
        let id = self.controller_id()?;
        let mut controls = self.controller_settings.clone();
        controls.set(w, h, preset);
        self.controller_profiles
            .save_controls(&self.root, id.as_ref(), &controls)
            .map_err(|e| format!("Controller layout could not be saved: {e}"))?;
        self.controller_settings = controls;
        Ok(())
    }

    fn cutout_rects(&self) -> Vec<Rect> {
        if self.inset_size != [self.graphics.width, self.graphics.height] {
            return Vec::new();
        }
        self.cutouts
            .iter()
            .map(|r| {
                Rect::new(
                    (r[0] as f32 - self.graphics.content.x) / self.density,
                    (r[1] as f32 - self.graphics.content.y) / self.density,
                    (r[2] - r[0]) as f32 / self.density,
                    (r[3] - r[1]) as f32 / self.density,
                )
            })
            .collect()
    }

    fn sync_background(&mut self, app: &AndroidApp) {
        let want = self.playback.run_in_background
            && self.game.as_ref().is_some_and(|g| g.system == System::Ps1)
            && self.core.is_some()
            && !self.activity.user_paused
            && !self.activity.time_paused
            && !self.destroyed;
        if want != self.background_requested && (!want || self.activity.foreground) {
            self.background_requested = want;
            if !want {
                self.background_ready = false;
            }
            self.platform.background(app, want);
        }
        self.activity.background_playback = want && self.background_ready;
    }

    fn save_playback(&mut self) -> Result<(), String> {
        self.queue.set_volume(self.playback.output_volume());
        self.playback.save(&self.root).map_err(|e| e.to_string())?;
        self.playback_changed = None;
        Ok(())
    }
    fn timescale_changed(&mut self) {
        let speed = self.playback.effective_speed();
        let paused = speed.is_paused();
        if paused != self.activity.time_paused {
            self.activity.time_paused = paused;
            self.touches.cancel();
            self.phone_input.cancel();
            if let Some(core) = &mut self.core {
                core.input(0, [0; 4], false);
            }
            self.checkpoint_dirty = self.core.is_some();
            self.checkpoint_after_present = self.core.is_some();
        }
        self.queue.set_speed(speed);
        self.queue.set_paused(self.activity.user_paused || paused);
        self.playback_changed = Some(Instant::now());
        self.next_frame = Instant::now();
        self.next_present = Instant::now();
        self.dirty = true;
    }

    fn refresh_menu(&mut self, app: &AndroidApp) {
        let mut rows = Vec::new();
        let mut actions = Vec::new();
        let mut add = |action, title: String, detail: String| {
            actions.push(action);
            rows.push(Row::action(title, detail));
        };
        let (title, description, back, back_action) = match self.screen {
            Screen::Home => {
                for (i, game) in self.library.games.iter().enumerate() {
                    let discs = game.disc_list();
                    add(
                        Action::ChooseGame(i),
                        game.title.clone(),
                        if discs.len() > 1 {
                            format!("{} · {} discs", game.system.label(), discs.len())
                        } else {
                            game.system.label().into()
                        },
                    );
                }
                add(
                    Action::Settings,
                    "Settings".into(),
                    "Controllers, sound, source and licenses".into(),
                );
                (
                    "Games".into(),
                    if self.library.games.is_empty() {
                        "Add games to scan storage or choose files.".into()
                    } else {
                        format!(
                            "{} games · {} discs",
                            self.library.games.len(),
                            self.library
                                .games
                                .iter()
                                .map(|g| g.disc_list().len())
                                .sum::<usize>()
                        )
                    },
                    "Add games".into(),
                    Action::Scan,
                )
            }
            Screen::Settings => {
                if self
                    .core
                    .as_ref()
                    .and_then(Core::disc_status)
                    .is_some_and(|(_, count)| count > 1)
                {
                    add(
                        Action::Discs,
                        "Change disc".into(),
                        "Keep the current session and memory card".into(),
                    );
                }
                add(
                    Action::Home,
                    "Game library".into(),
                    "Choose another game or add games".into(),
                );
                add(
                    Action::Controllers,
                    "Controller settings".into(),
                    "Layout, size and optional overlap".into(),
                );
                if self.game.as_ref().is_none_or(|g| g.system == System::Ps1) {
                    add(
                        Action::ToggleBackground,
                        "Run in background".into(),
                        "PS1 · keep playing when minimized · off pauses automatically".into(),
                    );
                }
                add(
                    Action::Firmware,
                    "PS1 firmware".into(),
                    "Optional · choose your own compatible dump".into(),
                );
                add(
                    Action::Ps2Firmware,
                    "PS2 firmware".into(),
                    "Required · choose your compatible dump".into(),
                );
                add(
                    Action::About,
                    "About psx".into(),
                    "Source, licenses and privacy".into(),
                );
                if self.core.is_some() {
                    add(
                        Action::Restart,
                        "Restart game".into(),
                        "Boot the disc again; keep the memory card".into(),
                    );
                    add(
                        Action::Close,
                        "Close game".into(),
                        "Save this session and return to the library".into(),
                    );
                }
                (
                    "Settings".into(),
                    if let Some(game) = &self.game {
                        if self.activity.user_paused || self.activity.time_paused {
                            format!("Paused · {}", game.title)
                        } else {
                            game.title.clone()
                        }
                    } else {
                        String::new()
                    },
                    if self.core.is_some() {
                        "Back to game".into()
                    } else {
                        "Back to games".into()
                    },
                    if self.core.is_some() {
                        Action::Resume
                    } else {
                        Action::Home
                    },
                )
            }
            Screen::Sound => {
                add(
                    Action::SetVolume(self.playback.volume),
                    "Game volume".into(),
                    String::new(),
                );
                add(
                    Action::ToggleMute,
                    if self.playback.output_volume() == 0 {
                        "Unmute"
                    } else {
                        "Mute"
                    }
                    .into(),
                    "Keep your chosen volume for later".into(),
                );
                ("Volume".into(), String::new(), "Back".into(), Action::Back)
            }
            Screen::Timescale => {
                add(
                    Action::SetTimescale(self.playback.speed.slider()),
                    "Time scale".into(),
                    "0 pauses · max uncapped".into(),
                );
                add(
                    Action::ToggleTimescale,
                    if self.playback.timescale_enabled {
                        "Normal time"
                    } else {
                        "Use chosen scale"
                    }
                    .into(),
                    "Tap the hourglass to switch".into(),
                );
                ("Time".into(), String::new(), "Back".into(), Action::Back)
            }
            Screen::Access => {
                if self.scan_available {
                    add(
                        Action::GrantStorage,
                        "Allow storage access".into(),
                        "Open the system permission screen".into(),
                    );
                }
                add(
                    Action::Browse,
                    "Browse files instead".into(),
                    "Choose a CUE and its tracks, CHD, ISO, PBP or PS-EXE".into(),
                );
                (
                    "Scan storage".into(),
                    if self.scan_available {
                        "Find games on shared and removable storage. Other apps' private data is protected. Files stay on this device.".into()
                    } else {
                        "Whole-storage scanning is unavailable in this build or Android version. Use the system picker to add a game.".into()
                    },
                    "Back".into(),
                    Action::Home,
                )
            }
            Screen::Scanning | Screen::Adding => (
                "Scan storage".into(),
                if self.cancel_work.load(Ordering::Relaxed) {
                    "Stopping...".into()
                } else if self.screen == Screen::Adding {
                    "Adding selected games. Checking identities so existing memory cards stay connected. Large discs can take a moment.".into()
                } else {
                    "Looking for supported PS1 images across accessible storage. Checking headers and CUE tracks; archives need extraction.".into()
                },
                "Cancel".into(),
                Action::CancelWork,
            ),
            Screen::Results => {
                add(
                    Action::SelectAll,
                    if self.scan_games.iter().all(|(_, c)| *c) {
                        "Clear selection".into()
                    } else {
                        "Select all".into()
                    },
                    "Only selected games are added".into(),
                );
                for (i, (game, _)) in self.scan_games.iter().enumerate() {
                    let detail = if game.discs.len() > 1 {
                        format!(
                            "{} discs: {}",
                            game.discs.len(),
                            game.discs
                                .iter()
                                .map(|d| d.number.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    } else {
                        game.discs
                            .first()
                            .map_or(String::new(), |d| d.format.clone())
                    };
                    add(Action::ToggleGame(i), game.title.clone(), detail);
                }
                add(
                    Action::Browse,
                    "Browse files".into(),
                    "Optional · select a game manually".into(),
                );
                (
                    "Choose games".into(),
                    self.message.clone(),
                    format!(
                        "Add selected ({})",
                        self.scan_games.iter().filter(|(_, c)| *c).count()
                    ),
                    Action::AddSelected,
                )
            }
            Screen::Discs(index) => {
                if let Some(game) = self.library.games.get(index) {
                    let running = self.game.as_ref().is_some_and(|g| g.id == game.id);
                    let discs = game.disc_list();
                    let (current, count) = if running {
                        self.core
                            .as_ref()
                            .and_then(Core::disc_status)
                            .map_or((game.last_disc as u32, discs.len() as u32), |s| s)
                    } else {
                        (game.last_disc as u32, discs.len() as u32)
                    };
                    for i in 0..count as usize {
                        add(
                            Action::ChooseDisc(i),
                            discs
                                .get(i)
                                .map_or_else(|| format!("Disc {}", i + 1), |d| d.label.clone()),
                            if running && current as usize == i {
                                "Currently inserted".into()
                            } else if running {
                                "Insert without restarting".into()
                            } else {
                                "Start game with this disc".into()
                            },
                        );
                    }
                    (
                        game.title.clone(),
                        if running {
                            "Change discs when the game asks. The session and card stay open."
                                .into()
                        } else {
                            "Choose a starting disc. All discs share one memory card.".into()
                        },
                        "Back".into(),
                        Action::Back,
                    )
                } else {
                    (
                        "Game unavailable".into(),
                        "Return to the library and scan again.".into(),
                        "Back".into(),
                        Action::Home,
                    )
                }
            }
            Screen::Controllers | Screen::Editing => {
                let (w, h) = self.dimensions();
                let preset = self.controller_settings.preset(w, h);
                if self.game.is_some() {
                    add(
                        Action::ToggleGameController,
                        "Custom controller for this game".into(),
                        if self.custom_controller() {
                            "On · layout and bindings apply only to this game"
                        } else {
                            "Off · use the app default controller"
                        }
                        .into(),
                    );
                }
                add(
                    Action::EditControls,
                    if self.custom_controller() { "Edit this game's layout" } else { "Edit default layout" }.into(),
                    "Edit live; move, resize or mirror groups. Shapes have spacing; Single edits one button"
                        .into(),
                );
                add(
                    Action::ToggleFollowThumb,
                    "Follow thumb".into(),
                    if preset.follow_thumb {
                        "D-pad center follows your thumb; lift to release".into()
                    } else {
                        "Fixed center; keeps tracking until you lift".into()
                    },
                );
                add(
                    Action::ToggleOverlap,
                    "Allow overlap".into(),
                    if preset.allow_overlap {
                        "Controls may cover the game picture".into()
                    } else {
                        "The picture stays clear of every control".into()
                    },
                );
                if preset.analog {
                    add(
                        Action::ToggleStickClick,
                        "Double-tap stick click".into(),
                        "Tap, then hold the stick to press L3/R3; lift to release. Buttons remain available".into(),
                    );
                }
                add(
                    Action::PhoneButtons,
                    "Phone buttons".into(),
                    if self.custom_controller() {
                        "Volume-key assignments for this game"
                    } else {
                        "Default volume-key assignments"
                    }
                    .into(),
                );
                if !preset.is_default_with_cutouts(w, h, &self.cutout_rects()) {
                    add(
                        Action::ResetControls,
                        "Reset to defaults".into(),
                        "Restore this display's controls and layout".into(),
                    );
                }
                (
                    "Controllers".into(),
                    format!(
                        "{} · {}. Layouts are saved separately for tall, square and wide displays.",
                        if self.custom_controller() {
                            "This game"
                        } else {
                            "App default"
                        },
                        ControllerSettings::profile(w, h)
                    ),
                    "Done".into(),
                    Action::Back,
                )
            }
            Screen::PhoneButtons => {
                for key in [PhoneKey::VolumeUp, PhoneKey::VolumeDown] {
                    add(
                        Action::BindPhoneKey(key),
                        key.label().into(),
                        self.phone_bindings
                            .get(key)
                            .map_or("System volume", Button::label)
                            .into(),
                    );
                }
                if self.phone_bindings != Bindings::default() {
                    add(
                        Action::ResetPhoneKeys,
                        "Reset phone buttons".into(),
                        "Use both keys for system volume again".into(),
                    );
                }
                (
                    "Phone buttons".into(),
                    format!(
                        "{} · During play only; menus use system volume. Power stays with Android.",
                        if self.custom_controller() {
                            self.game.as_ref().unwrap().title.as_str()
                        } else {
                            "App default for games without a custom controller"
                        }
                    ),
                    "Back".into(),
                    Action::Back,
                )
            }
            Screen::PhoneBinding(key) => {
                add(
                    Action::SetPhoneKey(key, None),
                    "System volume".into(),
                    "Leave this key unassigned".into(),
                );
                for button in Button::ALL {
                    if !button.requires_sticks() || self.controller_settings.analog() {
                        add(
                            Action::SetPhoneKey(key, Some(button)),
                            button.label().into(),
                            String::new(),
                        );
                    }
                }
                (
                    key.label().into(),
                    "Hold the phone key to hold its controller button; release to let go.".into(),
                    "Back".into(),
                    Action::Back,
                )
            }
            Screen::About => {
                add(
                    Action::Source,
                    "Project source".into(),
                    "github.com/WigglyDon/psx".into(),
                );
                for (title, path) in [
                    ("psx · MIT", "notices/host/LICENSE"),
                    ("SwanStation · GPL", "notices/swanstation/LICENSE"),
                    ("ARMSX2 · GPL", "notices/armsx2/COPYING.GPLv3"),
                    ("OpenBIOS · MIT", "notices/openbios/LICENSE"),
                    ("Noto Sans · SIL OFL", "notices/fonts/NotoSans-LICENSE"),
                    ("Dependency notices", "notices/INDEX.txt"),
                ] {
                    add(
                        Action::License(path),
                        title.into(),
                        "Read bundled license".into(),
                    );
                }
                ("About psx".into(),"Games, firmware and saves stay on this device. No accounts, analytics or uploads.".into(),"Back".into(),Action::Back)
            }
            Screen::License(path) => {
                let body = std::ffi::CString::new(path).ok().and_then(|p|app.asset_manager().open(&p)).and_then(|mut asset|asset.buffer().ok().map(|b|String::from_utf8_lossy(b).into_owned())).unwrap_or_else(||"This notice could not be read. All notices are also packaged in the APK assets.".into());
                let width = self.menu_text_width();
                let paragraphs = self.graphics.text_rows(&body, width);
                for _ in &paragraphs {
                    actions.push(Action::Back);
                }
                rows = paragraphs;
                ("Licenses".into(), path.into(), "Back".into(), Action::Back)
            }
            Screen::Recover(index) => {
                if let Some(index) = index {
                    add(
                        Action::Relink(index),
                        "Locate original disc".into(),
                        "Verify its contents and keep this entry's progress".into(),
                    );
                } else {
                    add(
                        Action::Scan,
                        "Add games".into(),
                        "Find the original game in storage".into(),
                    );
                }
                (
                    "Game unavailable".into(),
                    self.message.clone(),
                    "Library".into(),
                    Action::Home,
                )
            }
            Screen::GameActions(index) => {
                if let Some(game) = self.library.games.get(index) {
                    if let Ok(id) = GameId::parse(&game.id) {
                        add(
                            Action::PinGame(id),
                            "Add to Home Screen".into(),
                            "Open this game directly · confirmation required".into(),
                        );
                    }
                    (
                        game.title.clone(),
                        self.message.clone(),
                        "Back".into(),
                        Action::Home,
                    )
                } else {
                    (
                        "Game unavailable".into(),
                        "Return to the library.".into(),
                        "Back".into(),
                        Action::Home,
                    )
                }
            }
            Screen::Console(index) => {
                for system in [System::Ps1, System::Ps2] {
                    add(
                        Action::ChooseSystem(index, system),
                        system.label().into(),
                        String::new(),
                    );
                }
                (
                    "Choose console".into(),
                    "This container does not identify the console. Choose it once for this game."
                        .into(),
                    "Back".into(),
                    Action::Back,
                )
            }
            Screen::Error => {
                rows = self
                    .graphics
                    .text_rows(&self.message, self.menu_text_width());
                for _ in &rows {
                    actions.push(Action::Back);
                }
                if self
                    .session
                    .as_ref()
                    .is_some_and(|s| s.state == SessionState::Unavailable)
                {
                    rows.insert(
                        0,
                        Row::action(
                            "Restart game",
                            "Use the current card; discard this saved machine state",
                        ),
                    );
                    actions.insert(0, Action::Restart);
                }
                (
                    "Could not continue".into(),
                    "Details".into(),
                    "Back".into(),
                    Action::Back,
                )
            }
        };
        if self.screen == Screen::Results {
            for (row, (_, checked)) in rows.iter_mut().skip(1).zip(&self.scan_games) {
                row.checked = Some(*checked);
            }
        }
        if let Screen::PhoneBinding(key) = self.screen {
            for (row, action) in rows.iter_mut().zip(&actions) {
                if let Action::SetPhoneKey(_, button) = action {
                    row.checked = Some(*button == self.phone_bindings.get(key));
                }
            }
        }
        if self.screen == Screen::Home {
            self.platform
                .refresh_game_icons(app, self.library.games.clone(), &self.root);
            for (row, game) in rows.iter_mut().zip(&self.library.games) {
                row.secondary = true;
                row.artwork = Some(Artwork::local(
                    &game.id,
                    &game.title,
                    &self.root.join("saves").join(format!("{}.srm", game.id)),
                ));
            }
            rows.pop(); // Settings lives in the app bar; its action index stays stable.
        }
        self.actions = actions;
        self.back_action = back_action;
        let mut menu = Menu::new(title, description, rows, &back);
        menu.cutouts = self.cutout_rects();
        menu.over_game = self.core.is_some();
        menu.home = self.screen == Screen::Home;
        menu.settings = menu.home.then_some(self.library.games.len());
        if self.screen == Screen::Controllers {
            let (w, h) = self.dimensions();
            let preset = self.controller_settings.preset(w, h);
            for (row, action) in menu.rows.iter_mut().zip(&self.actions) {
                row.toggle = match action {
                    Action::ToggleGameController => Some(self.custom_controller()),
                    Action::ToggleFollowThumb => Some(preset.follow_thumb),
                    Action::ToggleOverlap => Some(preset.allow_overlap),
                    Action::ToggleStickClick => Some(preset.stick_click_gesture),
                    _ => None,
                };
            }
        }
        if self.screen == Screen::Settings {
            for (row, action) in menu.rows.iter_mut().zip(&self.actions) {
                if matches!(action, Action::ToggleBackground) {
                    row.toggle = Some(self.playback.run_in_background);
                }
            }
        }
        if self.screen == Screen::Sound {
            menu.anchor = Some(self.layout.mute);
            menu.rows[0].slider = Some(self.playback.volume);
            menu.rows[0].height = 104.;
        }
        if self.screen == Screen::Timescale {
            menu.anchor = Some(self.layout.speed);
            menu.rows[0].slider = Some(self.playback.speed.slider());
            menu.rows[0].slider_label = Some(self.playback.speed.label());
            menu.rows[0].height = 104.;
        }
        let (w, h) = self.dimensions();
        let panel = menu.surface(w, h);
        menu.description_lines = self
            .graphics
            .menu_description(&menu.description, Menu::column(panel.w).w);
        self.menu = Some(menu);
        self.dirty = true;
    }
    fn open_game(&mut self, app: &AndroidApp, launch: Launch) -> Result<(), String> {
        let Launch::OpenGame(id) = launch;
        let Some(index) = self.library.games.iter().position(|g| g.id == id.as_str()) else {
            self.message = "This game is not in this device's library. Add the original files to recover its entry.".into();
            self.navigate(app, Screen::Recover(None));
            return Ok(());
        };
        if self.library.games[index]
            .disc_list()
            .iter()
            .any(|d| fs::File::open(&d.path).is_err())
        {
            self.message = "The game's files moved or access was removed. Locate the original disc; your cards, session and controls are kept.".into();
            self.navigate(app, Screen::Recover(Some(index)));
            return Ok(());
        }
        if let Some(index) = self
            .library
            .games
            .iter()
            .position(|g| g.id == id.as_str() && g.system == System::Unknown)
        {
            self.navigate(app, Screen::Console(index));
            return Ok(());
        }
        let game = self
            .library
            .games
            .iter()
            .find(|g| g.id == id.as_str())
            .cloned()
            .ok_or(
                "This game is not in this device's library. Locate its files to recover the entry.",
            )?;
        if game.system == System::Ps2 {
            let asset_name = std::ffi::CString::new("ps2/resources.tsv").unwrap();
            let mut index = app
                .asset_manager()
                .open(&asset_name)
                .ok_or("Bundled PS2 resources are unavailable.")?;
            let index = std::str::from_utf8(index.buffer().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            for line in index.lines() {
                let (name, digest) = line.split_once('\t').ok_or("Invalid PS2 resource index.")?;
                let path = std::ffi::CString::new(format!("ps2/resources/{name}"))
                    .map_err(|e| e.to_string())?;
                let mut asset = app
                    .asset_manager()
                    .open(&path)
                    .ok_or("A bundled PS2 resource is missing.")?;
                psx_player::ps2::install_resource(
                    &self.root,
                    name,
                    digest,
                    asset.buffer().map_err(|e| e.to_string())?,
                )?;
            }
        }
        self.start(game)?;
        self.resize(app)
    }
    fn start(&mut self, game: Game) -> Result<(), String> {
        if !matches!(game.system, System::Ps1 | System::Ps2) {
            return Err("This console is not supported yet.".into());
        }
        let id = GameId::parse(&game.id)?;
        if self.core.is_some() && self.session.as_ref().is_some_and(|s| s.saved.game == id) {
            self.ensure_session_available()?;
            self.screen = Screen::Settings;
            self.closing_menu = self.menu.take();
            self.dirty = true;
            return Ok(());
        }
        // Resolve content before stopping another game's owned runtime. A
        // missing launch target must not reset or replace its durable entry.
        for disc in game.disc_list() {
            fs::File::open(&disc.path).map_err(|_| "This game's files are unavailable. Locate the game to keep its existing progress.".to_string())?;
        }
        if game.system == System::Ps2 && unsafe { libc::sysconf(libc::_SC_PAGESIZE) } != 4096 {
            return Err(
                "This PS2 build requires 4 KiB host pages. Your saved games are kept.".into(),
            );
        }
        self.close()?;
        // Native teardown may unbind its EGL context. The platform owns this
        // thread's context; establish it before the next core queries or uses GL.
        self.graphics.restore_current()?;
        let session = Runtime::restoring(&self.root, SavedSession { game: id.clone() })
            .map_err(|e| e.to_string())?;
        // Per-entry settings and firmware selection share the card/state writer.
        let ps2_system = if game.system == System::Ps2 {
            Some(psx_player::ps2::prepare(
                &self.root,
                &id,
                &self.graphics.renderer(),
            )?)
        } else {
            None
        };
        let launch_path = game.launch_path(&self.root).map_err(|e| e.to_string())?;
        let library = Path::new(if game.system == System::Ps2 {
            "libarmsx2.so"
        } else {
            "libswanstation.so"
        });
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let jit = page_size == 4096;
        // Existing cached interpreter avoids 4KB JIT guard/mprotect assumptions
        // on 16KB kernels. Runtime performance there is still unqualified.
        let mut core = unsafe {
            Core::load(
                game.system,
                library,
                &launch_path,
                HostConfig {
                    system: ps2_system.as_deref().unwrap_or(&self.root.join("system")),
                    save: &self.root.join("saves"),
                    audio: self.queue.clone(),
                    framebuffer: self.graphics.framebuffer(),
                    proc_address: self.graphics.proc_address(),
                    jit,
                    initial_disc: game.last_disc,
                },
                self.graphics.gles_version(),
            )?
        };
        core.set_frame_waker(self.frame_waker.clone());
        let card = if game.system == System::Ps1 {
            let card = MemoryCard::open(self.root.join("saves").join(format!("{}.srm", game.id)))
                .map_err(|e| e.to_string())?;
            core.restore_card(&card)?;
            Some(card)
        } else {
            let deadline = Instant::now() + Duration::from_secs(15);
            while !core.ready() {
                #[cfg(feature = "graphics-observer")]
                if psx_player::graphics_observer::stop_requested() {
                    return Err("Development observation stopped.".into());
                }
                core.run();
                if let Some(failure) = core.producer_failure() {
                    return Err(RenderError::Producer(failure).to_string());
                }
                if Instant::now() >= deadline {
                    return Err(
                        "The PS2 firmware or renderer did not finish starting. Your card is kept."
                            .into(),
                    );
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            core.pause(true)?;
            None
        };
        let av = core.state().av;
        if !(45.0..=65.0).contains(&av.timing.fps)
            || !matches!(av.timing.sample_rate, 44100. | 48000.)
        {
            return Err("The core requested unsupported audio or frame timing.".into());
        }
        log::info!(
            "Session started: fps={:.4}, sample_rate={}, host_page={}, jit={}",
            av.timing.fps,
            av.timing.sample_rate,
            page_size,
            jit
        );
        self.queue.set_sample_rate(av.timing.sample_rate as usize);
        self.core = Some(core);
        self.session = Some(session);
        // A failed restore must stay unavailable, including after ordinary
        // pause/focus updates. It cannot run or overwrite the retained state.
        self.session.as_mut().unwrap().state = SessionState::Unavailable;
        self.card = card;
        self.game = Some(game.clone());
        let analog = game.system == System::Ps2;
        self.phone_bindings = self.controller_profiles.bindings(Some(&id), analog);
        self.controller_settings = self
            .controller_profiles
            .controls(&self.root, Some(&id), analog)
            .map_err(|e| e.to_string())?;
        self.library.put(game.clone());
        self.library.save(&self.root).map_err(|e| e.to_string())?;
        self.activity.user_paused = false;
        self.checkpoint_dirty = false;
        self.checkpoint_valid = false;
        let identity = self.core.as_mut().unwrap().card_identity()?;
        let checkpoint = resume::load(&self.root, &game, &identity).map_err(|e| e.to_string())?;
        if let Some(checkpoint) = checkpoint {
            let core = self.core.as_mut().unwrap();
            core.restore_checkpoint(&checkpoint.bytes)?;
            // The durable card is authoritative even if the donor embeds one.
            if let Some(card) = &self.card {
                core.restore_card(card)?;
            }
            core.pause(false)?;
            core.run(); // Produce the restored picture, including paused resumes.
            if let Some(failure) = core.producer_failure() {
                return Err(RenderError::Producer(failure).to_string());
            }
            self.activity.user_paused = checkpoint.metadata.paused;
            resume::mark_active(&self.root, &game).map_err(|e| e.to_string())?;
            log::info!("Automatic session restored");
            self.checkpoint_valid = true;
        }
        self.closing_menu = self.menu.take();
        self.next_frame = Instant::now();
        self.last_flush = Instant::now();
        self.last_checkpoint = Instant::now() - Duration::from_secs(30);
        self.last_report = Instant::now();
        self.frames = 0;
        self.run_time = Duration::ZERO;
        self.presents = 0;
        self.present_time = Duration::ZERO;
        self.max_present_time = Duration::ZERO;
        self.message = game.title;
        self.screen = Screen::Settings;
        self.session.as_mut().unwrap().state = SessionState::Restoring;
        Ok(())
    }
    fn ensure_session_available(&self) -> Result<(), String> {
        if let Some(session) = &self.session {
            session.ensure_available().map_err(str::to_owned)?;
        }
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        self.ensure_session_available()?;
        let core = self.core.as_mut().unwrap();
        let owns_clock = core.owns_clock();
        let previous_frames = core.state().hardware_frames;
        let speed = core.set_speed(self.playback.effective_speed())?;
        self.queue.set_speed(speed);
        core.pause(false)?;
        core.input(
            self.touches.frame_mask() | self.phone_input.frame_mask(),
            self.touches.axes(),
            speed.is_fast(),
        );
        let before = Instant::now();
        core.run();
        if let Some(failure) = core.producer_failure() {
            if let Some(detail) = core.state().last_message {
                log::error!("PS2 graphics failure: {detail}");
            }
            self.render_failed(RenderError::Producer(failure));
            return Ok(());
        }
        let state = core.state();
        if state.hardware_frames != previous_frames {
            if let Some(pixels) = &state.pixels {
                if let Err(error) = unsafe { self.graphics.upload_game(pixels) } {
                    self.render_failed(error);
                    return Ok(());
                }
            }
        }
        self.dirty |= !owns_clock || core.state().hardware_frames != previous_frames;
        self.checkpoint_dirty = true;
        self.run_time += before.elapsed();
        self.frames += 1;
        if core.state().shutdown {
            return Err("The game session stopped.".into());
        }
        if self.game.as_ref().is_some_and(|g| g.system == System::Ps1)
            && core.state().hardware_frames > 0
            && core.state().software_frames > 0
        {
            return Err("The hardware renderer stopped. Close and reopen the game.".into());
        }
        if core.state().hardware_frames == 0
            && self.last_report.elapsed() >= Duration::from_secs(10)
        {
            return Err("The game has not produced a hardware-rendered picture. Reopen it or check its firmware.".into());
        }
        if core.state().frame_width > 1024 || core.state().frame_height > 1024 {
            return Err("The core output exceeded the native-resolution buffer.".into());
        }
        let fps = core.state().av.timing.fps;
        let target = self.queue.target_frames() as f64;
        let error = (self.queue.queued() as f64 - target) / target;
        let correction = 1. + (error * 0.003).clamp(-0.005, 0.005);
        if owns_clock {
            // The donor's CPU limiter owns normal/fractional/unlimited time.
            // Frame notifications bypass this audio/input housekeeping bound.
            self.next_frame = Instant::now() + Duration::from_millis(8);
        } else if let Some(multiplier) = speed.multiplier() {
            self.next_frame += Duration::from_secs_f64(correction / fps / multiplier);
        } else {
            self.next_frame = Instant::now();
        }
        if Instant::now().saturating_duration_since(self.next_frame) > Duration::from_millis(100) {
            self.next_frame = Instant::now();
        }
        if self.sound.is_none() {
            self.sound = Some(Sound::start(
                self.queue.clone(),
                core.state().av.timing.sample_rate as i32,
            )?);
        }
        // PS1 exposes card RAM that Rust must poll. PS2 owns files and flushes
        // each page write itself; pausing its VM every second stalls audio.
        // Full native flush/sync still happens at checkpoints/interruptions.
        if self.card.is_some() && self.last_flush.elapsed() >= Duration::from_secs(1) {
            self.flush()?;
            self.last_flush = Instant::now();
        }
        if !self.checkpoint_after_present
            && self.last_checkpoint.elapsed() >= Duration::from_secs(30)
        {
            self.queue_checkpoint()?;
        }
        if self.last_report.elapsed() >= Duration::from_secs(10) {
            let elapsed = self.last_report.elapsed().as_secs_f64();
            let core = self.core.as_ref().unwrap();
            if let Some([fps, speed, average, minimum, maximum]) = core.state().metrics {
                log::info!("PS2 native metrics: vps={fps:.2} speed={speed:.1}% frame_ms={average:.2} min_ms={minimum:.2} max_ms={maximum:.2}");
            }
            if self.game.as_ref().is_some_and(|g| g.system == System::Ps2) {
                if let Some([ee, gs, gpu, gs_ms, gpu_ms, draws, readbacks, barriers, waits]) =
                    core.state().render_costs
                {
                    log::info!("PS2 graphics: ee={ee:.1}% gs={gs:.1}% gpu={gpu:.1}% gs_ms={gs_ms:.2} gpu_ms={gpu_ms:.2} draws={draws:.1} readbacks={readbacks:.1} barriers={barriers:.1} blocking_waits={waits:.1}");
                }
                log::info!("PS2 frontend: calls_per_second={:.2} call_ms={:.2} hardware_frames={} software_frames={} audio_queued={} underruns={} dropped={} requested={}", self.frames as f64/elapsed, self.run_time.as_secs_f64()*1000./self.frames as f64, core.state().hardware_frames,core.state().software_frames,self.queue.queued(),self.queue.underruns.load(Ordering::Relaxed),self.queue.dropped.load(Ordering::Relaxed),self.playback.effective_speed().label());
                log::info!(
                    "PS2 presentation: per_second={:.2} mean_ms={:.2} max_ms={:.2}",
                    self.presents as f64 / elapsed,
                    self.present_time.as_secs_f64() * 1000. / self.presents.max(1) as f64,
                    self.max_present_time.as_secs_f64() * 1000.
                );
            } else {
                log::info!("playback fps={:.2} speed={:.1}% core_ms={:.2} hw_frames={} sw_frames={} audio_queued={} underruns={} dropped={} target={}",
                self.frames as f64/elapsed,self.frames as f64/elapsed/fps*100.,self.run_time.as_secs_f64()*1000./self.frames as f64,
                core.state().hardware_frames,core.state().software_frames,self.queue.queued(),self.queue.underruns.load(Ordering::Relaxed),self.queue.dropped.load(Ordering::Relaxed), self.playback.effective_speed().label());
            }
            self.last_report = Instant::now();
            self.frames = 0;
            self.run_time = Duration::ZERO;
            self.presents = 0;
            self.present_time = Duration::ZERO;
            self.max_present_time = Duration::ZERO;
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<(), String> {
        if let Some(core) = &mut self.core {
            core.flush_card(self.card.as_mut())?;
        }
        Ok(())
    }
    fn checkpoint(&mut self) -> Result<(), String> {
        self.checkpoint_after_present = false;
        self.finish_checkpoint()?;
        self.queue_checkpoint()?;
        self.finish_checkpoint()
    }
    fn checkpoint_interruption(&mut self) -> Result<(), String> {
        #[cfg(feature = "graphics-observer")]
        if psx_player::graphics_observer::stop_requested() {
            return self.finish_checkpoint();
        }
        if self.activity.background_playback {
            // Continue-play callbacks can arrive repeatedly during one
            // transition. Keep one bounded write in flight without waiting
            // for storage while the same core/audio owner is still running.
            self.queue_checkpoint()
        } else {
            self.checkpoint()
        }
    }
    fn finish_checkpoint(&mut self) -> Result<(), String> {
        if let Some(work) = self.checkpoint_work.take() {
            work.recv()
                .map_err(|_| "The session save worker stopped.".to_string())??;
            self.checkpoint_valid = true;
        }
        Ok(())
    }
    fn queue_checkpoint(&mut self) -> Result<(), String> {
        if self.ensure_session_available().is_err() {
            return Ok(());
        }
        if self.checkpoint_work.is_some() {
            return Ok(());
        }
        self.flush()?;
        if !self.checkpoint_dirty {
            return Ok(());
        }
        if let (Some(core), Some(game)) = (&mut self.core, &self.game) {
            let before = Instant::now();
            let state = core.checkpoint()?;
            let captured_in = before.elapsed();
            let before_cards = Instant::now();
            let card = core.card_identity()?;
            log::info!(
                "Session snapshot captured in {:.2} ms; cards verified in {:.2} ms",
                captured_in.as_secs_f64() * 1000.,
                before_cards.elapsed().as_secs_f64() * 1000.
            );
            let game = game.clone();
            let root = self.root.clone();
            let paused = self.activity.user_paused;
            let (tx, rx) = mpsc::sync_channel(1);
            self.checkpoint_work = Some(rx);
            std::thread::spawn(move || {
                let before = Instant::now();
                let result = state.into_bytes().and_then(|state| {
                    log::info!(
                        "Session archive packaged in {:.2} ms",
                        before.elapsed().as_secs_f64() * 1000.
                    );
                    resume::save(&root, &game, paused, &card, &state)
                        .map_err(|e| format!("The session could not be saved: {e}"))
                });
                if result.is_ok() {
                    log::info!("Automatic session saved");
                }
                let _ = tx.send(result);
            });
            self.checkpoint_dirty = false;
            self.last_checkpoint = Instant::now();
        }
        Ok(())
    }
    fn stop_sound(&mut self) {
        self.sound.take();
        self.queue.clear_stopped();
    }
    fn interrupt(&mut self) -> Result<(), String> {
        if self.playback_changed.is_some() {
            self.save_playback()?;
        }
        self.frames = 0;
        self.run_time = Duration::ZERO;
        self.presents = 0;
        self.present_time = Duration::ZERO;
        self.max_present_time = Duration::ZERO;
        self.last_report = Instant::now();
        self.cancel_input();
        self.stop_sound();
        self.flush()
    }
    fn cancel_input(&mut self) {
        self.speaker_press.cancel();
        self.time_press.cancel();
        self.touches.cancel();
        self.phone_input.cancel();
        if let Some(menu) = &mut self.menu {
            menu.cancel();
        }
        if let Some(editor) = &mut self.editor {
            editor.cancel_touch();
        }
        if let Some(core) = &mut self.core {
            core.input(0, [0; 4], false);
        }
    }
    fn close(&mut self) -> Result<(), String> {
        self.unload(true)
    }
    fn render_failed(&mut self, error: RenderError) {
        if !error.requires_reset() {
            self.fail(error.to_string());
            self.activity.surface = false;
            return;
        }
        log::error!("{error}; retiring the runtime before graphics recovery");
        let recover = (|| -> Result<(), String> {
            // Finish a snapshot already captured before the reported fault,
            // but never capture a new machine state from a lost context. Keep
            // the preceding checkpoint unchanged, even if newer cards make
            // it unavailable. The normal close still flushes and joins owners.
            self.checkpoint_after_present = false;
            self.checkpoint_dirty = false;
            self.activity.user_paused = true;
            if self
                .core
                .as_ref()
                .is_some_and(|core| core.producer_failure().is_some())
            {
                // A failed producer must retire before any pause/flush callback
                // can release its handoff and resume more guest graphics work.
                self.cancel_input();
                self.stop_sound();
                self.finish_checkpoint()?;
                self.checkpoint_valid = false;
                self.core.take();
                self.close()?;
                unsafe { self.graphics.replace_context()? };
                return Ok(());
            }
            self.interrupt()?;
            self.finish_checkpoint()?;
            self.checkpoint_valid = false;
            let ps2 = matches!(self.core, Some(Core::Ps2(..)));
            self.close()?;
            if !ps2 {
                unsafe { self.graphics.replace_context()? };
            }
            Ok(())
        })();
        match recover {
            Ok(()) => self.fail("Graphics stopped. Existing memory cards and saved sessions are kept. Reopen the game to try again.".into()),
            Err(cause) => {
                self.fail(format!("Graphics stopped. Close and reopen psx. {cause}"));
                self.activity.surface = false;
            }
        }
    }
    fn unload(&mut self, clear_active: bool) -> Result<(), String> {
        let had_game = self.core.is_some();
        self.interrupt()?;
        self.checkpoint()?;
        let before = if self.checkpoint_valid
            && self.game.as_ref().is_some_and(|g| g.system == System::Ps2)
        {
            self.core.as_mut().map(Core::card_identity).transpose()?
        } else {
            None
        };
        let retire_graphics = matches!(self.core, Some(Core::Ps2(..)));
        self.core.take();
        if let (Some(game), Some(before)) = (&self.game, before) {
            let cards = self.root.join("ps2").join(&game.id).join("pcsx2/memcards");
            let after = psx_player::ps2::card_identity(&cards).map_err(|e| e.to_string())?;
            resume::seal_closed_cards(&self.root, game, &before, &after)
                .map_err(|e| e.to_string())?;
        }
        if let Some(session) = &mut self.session {
            session.state = SessionState::CheckpointedUnloaded;
        }
        self.session.take();
        self.card = None;
        self.game = None;
        let analog = self.controller_settings.analog();
        self.phone_bindings = self.controller_profiles.bindings(None, analog);
        self.controller_settings = self
            .controller_profiles
            .controls(&self.root, None, analog)
            .map_err(|e| e.to_string())?;
        self.queue.clear_stopped();
        if had_game && clear_active {
            resume::clear_active(&self.root).map_err(|e| e.to_string())?;
        }
        if retire_graphics && clear_active {
            // All writers and CPU/GS owners retired; cards are sealed. Drop
            // their GL share group before another runtime can inherit it.
            // Final app shutdown instead destroys Graphics normally.
            unsafe { self.graphics.replace_context()? };
        }
        Ok(())
    }
    fn fail(&mut self, error: String) {
        self.speaker_press.cancel();
        self.time_press.cancel();
        self.dirty = true;
        log::error!("{error}");
        self.message = error;
        self.screen = Screen::Error;
        self.menu = None;
        self.closing_menu = None;
        self.editor = None;
        self.activity.user_paused = true;
        self.touches.cancel();
        self.phone_input.cancel();
        self.stop_sound();
    }
}
