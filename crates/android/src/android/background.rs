//! Android's mandatory service callbacks forward here; all policy is Rust.
use super::platform::{post, Event};
use jni::{
    objects::{JObject, JValue},
    sys::jint,
    JNIEnv,
};
type Result<T> = jni::errors::Result<T>;

fn intent<'a>(env: &mut JNIEnv<'a>, class: &str) -> Result<JObject<'a>> {
    let intent = env.new_object("android/content/Intent", "()V", &[])?;
    let package = env.new_string("com.machinerhythm.psx")?;
    let class = env.new_string(class)?;
    env.call_method(
        &intent,
        "setClassName",
        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&package), JValue::Object(&class)],
    )?;
    Ok(intent)
}
pub fn set(env: &mut JNIEnv, activity: &JObject, enabled: bool) -> Result<()> {
    let intent = intent(env, "com.machinerhythm.psx.PlaybackService")?;
    if enabled {
        env.call_method(
            activity,
            "startForegroundService",
            "(Landroid/content/Intent;)Landroid/content/ComponentName;",
            &[JValue::Object(&intent)],
        )?;
    } else {
        env.call_method(
            activity,
            "stopService",
            "(Landroid/content/Intent;)Z",
            &[JValue::Object(&intent)],
        )?;
    }
    Ok(())
}
fn notification(env: &mut JNIEnv, service: &JObject) -> Result<()> {
    let name = env.new_string("notification")?;
    let manager = env
        .call_method(
            service,
            "getSystemService",
            "(Ljava/lang/String;)Ljava/lang/Object;",
            &[JValue::Object(&name)],
        )?
        .l()?;
    let channel_id = env.new_string("background-playback")?;
    let channel_name = env.new_string("Background play")?;
    let channel = env.new_object(
        "android/app/NotificationChannel",
        "(Ljava/lang/String;Ljava/lang/CharSequence;I)V",
        &[
            JValue::Object(&channel_id),
            JValue::Object(&channel_name),
            JValue::Int(2),
        ],
    )?;
    env.call_method(
        manager,
        "createNotificationChannel",
        "(Landroid/app/NotificationChannel;)V",
        &[JValue::Object(&channel)],
    )?;
    let builder = env.new_object(
        "android/app/Notification$Builder",
        "(Landroid/content/Context;Ljava/lang/String;)V",
        &[JValue::Object(service), JValue::Object(&channel_id)],
    )?;
    let info = env
        .call_method(
            service,
            "getApplicationInfo",
            "()Landroid/content/pm/ApplicationInfo;",
            &[],
        )?
        .l()?;
    let icon = env.get_field(info, "icon", "I")?.i()?;
    env.call_method(
        &builder,
        "setSmallIcon",
        "(I)Landroid/app/Notification$Builder;",
        &[JValue::Int(icon)],
    )?;
    for (method, text) in [
        ("setContentTitle", "psx"),
        ("setContentText", "Background play enabled · tap to return"),
    ] {
        let text = env.new_string(text)?;
        env.call_method(
            &builder,
            method,
            "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;",
            &[JValue::Object(&text)],
        )?;
    }
    env.call_method(
        &builder,
        "setOngoing",
        "(Z)Landroid/app/Notification$Builder;",
        &[JValue::Bool(1)],
    )?;
    let open = intent(env, "com.machinerhythm.psx.PsxActivity")?;
    env.call_method(
        &open,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(0x24000000)],
    )?;
    let pending = env
        .call_static_method(
            "android/app/PendingIntent",
            "getActivity",
            "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
            &[
                JValue::Object(service),
                JValue::Int(0),
                JValue::Object(&open),
                JValue::Int(0x0c000000),
            ],
        )?
        .l()?;
    env.call_method(
        &builder,
        "setContentIntent",
        "(Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;",
        &[JValue::Object(&pending)],
    )?;
    let pause = intent(env, "com.machinerhythm.psx.PlaybackService")?;
    let action = env.new_string("psx.PAUSE")?;
    env.call_method(
        &pause,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&action)],
    )?;
    let pending = env
        .call_static_method(
            "android/app/PendingIntent",
            "getService",
            "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;",
            &[
                JValue::Object(service),
                JValue::Int(1),
                JValue::Object(&pause),
                JValue::Int(0x0c000000),
            ],
        )?
        .l()?;
    let label = env.new_string("Pause")?;
    env.call_method(
        &builder,
        "addAction",
        "(ILjava/lang/CharSequence;Landroid/app/PendingIntent;)Landroid/app/Notification$Builder;",
        &[
            JValue::Int(0),
            JValue::Object(&label),
            JValue::Object(&pending),
        ],
    )?;
    let notification = env
        .call_method(builder, "build", "()Landroid/app/Notification;", &[])?
        .l()?;
    let sdk = env
        .get_static_field("android/os/Build$VERSION", "SDK_INT", "I")?
        .i()?;
    if sdk >= 29 {
        env.call_method(
            service,
            "startForeground",
            "(ILandroid/app/Notification;I)V",
            &[
                JValue::Int(1),
                JValue::Object(&notification),
                JValue::Int(2),
            ],
        )?;
    } else {
        env.call_method(
            service,
            "startForeground",
            "(ILandroid/app/Notification;)V",
            &[JValue::Int(1), JValue::Object(&notification)],
        )?;
    }
    Ok(())
}
#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PlaybackService_started(
    mut env: JNIEnv,
    service: JObject,
    intent: JObject,
) -> jint {
    let result: Result<()> = env.with_local_frame(64, |env| {
        if !intent.is_null() {
            let action = env
                .call_method(intent, "getAction", "()Ljava/lang/String;", &[])?
                .l()?;
            if !action.is_null()
                && env
                    .get_string(&action.into())?
                    .to_str()
                    .is_ok_and(|s| s == "psx.PAUSE")
            {
                post(Event::PauseBackground);
                env.call_method(&service, "stopSelf", "()V", &[])?;
                return Ok(());
            }
        }
        notification(env, &service)?;
        post(Event::Background(true));
        Ok(())
    });
    if let Err(error) = result {
        let _ = env.exception_clear();
        log::error!("Background playback service: {error}");
        post(Event::Error(
            "Android could not start background play. The game is paused.".into(),
        ));
        let _ = env.call_method(service, "stopSelf", "()V", &[]);
    }
    2 // START_NOT_STICKY: never resurrect playback without the player.
}
#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_PlaybackService_stopped(
    _env: JNIEnv,
    _service: JObject,
) {
    post(Event::Background(false));
}
