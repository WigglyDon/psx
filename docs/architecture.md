# Architecture

## Product boundary

psx is a native PS1 player with a small user experience. Rust owns the app.
The borrowed core owns emulation, including guest CPU execution, emulated devices,
graphics and audio generation, disc interpretation and compatibility behavior.

Keep the upstream core intact behind its libretro C ABI. The native Rust host
supplies the callbacks and environment that the pinned core actually requires:
input state, file access, presentation, hardware graphics context, audio
consumption, persistence and host pacing. A single session owns the core.
FFI state must not outlive its session or cross thread/context boundaries
without the interface's required synchronization.

SwanStation is the pinned Android prototype candidate. Its suitability for
the eventual two-store product remains conditional on the checks in
[distribution.md](distribution.md). Keep player behavior in shared Rust and
platform handles inside small adapters. This boundary permits a deliberate
donor change if necessary; it does not call for a user-facing core selector.

## Android

The implementation is the `psx-player` Rust crate plus `psx-android`.
`psx-player` owns pause policy, touch layouts, audio buffering, imports,
memory-card durability and the narrow libretro host. It has no Android
handles. `psx-android` owns only platform event/SDK calls, EGL/window
presentation and AAudio delivery. Rust `xtask` builds and packages the app.
See [build.md](build.md) for the callback-only platform shim.

`android_main` is the sole core/GL owner. The core is deliberately neither
Send nor Sync. A thread-local session callback pointer remains live through
unload/deinit. Audio callbacks consume a bounded atomic stereo FIFO and never
enter the core or perform file work. A separate import worker streams user
documents; it never owns emulation or GL. Native Android dialogs provide
standard accessible menus and errors; the game and touch controls use GLES.

The fixed renderer is SwanStation OpenGL ES 3.0 at 1x resolution. The host
negotiates the exact context version, provides a persistent framebuffer,
calls context reset after game load and context destroy while current before
unload. A pbuffer keeps the same context current across Android surface
replacement; the emulated machine is retained. A NULL frame preserves the
previous picture. Hardware-frame and software-frame counts remain distinct.

The existing recompiler with LUT fast memory is used on 4 KB Android. On
other host page sizes the existing cached interpreter avoids the donor's
4 KB JIT guard assumptions. This is preparation, not measured 16 KB support.
Frame pacing follows the donor's reported PAL/NTSC timing, with small bounded
audio-fill correction and no unbounded catch-up. Intentional pause survives
lifecycle changes; input is released and audio is closed on interruption.

Rust Android/NDK bindings and NativeActivity implement the first host.
The runtime, controls, UI and save/file policies remain Rust. The generated
callback shim completes the document-picker result path; multiple CUE/BIN
selection and streaming import have run on the target Fold.

Prefer existing core rendering paths and direct native audio delivery. Avoid
unnecessary full-frame copies through managed UI and unbounded queues. Use
native PS1 resolution initially, normal emulated speed, automatic compatibility
defaults and one selected rendering backend. Treat low memory, low heat and
responsive input as measured goals, not consequences of a small interface.

Prove that the intended hardware-rendering path is active; a successful boot
through a software fallback is a different observation. Keep C-ABI callback
lifetimes, graphics-context reset/destruction and audio-thread ownership
explicit at the Rust boundary.

Surface destruction is not an instruction to erase a game session. Handle
pause, resume, focus, cancellation, orientation and folded/unfolded layouts.
Release held input on interruption. Preserve intentional user pause.

Touch press transitions come from shared Rust input state. Android provides
a short virtual-key haptic for a newly engaged direction or button, including
sliding into another control. Holding a control does not repeat vibration.
The public [View haptic API](https://developer.android.com/develop/ui/views/haptics/haptic-feedback)
respects system touch-feedback settings and needs no vibration permission.
Haptic strength and availability remain platform-owned.

## iOS route

Use the same Rust session, input and save policies behind thin iOS bindings.
The eventual store build must operate without JIT or debugger attachment.
Prove the donor's non-JIT execution and an available iOS presentation path
on a physical device early, before promising iPhone support. Do not assume
Android Vulkan integration establishes a Metal or other iOS graphics path.
Keep this a bounded feasibility check alongside Android first playable,
not a second full product implementation.

## Files and saves

The implemented format/grant/card contract and current limits are in
[formats.md](formats.md). Neither an archive nor a content URI is passed as
an ordinary path. Successfully imported private copies survive provider
revocation, and recent metadata is updated only for a loaded session.

The ordinary action is to select one game. Android content URIs are access
handles, not ordinary filesystem paths. Use system document pickers and keep
their access grants valid. An iOS file selection has its own scoped access
and persistence rules. Disc sets can require companion files;
support the chosen input deliberately and explain any missing inputs plainly.
Do not read whole discs into memory by default.

Keep memory-card persistence separate from optional save-state resume.
Persist ordinary in-game saves safely and prove them after process restart.
Do not let an old save state silently roll back newer memory-card progress.

## Everyday use

The first screen offers Open game and, when available, a valid recent game.
The play screen offers comfortable controls and a clear pause action.
Choose core defaults in the app. Ask about firmware or missing companion
files only when the selected game needs them, using an actionable message.
Keep navigation accessible and usable across display sizes. Treat reliable
reopening, sound interruptions and saved progress as part of the experience.

## Delivery boundary

A working first playable means actual gameplay, sound, controls, durable
saving and interruption handling on the target device. A built library or
synthetic output is a technical checkpoint.

Keep this design small. Choose dependencies and implementations based on
the first playable result. Broad platform abstraction, new emulation work,
enhancement UI and performance-superiority claims require their own evidence.

## Primary references

- SwanStation: https://github.com/libretro/swanstation
- Selected source: https://github.com/libretro/swanstation/tree/e436e9b85374def58d211df7f78880f0f4a917b4
- Rust Android host: https://github.com/rust-mobile/android-activity

These sources justify the initial integration route; they do not prove this
particular application works on a phone.

