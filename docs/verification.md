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

## Device and store limits

The build checkpoint has no Fold gameplay, audible-sound, simultaneous
physical-touch, real in-game save/restart, fold transition or sustained-speed
result yet. The owner selected a private local game; its archive is still
downloading during preparation. Phone operation requires the current
stock-device procedure and a fresh exclusive window. No device has been
operated during this checkpoint.

16 KB alignment is verified; 16 KB runtime/performance is not. Physical
stock-iOS non-JIT gameplay/audio and this application's combined App Store
distribution basis remain unverified. The first-playable issue stays open.

Record subsequent device results under
[FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1), separating
each observation above. Keep device identifiers, raw logs, captures, game
images, firmware, signing material and saved progress private.
