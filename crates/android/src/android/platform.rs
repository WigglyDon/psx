use android_activity::AndroidApp;
use jni::{
    objects::{GlobalRef, JObject, JString, JValue},
    sys::{jint, jobject},
    JNIEnv, JavaVM,
};
use psx_player::files::{self, Game};
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    os::fd::{FromRawFd, RawFd},
    path::{Path, PathBuf},
    sync::{mpsc, Arc, Mutex},
};

pub const PICK_GAME: i32 = 101;
pub const PICK_FIRMWARE: i32 = 102;

pub enum Event {
    Menu(i32),
    Documents(i32, Option<GlobalRef>),
    Focus(i32),
    Error(String),
    Insets([i32; 4]),
}
static EVENTS: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());
fn post(event: Event) {
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
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_documentResult(
    env: JNIEnv,
    _this: JObject,
    request: jint,
    result: jint,
    intent: JObject,
) {
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
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_dialogResult(
    _env: JNIEnv,
    _this: JObject,
    index: jint,
) {
    post(Event::Menu(index));
}
#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PsxActivity_dialogCancel(
    _env: JNIEnv,
    _this: JObject,
) {
    post(Event::Menu(-1));
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
    pub unsafe fn new(app: &AndroidApp) -> Result<Self, String> {
        let vm = JavaVM::from_raw(app.vm_as_ptr() as _).map_err(|e| e.to_string())?;
        let activity = {
            let env = vm.attach_current_thread().map_err(|e| e.to_string())?;
            let local = JObject::from_raw(app.activity_as_ptr() as jobject);
            let global = env.new_global_ref(&local).map_err(|e| e.to_string())?;
            // Android owns this reference, not this local JObject wrapper.
            let _ = local.into_raw();
            global
        };
        Ok(Self {
            vm: Arc::new(vm),
            activity,
        })
    }

    fn ui(
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
    }

    pub fn menu(&self, app: &AndroidApp, title: String, message: String, items: Vec<String>) {
        self.ui(app,move |env,activity| {
            let builder=env.new_object("android/app/AlertDialog$Builder","(Landroid/content/Context;)V",&[JValue::Object(activity)])?;
            let title=env.new_string(title)?;let message=env.new_string(message)?;
            env.call_method(&builder,"setTitle","(Ljava/lang/CharSequence;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(&title)])?;
            let explanation=env.new_object("android/widget/TextView","(Landroid/content/Context;)V",&[JValue::Object(activity)])?;
            env.call_method(&explanation,"setText","(Ljava/lang/CharSequence;)V",&[JValue::Object(&message)])?;
            env.call_method(&explanation,"setPadding","(IIII)V",&[JValue::Int(32),JValue::Int(16),JValue::Int(32),JValue::Int(24)])?;
            env.call_method(&builder,"setView","(Landroid/view/View;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(&explanation)])?;
            let array=env.new_object_array(items.len() as i32,"java/lang/CharSequence",JObject::null())?;
            for (i,item) in items.iter().enumerate() { let text=env.new_string(item)?;env.set_object_array_element(&array,i as i32,text)?; }
            env.call_method(&builder,"setItems","([Ljava/lang/CharSequence;Landroid/content/DialogInterface$OnClickListener;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(&array),JValue::Object(activity)])?;
            env.call_method(&builder,"setOnCancelListener","(Landroid/content/DialogInterface$OnCancelListener;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(activity)])?;
            env.call_method(&builder,"show","()Landroid/app/AlertDialog;",&[])?;
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
                    view,
                    "getRootWindowInsets",
                    "()Landroid/view/WindowInsets;",
                    &[],
                )?
                .l()?;
            if insets.is_null() {
                return Ok(());
            }
            let mut edges = [0; 4];
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
            let cutout = env
                .call_method(
                    &insets,
                    "getDisplayCutout",
                    "()Landroid/view/DisplayCutout;",
                    &[],
                )?
                .l()?;
            if !cutout.is_null() {
                for (i, name) in [
                    "getSafeInsetLeft",
                    "getSafeInsetTop",
                    "getSafeInsetRight",
                    "getSafeInsetBottom",
                ]
                .iter()
                .enumerate()
                {
                    edges[i] = edges[i].max(env.call_method(&cutout, name, "()I", &[])?.i()?);
                }
            }
            post(Event::Insets(edges));
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

    pub fn notices(&self, app: &AndroidApp, body: String) {
        self.ui(app,move|env,activity| {
            let builder=env.new_object("android/app/AlertDialog$Builder","(Landroid/content/Context;)V",&[JValue::Object(activity)])?;
            let title=env.new_string("Source, licenses and privacy")?;
            env.call_method(&builder,"setTitle","(Ljava/lang/CharSequence;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(&title)])?;
            let scroll=env.new_object("android/widget/ScrollView","(Landroid/content/Context;)V",&[JValue::Object(activity)])?;
            let text=env.new_object("android/widget/TextView","(Landroid/content/Context;)V",&[JValue::Object(activity)])?;
            let content=env.new_string(body)?;
            env.call_method(&text,"setAutoLinkMask","(I)V",&[JValue::Int(1)])?;
            env.call_method(&text,"setText","(Ljava/lang/CharSequence;)V",&[JValue::Object(&content)])?;
            env.call_method(&text,"setPadding","(IIII)V",&[JValue::Int(32),JValue::Int(16),JValue::Int(32),JValue::Int(24)])?;
            env.call_method(&scroll,"addView","(Landroid/view/View;)V",&[JValue::Object(&text)])?;
            env.call_method(&builder,"setView","(Landroid/view/View;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(&scroll)])?;
            let back=env.new_string("Back")?;
            env.call_method(&builder,"setPositiveButton","(Ljava/lang/CharSequence;Landroid/content/DialogInterface$OnClickListener;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(&back),JValue::Object(activity)])?;
            env.call_method(&builder,"setOnCancelListener","(Landroid/content/DialogInterface$OnCancelListener;)Landroid/app/AlertDialog$Builder;",&[JValue::Object(activity)])?;
            env.call_method(&builder,"show","()Landroid/app/AlertDialog;",&[])?;
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

    pub fn import(
        &self,
        root: PathBuf,
        request: i32,
        intent: GlobalRef,
    ) -> mpsc::Receiver<Result<Option<Game>, String>> {
        let platform = self.clone();
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = platform
                .import_inner(&root, request, &intent)
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
        if request == PICK_FIRMWARE {
            if documents.len() != 1 {
                return Err("Select one PS1 firmware file.".into());
            }
            let name = documents.keys().next().unwrap();
            let path = staging.join(name);
            let size = fs::metadata(&path)?.len();
            if ![512 * 1024, 4 * 1024 * 1024, 0x3E66F0].contains(&size) {
                return Err("That file is not a supported PS1 firmware size. Select a compatible firmware dump.".into());
            }
            fs::create_dir_all(root.join("system"))?;
            fs::rename(&path, root.join("system").join(name))?;
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
