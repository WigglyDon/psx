fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("android") {
        // khronos-egl's no-pkg-config feature leaves platform linkage to us.
        println!("cargo:rustc-link-lib=EGL");
    }
}
