# FIRST-PLAYABLE-1 observations

## Build checkpoint, 2026-10-18

Source scope: native Rust Android host and shared player, unmodified
SwanStation `e436e9b85374def58d211df7f78880f0f4a917b4`, generated callback-only
NativeActivity subclass. Exact build dependencies and reproduction are in
[build.md](build.md) and `Cargo.lock`.

- `cargo test --workspace --locked --offline`: six player tests pass.
  These cover simultaneous/diagonal input and cancellation, intentional pause,
  bounded audio wrap/overflow/silence, companion path rejection, missing-track
  import/reopen and atomic card reopening/corrupt-length preservation.
- Host and Android ARM64 Clippy with `-D warnings`: pass.
- Development APK: `build/psx-development.apk`, about 8.3 MiB, package
  `com.machinerhythm.psx`, minimum API 28, target API 36.
- APK SHA-256:
  `d366e40dda0eb9be2879941e9ebb73330cd325c163d5a6af534aead61e1054a2`.
- APK signature verification and 16 KB ZIP alignment: pass.
- Both packaged native libraries: AArch64 ELF, every LOAD segment aligned
  to at least 16 KB. C++ runtime is statically linked; no unbundled
  `libc++_shared.so` dependency.
- Upstream license hash matches the foundation. Original core/dependency
  license files, embedded notices, Rust runtime and NDK notices are retained
  in the APK. Source submodule is unchanged.

## First device checkpoint, 2026-10-18

The stock Fold is running the development APK. Device testing found and
corrected three host integration failures: the missing explicit EGL link,
JNI callback lookup without a class-loader library association, and the
`ndk` 0.9 wrapper rejecting AAudio's positive buffer-size result. Link-time
undefined-symbol rejection and an APK EGL-dependency check now cover the
first failure. Rust layout also accounts for system bars and cutout insets.

- Current local APK SHA-256:
  `b4744d0304263d624fb04c9e51b56aa7403644d992987916a2ca30e82d8e9ee9`.
- Installation/update preserves app data. Native menus and the ordinary
  system document picker work, including multi-select CUE/BIN import.
- An owner-selected Crash Bandicoot disc boots with the bundled OpenBIOS
  fallback. The first level renders and responds to direction input.
- The GLES path is active on the physical GPU through Android's ANGLE
  driver. Core counters show hardware frames and zero software frames.
- Initial ten-second windows through boot/title run at 59.61–59.74 fps,
  99.7–99.9% of the core's 59.8173 Hz, with 3.25–3.84 ms average core time.
  This is an initial observation, not a sustained-gameplay result.
- AAudio opens at 44.1 kHz stereo, 89-frame bursts, 267-frame device buffer.
  The owner confirms clear audible game sound. Initial buffering dropped
  787 frames; 44 callback underruns were counted over the first 50 seconds.
  Counters and audible quality are separate observations.
- Android ARM64 Clippy and formatting pass after these fixes. Signature,
  ZIP/ELF alignment and required-entry checks pass on the updated APK.

The owner reports that simultaneous movement/jump/spin controls and the
requested unfold/resume check work. Direct captures show moving first-level
gameplay. A 70-second window during owner play measures 59.80–59.84 fps
(100.0% rounded), 3.53–5.26 ms average core time per frame, zero software
frames, and no new FIFO underruns or dropped frames. The initial counters
remain 72 underruns and 787 dropped frames; this observation does not erase
startup interruptions. Broad compatibility, thermal endurance and other
devices are not measured.

At the owner's request, discrete press haptics have been added in Rust.
Seven focused tests, host/Android Clippy, formatting and updated package
checks pass. Physical haptic feedback is pending installation after the
current in-game save. The prepared APK SHA-256 is
`ff04864372124e4849442bb4ba56a797a76d13d695862c9999c7bddddb14a886`.

Real in-game save/restart, interruption behavior and direct inner-display
layout observation remain under test. The phone remains in the explicitly
authorized task-wide session. Games, saved progress, identifiers, raw logs
and captures remain private.

## Store limits

16 KB alignment is verified; 16 KB runtime/performance is not. Physical
stock-iOS non-JIT gameplay/audio and this application's combined App Store
distribution basis remain unverified. The first-playable issue stays open.

Record subsequent device results under
[FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1), separating
each observation above. Keep device identifiers, raw logs, captures, game
images, firmware, signing material and saved progress private.
