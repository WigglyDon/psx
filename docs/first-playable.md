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

Prepare and build without holding the phone. Use the existing stock-device
procedure and current exclusive operating session before installation/testing.
Keep game data, proprietary firmware, saved progress, signing material,
credentials, device identifiers and raw private logs outside public source.

Continue the complete implementation autonomously under this issue. Give
progress updates at useful milestones and a full handback with commit/build
identity, reproduction instructions, observed outcomes and remaining limits.

Core redesign, language translation of the core, cloud services, library
scanning, enhancement menus and a full second platform port are outside this
first milestone. If a donor integration requires substantial core repair,
report the concrete blocker and evaluate a narrower integration or donor
change rather than silently expanding the task.

## Required local resources

A local implementation session with Rust/Android build tools and normal
GitHub access; the connected Fold when testing begins; one game selected or
provided privately by its owner.

