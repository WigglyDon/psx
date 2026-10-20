use android_activity::AndroidApp;
use jni::{
    objects::{GlobalRef, JObject, JString, JValue},
    sys::{jint, jobject},
    JNIEnv, JavaVM,
};
use psx_player::files::{self, Game};
use psx_player::session::{Launch, LaunchQueue};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    os::fd::{FromRawFd, RawFd},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};

pub const PICK_GAME: i32 = 101;
pub const PICK_FIRMWARE: i32 = 102;
pub const PICK_STORAGE: i32 = 103;
pub const PICK_PS2_FIRMWARE: i32 = 105;

pub enum Event {
    Documents(i32, Option<GlobalRef>),
    Storage(Option<Vec<PathBuf>>),
    ScanUnavailable,
    Focus(i32),
    Background(bool),
    PauseBackground,
    Error(String),
    Notice(String),
    LaunchError(String),
    RefreshInsets,
    Insets {
        size: [i32; 2],
        density: f32,
        edges: [i32; 4],
        fullscreen_edges: Option<[i32; 4]>,
        status_bar_top: i32,
        cutouts: Vec<[i32; 4]>,
        top_margin_available: bool,
        hidden_bars_visible: bool,
    },
}
static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());
static LAUNCHES: Mutex<Option<LaunchQueue>> = Mutex::new(None);
pub fn next_launch() -> Option<Launch> {
    LAUNCHES.lock().ok()?.as_mut()?.pop()
}
fn receive_launch(env: &mut JNIEnv, intent: &JObject) -> JResult<()> {
    if intent.is_null() {
        return Ok(());
    }
    let data = env
        .call_method(intent, "getDataString", "()Ljava/lang/String;", &[])?
        .l()?;
    if data.is_null() {
        return Ok(());
    }
    let value: String = env.get_string(&JString::from(data))?.into();
    let result = Launch::from_url(&value).and_then(|launch| {
        LAUNCHES
            .lock()
            .map_err(|_| "Game launch is temporarily unavailable.")?
            .get_or_insert_with(LaunchQueue::default)
            .push(launch)
    });
    if let Err(error) = result {
        post(Event::LaunchError(error.into()));
    }
    Ok(())
}
#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_gameIntent(
    mut env: JNIEnv,
    _this: JObject,
    intent: JObject,
) {
    if receive_launch(&mut env, &intent).is_err() {
        let _ = env.exception_clear();
        post(Event::Error("This game link could not be opened.".into()));
    }
}
pub(super) fn post(event: Event) {
    if let Ok(mut events) = EVENTS.lock() {
        if events.len() < 16 {
            events.push_back(event);
        }
    }
}
pub fn events() -> Vec<Event> {
    EVENTS
        .lock()
        .map(|mut q| q.drain(..).collect())
        .unwrap_or_default()
}

#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_insetsChanged(
    _env: JNIEnv,
    _this: JObject,
) {
    post(Event::RefreshInsets);
}

#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_documentResult(
    env: JNIEnv,
    _this: JObject,
    request: jint,
    result: jint,
    intent: JObject,
) {
    if request == PICK_STORAGE {
        post(Event::Documents(request, None));
        return;
    }
    if result != -1 || intent.is_null() {
        post(Event::Documents(request, None));
        return;
    }
    match env.new_global_ref(intent) {
        Ok(intent) => post(Event::Documents(request, Some(intent))),
        Err(_) => post(Event::Error(
            "The selected file could not be read. Try selecting it again.".into(),
        )),
    }
}
#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_audioFocusChanged(
    _env: JNIEnv,
    _this: JObject,
    focus: jint,
) {
    post(Event::Focus(focus));
}

#[derive(Clone)]
pub struct Platform {
    vm: Arc<JavaVM>,
    activity: GlobalRef,
}
type JResult<T> = jni::errors::Result<T>;

impl Platform {
    pub fn background(&self, app: &AndroidApp, enabled: bool) {
        self.ui(app, move |env, activity| {
            super::background::set(env, activity, enabled)
        });
    }
    pub fn background_permission(&self, app: &AndroidApp) {
        self.ui(app, |env, activity| {
            if env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?
                >= 33
            {
                let permission = env.new_string("android.permission.POST_NOTIFICATIONS")?;
                if env
                    .call_method(
                        activity,
                        "checkSelfPermission",
                        "(Ljava/lang/String;)I",
                        &[JValue::Object(&permission)],
                    )?
                    .i()?
                    != 0
                {
                    let permissions = env.new_object_array(1, "java/lang/String", &permission)?;
                    env.call_method(
                        activity,
                        "requestPermissions",
                        "([Ljava/lang/String;I)V",
                        &[JValue::Object(&permissions), JValue::Int(104)],
                    )?;
                }
            }
            Ok(())
        });
    }
    pub unsafe fn new(app: &AndroidApp) -> Result<Self, String> {
        let vm = JavaVM::from_raw(app.vm_as_ptr() as _).map_err(|e| e.to_string())?;
        let activity = {
            let mut env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let local = JObject::from_raw(app.activity_as_ptr() as jobject);
            let global = env.new_global_ref(&local).map_err(|e| e.to_string())?;
            let intent = env
                .call_method(&local, "getIntent", "()Landroid/content/Intent;", &[])
                .and_then(|v| v.l())
                .map_err(|e| e.to_string())?;
            receive_launch(&mut env, &intent).map_err(|e| e.to_string())?;
            // Android owns this reference, not this local JObject wrapper.
            let _ = local.into_raw();
            global
        };
        Ok(Self {
            vm: Arc::new(vm),
            activity,
        })
    }

    pub(super) fn ui(
        &self,
        app: &AndroidApp,
        callback: impl FnOnce(&mut JNIEnv, &JObject) -> JResult<()> + Send + 'static,
    ) {
        let platform = self.clone();
        app.run_on_java_main_thread(Box::new(move || {
            let Ok(mut env) = platform.vm.attach_current_thread() else {
                return;
            };
            if let Err(error) =
                env.with_local_frame(64, |env| callback(env, platform.activity.as_obj()))
            {
                let _ = env.exception_clear();
                log::error!("Android operation failed: {error}");
                post(Event::Error(
                    "Android could not complete that action. Please try again.".into(),
                ));
            }
        }));
    }

    pub fn configure(&self, app: &AndroidApp) {
        self.ui(app, |env, activity| {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            env.call_method(&window, "addFlags", "(I)V", &[JValue::Int(128)])?; // keep screen on while visible
            env.call_method(
                activity,
                "setVolumeControlStream",
                "(I)V",
                &[JValue::Int(3)],
            )?;
            Ok(())
        });
        self.fullscreen(app);
    }

    pub fn fullscreen(&self, app: &AndroidApp) {
        self.ui(app, |env, activity| {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            let view = env
                .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                .l()?;
            let sdk = env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?;
            if sdk >= 30 {
                env.call_method(
                    &window,
                    "setDecorFitsSystemWindows",
                    "(Z)V",
                    &[JValue::Bool(0)],
                )?;
                let controller = env
                    .call_method(
                        &view,
                        "getWindowInsetsController",
                        "()Landroid/view/WindowInsetsController;",
                        &[],
                    )?
                    .l()?;
                if !controller.is_null() {
                    let status = env
                        .call_static_method(
                            "android/view/WindowInsets$Type",
                            "statusBars",
                            "()I",
                            &[],
                        )?
                        .i()?;
                    env.call_method(
                        &controller,
                        "setSystemBarsBehavior",
                        "(I)V",
                        &[JValue::Int(if sdk >= 31 { 1 } else { 2 })],
                    )?;
                    let navigation = env
                        .call_static_method(
                            "android/view/WindowInsets$Type",
                            "navigationBars",
                            "()I",
                            &[],
                        )?
                        .i()?;
                    env.call_method(&controller, "hide", "(I)V", &[JValue::Int(status)])?;
                    // BEHAVIOR_DEFAULT keeps gesture navigation active even
                    // while hidden (API 31+). Transient-bars mode consumed the
                    // first swipe on the Fold. Older releases retain visible
                    // navigation. Reserve its touch strip in either mode.
                    env.call_method(
                        &controller,
                        if sdk >= 31 { "hide" } else { "show" },
                        "(I)V",
                        &[JValue::Int(navigation)],
                    )?;
                }
            } else {
                // Edge-to-edge fullscreen status area, with navigation shown.
                env.call_method(
                    &view,
                    "setSystemUiVisibility",
                    "(I)V",
                    &[JValue::Int(0x704)],
                )?;
            }
            Ok(())
        });
    }

    pub fn haptic(&self, app: &AndroidApp) {
        self.ui(app, |env, activity| {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            let view = env
                .call_method(window, "getDecorView", "()Landroid/view/View;", &[])?
                .l()?;
            // VIRTUAL_KEY, with no flags overriding the user's system setting.
            // View haptics need no vibration permission and degrade to no effect.
            env.call_method(view, "performHapticFeedback", "(I)Z", &[JValue::Int(1)])?;
            Ok(())
        });
    }

    pub fn storage(&self, app: &AndroidApp, request_access: bool) {
        self.ui(app, move |env, activity| {
            let sdk = env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?;
            // Older supported Android releases retain the document-picker route.
            if sdk < 30 || !cfg!(feature = "storage-scan") {
                post(Event::ScanUnavailable);
                return Ok(());
            }
            let granted = env
                .call_static_method(
                    "android/os/Environment",
                    "isExternalStorageManager",
                    "()Z",
                    &[],
                )?
                .z()?;
            if !granted {
                if request_access {
                    let action =
                        env.new_string("android.settings.MANAGE_APP_ALL_FILES_ACCESS_PERMISSION")?;
                    let intent = env.new_object(
                        "android/content/Intent",
                        "(Ljava/lang/String;)V",
                        &[JValue::Object(&action)],
                    )?;
                    let package = env
                        .call_method(activity, "getPackageName", "()Ljava/lang/String;", &[])?
                        .l()?;
                    let package: String = env.get_string(&JString::from(package))?.into();
                    let address = env.new_string(format!("package:{package}"))?;
                    let uri = env
                        .call_static_method(
                            "android/net/Uri",
                            "parse",
                            "(Ljava/lang/String;)Landroid/net/Uri;",
                            &[JValue::Object(&address)],
                        )?
                        .l()?;
                    env.call_method(
                        &intent,
                        "setData",
                        "(Landroid/net/Uri;)Landroid/content/Intent;",
                        &[JValue::Object(&uri)],
                    )?;
                    env.call_method(
                        activity,
                        "startActivityForResult",
                        "(Landroid/content/Intent;I)V",
                        &[JValue::Object(&intent), JValue::Int(PICK_STORAGE)],
                    )?;
                } else {
                    post(Event::Storage(None));
                }
                return Ok(());
            }
            let name = env.new_string("storage")?;
            let manager = env
                .call_method(
                    activity,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&name)],
                )?
                .l()?;
            let volumes = env
                .call_method(manager, "getStorageVolumes", "()Ljava/util/List;", &[])?
                .l()?;
            let count = env.call_method(&volumes, "size", "()I", &[])?.i()?;
            let mut roots = Vec::new();
            for i in 0..count.min(64) {
                let volume = env
                    .call_method(&volumes, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])?
                    .l()?;
                let directory = env
                    .call_method(volume, "getDirectory", "()Ljava/io/File;", &[])?
                    .l()?;
                if !directory.is_null() {
                    let path = env
                        .call_method(directory, "getAbsolutePath", "()Ljava/lang/String;", &[])?
                        .l()?;
                    let path: String = env.get_string(&JString::from(path))?.into();
                    roots.push(PathBuf::from(path));
                }
            }
            roots.sort();
            roots.dedup();
            post(Event::Storage(Some(roots)));
            Ok(())
        });
    }

    pub fn insets(&self, app: &AndroidApp) {
        self.ui(app, |env, activity| {
            let window = env
                .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                .l()?;
            let view = env
                .call_method(window, "getDecorView", "()Landroid/view/View;", &[])?
                .l()?;
            let insets = env
                .call_method(
                    &view,
                    "getRootWindowInsets",
                    "()Landroid/view/WindowInsets;",
                    &[],
                )?
                .l()?;
            if insets.is_null() {
                return Ok(());
            }
            let sdk = env
                .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
                .i()?;
            // A cutout margin is reusable only when no system decoration owns
            // its top edge. Requested immersive flags alone do not establish
            // this: Android can keep a call indicator/status bar visible.
            let mut hidden_bars_visible = false;
            let mut status_bar_top = 0;
            let top_margin_available = if sdk >= 30 {
                let bars = env
                    .call_static_method("android/view/WindowInsets$Type", "systemBars", "()I", &[])?
                    .i()?;
                let occupied = env
                    .call_method(
                        &insets,
                        "getInsets",
                        "(I)Landroid/graphics/Insets;",
                        &[JValue::Int(bars)],
                    )?
                    .l()?;
                let status = env
                    .call_static_method("android/view/WindowInsets$Type", "statusBars", "()I", &[])?
                    .i()?;
                let status_visible = env
                    .call_method(&insets, "isVisible", "(I)Z", &[JValue::Int(status)])?
                    .z()?;
                if status_visible {
                    let bounds = env
                        .call_method(
                            &insets,
                            "getInsetsIgnoringVisibility",
                            "(I)Landroid/graphics/Insets;",
                            &[JValue::Int(status)],
                        )?
                        .l()?;
                    status_bar_top = env.get_field(bounds, "top", "I")?.i()?;
                }
                if sdk >= 31 {
                    let navigation = env
                        .call_static_method(
                            "android/view/WindowInsets$Type",
                            "navigationBars",
                            "()I",
                            &[],
                        )?
                        .i()?;
                    hidden_bars_visible = status_visible
                        || env
                            .call_method(&insets, "isVisible", "(I)Z", &[JValue::Int(navigation)])?
                            .z()?;
                }
                // Transient bars can be visible without contributing layout
                // insets. Query status-bar visibility separately as well.
                !status_visible && env.get_field(occupied, "top", "I")?.i()? == 0
            } else {
                // API 28–29 keep the toolbar inside the safe content area.
                false
            };
            let mut edges = [0; 4];
            let mut decoration = [0; 4];
            let mut navigation_edges = [0; 4];
            let mut fullscreen_decoration = [0; 4];
            let mut camera_edges = [0; 4];
            let mut cutouts = Vec::new();
            let size = [
                env.call_method(&view, "getWidth", "()I", &[])?.i()?,
                env.call_method(&view, "getHeight", "()I", &[])?.i()?,
            ];
            for (i, name) in [
                "getSystemWindowInsetLeft",
                "getSystemWindowInsetTop",
                "getSystemWindowInsetRight",
                "getSystemWindowInsetBottom",
            ]
            .iter()
            .enumerate()
            {
                edges[i] = env.call_method(&insets, name, "()I", &[])?.i()?;
            }
            if sdk >= 30 {
                let bars = env
                    .call_static_method("android/view/WindowInsets$Type", "systemBars", "()I", &[])?
                    .i()?;
                let occupied = env
                    .call_method(
                        &insets,
                        "getInsets",
                        "(I)Landroid/graphics/Insets;",
                        &[JValue::Int(bars)],
                    )?
                    .l()?;
                let navigation = env
                    .call_static_method(
                        "android/view/WindowInsets$Type",
                        "navigationBars",
                        "()I",
                        &[],
                    )?
                    .i()?;
                let nav = env
                    .call_method(
                        &insets,
                        "getInsetsIgnoringVisibility",
                        "(I)Landroid/graphics/Insets;",
                        &[JValue::Int(navigation)],
                    )?
                    .l()?;
                let status = env
                    .call_static_method("android/view/WindowInsets$Type", "statusBars", "()I", &[])?
                    .i()?;
                // A revealed status bar overlays fullscreen play instead of
                // shrinking it. Caption/navigation decoration stays reserved.
                let without_status = env
                    .call_method(
                        &insets,
                        "getInsets",
                        "(I)Landroid/graphics/Insets;",
                        &[JValue::Int(bars & !status)],
                    )?
                    .l()?;
                for (i, field) in ["left", "top", "right", "bottom"].iter().enumerate() {
                    decoration[i] = env.get_field(&occupied, field, "I")?.i()?;
                    navigation_edges[i] = env.get_field(&nav, field, "I")?.i()?;
                    fullscreen_decoration[i] = env.get_field(&without_status, field, "I")?.i()?;
                }
            }
            let cutout = env
                .call_method(
                    &insets,
                    "getDisplayCutout",
                    "()Landroid/view/DisplayCutout;",
                    &[],
                )?
                .l()?;
            if !cutout.is_null() {
                let bounds = env
                    .call_method(&cutout, "getBoundingRects", "()Ljava/util/List;", &[])?
                    .l()?;
                let count = env.call_method(&bounds, "size", "()I", &[])?.i()?;
                for i in 0..count.min(16) {
                    let r = env
                        .call_method(&bounds, "get", "(I)Ljava/lang/Object;", &[JValue::Int(i)])?
                        .l()?;
                    let mut rect = [0; 4];
                    for (edge, name) in rect.iter_mut().zip(["left", "top", "right", "bottom"]) {
                        *edge = env.get_field(&r, name, "I")?.i()?;
                    }
                    cutouts.push(rect);
                }
                for (i, name) in [
                    "getSafeInsetLeft",
                    "getSafeInsetTop",
                    "getSafeInsetRight",
                    "getSafeInsetBottom",
                ]
                .iter()
                .enumerate()
                {
                    camera_edges[i] = env.call_method(&cutout, name, "()I", &[])?.i()?;
                }
            }
            let local_side_camera = size[0] > size[1];
            if sdk >= 30 {
                edges = psx_player::input::safe_edges(
                    decoration,
                    navigation_edges,
                    camera_edges,
                    local_side_camera,
                );
            } else {
                for i in 0..4 {
                    edges[i] = edges[i].max(camera_edges[i]);
                }
            }
            let fullscreen_edges = (sdk >= 31).then(|| {
                psx_player::input::safe_edges(
                    fullscreen_decoration,
                    navigation_edges,
                    camera_edges,
                    local_side_camera,
                )
            });
            let resources = env
                .call_method(
                    &view,
                    "getResources",
                    "()Landroid/content/res/Resources;",
                    &[],
                )?
                .l()?;
            let metrics = env
                .call_method(
                    resources,
                    "getDisplayMetrics",
                    "()Landroid/util/DisplayMetrics;",
                    &[],
                )?
                .l()?;
            let density = env.get_field(metrics, "density", "F")?.f()?;
            post(Event::Insets {
                size,
                density,
                edges,
                fullscreen_edges,
                status_bar_top,
                cutouts,
                top_margin_available,
                hidden_bars_visible,
            });
            Ok(())
        });
    }

    pub fn pick(&self, app: &AndroidApp, request: i32) {
        self.ui(app, move |env, activity| {
            let action = env.new_string("android.intent.action.OPEN_DOCUMENT")?;
            let intent = env.new_object(
                "android/content/Intent",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&action)],
            )?;
            let category = env.new_string("android.intent.category.OPENABLE")?;
            let mime = env.new_string("*/*")?;
            env.call_method(
                &intent,
                "addCategory",
                "(Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&category)],
            )?;
            env.call_method(
                &intent,
                "setType",
                "(Ljava/lang/String;)Landroid/content/Intent;",
                &[JValue::Object(&mime)],
            )?;
            env.call_method(
                &intent,
                "addFlags",
                "(I)Landroid/content/Intent;",
                &[JValue::Int(1 | 64)],
            )?;
            if request == PICK_GAME {
                let key = env.new_string("android.intent.extra.ALLOW_MULTIPLE")?;
                env.call_method(
                    &intent,
                    "putExtra",
                    "(Ljava/lang/String;Z)Landroid/content/Intent;",
                    &[JValue::Object(&key), JValue::Bool(1)],
                )?;
            }
            env.call_method(
                activity,
                "startActivityForResult",
                "(Landroid/content/Intent;I)V",
                &[JValue::Object(&intent), JValue::Int(request)],
            )?;
            Ok(())
        });
    }

    pub fn focus(&self, app: &AndroidApp, want: bool) {
        self.ui(app, move |env, activity| {
            let name = env.new_string("audio")?;
            let manager = env
                .call_method(
                    activity,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&name)],
                )?
                .l()?;
            if want {
                let result = env
                    .call_method(
                        &manager,
                        "requestAudioFocus",
                        "(Landroid/media/AudioManager$OnAudioFocusChangeListener;II)I",
                        &[JValue::Object(activity), JValue::Int(3), JValue::Int(1)],
                    )?
                    .i()?;
                post(Event::Focus(if result == 1 { 1 } else { -1 }));
            } else {
                env.call_method(
                    &manager,
                    "abandonAudioFocus",
                    "(Landroid/media/AudioManager$OnAudioFocusChangeListener;)I",
                    &[JValue::Object(activity)],
                )?;
            }
            Ok(())
        });
    }

    pub fn finish(&self, app: &AndroidApp) {
        self.ui(app, |env, a| {
            env.call_method(a, "finish", "()V", &[])?;
            Ok(())
        });
    }

    pub fn open_source(&self, app: &AndroidApp) {
        self.ui(app, |env, activity| {
            let action = env.new_string("android.intent.action.VIEW")?;
            let address = env.new_string("https://github.com/WigglyDon/psx")?;
            let uri = env
                .call_static_method(
                    "android/net/Uri",
                    "parse",
                    "(Ljava/lang/String;)Landroid/net/Uri;",
                    &[JValue::Object(&address)],
                )?
                .l()?;
            let intent = env.new_object(
                "android/content/Intent",
                "(Ljava/lang/String;Landroid/net/Uri;)V",
                &[JValue::Object(&action), JValue::Object(&uri)],
            )?;
            env.call_method(
                activity,
                "startActivity",
                "(Landroid/content/Intent;)V",
                &[JValue::Object(&intent)],
            )?;
            Ok(())
        });
    }

    pub fn import(
        &self,
        root: PathBuf,
        request: i32,
        intent: GlobalRef,
        relink: Option<(Game, usize)>,
    ) -> mpsc::Receiver<Result<Option<Game>, String>> {
        let platform = self.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = platform
                .import_inner(&root, request, &intent)
                .and_then(|selected| {
                    if let (Some((game, index)), Some(selected)) = (relink, &selected) {
                        psx_player::library::relink(
                            game,
                            index,
                            &selected.path,
                            &std::sync::atomic::AtomicBool::new(false),
                        )
                        .map(Some)
                        .map_err(Into::into)
                    } else {
                        Ok(selected)
                    }
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        rx
    }

    fn import_inner(
        &self,
        root: &Path,
        request: i32,
        intent: &GlobalRef,
    ) -> Result<Option<Game>, Box<dyn std::error::Error>> {
        let mut env = self.vm.attach_current_thread()?;
        let resolver = env
            .call_method(
                self.activity.as_obj(),
                "getContentResolver",
                "()Landroid/content/ContentResolver;",
                &[],
            )?
            .l()?;
        let mut uris = Vec::new();
        let clip = env
            .call_method(
                intent.as_obj(),
                "getClipData",
                "()Landroid/content/ClipData;",
                &[],
            )?
            .l()?;
        if !clip.is_null() {
            let count = env.call_method(&clip, "getItemCount", "()I", &[])?.i()?;
            if !(1..=100).contains(&count) {
                return Err("Select one game and at most 99 companion files.".into());
            }
            for i in 0..count {
                let item = env
                    .call_method(
                        &clip,
                        "getItemAt",
                        "(I)Landroid/content/ClipData$Item;",
                        &[JValue::Int(i)],
                    )?
                    .l()?;
                uris.push(
                    env.call_method(item, "getUri", "()Landroid/net/Uri;", &[])?
                        .l()?,
                );
            }
        } else {
            let uri = env
                .call_method(intent.as_obj(), "getData", "()Landroid/net/Uri;", &[])?
                .l()?;
            if uri.is_null() {
                return Err("No file was selected.".into());
            }
            uris.push(uri);
        }
        // A single import may be active. Private staging is never used for play.
        let staging = root.join("importing");
        if staging.exists() {
            fs::remove_dir_all(&staging)?;
        }
        fs::create_dir_all(&staging)?;
        let mut documents = BTreeMap::new();
        for uri in uris {
            let flags = env
                .call_method(intent.as_obj(), "getFlags", "()I", &[])?
                .i()?
                & 1;
            if flags != 0
                && env
                    .call_method(
                        &resolver,
                        "takePersistableUriPermission",
                        "(Landroid/net/Uri;I)V",
                        &[JValue::Object(&uri), JValue::Int(flags)],
                    )
                    .is_err()
            {
                env.exception_clear()?; // Private streamed copy does not depend on persistent provider support.
            }
            let name = document_name(&mut env, &resolver, &uri)?;
            files::safe_name(&name)?;
            if documents
                .keys()
                .any(|key: &String| key.eq_ignore_ascii_case(&name))
            {
                return Err(
                    "Two selected files have the same name. Select only one game and its tracks."
                        .into(),
                );
            }
            let mode = env.new_string("r")?;
            if request == PICK_GAME
                && !matches!(
                    files::extension(&name).as_str(),
                    "cue" | "bin" | "chd" | "pbp" | "iso" | "exe"
                )
            {
                return Err("Select a CHD, PBP, ISO, PS-EXE, or a CUE and its BIN tracks. Extract ZIP/7z archives first.".into());
            }
            let parcel = env
                .call_method(
                    &resolver,
                    "openFileDescriptor",
                    "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;",
                    &[JValue::Object(&uri), JValue::Object(&mode)],
                )?
                .l()?;
            if parcel.is_null() {
                return Err("The document provider could not open the file.".into());
            }
            let descriptor = env.call_method(&parcel, "getFd", "()I", &[])?.i()?;
            // Duplicate ownership before closing ParcelFileDescriptor; never
            // turn a content URI into a guessed filesystem path.
            let owned: RawFd = unsafe { libc::dup(descriptor) };
            env.call_method(&parcel, "close", "()V", &[])?;
            if owned < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let input = unsafe { fs::File::from_raw_fd(owned) };
            let digest = files::copy_document(input, &staging, &name)?;
            documents.insert(name, digest);
            // We retain the independent private copy. Release grants after
            // import rather than accumulating the provider's limited quota.
            if flags != 0
                && env
                    .call_method(
                        &resolver,
                        "releasePersistableUriPermission",
                        "(Landroid/net/Uri;I)V",
                        &[JValue::Object(&uri), JValue::Int(flags)],
                    )
                    .is_err()
            {
                env.exception_clear()?;
            }
        }
        if request == PICK_FIRMWARE || request == PICK_PS2_FIRMWARE {
            if documents.len() != 1 {
                return Err("Select one firmware file.".into());
            }
            let name = documents.keys().next().unwrap();
            let path = staging.join(name);
            let size = fs::metadata(&path)?.len();
            if request == PICK_PS2_FIRMWARE && size != 4 * 1024 * 1024 {
                return Err("Select a compatible 4 MiB PS2 firmware dump.".into());
            }
            if request == PICK_FIRMWARE && ![512 * 1024, 4 * 1024 * 1024, 0x3E66F0].contains(&size)
            {
                return Err("That file is not a supported PS1 firmware size. Select a compatible firmware dump.".into());
            }
            let system = root.join(if request == PICK_PS2_FIRMWARE {
                "system/ps2/bios"
            } else {
                "system"
            });
            fs::create_dir_all(&system)?;
            fs::rename(&path, system.join(name))?;
            fs::remove_dir_all(staging)?;
            Ok(None)
        } else {
            Ok(Some(files::finish_import(root, &staging, &documents)?))
        }
    }
}

fn document_name(env: &mut JNIEnv, resolver: &JObject, uri: &JObject) -> JResult<String> {
    let cursor=env.call_method(resolver,"query","(Landroid/net/Uri;[Ljava/lang/String;Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;)Landroid/database/Cursor;",&[JValue::Object(uri),JValue::Object(&JObject::null()),JValue::Object(&JObject::null()),JValue::Object(&JObject::null()),JValue::Object(&JObject::null())])?.l()?;
    let result = (|| {
        if !cursor.is_null() && env.call_method(&cursor, "moveToFirst", "()Z", &[])?.z()? {
            let column = env.new_string("_display_name")?;
            let index = env
                .call_method(
                    &cursor,
                    "getColumnIndex",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&column)],
                )?
                .i()?;
            if index >= 0 {
                let name: JString = env
                    .call_method(
                        &cursor,
                        "getString",
                        "(I)Ljava/lang/String;",
                        &[JValue::Int(index)],
                    )?
                    .l()?
                    .into();
                return Ok(env.get_string(&name)?.into());
            }
        }
        let last: JString = env
            .call_method(uri, "getLastPathSegment", "()Ljava/lang/String;", &[])?
            .l()?
            .into();
        let name = env.get_string(&last)?.into();
        Ok(name)
    })();
    if !cursor.is_null() {
        env.call_method(&cursor, "close", "()V", &[])?;
    }
    result
}
