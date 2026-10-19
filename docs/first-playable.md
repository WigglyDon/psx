# FIRST-PLAYABLE-1: Rust native PS1 player on Android

## Goal

Build a minimal native PS1 player for a stock Android foldable. Ordinary use
is **pick a local game file and play**, with responsive touch controls,
stable sound and dependable in-game memory-card saves.

All new application, UI, runtime, host, input and save/file logic is Rust.
Start with the pinned SwanStation C++ candidate through its libretro C interface.
Retain upstream emulation behavior and optimized execution.

The intended finished product is clean, usable and releasable through Google
Play and the Apple App Store. Read [distribution.md](distribution.md) before
fixing the donor and platform boundary. Android remains the first playable
target; store approval is not an acceptance claim for this milestone.

## Scope

1. Incorporate the pinned upstream core with its licenses, attribution and
   vendored dependency notices. Record exact build inputs and local changes.
2. Create the smallest practical native Rust Android ARM64 host. Reuse Rust
   Android/NDK bindings; document any indispensable platform callback shim.
3. Produce repeatable build/package instructions and a development APK.
   Verify the actual target ABI and native-library compatibility. Use the
   current Play target SDK and prepare the native build for 16 KB host pages.
4. Provide one local game picker and a fullscreen game/control surface.
   Resolve companion disc files and any required one-time firmware selection
   for the selected input.
5. Connect the core's native renderer and audio output with sensible fixed
   defaults. Start at native PS1 rendering resolution.
6. Implement simultaneous touch directions/buttons and reliable cancellation,
   plus useful layouts on the outer and inner displays.
7. Persist memory cards safely across app closure and process restart.
8. Handle user pause, background/foreground and surface/layout transitions
   without stuck input, continuing background audio or lost completed saves.
9. Keep development diagnostics outside the ordinary play flow. Make the
   navigation, touch targets and file-error messages usable from this build.

## Early store feasibility

Record the donor's distribution basis and outstanding permissions before
calling it the permanent cross-platform choice. Preserve all inherited terms.
Identify its iOS execution path with JIT disabled and the actual graphics
backend. When an authorized Mac/Xcode environment and physical iOS device
are available, run a bounded non-JIT gameplay/audio check without a debugger
providing extra execution privileges. A build alone cannot settle performance.

If those resources are unavailable, record iOS suitability as unverified and
continue the Android prototype without making shared logic Android-specific.
Resolve this check before a full iOS port or substantial donor-specific work.
Do not turn a failed check into a new emulation-core development project.

## Acceptance on the target Fold

Using one owner-selected local game:

- Select and launch through the ordinary app flow.
- Demonstrate actual moving gameplay, audible sound and simultaneous input.
- Exercise pause/resume, input cancellation, background/foreground and
  folded/unfolded display transitions.
- Make an ordinary in-game save, close/restart the app and load that save.
- Record a bounded repeatable gameplay observation, achieved emulation speed,
  audio interruptions and material responsiveness or lifecycle problems.

Report build and automated checks separately from device outcomes. Do not
claim superiority, broad compatibility or a completion time without evidence.

## Execution

### Owner additions, 2026-10-18

Complete the native Rust-rendered interface: embedded home/game menus,
clear errors and source/license pages, an ergonomic full game view, proper
controller symbols, discrete press haptics, clean sans-serif typography and
a cog for the app menu. Preserve the game aspect ratio while maximizing its
view. Use compact console-style panels, restrained frames and highlights,
back navigation and icon actions. Keep the picture
**clear of every control by default**. Controller settings provide a
preview editor for moving/resizing individual controls and the game picture.
Tap selects an element, dragging moves it, and dragging a corner grip resizes
it continuously; retain Save/Cancel behavior. Add light snapping with visible
guides for edges, centers and balanced left/right placement; a slow drag must
escape the snap. Give Triangle and Cross
a real gap in the compact default layout. Preserve existing custom positions.
Show Reset only for a changed selection and restore just that element; offer a
whole-layout reset in
controller settings only when the profile differs from defaults. Provide
an explicit **Allow overlap** toggle (off by default), and reset. Persist
separate tall, square and wide layouts. Keep this customization behind the
cog rather than crowding ordinary play. Bluetooth/USB controller support is
a later addition through the same logical input boundary.

Use actual window dimensions across inner/outer display rotations to choose
the larger clear picture, without stretching it. Keep the D-pad visually a
digital cross with its following behavior. Make face-button touch targets
forgiving in unused control space without taking touches from the picture.
Treat ordinary phone landscape as a primary layout. Center the picture against
the display width, using local camera bounds instead of excluding a whole
side strip where the platform supports it. Keep the camera clear of controls
and game content. Give shoulder keys simple symmetric faces and separate
Start/Select keys while retaining usable touch areas. Landscape shoulder pairs
sit level, with L1/R1 outside and L2/R2 inside.
Place Start/Select toward the picture within their reserved landscape rails.

Settings during play are a native sheet over the running game. Opening the
cog releases held input and preserves explicit pause intent. Provide a
small separate pause/play, speed, volume and settings controls near the
top, using spare camera-margin space where safe. Tap the speaker to mute or
unmute; hold it to open the compact live volume slider. Zero mutes and raising
the slider restores sound. Show intentional pause immediately on the frozen
picture, without an indicator fade or waiting for checkpoint storage first.
Avoid audio-device churn during a foreground pause and coalesce rapid save
requests without blocking input; interruption saves must remain durable.
Persist a useful perceptual volume range. Tap speed to cycle only 1x, 2x and
5x, retaining pitched audio unless muted. Achieved speed depends on the
game/device; measure the actual modes. Keep the main settings sheet small.
Give folded landscape side rails and a picture that uses the available
height; adapt the inner-screen control band across both rotations.
Use original psx launcher branding and retain borrowed notices. Investigate
the donor/OpenBIOS startup transition through its supported interfaces;
preserve its license and keep proprietary startup artwork out of app branding.
Use the free firmware's supported fast-boot build mode to omit the interrupted
intro, retaining its exact source, notices and reproduction instructions.

Track a captured D-pad finger until lift or interruption even outside its
visible area. Let its center follow thumb drift by default, with a neutral
zone, stable eight-way directions and a fixed-center option. Preserve direct
direction taps and independent face-button fingers. Animate the thumb/base
within the reserved control space, keeping the default picture unobstructed.

Press/release highlights and transitions into/out of the embedded settings
should animate smoothly without delaying logical input or haptics. Add game
icons to the collection using available local metadata, with clear distinct
placeholders where artwork is unavailable. Keep ordinary navigation small.

Provide a local list of imported games and clear disc selection for
multi-disc games, including runtime changes through the donor's existing
disc-control interface. This explicitly extends the original single-recent
game scope; it is a local file collection, without an online game database.

The primary add flow is **Scan storage**, select discovered games, and retain
the selection for future play. **Add games** reruns the same scan. Manual
file/folder navigation is optional. Scan shared storage when the platform
grants it, and report actual coverage and access limits honestly. Recognize
supported containers and CUE companions, group disc sets, and distinguish
candidates, missing files, archives and unsupported inputs. Do not claim
compatibility from a filename. Preserve a route consistent with store
requirements; broad Android storage access needs its own Play review basis.
Use owner-selected private single-disc and multi-disc games for acceptance.
All private-input rules still apply. Verify collection persistence, starting
disc selection, runtime swapping, persistent choices and existing card IDs.

Prepare and build without holding the phone. Use the existing stock-device
procedure and current exclusive operating session before installation/testing.
Keep game data, proprietary firmware, saved progress, signing material,
credentials, device identifiers and raw private logs outside public source.

Continue the complete implementation autonomously under this issue. Give
progress updates at useful milestones and a full handback with commit/build
identity, reproduction instructions, observed outcomes and remaining limits.

Core redesign, language translation of the core, cloud services, an online
game database, emulation-enhancement catalogs and a full second platform port
remain outside this milestone. If a donor integration requires substantial core repair,
report the concrete blocker and evaluate a narrower integration or donor
change rather than silently expanding the task.

### Owner follow-up: responsiveness and product cleanup

Retain committed control profiles and consistent toolbar hit testing through
folds and rotations. Give thumb controls more bottom clearance while retaining
the largest clear 4:3 picture and usable button sizes. Center the ordinary
in-game panels in safe content; keep anchored volume controls near their icon.
Apply volume while dragging and change speed without reopening the audio device.

Automatically checkpoint the emulated machine and restore the active session
after a fresh process, preserving disc selection and pause intent. Keep ordinary
memory cards authoritative; reject incompatible or stale states without rolling
back newer card progress. Offer an explicit disc restart that retains the card.

Minimizing pauses by default. An off-by-default Run in background toggle may
continue the session through Android's supported foreground playback route,
with a visible notification and pause action. Respect explicit pause and audio
focus. Reopening must reuse the existing player, never create a second core owner.
Keep Android navigation available for a single Home gesture, with controller
touches clear of the system navigation strip. Preserve camera-safe placement
and verify the resulting folded-landscape spacing.
The owner also requests a hidden Home handle until interaction; use supported
per-window behavior and verify that one swipe still leaves the app.

Replace prototype branding with original app/repository artwork and favicon
assets using a blue Cross on a black circular button in grey plastic,
as requested by the owner after reviewing the controller silhouette. Remove
obsolete paths, redundant state writes and stale public-facing copy while
preserving existing data, borrowed notices and useful verification.

## Required local resources

A local implementation session with Rust/Android build tools and normal
GitHub access; the connected Fold when testing begins; one game selected or
provided privately by its owner.

