# Android development build

Clone with `git clone --recurse-submodules https://github.com/WigglyDon/psx`.
For an existing checkout, run `git submodule update --init --recursive`.
The submodule must be the exact revision in `upstream.lock.json`; the build
rejects a different revision, modified tracked core source or license hash.

The development build uses Rust 1.96.1 (pinned in `rust-toolchain.toml`), the
`aarch64-linux-android` Rust target, Android NDK 30.0.16248370, Android build
tools 37.0.0, compile platform `android-37.0`, Java with the `javac` module,
CMake 4.3, Ninja and the standard `zip`/`unzip` utilities. Android 9/API 28
is the minimum OS; the manifest targets API 36. Compile SDK and target SDK
are separate. The first build needs ordinary crates.io access; `Cargo.lock`
pins the resolved packages. SDK, NDK and JDK are build tools, not copied
from another application's source.

```sh
export ANDROID_SDK_ROOT=/path/to/android-sdk
export ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/30.0.16248370"
export ANDROID_BUILD_TOOLS=37.0.0
export ANDROID_PLATFORM=android-37.0
cargo fetch --locked
cargo test --workspace --locked
cargo xtask apk
```

The result is `build/psx-development.apk`. `cargo xtask verify` repeats its
signature, ZIP alignment, AArch64 ELF alignment and required-entry checks.
Raw build logs and APKs stay outside Git. Development signing material stays
under ignored `.local/signing/`; retain that key to update an installation
without losing its data. The development key is not a store signing key.

`xtask` is Rust. It builds the unmodified upstream CMake target, statically
links its C++ runtime, builds the Rust NativeActivity library, generates the
callback forwarding class described below, compiles it with Android's D8,
packages licenses, aligns the APK and signs it. No Gradle application layer,
managed UI renderer or native-core downloader is involved.

The native libraries use ELF LOAD alignment of at least 16 KB; the APK is
checked with `zipalign -c -P 16`. These checks establish alignment, not
16 KB runtime qualification. The donor's JIT guard and host-page constants
assume 4 KB. The Rust host selects its existing cached interpreter on other
page sizes, preserving a path to testing without changing core internals.
Actual speed and complete memory mapping behavior on 16 KB Android remain
unverified until that environment runs the application.

## The Android callback shim

`android-activity` supplies the NativeActivity lifecycle and input callbacks,
but Android's activity-result, dialog-listener and audio-focus-listener
callbacks are Java virtual methods/interfaces. The build generates one small
`PsxActivity` class, extending NativeActivity and forwarding those signatures
to exported Rust JNI functions. It contains no picker, UI, file, save, input,
audio or emulation policy. All those decisions and SDK calls are Rust.
This is the minimal platform callback exception allowed by `AGENTS.md`.

## Device verification

Build before acquiring the device. Follow the current installed stock-device
procedure, its private target binding, standing preferences and exclusive
transaction. Do not select an arbitrary first ADB device or reinstall by
uninstalling an existing app. Install this APK as `com.machinerhythm.psx` and
preserve existing data and the signing key across updates.

Use the app's Open game action and Android document picker. A recent game is
an imported private copy, so reopening does not depend on a provider retaining
the URI grant. Use the scope and acceptance observations in
[FIRST-PLAYABLE-1](https://github.com/WigglyDon/psx/issues/1). Report actual
gameplay, audible output, simultaneous input, card saving after process
restart and fold/lifecycle transitions separately from host checks. The
`psx` log tag emits bounded ten-second playback counters outside the normal
UI. Keep raw device logs and media private.

An AAB/store package and physical iOS check are later delivery work; this APK
does not establish either store's acceptance.
