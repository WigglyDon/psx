#[cfg(all(target_os = "android", not(psx_minimal_host)))]
mod android;
#[cfg(all(target_os = "android", psx_minimal_host))]
mod minimal;

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("psx")
            .with_max_level(log::LevelFilter::Info),
    );
    #[cfg(not(psx_minimal_host))]
    let result = android::run(app);
    #[cfg(psx_minimal_host)]
    let result = minimal::run(app);
    if let Err(error) = result {
        log::error!("Player stopped: {error}");
    }
}
