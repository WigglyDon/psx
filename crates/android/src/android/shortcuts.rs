//! Public SDK shortcut calls. Intent identity and all product policy stay Rust.
use super::platform::{post, Event, Platform};
use android_activity::AndroidApp;
use jni::{
    objects::{JObject, JString, JValue},
    JNIEnv,
};
use psx_player::{
    artwork::Artwork,
    files::Game,
    session::{GameId, Launch},
};
use std::path::Path;
type Result<T> = jni::errors::Result<T>;

fn game_intent<'a>(env: &mut JNIEnv<'a>, context: &JObject, id: &GameId) -> Result<JObject<'a>> {
    let intent = env.new_object("android/content/Intent", "()V", &[])?;
    let package = env.new_string("com.machinerhythm.psx")?;
    let class = env.new_string("com.machinerhythm.psx.PsxActivity")?;
    env.call_method(
        &intent,
        "setClassName",
        "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&package), JValue::Object(&class)],
    )?;
    let action = env.new_string("android.intent.action.VIEW")?;
    env.call_method(
        &intent,
        "setAction",
        "(Ljava/lang/String;)Landroid/content/Intent;",
        &[JValue::Object(&action)],
    )?;
    let url = env.new_string(Launch::OpenGame(id.clone()).url())?;
    let uri = env
        .call_static_method(
            "android/net/Uri",
            "parse",
            "(Ljava/lang/String;)Landroid/net/Uri;",
            &[JValue::Object(&url)],
        )?
        .l()?;
    env.call_method(
        &intent,
        "setData",
        "(Landroid/net/Uri;)Landroid/content/Intent;",
        &[JValue::Object(&uri)],
    )?;
    env.call_method(
        &intent,
        "addFlags",
        "(I)Landroid/content/Intent;",
        &[JValue::Int(0x10000000 | 0x04000000)],
    )?;
    let _ = context;
    Ok(intent)
}
fn info<'a>(
    env: &mut JNIEnv<'a>,
    context: &JObject,
    game: &Game,
    artwork: &Artwork,
) -> Result<JObject<'a>> {
    let id = GameId::parse(&game.id).expect("validated at SDK boundary");
    let key = env.new_string(format!("game.{}", id.as_str()))?;
    let builder = env.new_object(
        "android/content/pm/ShortcutInfo$Builder",
        "(Landroid/content/Context;Ljava/lang/String;)V",
        &[JValue::Object(context), JValue::Object(&key)],
    )?;
    let title = env.new_string(&game.title)?;
    env.call_method(
        &builder,
        "setShortLabel",
        "(Ljava/lang/CharSequence;)Landroid/content/pm/ShortcutInfo$Builder;",
        &[JValue::Object(&title)],
    )?;
    let intent = game_intent(env, context, &id)?;
    env.call_method(
        &builder,
        "setIntent",
        "(Landroid/content/Intent;)Landroid/content/pm/ShortcutInfo$Builder;",
        &[JValue::Object(&intent)],
    )?;
    let mut pixels = vec![0i32; 128 * 128];
    let [r, g, b] = artwork.color.map(|v| (v * 255.) as u32);
    let background = (0xff000000 | r << 16 | g << 8 | b) as i32;
    pixels.fill(background);
    if let Some(frame) = artwork.icon.as_ref().and_then(|icon| icon.frames.first()) {
        for y in 0..96 {
            for x in 0..96 {
                let [r, g, b, a] = frame[(y / 6) * 16 + x / 6];
                if a != 0 {
                    pixels[(y + 16) * 128 + x + 16] =
                        (0xff000000u32 | (r as u32) << 16 | (g as u32) << 8 | b as u32) as i32;
                }
            }
        }
    }
    let array = env.new_int_array(pixels.len() as i32)?;
    env.set_int_array_region(&array, 0, &pixels)?;
    let config = env
        .get_static_field(
            "android/graphics/Bitmap$Config",
            "ARGB_8888",
            "Landroid/graphics/Bitmap$Config;",
        )?
        .l()?;
    let bitmap = env
        .call_static_method(
            "android/graphics/Bitmap",
            "createBitmap",
            "([IIILandroid/graphics/Bitmap$Config;)Landroid/graphics/Bitmap;",
            &[
                JValue::Object(&array),
                JValue::Int(128),
                JValue::Int(128),
                JValue::Object(&config),
            ],
        )?
        .l()?;
    let bitmap = if artwork.icon.is_none() {
        env.call_method(
            &bitmap,
            "copy",
            "(Landroid/graphics/Bitmap$Config;Z)Landroid/graphics/Bitmap;",
            &[JValue::Object(&config), JValue::Bool(1)],
        )?
        .l()?
    } else {
        bitmap
    };
    if artwork.icon.is_none() {
        let canvas = env.new_object(
            "android/graphics/Canvas",
            "(Landroid/graphics/Bitmap;)V",
            &[JValue::Object(&bitmap)],
        )?;
        let paint = env.new_object("android/graphics/Paint", "(I)V", &[JValue::Int(1)])?;
        env.call_method(&paint, "setColor", "(I)V", &[JValue::Int(-1)])?;
        env.call_method(&paint, "setTextSize", "(F)V", &[JValue::Float(72.)])?;
        let align = env
            .get_static_field(
                "android/graphics/Paint$Align",
                "CENTER",
                "Landroid/graphics/Paint$Align;",
            )?
            .l()?;
        env.call_method(
            &paint,
            "setTextAlign",
            "(Landroid/graphics/Paint$Align;)V",
            &[JValue::Object(&align)],
        )?;
        let letter = env.new_string(artwork.initial.to_string())?;
        env.call_method(
            &canvas,
            "drawText",
            "(Ljava/lang/String;FFLandroid/graphics/Paint;)V",
            &[
                JValue::Object(&letter),
                JValue::Float(64.),
                JValue::Float(90.),
                JValue::Object(&paint),
            ],
        )?;
    }
    let icon = env
        .call_static_method(
            "android/graphics/drawable/Icon",
            "createWithBitmap",
            "(Landroid/graphics/Bitmap;)Landroid/graphics/drawable/Icon;",
            &[JValue::Object(&bitmap)],
        )?
        .l()?;
    env.call_method(
        &builder,
        "setIcon",
        "(Landroid/graphics/drawable/Icon;)Landroid/content/pm/ShortcutInfo$Builder;",
        &[JValue::Object(&icon)],
    )?;
    env.call_method(
        &builder,
        "build",
        "()Landroid/content/pm/ShortcutInfo;",
        &[],
    )?
    .l()
}
impl Platform {
    pub fn refresh_game_icons(&self, app: &AndroidApp, games: Vec<Game>, root: &Path) {
        let root = root.to_owned();
        self.ui(app, move |env, activity| {
            let service = env.new_string("shortcut")?;
            let manager = env
                .call_method(
                    activity,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&service)],
                )?
                .l()?;
            if manager.is_null() {
                return Ok(());
            }
            let pinned = env
                .call_method(&manager, "getPinnedShortcuts", "()Ljava/util/List;", &[])?
                .l()?;
            let count = env.call_method(&pinned, "size", "()I", &[])?.i()?;
            for index in 0..count {
                env.with_local_frame(64, |env| -> Result<()> {
                    let shortcut = env
                        .call_method(
                            &pinned,
                            "get",
                            "(I)Ljava/lang/Object;",
                            &[JValue::Int(index)],
                        )?
                        .l()?;
                    let key = env
                        .call_method(&shortcut, "getId", "()Ljava/lang/String;", &[])?
                        .l()?;
                    let key: String = env.get_string(&JString::from(key))?.into();
                    let Some(id) = key
                        .strip_prefix("game.")
                        .and_then(|s| GameId::parse(s).ok())
                    else {
                        return Ok(());
                    };
                    let Some(game) = games.iter().find(|g| g.id == id.as_str()) else {
                        return Ok(());
                    };
                    let artwork = Artwork::local(
                        &game.id,
                        &game.title,
                        &root.join("saves").join(format!("{}.srm", game.id)),
                    );
                    let updated = info(env, activity, game, &artwork)?;
                    let list = env
                        .call_static_method(
                            "java/util/Collections",
                            "singletonList",
                            "(Ljava/lang/Object;)Ljava/util/List;",
                            &[JValue::Object(&updated)],
                        )?
                        .l()?;
                    env.call_method(
                        &manager,
                        "updateShortcuts",
                        "(Ljava/util/List;)Z",
                        &[JValue::Object(&list)],
                    )?;
                    Ok(())
                })?;
            }
            Ok(())
        });
    }
    pub fn pin_game(&self, app: &AndroidApp, game: Game, root: &Path) {
        if GameId::parse(&game.id).is_err() {
            return;
        }
        let artwork = Artwork::local(
            &game.id,
            &game.title,
            &root.join("saves").join(format!("{}.srm", game.id)),
        );
        self.ui(app, move |env, activity| {
            let name = env.new_string("shortcut")?;
            let manager = env.call_method(activity, "getSystemService", "(Ljava/lang/String;)Ljava/lang/Object;", &[JValue::Object(&name)])?.l()?;
            if manager.is_null() || !env.call_method(&manager, "isRequestPinShortcutSupported", "()Z", &[])?.z()? {
                post(Event::Notice("This Home Screen does not support pinned game icons.".into()));
                return Ok(());
            }
            let shortcut = info(env, activity, &game, &artwork)?;
            let callback = env.new_object("android/content/Intent", "()V", &[])?;
            let package = env.new_string("com.machinerhythm.psx")?;
            let class = env.new_string("com.machinerhythm.psx.ShortcutReceiver")?;
            env.call_method(&callback, "setClassName", "(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;", &[JValue::Object(&package), JValue::Object(&class)])?;
            let data = env.new_string(format!("machinerhythm-psx://pinned/{}", game.id))?;
            let uri = env.call_static_method("android/net/Uri", "parse", "(Ljava/lang/String;)Landroid/net/Uri;", &[JValue::Object(&data)])?.l()?;
            env.call_method(&callback, "setData", "(Landroid/net/Uri;)Landroid/content/Intent;", &[JValue::Object(&uri)])?;
            let pending = env.call_static_method("android/app/PendingIntent", "getBroadcast", "(Landroid/content/Context;ILandroid/content/Intent;I)Landroid/app/PendingIntent;", &[JValue::Object(activity), JValue::Int(0), JValue::Object(&callback), JValue::Int(0x04000000 | 0x08000000)])?.l()?;
            let sender = env.call_method(&pending, "getIntentSender", "()Landroid/content/IntentSender;", &[])?.l()?;
            let accepted = env.call_method(&manager, "requestPinShortcut", "(Landroid/content/pm/ShortcutInfo;Landroid/content/IntentSender;)Z", &[JValue::Object(&shortcut), JValue::Object(&sender)])?.z()?;
            post(Event::Notice(if accepted { "Confirm the game icon on your Home Screen." } else { "The Home Screen declined this game icon." }.into()));
            Ok(())
        });
    }
}
#[no_mangle]
pub extern "system" fn Java_com_machinerhythm_psx_ShortcutReceiver_confirmed(
    mut env: JNIEnv,
    _this: JObject,
    context: JObject,
    intent: JObject,
) {
    let result: Result<()> = (|| {
        let data = env
            .call_method(&intent, "getDataString", "()Ljava/lang/String;", &[])?
            .l()?;
        if data.is_null() {
            return Ok(());
        }
        let data: String = env.get_string(&JString::from(data))?.into();
        let Some(id) = data
            .strip_prefix("machinerhythm-psx://pinned/")
            .and_then(|s| GameId::parse(s).ok())
        else {
            return Ok(());
        };
        let files = env
            .call_method(&context, "getFilesDir", "()Ljava/io/File;", &[])?
            .l()?;
        let path = env
            .call_method(&files, "getAbsolutePath", "()Ljava/lang/String;", &[])?
            .l()?;
        let path: String = env.get_string(&JString::from(path))?.into();
        let target = Path::new(&path)
            .join("shortcuts")
            .join(format!("{}.confirmed", id.as_str()));
        if let Err(error) = psx_player::saves::atomic_write(&target, b"pinned\n") {
            log::error!("Shortcut confirmation could not be recorded: {error}");
        }
        post(Event::Notice("Game icon added to your Home Screen.".into()));
        Ok(())
    })();
    if result.is_err() {
        let _ = env.exception_clear();
        log::error!("Shortcut confirmation could not be read");
    }
}
