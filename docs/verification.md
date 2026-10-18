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
- An owner-selected PS1 disc boots with the bundled OpenBIOS
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
checks pass. The owner confirms one haptic pulse for each new control press,
with no repeated vibration while held. The update is installed and reopens
the imported disc. Its APK SHA-256, including clarified in-game saving text,
is `26d11a5eed4e5d1ab10f783ee4cc79f1fb9358bea1698cae3b837515b4041547`.

The owner exercised checkpoints and level completion, then returned to the
first level after game over. A private card snapshot remains byte-identical;
this is not an ordinary card save. The pause menu now explains that the game
controls save points and completed in-game card saves survive restart. Normal
Close game flushed and returned to the native menu without a crash.

Direct inner-display observation subsequently showed the same game/core
process continuing after the display changed to the inner screen. The older
layout was uncomfortable; the owner requested a separate control area and
an embedded interface, which are being verified below.

Real in-game save/restart and interruption behavior remain under test.
The phone remains in the explicitly
authorized task-wide session. Games, saved progress, identifiers, raw logs
and captures remain private.

## Native interface and collection checkpoint

The development APK with the Rust-rendered menus, Noto Sans, vector control
symbols, native layout editor and local multi-disc library is installed.
Current local SHA-256:
`e40ffbba38aa000fec86c3b1942ae0b08f1d692f6157e33bdf09cbc9b1e43fbd`.
The signature, required entries, AArch64 libraries and ZIP/ELF 16 KB alignment
checks pass. Twelve focused host tests pass, including non-overlap geometry,
explicit overlap, scrolling/multitouch cancellation, header/companion checks,
disc grouping and preserving a card ID when another disc is added. Host,
Android and Android scoped-feature Clippy pass with warnings denied.

Direct device observations: the embedded home/menu renders with the new font
and retains the existing game entry. Add games opens the intended storage
permission flow. The app-specific system grant was observed off, enabled
through Android's public UI for the requested scan, and observed checked.
Returning to psx scans accessible storage and presents selectable game sets,
including single-disc and three-/four-disc entries with the correct numeric
disc lists. Referenced BIN tracks do not appear as separate games. Scan
coverage, unreadable/rejected counts and archive limits are visible. The
selection UI and Select all work. Raw results and captures remain private.

The selected collection survives a force-stop/new process with all its disc
entries retained. Choosing a starting disc from a multi-disc entry loads the
generated playlist and reaches the game's rendered title/attract sequence.
The inner-display capture shows the 4:3 game rectangle entirely above the
separate controls. Initial ten-second boot/attract windows measure 58.79 fps
then 59.82 fps, hardware frames only, with zero FIFO underruns/dropped frames
in those windows. This is not yet a new bounded gameplay run.

The scoped APK also builds and passes packaging/signature/alignment checks;
its dumped manifest has no all-files permission. It is not installed over
the broad-scan development build. Runtime switching, editor persistence,
lifecycle and ordinary card-save loading remain under device checks.

## Graphics and direct-manipulation checkpoint

Subsequent installed APK SHA-256:
`918a86efe809a0b1a08e0ecf02977429c134b345bc0195342759aa4ee45967c3`.
The embedded settings now fit without a duplicate Resume row, and the narrow
scrollbar renders correctly. Library entries show distinct local thumbnails.
The latest addition was selected through Scan storage and retained alongside
the existing single-/multi-disc entries.

Device testing exposed two shared-context integration defects. The core's
PS1 subtractive blend equation made host controls/menus dark; the Rust UI now
sets its own additive equation. Changing the active program without restoring
it invalidated the donor's cached binding across duplicate frames, hiding
game text. The host now restores the actual entry program after drawing.
Direct device captures confirm colored native controls/menus and restored
title/load-screen labels. The load flow reports an empty card and responds
to input; it is not evidence of a completed save.

Runtime Change disc selected a second image, reported it as inserted from
the core's callback, then selected the first image again. Resuming returned
to the same in-game screen without booting again. This verifies the host's
swap path, not progression through a game's later disc-change prompt.
The older editor's move/resize/Done path ran on the inner display and was
reset after testing. The requested replacement uses visible corner grips,
continuous anchored resizing and tap/drag movement; it is prepared for the
next connected device pass. Save/Cancel and separate display profiles remain.

Seventeen focused host tests pass, including four-corner anchoring,
second-finger rejection, game aspect/separation during resizing, fast tap
latching, visual interpolation reversals and local memory-card icon decoding.
Host and Android ARM64 Clippy pass with warnings denied. The corner-editor
APK builds and passes signature, required-entry and ZIP/ELF alignment checks.
Private captures, game files and card snapshots remain outside Git.

The current USB connection was lost before that installation. Ordinary
in-game save/restart, direct corner gestures/persistence and the updated
fold/lifecycle flow remain under device acceptance. The issue stays open.

## Store limits

16 KB alignment is verified; 16 KB runtime/performance is not. Physical
stock-iOS non-JIT gameplay/audio and this application's combined App Store
distribution basis remain unverified. The first-playable issue stays open.

Record subsequent device results under
[FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1), separating
each observation above. Keep device identifiers, raw logs, captures, game
images, firmware, signing material and saved progress private.
