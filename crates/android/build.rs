fn main() {
    println!("cargo:rustc-check-cfg=cfg(psx_minimal_host)");
    println!("cargo:rerun-if-env-changed=PSX_MINIMAL_RENDERER_HOST");
    println!("cargo:rerun-if-env-changed=PSX_MINIMAL_SOURCE_REVISION");
    if std::env::var("PSX_MINIMAL_RENDERER_HOST").as_deref() == Ok("1") {
        assert!(
            std::env::var_os("CARGO_FEATURE_PRODUCER_LOCAL_OUTPUT_SYNC").is_some(),
            "minimal host requires the frozen creator reference contract"
        );
        println!("cargo:rustc-cfg=psx_minimal_host");
        println!(
            "cargo:rustc-env=PSX_MINIMAL_SOURCE_REVISION={}",
            std::env::var("PSX_MINIMAL_SOURCE_REVISION").unwrap_or_else(|_| "unbound-check".into())
        );
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // khronos-egl's no-pkg-config feature leaves platform linkage to us.
        println!("cargo:rustc-link-lib=EGL");
    }
}
