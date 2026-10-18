#[cfg(target_os = "android")]
mod android;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("psx")
            .with_max_level(log::LevelFilter::Info),
    );
    if let Err(error) = android::run(app) {
        log::error!("Player stopped: {error}");
    }
}
