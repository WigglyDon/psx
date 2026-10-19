# FIRST-PLAYABLE-1 verification

Reviewed 2026-10-18. Hardware-rendered gameplay, owner-confirmed clear sound,
simultaneous touch controls and a completed memory-card save loaded after a
fresh process have run on the stock Fold. Automatic session restore has also
returned to gameplay after a fresh process and an APK update. Latest display
and lifecycle corrections remain under acceptance; build success and older
play observations do not establish acceptance of later changes.

## Build checks

All new player, platform, UI and build logic is Rust. The generated Android
classes forward indispensable activity/service callbacks. SwanStation remains
unchanged at `e436e9b85374def58d211df7f78880f0f4a917b4`.
[build.md](build.md), `rust-toolchain.toml` and `Cargo.lock` record reproduction.

- All 45 host tests pass, including atomic card reopening, bounded audio and
  live speed changes, captured simultaneous touch, following-thumb behavior,
  clear picture/control geometry, saved display profiles, stale-inset handling,
  corner editing/reset, continuous volume, centered panels, library migration,
  file/companion validation, disc grouping, artwork, checkpoint integrity and
  mutually exclusive speaker tap/hold with drift/interruption cancellation.
- Host and Android ARM64 Clippy pass with warnings denied; formatting passes.
- APK signature, resources/notices, ARM64 ABI, EGL dependency and 16 KB
  ZIP/ELF LOAD alignment checks pass. C++ runtime linkage is static.
- Minimum Android API is 28; target API is 36. Alignment is a packaging result,
  not proof of complete 16 KB runtime support.
- The pinned GPL license hash matches; inherited core/dependency, Rust, NDK
  and font notices remain intact. Games, proprietary firmware, saves, keys,
  device identifiers, raw logs and captures remain private.

## Device observations

| Behavior | Observed result |
| --- | --- |
| Ordinary open | Document selection, including CUE/BIN companions, imported and launched an owner-selected disc. Embedded library/disc menus subsequently launch private single- and multi-disc games. |
| Rendering and sound | GLES hardware output on the physical GPU through ANGLE; hardware-frame counters and zero software frames in measured windows. AAudio delivers 44.1 kHz stereo; the owner confirmed clear audible output. |
| Simultaneous input | The owner confirmed movement with jumping/spinning. Presses have one haptic pulse without hold repetition. Captured D-pad drift/reversal/release ran; the owner reported a substantial improvement. |
| Ordinary card save | An in-game save completed, then loaded the saved character/location after Close game, force-stop and a fresh process. The same save loaded after an APK update. This is separate from emulator checkpoints. |
| Runtime disc change | A nonzero starting disc booted. An existing save requested another disc; the embedded menu inserted it in the same session and play continued. |
| Library and editor | Eight games/thirteen discs persist. Explicit scan/selection, companion rejection, numeric disc grouping and retained card IDs ran. A corner resize persisted; individual Reset, Cancel and whole-profile reset ran without replacing unrelated owner data. |
| Embedded UI | Console panels, typography, geometric control symbols, local card icons/placeholders and live settings render natively. Settings preserve intentional pause. Volume zero/mute/restore and speed cycling ran. The latest centered settings panel was visually verified. |
| Continuous volume | DOWN/MOVE changed the visible volume before UP on the Fold, with slider capture retained. The real-time gain uses that same setting immediately. This is a visual/input observation, not a microphone measurement. |
| Automatic session resume | Gameplay was paused, backgrounded, force-stopped and reopened in a fresh process. The same location and explicit pause returned. A later update retained and restored that checkpoint. The bounded files stayed private. |
| Optional background play | The new toggle starts a media-playback foreground service with an ongoing notification and pause action. Counters advance while minimized. Turning it off removes the service; minimizing then stops playback/counters and saves the session. The transition hitch and re-entry defect found below are being corrected. |

The original 70-second play window measured 59.80–59.84 fps against 59.8173 Hz,
with 3.53–5.26 ms average core time and no additional FIFO underruns or drops.
Its startup already counted 72 underruns and 787 dropped audio frames. A later
50-second following-controller window measured 59.81–59.84 fps, 4.38–4.82 ms
core time, one new underrun and no dropped audio frames. These were bounded
walking/gameplay observations, not thermal endurance or broad compatibility.

## Speed and interruption checks

After the owner's call ended, a normal-mode window measured 59.79–59.83 fps.
An owner-selected 2× run reached 119.59 fps in a steady ten-second window.
A separate 5× gameplay run reached 299.03–299.15 fps across four consecutive
ten-second windows: approximately 500% of the donor timing, 2.11–2.21 ms
average core time, hardware output only, no drops and no further underruns
within those four windows. Startup/display/speed transitions had already
accumulated underruns; this is not a claim of uninterrupted audio throughout.
The owner reported that play worked well, then identified display-layout and
responsiveness defects. The multiplier label alone is not an achieved-speed
measurement.

The before-first-frame audio-focus-denial check also ran: visible controls and
an interruption message remain available, and the toolbar stays below visible
system decoration. Later normal play returned when audio focus was available.
Another app's audio ownership was not overridden.

The first background-play pass maintained approximately 59.75–59.84 fps while
minimized. Several lifecycle callbacks each synchronously saved a checkpoint
while play continued, producing a visible/audio transition hitch. The correction
routes continue-play snapshots through the existing bounded worker; default
pause/closure still wait for durability. This correction requires a new pass.

## Current acceptance work

The session-resume APK `7514054ad984a48cf372d280833c98c7498c9b73e8d60c8c630ed78a9c911b6c`
proved fresh-process restore and continuous slider capture. The branded build
`19029ff340e5105e4a558ee7ba630c291248b1c4fedacc450adc9c4b66d8edf1`
proved restore across update, higher thumb clearance, centered panels and the
background service behavior above. These hashes identify private development
APKs, not published store releases.

The owner then reproduced a second NativeActivity starting inside the same
process when reopening. The process-wide core lease correctly refused another
owner. Source now requests one reusable `singleTask` activity, preserves the
active marker when an empty player closes, and reconciles late EGL dimensions
with size-tagged insets. Fresh launch/re-entry and inner/outer rotation checks
must pass on the corrected APK before this milestone is closed. The user's
checkpoint and card were preserved privately before replacing the test build.
The corrected runtime source is `806f3ef`; its initial APK SHA-256 is
`20eebba91a9442f2db621d86ee8daaf7d81e1368806634792f911eb067a6b255`.
The subsequent artwork-only build below is now installed for acceptance.

The subsequent memory-card branding revision changes artwork only. Its installed
APK SHA-256 is `40a38c2c3369c221390f1e09927413c191686df7caf11589ccecf69e0244d7ef`.
The masthead, adaptive/themed icons and 16/32/48 pixel favicon were updated
together. Android resource compilation and signed APK checks pass; this
build ran gameplay on the Fold. Launcher-icon appearance has not been inspected.

On this build, a paused game was checkpointed, backgrounded, force-stopped and
launched in a fresh process. The same scene and explicit pause returned; the
new process logged automatic session restoration. A Home/launcher return and
another explicit launch both reused the existing activity, retained the same
process and scene, and produced no second GLES startup or core-owner refusal.
The owner was also playing during the earlier update observation, so that
observation does not establish an exact scene comparison across that update.
The owner then physically folded/unfolded and rotated the displays and reported
the layouts working very well, with possible off-center folded-landscape spacing
near the camera. Recorded drawable/inset transitions reached outer portrait and
landscape on the same core thread. The revised background-transition pass remains
pending. A USB interruption during this check was followed by fresh admission;
no actions were replayed against an absent device.

The owner's follow-up changes keep Android navigation visible for direct Home
gestures, add a clear fading pause indicator and change the speaker to tap for
mute/hold for the live slider. The 45 host tests and host/Android Clippy checks
cover the updated source; the next APK requires its own device pass. No longer
play or thermal result is inferred from these focused checks.
The prepared APK SHA-256 is
`1532fb0f00596222cf6cad08a1235148af3baa2d47618f2021d3ef3ee1eff1c6`;
signature, resource/notices, ARM64 and 16 KB alignment verification pass.

## Corresponding source and release limits

`cargo xtask source` exports committed project/core files and locked Rust
sources with their original notices. The clean extracted `9dbbb20` archive ran
37 tests offline, verified the core inventory and built a complete signed
ARM64 APK without Git metadata. Its signature/resource/ABI/alignment checks
passed. Later source archives were inspected for matching application/license
files and exclusion of private root directories. Updated source delivery must
be paired with the exact final APK source revision; an extracted developer's
new signing key must not replace the owner's existing installation.

The matching `806f3ef2250e` source archive has SHA-256
`9392768973f182b2b9e769a1053cfe47a9e9087911c2c5d28977c54a2703da18`.
Its clean Gitless source tree ran all 43 host tests offline. The manifest,
background adapter, resume implementation, brand master and upstream license
compare exactly with the committed inputs; its archive inventory excludes
private game/card/firmware, build, target and local-evidence root directories.

The matching artwork revision `7c050369a20b` source archive has SHA-256
`035183c152de8861f2342fe4fb773c48f240291cd9a5b3f2ac78b09f338939e9`.
Its brand inputs match the installed APK's source revision; it retains the
same runtime and borrowed notices.

The donor's included OpenBIOS remains the no-proprietary-firmware route. Its
startup transition/cutoff is unresolved: the existing fast-boot patch does not
support OpenBIOS. The host does not guess when to hide/crop game output or use
proprietary startup artwork as branding.

16 KB runtime/performance, custom-UI accessibility, Play AAB delivery, broad-scan
and foreground-service eligibility, and the combined App Store distribution
basis remain unverified. Physical stock-iOS non-JIT testing is deferred until
an Apple computer is available. See [distribution.md](distribution.md).

Continue under [FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1).
Publish source and suitable summaries; keep private inputs and raw evidence local.
