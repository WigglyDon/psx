mod graphics;
mod platform;
mod sound;

use android_activity::{
    input::{InputEvent, KeyAction, Keycode, MotionAction},
    AndroidApp, InputStatus, MainEvent, PollEvent,
};
use graphics::Graphics;
use platform::{Event, Platform, PICK_FIRMWARE, PICK_GAME, PICK_STORAGE};
use psx_player::{
    artwork::Artwork,
    audio::AudioQueue,
    files::{self, Game},
    input::{ControllerSettings, Layout, Touches},
    library::{self, Library},
    retro::{Core, HostConfig},
    saves::MemoryCard,
    ui::{EditResult, Editor, Menu, Row, Visuals},
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
    ChooseDisc(usize),
    ToggleGame(usize),
    SelectAll,
    AddSelected,
    CancelWork,
    Settings,
    Home,
    Discs,
    Controllers,
    ToggleOverlap,
    EditControls,
    ResetControls,
    SaveControls,
    CancelEdit,
    License(&'static str),
    Source,
    Back,
    Resume,
    Firmware,
    Close,
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
    About,
    License(&'static str),
    Controllers,
    Editing,
    Error,
}
enum Work {
    Scanned(library::Scan),
    Added(Library),
}
struct Player {
    root: PathBuf,
    platform: Platform,
    core: Option<Core>,
    card: Option<MemoryCard>,
    game: Option<Game>,
    library: Library,
    controller_settings: ControllerSettings,
    queue: Arc<AudioQueue>,
    sound: Option<Sound>,
    activity: Activity,
    touches: Touches,
    layout: Layout,
    density: f32,
    insets: [i32; 4],
    screen: Screen,
    menu: Option<Menu>,
    closing_menu: Option<Menu>,
    visuals: Visuals,
    visual_animating: bool,
    last_visual: Instant,
    editor: Option<Editor>,
    picker: bool,
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
    let mut player = Player {
        library: Library::load(&root)
            .map_err(|e| format!("The saved library could not be read: {e}"))?,
        controller_settings: ControllerSettings::load(&root).map_err(|e| e.to_string())?,
        root,
        platform,
        graphics,
        core: None,
        card: None,
        game: None,
        queue: Arc::new(AudioQueue::new(4096)),
        sound: None,
        activity: Activity::default(),
        touches: Touches::default(),
        layout: Layout::new(360., 800.),
        density: 1.,
        insets: [0; 4],
        screen: Screen::Home,
        menu: None,
        closing_menu: None,
        visuals: Visuals::default(),
        visual_animating: true,
        last_visual: now,
        editor: None,
        picker: false,
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
        last_flush: now,
        last_report: now,
        frames: 0,
        run_time: Duration::ZERO,
    };
    player.refresh_menu(&app);
    while !player.destroyed {
        let timeout = if player.activity.can_run() && player.core.is_some() {
            player
                .next_frame
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(8))
        } else if player.visual_animating {
            Duration::from_millis(16)
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
        if player.menu.is_none()
            && player.editor.is_none()
            && player.screen != Screen::Error
            && !player.picker
            && player.import.is_none()
            && player.activity.foreground
            && player.activity.surface
            && (player.core.is_none() || player.activity.user_paused)
        {
            player.navigate(
                &app,
                if player.core.is_some() {
                    Screen::Settings
                } else {
                    Screen::Home
                },
            );
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
        player.visual_animating = player.visuals.advance(
            (now - player.last_visual).as_secs_f32(),
            player.touches.mask(),
            player.menu.is_some() || player.editor.is_some(),
            animated_icons,
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
        if player.dirty && player.activity.surface && player.activity.foreground {
            if let Err(error) = unsafe {
                player.graphics.render(
                    player.density,
                    &player.layout,
                    frame,
                    player.menu.as_ref().or(player.closing_menu.as_ref()),
                    player.editor.as_ref(),
                    &player.visuals,
                )
            } {
                player.fail(error);
                player.activity.surface = false;
            }
            player.dirty = false;
        }
    }
    player.stop_sound();
    player.cancel_work.store(true, Ordering::Relaxed);
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
                self.platform.insets(app);
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
                self.platform.insets(app);
            }
            MainEvent::GainedFocus => {
                self.activity.focused = true;
                self.platform.fullscreen(app);
                self.platform.insets(app);
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
        let content = app.content_rect();
        let right = if content.right > 0 {
            content.right
        } else {
            self.graphics.width
        };
        let bottom = if content.bottom > 0 {
            content.bottom
        } else {
            self.graphics.height
        };
        self.graphics.content_rect(
            content.left.max(self.insets[0]),
            content.top.max(self.insets[1]),
            right.min(self.graphics.width - self.insets[2]),
            bottom.min(self.graphics.height - self.insets[3]),
        );
        self.density = app
            .config()
            .density()
            .map(|d| d as f32 / 160.)
            .filter(|d| *d >= 0.5 && *d <= 6.)
            .unwrap_or(1.);
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
        self.layout = self.controller_settings.preset(w, h).layout(w, h);
        if let Some(menu) = &mut self.menu {
            menu.description_lines = self
                .graphics
                .menu_description(&menu.description, Menu::column(w).w);
            menu.cancel();
            menu.clamp_scroll(w, h);
        }
        if matches!(self.screen, Screen::License(_)) {
            self.refresh_menu(app);
        }
        let content = self.graphics.content;
        if previous
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
                "Display resized: surface={}x{} content={}x{} at {},{} density={}",
                self.graphics.width,
                self.graphics.height,
                content.w,
                content.h,
                content.x,
                content.y,
                self.density
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
                    let x = (pointer.x() - self.graphics.content.x) / self.density;
                    let y = (pointer.y() - self.graphics.content.y) / self.density;
                    let w = self.graphics.content.w / self.density;
                    let h = self.graphics.content.h / self.density;
                    match motion.action() {
                        MotionAction::Down => menu.down(pointer.pointer_id(), x, y),
                        MotionAction::Move => {
                            for pointer in motion.pointers() {
                                menu.motion(
                                    pointer.pointer_id(),
                                    (pointer.x() - self.graphics.content.x) / self.density,
                                    (pointer.y() - self.graphics.content.y) / self.density,
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
                                } else {
                                    self.actions.get(index).cloned()
                                };
                                feedback = action.is_some();
                            }
                        }
                        MotionAction::Cancel => menu.cancel(),
                        _ => {}
                    }
                } else {
                    match motion.action() {
                        MotionAction::Cancel => self.touches.cancel(),
                        MotionAction::Up | MotionAction::PointerUp => self
                            .touches
                            .release(motion.pointer_at_index(motion.pointer_index()).pointer_id()),
                        MotionAction::Down | MotionAction::PointerDown | MotionAction::Move => {
                            for pointer in motion.pointers() {
                                let x = (pointer.x() - self.graphics.content.x) / self.density;
                                let y = (pointer.y() - self.graphics.content.y) / self.density;
                                if motion.action() == MotionAction::Down
                                    && self.layout.pause.contains(x, y)
                                    && self.core.is_some()
                                {
                                    action = Some(Action::Settings);
                                    feedback = true;
                                } else if self.activity.can_run() {
                                    let pressed = self
                                        .touches
                                        .update(pointer.pointer_id(), self.layout.mask_at(x, y));
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
            Event::Documents(request, intent) => {
                self.picker = false;
                if request == PICK_STORAGE {
                    self.platform.storage(app, false);
                } else if let Some(intent) = intent {
                    self.activity.loading = true;
                    self.import = Some(self.platform.import(self.root.clone(), request, intent));
                    self.message = "Copying your selected files into private storage. Large discs can take a moment.".into();
                    self.refresh_menu(app);
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
            }
            Event::Error(error) => {
                self.picker = false;
                self.fail(error);
            }
            Event::Insets(edges) => {
                if edges != self.insets {
                    self.insets = edges;
                    self.touches.cancel();
                    if let Err(error) = self.resize(app) {
                        self.fail(error);
                    }
                }
            }
        }
    }

    fn navigate(&mut self, app: &AndroidApp, screen: Screen) {
        self.closing_menu = None;
        self.screen = screen;
        self.activity.user_paused = self.core.is_some();
        if let Err(error) = self.interrupt() {
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
            Action::Browse | Action::Firmware => {
                self.picker = true;
                self.platform.pick(
                    app,
                    if matches!(action, Action::Browse) {
                        PICK_GAME
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
                if game.disc_list().len() > 1 {
                    self.navigate(app, Screen::Discs(index));
                } else {
                    self.start(game)?;
                }
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
                    files::remember(&self.root, &game).map_err(|e| e.to_string())?;
                    self.message = format!("Disc {} inserted. Resume when ready.", index + 1);
                    self.navigate(app, Screen::Settings);
                } else {
                    game.last_disc = index;
                    self.start(game)?;
                }
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
                self.activity.user_paused = false;
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
            Action::ToggleOverlap => {
                let (w, h) = self.dimensions();
                let mut preset = self.controller_settings.preset(w, h);
                preset.allow_overlap = !preset.allow_overlap;
                self.controller_settings.set(w, h, preset);
                self.controller_settings
                    .save(&self.root)
                    .map_err(|e| e.to_string())?;
                self.resize(app)?;
                self.refresh_menu(app);
            }
            Action::ResetControls => {
                let (w, h) = self.dimensions();
                self.controller_settings.set(w, h, Default::default());
                self.controller_settings
                    .save(&self.root)
                    .map_err(|e| e.to_string())?;
                self.resize(app)?;
                self.refresh_menu(app);
            }
            Action::EditControls => {
                self.screen = Screen::Editing;
                let (w, h) = self.dimensions();
                self.editor = Some(Editor::new(self.controller_settings.preset(w, h), w, h));
                self.menu = None;
                self.dirty = true;
            }
            Action::SaveControls => {
                if let Some(editor) = self.editor.take() {
                    self.controller_settings
                        .set(editor.width, editor.height, editor.preset);
                    self.controller_settings
                        .save(&self.root)
                        .map_err(|e| e.to_string())?;
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
                self.message = "Game closed. Completed in-game memory-card saves are kept.".into();
                self.navigate(app, Screen::Home);
            }
            Action::About => self.navigate(app, Screen::About),
            Action::License(path) => self.navigate(app, Screen::License(path)),
            Action::Source => self.platform.open_source(app),
            Action::Back => match self.screen {
                Screen::Scanning | Screen::Adding => return self.action(app, Action::CancelWork),
                Screen::License(_) => self.navigate(app, Screen::About),
                Screen::Editing => return self.action(app, Action::CancelEdit),
                Screen::Controllers => self.navigate(app, Screen::Settings),
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
                            format!("{} discs", discs.len())
                        } else {
                            String::new()
                        },
                    );
                }
                add(
                    Action::Settings,
                    "Settings".into(),
                    "Controllers, firmware, source and licenses".into(),
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
                add(
                    Action::Firmware,
                    "PS1 firmware".into(),
                    "Optional · choose your own compatible dump".into(),
                );
                add(
                    Action::About,
                    "About psx".into(),
                    "Source, licenses and privacy".into(),
                );
                if self.core.is_some() {
                    add(
                        Action::Close,
                        "Close game".into(),
                        "Keeps completed in-game saves; checkpoints may be temporary".into(),
                    );
                }
                (
                    "Settings".into(),
                    if self.core.is_some() {
                        self.message.clone()
                    } else {
                        String::new()
                    },
                    if self.core.is_some() {
                        "Resume game".into()
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
                add(
                    Action::EditControls,
                    "Edit layout".into(),
                    "Tap to select; drag elements or their corner grips".into(),
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
                if !preset.is_default(w, h) {
                    add(
                        Action::ResetControls,
                        "Reset to defaults".into(),
                        "Restore this display's whole layout".into(),
                    );
                }
                (
                    "Controllers".into(),
                    format!(
                        "{}. Layouts are saved separately for tall, square and wide displays.",
                        ControllerSettings::profile(w, h)
                    ),
                    "Done".into(),
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
                    (
                        "OpenBIOS · MIT",
                        "notices/swanstation/dep/openbios/LICENSE.txt",
                    ),
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
                let width = Menu::column(self.dimensions().0).w - 8.;
                let paragraphs = self.graphics.text_rows(&body, width);
                for _ in &paragraphs {
                    actions.push(Action::Back);
                }
                rows = paragraphs;
                ("Licenses".into(), path.into(), "Back".into(), Action::Back)
            }
            Screen::Error => {
                rows = self
                    .graphics
                    .text_rows(&self.message, Menu::column(self.dimensions().0).w - 8.);
                for _ in &rows {
                    actions.push(Action::Back);
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
        if self.screen == Screen::Home {
            for (row, game) in rows.iter_mut().zip(&self.library.games) {
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
        menu.home = self.screen == Screen::Home;
        menu.settings = menu.home.then_some(self.library.games.len());
        if self.screen == Screen::Controllers {
            if let Some(row) = menu.rows.get_mut(1) {
                let (w, h) = self.dimensions();
                row.toggle = Some(self.controller_settings.preset(w, h).allow_overlap);
            }
        }
        menu.description_lines = self
            .graphics
            .menu_description(&menu.description, Menu::column(self.dimensions().0).w);
        self.menu = Some(menu);
        self.dirty = true;
    }
    fn start(&mut self, game: Game) -> Result<(), String> {
        self.close()?;
        let launch_path = game.launch_path(&self.root).map_err(|e| e.to_string())?;
        let library = Path::new("libswanstation.so");
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        let jit = page_size == 4096;
        // Existing cached interpreter avoids 4KB JIT guard/mprotect assumptions
        // on 16KB kernels. Runtime performance there is still unqualified.
        let mut core = unsafe {
            Core::load(
                library,
                &launch_path,
                HostConfig {
                    system: &self.root.join("system"),
                    save: &self.root.join("saves"),
                    audio: self.queue.clone(),
                    framebuffer: self.graphics.framebuffer(),
                    proc_address: self.graphics.proc_address(),
                    jit,
                    initial_disc: game.last_disc,
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
        self.library.put(game.clone());
        self.library.save(&self.root).map_err(|e| e.to_string())?;
        files::remember(&self.root, &game).map_err(|e| e.to_string())?;
        self.activity.user_paused = false;
        self.closing_menu = self.menu.take();
        self.next_frame = Instant::now();
        self.last_flush = Instant::now();
        self.last_report = Instant::now();
        self.frames = 0;
        self.run_time = Duration::ZERO;
        self.message = game.title;
        Ok(())
    }
    fn tick(&mut self) -> Result<(), String> {
        self.dirty = true;
        // A reopened audio stream starts a new active observation window.
        // Time spent in settings or the background is not emulation slowdown.
        if self.sound.is_none() {
            self.last_report = Instant::now();
            self.frames = 0;
            self.run_time = Duration::ZERO;
        }
        let core = self.core.as_mut().unwrap();
        core.state_mut().buttons = self.touches.frame_mask();
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
        if let Some(menu) = &mut self.menu {
            menu.cancel();
        }
        if let Some(editor) = &mut self.editor {
            editor.cancel_touch();
        }
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
        self.screen = Screen::Error;
        self.menu = None;
        self.closing_menu = None;
        self.editor = None;
        self.activity.user_paused = true;
        self.touches.cancel();
        self.stop_sound();
    }
}
