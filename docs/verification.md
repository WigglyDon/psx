# FIRST-PLAYABLE-1 verification

Reviewed 2026-10-18. The Fold has run actual hardware-rendered gameplay,
owner-confirmed clear sound, simultaneous touch controls and an ordinary
memory-card save loaded after a fresh process and APK update. The latest
interface and faster playback modes have separate acceptance work below.
Build success is not a gameplay or store-approval result.

## Current build

The installed APK was built from `7478102`, with SHA-256
`7aa9e3bd8578e79f6ee1911fbc2997a40773af76945a6497ea77a0f9be10e975`.
Existing app data is retained. The final volume popover fits both controls
without scrolling, and its listening level and 1x choice survived an update
and fresh process. Close game returned to the saved library.

All new player, platform, UI and build logic is Rust. The generated
NativeActivity subclass only forwards indispensable Android callbacks.
SwanStation remains unchanged at
`e436e9b85374def58d211df7f78880f0f4a917b4`. Build inputs and reproduction are
in [build.md](build.md), `rust-toolchain.toml` and `Cargo.lock`.

- All 37 host tests pass. They cover bounded audio/gain/resampling, independent
  touch input and cancellation, following-thumb behavior, clear picture/control
  geometry, perceptual-volume migration, menu/slider interactions, corner
  resizing/reset/persistence, companion validation, disc grouping/card identity,
  artwork decoding and atomic card reopening/corrupt-length preservation.
- Host and Android ARM64 Clippy pass with warnings denied; formatting passes.
- APK signature, required resources/notices, ARM64 ABI, EGL dependency and
  16 KB ZIP/ELF LOAD alignment checks pass. The C++ runtime is statically linked.
- Minimum Android API is 28; target API is 36. Alignment checks establish
  packaging compatibility, not complete 16 KB runtime support.
- The pinned upstream license hash matches; original core/dependency, font,
  Rust runtime and NDK notices are retained. Games, proprietary firmware,
  saved progress, keys, device identifiers, raw logs and captures stay private.

## Actual play, sound and saving

These are observations from earlier playable APKs; they do not establish
acceptance of every later interface or speed change.

| Behavior | Observed result |
| --- | --- |
| Ordinary open | System document selection, including CUE/BIN multi-selection, imported a private copy and launched an owner-selected disc. Later library entries launch through embedded single-/multi-disc menus. |
| Rendering | GLES hardware output on the physical GPU through Android's ANGLE driver; hardware-frame counters and zero software frames during the measured play windows. |
| Sound | AAudio at 44.1 kHz stereo; the owner confirmed clear audible game sound. |
| Two-thumb input | The owner confirmed moving while jumping/spinning. Each new press produces one haptic pulse, without repeated vibration while held. |
| Following D-pad | Direction continued outside the visible well, reversed there and cleared on lift. The visual stayed inside its well and eased back. The owner reported a major improvement in feel. |
| Ordinary card save | An in-game Save Game completed. Close game, force-stop, fresh process and the game's Load Game returned to the saved character and location. This was a real card save, not a checkpoint or host state. |
| Save across update | The same ordinary save loaded after the following-controller APK update. Its actual memory-card icon appeared in the library. |
| Runtime disc change | A nonzero starting disc booted. Loading an existing card produced the game's request for another disc. The embedded disc menu changed it in the same session and play continued. |
| Display/lifecycle | The owner played through fold/unfold and rotation in the larger-picture/live-settings build. The core/game process was retained across observed display transitions. Updated layouts still need the final posture pass. |

A 70-second early owner-play window measured 59.80–59.84 fps against the
core's 59.8173 Hz, with 3.53–5.26 ms average core time, hardware output only
and no additional FIFO underruns or dropped frames. Initial startup counters
were 72 underruns and 787 dropped frames; the later window does not erase them.

The following-controller build (`19e12cf`, APK
`0d54e692f50c81c07d0284db6b732353c0c8d02e6a0dcab7e088e103a3f56875`)
recorded a separate 50-second walking/drift/reversal/standing observation:
59.81–59.84 fps, 4.38–4.82 ms average core time, hardware output only, one new
FIFO underrun and no dropped audio frames. Neither window is a combat,
thermal-endurance or broad-compatibility measurement.

## Native interface, collection and controls

The saved collection retains eight games and thirteen discs. Scan storage
ran through Android's explicit app-specific grant and showed honest scan
coverage, rejected/missing inputs and archive limits. Companion BIN tracks
were not listed as separate games. Selection, adding entries and a fresh
process retained the collection and existing card identities. Multi-disc
sets expose numerically ordered initial and runtime disc selection.

The embedded menus, errors, source/license pages, Noto Sans and geometric
control symbols render in Rust. Local card artwork and distinct placeholders
appear without an online game database. The current console-style library,
starting-disc menu and settings sheet render on the inner display. Before/after
private comparisons found byte-identical cards and controller presets across
this update pass; the original volume, mute state and 1x choice were restored.

Controller editing ran on the Fold: a corner drag resized the D-pad and Done
persisted it through a new process/update. Reset restored only the selected
element and disappeared at its default. Cancel left the saved layout
byte-identical. Whole-layout reset turned overlap off, restored following and
hid itself when the profile matched defaults. The tests additionally exercise
all four grips, aspect preservation, independent fingers and display shapes.
Only task-created layout experiments were reset.

The earlier live-settings build (APK
`40c923db00a68c089108597371e8015dd776d978de316fb2e7f08701f9c2389e`)
showed settings over a game that continued running. Pause intent has a
separate control. The latest toolbar, volume popover and speed-cycle UI ran
while Android denied audio focus during an interruption:

- Before-first-frame interruption now shows a message and visible play controls.
- The quick controls sit below the visible system status bar during interruption.
- Speaker opens the volume popover; zero mutes, raising the slider unmutes,
  and Mute/Unmute retains the selected level. The original listening level
  was restored after the check.
- Speed cycles 1x → 2x → 5x → 1x by single taps, without opening a submenu;
  the check returned it to 1x.

This interruption pass did not run gameplay or produce game sound. Achieved
2x/5x speed, pitched audio quality, return from audio-focus denial, and the
latest layouts across inner/outer rotations remain pending. The interface
showing a multiplier is not proof that the core achieved it.

## Corresponding source

`cargo xtask source` exports committed public project/core files and the
locked Rust dependency sources with their notices. Archive inspection found
no project-private game/card/firmware, signing, capture/log, build, target or
Git directories. The exact host, upstream GPL and font license files matched
those in a clean extracted archive.

The extracted `9dbbb20` source archive ran all 37 host tests offline, verified
the pinned core inventory, generated notices and built a complete signed
ARM64 APK without Git metadata. Signature/resource/ABI/alignment checks also
passed there. Its build tool code is the release-source tooling used by the
later UI commits. This verifies that packaging path; it is not another device
runtime result. Each extracted build generates its own private development
key and should not replace the owner's existing signed installation.

## Remaining release limits

The issue stays open for the current device acceptance above. The donor's
included OpenBIOS is the current no-proprietary-firmware route. Its startup
transition/cutoff remains unresolved: the donor's existing fast-boot patch
does not support OpenBIOS. The host has not guessed a time to hide or crop
that output, and proprietary startup artwork is not used as app branding.

16 KB runtime/performance, custom-UI accessibility qualification, a Play AAB,
broad-scan permission eligibility and this application's combined App Store
distribution basis are unverified. The owner deferred physical stock-iOS
non-JIT gameplay/audio until an Apple computer is available. See
[distribution.md](distribution.md). Source packaging and MIT host attribution
do not settle the borrowed core's GPL/store obligations.

Continue acceptance under
[FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1). Publish only
suitable source and summarized observations; keep private inputs and raw
device evidence local.
