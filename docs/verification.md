# FIRST-PLAYABLE-1 verification

Reviewed 2026-10-19. Hardware-rendered gameplay, owner-confirmed clear sound,
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

- All 56 host tests pass, including atomic card reopening, bounded audio and
  live speed changes, captured simultaneous touch, following-thumb behavior,
  clear picture/control geometry, saved display profiles, stale-inset handling,
  corner editing/reset, continuous volume, centered panels, library migration,
  file/companion validation, disc grouping, artwork, checkpoint integrity and
  mutually exclusive speaker tap/hold with drift/interruption cancellation,
  snapping with slow-drag escape, returning from background audio headroom,
  the pinned free firmware without replacing supplied files, and centered
  landscape picture/control placement around either side's camera, short
  landscape system-key clearance, editor reset, camera-safe live volume and
  pause indication with no elapsed animation time, silent foreground pause and
  rapid pause/resume between audio callbacks without replaying old samples.
  A recorded-insets regression checks the 1021-pixel folded-landscape picture
  against the old 962-pixel content height when the status bar appeared.
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
landscape on the same core thread. The later background-transition result is
recorded below. A USB interruption during this check was followed by fresh admission;
no actions were replayed against an absent device.

The owner's follow-up changes keep Android navigation visible for direct Home
gestures, add a clear fading pause indicator and change the speaker to tap for
mute/hold for the live slider. That APK was installed and restored the paused
scene after update. Its PAUSED panel rendered over the frozen picture. A short
speaker tap muted without opening a menu; a hold opened the live volume panel
and its release kept mute unchanged. One bottom swipe returned to the launcher;
with background play off, a checkpoint completed and playback reports stopped
through the minimized observation. Re-entry reused the same activity.
The installed APK SHA-256 is
`1532fb0f00596222cf6cad08a1235148af3baa2d47618f2021d3ef3ee1eff1c6`;
signature, resource/notices, ARM64 and 16 KB alignment verification pass.
Its source revision is `3eb8379712da`. The matching source archive SHA-256 is
`0acce81be96c93d4e134f964c1f1f8f5724b0204d61abe2deb65f00c90976b7e`.

Enabling background play started the media-playback foreground service. While
the launcher stayed focused, six consecutive ten-second windows measured
59.76–59.87 fps, hardware output only, no dropped source frames, and core time
6.48–6.78 ms. FIFO underruns nevertheless rose from 4 before the transition to
24 over that run. Synchronous repeated saves were removed; this is not a clean
background-audio result. The source now adds a bounded 50 ms output target only
while minimized, retaining 25 ms during foreground play and trimming the extra
backlog on return. It requires its own device check. The option was restored to
off, the owner's 58% unmuted/1× settings retained, and the latest session saved.
The device was released locked with its screen off, verified within the lease.

The owner requested more space between Triangle and Cross, alignment snapping
and a controller-shaped brand mark. The updated compact defaults reserve a
16-point gap without reducing face-button size, and recompute the largest clear
picture around it. Saved custom positions are preserved. Dragging shows light
edge/center/mirrored guides and can escape them with slow movement. Original
controller artwork replaces the card in the vector masters, masthead, adaptive
and monochrome icons, and favicon/raster exports. The 48 host tests, host and
Android Clippy pass; this later source is not yet device-qualified. The owner
has taken the phone back for personal use; further device access is on request.
The prepared APK SHA-256 is
`2c82a160fe72325fdbe9ea77fdbbc4960986fe7518f31300b40e103144fa0dc0`.
Signature, required resources/notices, native ABI and 16 KB ZIP/ELF alignment
checks pass. The subsequent installation check is recorded below.

In a newly authorized device window, that APK installed over the existing app.
The new process logged automatic restoration and displayed the owner's FF7
gameplay. Supplied firmware was absent from the pre-update system directory;
the prepared fallback's on-device hash matched the pinned image. Selecting
Crash started a new session without a restore log. An early capture showed
a black picture; later captures showed the game's intro and animated title.
This establishes a fresh disc boot with the new firmware, not a measured fast
startup or broad firmware compatibility. Startup latency still needs a timed
check before the boot-sequence work can be considered polished.

At the owner's request, the changed square-display profile was reset through
Controller settings. The Reset row disappeared, and the final private backup
contained an empty element override list: the new defaults are active. There
were no other customized profiles in the pre-update settings. The remaining
editor snapping/guide and minimized-audio checks were not completed. Background
play was returned to off; the owner's 75% unmuted volume and 1× speed were
retained. FF7's checkpoint remains backed up, but the attempted library return
did not complete: the final active marker still names Crash. Only psx was
stopped on handback. Sleep was requested, but the immediate policy read still
reported the screen on/unlocked; a locked/off handback was not established in
this window. Device access is again on request. Raw captures/logs and all game,
card and session data remain private.

## Corresponding source and release limits

The Cross-button APK below was subsequently installed over the existing app.
Crash restored into gameplay in outer-display landscape. Three steady ten-second
windows measured 59.80–59.84 fps, 3.83–3.96 ms core time, hardware output only,
zero audio underruns and zero dropped samples in the retained observation.
This is separate from an audible or background-audio acceptance check. Relevant
app data was backed up before and after installation, and background play remained
off. Only psx was stopped; the Fold was locked and verified screen-off before release.

The owner identified a real horizontal offset in this viewport. Its drawable
was 2342×1080 with content 2160×1021: a 182-pixel camera-edge exclusion shifted
the content center by 91 pixels. The camera's bounding rectangle occupied only
part of that side. New source uses local side-camera obstacles, staggered
shoulder caps and Start/Select pills, plus the shared camera-aware editor and
volume panel. A host geometry preview at the measured dimensions yields a
1361×1021 picture centered at physical x=1171, with every target clear of the
camera. The landscape APK was then installed with app data backed up before
and after. Crash restored paused; the actual content expanded to 2342×1021,
and its approximately 1361×1021 picture was visually centered on the physical
display with the controls clear of the camera. The owner reported that it
looked great. One bottom-edge swipe while the handle was invisible reached
Home; reopening reused the existing process and paused scene. Only psx was
stopped, and locked/screen-off handback was verified. No new sustained-speed,
audible-audio, editor-snapping or background-audio claim follows from this check.
The installed landscape APK SHA-256 is
`65c6af3fe5a98d5c022f120a20ddcd1f58af3232036e64ee434c8eae46b1e6d4`;
resource/notices, signature, native ABI and ZIP/ELF alignment checks pass.

The owner then requested simpler level shoulder caps, with L1/R1 outside in
landscape, and immediate pause feedback. Follow-up source changes those defaults,
updates pause visuals from the current state before presentation, and presents
the pause frame before waiting for its checkpoint. This follow-up was installed
with before/after app backups. Crash restored paused; landscape capture confirmed
the level L1/L2 and R2/R1 order and unchanged centered picture. The owner still
felt a slight pause/unpause delay in the haptic response. Frame-level latency was
not measured. Only psx was stopped; locked/screen-off handback was verified.
Its signed APK SHA-256 is
`01e6459fa6cbb311c341643b46794b8af25366be13f9f83efeeafdeed0eae0b2`;
the signature, notice/resource, ABI and ZIP/ELF alignment checks pass.

The next source moves Start/Select inward, retains a silent foreground audio
stream through intentional pause, and no longer waits for the storage worker
after toolbar toggles. Core snapshot preparation remains on the sole execution
owner after presentation; timing is logged for diagnosis. Minimize/focus-loss
audio closure and synchronous interruption saves are preserved. Host tests and
Clippy pass. This APK was installed with current app data backed up before and
after. The owner tried rapid toggles and reported that the response was immediate.
The matching log batch contains repeated checkpoints without AAudio reopening
between those toggles. Snapshot preparation measured 7.19–11.77 ms; storage
completed separately on the worker. This is not an input-to-haptic timing
measurement. The inward Start/Select placement was inspected on the outer
landscape display, with controls still separate from the picture.
The installed APK SHA-256 is
`30945e673a712b525a957c8f427a925387220d2be98c94c12de2e0c8baacb3f7`;
signature, notices/resources, native ABI and ZIP/ELF alignment checks pass.

The same check exposed a fullscreen regression: status and navigation bars
could remain shown, including after unfolding. A proposed source fix retains
the first-swipe navigation mode but observes bar visibility and re-hides after
a short delay, reapplying fullscreen after display changes. Android ARM64 Clippy
passes. This fix subsequently restored Crash after installation and a fresh
process. The owner folded/unfolded and rotated, reporting that bar hiding works.
The inner-screen capture also showed both bars hidden. Before display transitions,
four steady ten-second windows measured 59.79–59.85 fps with zero recorded audio
underruns/drops; the later fold/rotation phase accumulated two underruns, with
zero drops. This is separate from audible or thermal-endurance acceptance.
In the preceding pause-response window, a final fresh-process return check was
deferred when the regression was observed; progress was checkpointed and backed
up. That window ended with an owner-authorized exception to lock verification
after repeated waking/unlocking. The subsequent fullscreen check ended with
verified locked/screen-off handback after app backup and only-psx closure.
The installed fullscreen-fix APK SHA-256 is
`5bccf32af19d0a1f049288ca2a27fbf5f3aabc5879da75207490517f67111c6f`;
its signature, notice/resource, ABI and ZIP/ELF alignment checks pass.
Its matching runtime source revision is `bb6a1b2db93c`, with archive SHA-256
`c6e0da2de7cc4f42234c8e45768d1b74cbc75c924c162d1e7c8e93192ae75cb9`.
Changed runtime inputs and licenses compare exactly with the clean extracted
source; its archive excludes private root directories.

The owner then reported that the picture resized and failed to remain vertically
maximized in folded landscape. Recorded content changed from 2342×1021 to
2342×962 while the status bar appeared, then returned to its original height.
On the inner screen, status visibility could also disable toolbar use of the
camera margin and shrink the picture. The next source keeps stable fullscreen
insets and toolbar placement through these brief reveals, preserving navigation,
caption and camera clearance. Bar-only resize callbacks also no longer force an
immediate hide request during a navigation gesture. Host tests and Android/host
Clippy pass. The correction was installed on the Fold; a controlled comparison
during a bar reveal and the full fold/rotation matrix remain pending.
Its installed signed APK SHA-256 is
`532353c219f3f8797dd6762d1764d2d87ae9d8e7d95813ae14d98933ccb1f913`;
signature, notice/resource, ABI and ZIP/ELF alignment checks pass.
The matching runtime source is `b7134e4b2178`, archive SHA-256
`b133841ed0d5b30c9d1ee3183f6e84be4fa0404ad75ccaa5ebe2ea11a6f72df4`.
Changed runtime files and build/license inputs compare exactly with its clean
source tree; private root directories are absent from the archive.

The installed check restored the existing Crash scene after replacement. The
owner rotated the folded phone to landscape. Its 2342×1080 capture shows a
centered approximately 1361×1021 complete frame, extending from the top to
the reserved 59-pixel bottom gesture strip; private pixel probes confirm the
frame at row 1020 and app background beginning at row 1021. Status/navigation
bars were hidden in that capture. One injected bottom-edge swipe from hidden
navigation reached Home in portrait, and reopening retained the same process.
No controlled bar-reveal comparison was completed, so that screenshot alone
does not establish stability through every bar or display transition. The
retained log ring no longer contained the landscape resize event.

Later stationary 1× samples were around 59.8 fps with no dropped frames. Audio
underruns accumulated to 25 across the window, including interruptions; the
game was muted and no audible-sound acceptance is claimed for this update.
Session checkpoint and backup completed; only psx was force-stopped, and
locked/screen-off handback was verified before releasing the transaction.
The ten-minute window overran during delayed tool interactions, disclosed to
the owner. No device-wide settings or package permissions were modified.

With the current default dock, the recorded inner-landscape geometry models a
1871.3×1403.5 picture. A full-width 2152-pixel 4:3 picture needs 1614 pixels of
height, which the separate control dock does not leave. Inner portrait models
1956.7×1467.5; folded landscape models 1361.3×1021. These are layout-model results,
not new measured device picture bounds. New resize diagnostics include the
actual picture bounds so future device checks can distinguish these limits
from further regressions.

The owner also reported black strips inside the game picture. Offline inspection
of the latest private Crash checkpoint found a 512×240 emulated display with
the full 512×240 VRAM area active at origin (0,0). The saved game framebuffer
already contains 12 all-black rows at both top and bottom. A retained landscape
capture maps those rows to approximately 51 pixels per edge inside the 1021-pixel
picture. These particular bands precede frontend scaling; they are separate
from Android bars reducing the available window. The frontend still fits the
complete 4:3 frame. Removing game-written letterboxing would require an explicit
crop/zoom policy; this finding does not explain every possible game's borders.

The owner's single blue Cross button on grey plastic is the current identity.
The standalone, favicon, masthead and Android adaptive/themed artwork now use
that geometry. The 512-pixel export, 16-pixel favicon and masthead were visually
inspected off-device. Android resources compiled, and the signed APK passed
its notice/resource, ABI and ZIP/ELF alignment checks. Its SHA-256 is
`f2e51a184bb6a01a6fd1c1e2ef79780dbbc8fd59768e8c0dbf4c729f295f3185`.
This artwork-only update was prepared without phone access and subsequently
installed as recorded above. Its launcher appearance has not been inspected;
no gameplay or borrowed core code changed in that artwork revision.

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

The controller/fast-boot source revision is `620430b5b936`, matching the prepared
APK `2c82a160…fa0dc0` above. Its corresponding-source archive has SHA-256
`e12aeaaadf2742475fd08f36f5198efadec6e429c0f64ee80d1483c66b9689c7`.
The clean Gitless tree ran all 48 host tests offline. Its free firmware image
and license compare exactly with the committed inputs; the archive inventory
excludes private/build root directories. Borrowed source retains its exact
upstream bytes, including inherited whitespace.

The donor's fast-boot patch does not support its included OpenBIOS. The revised
source instead prepares a separately pinned, unmodified OpenBIOS image built
with upstream's documented `BOOT_MODE=fast`, omitting the shell/intro entirely.
A second clean build through Rust `cargo xtask firmware`, using only the vendored
source and pinned compiler image, reproduced the same 524,288-byte image:
`c13794bc14de130c363786f45710d6e472c092070c0524ddede50f41f948bc45`.
The source inventory and exact MIT notice are verified, and a host test proves
existing supplied firmware is preserved. The core and old checkpoint BIOS bytes
are unchanged. Crash subsequently reached its title from a fresh disc boot,
as recorded above; startup latency and broader compatibility remain unqualified.

16 KB runtime/performance, custom-UI accessibility, Play AAB delivery, broad-scan
and foreground-service eligibility, and the combined App Store distribution
basis remain unverified. Physical stock-iOS non-JIT testing is deferred until
an Apple computer is available. See [distribution.md](distribution.md).

Continue under [FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1).
Publish source and suitable summaries; keep private inputs and raw evidence local.
