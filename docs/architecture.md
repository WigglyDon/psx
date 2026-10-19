# Architecture

The active extension is [PLAYER-2](player-2.md): preserve the working PS1
experience while qualifying a PS2 engine and shared direct-game launch.
PS2 implementation and qualification are in progress; the existing PS1
observations do not establish PS2 acceptance.

## Product boundary

psx is a native PS1 player with a small user experience. Rust owns the app.
The borrowed core owns emulation, including guest CPU execution, emulated devices,
graphics and audio generation, disc interpretation and compatibility behavior.

Retain the useful core implementation behind its libretro C ABI. Targeted
repairs or a justified Rust replacement may change that boundary when they
solve an observed defect or required product behavior with less coupling.
Keep modifications reproducible, identify changed state formats and verify
the affected behavior; a whole-core rewrite is not the active milestone.
The native Rust host
supplies the callbacks and environment that the pinned core actually requires:
input state, file access, presentation, hardware graphics context, audio
consumption, persistence and host pacing. A single session owns the core.
FFI state must not outlive its session or cross thread/context boundaries
without the interface's required synchronization.

A stable game ID identifies the library entry and durable saved session;
an ephemeral runtime token identifies a current assignment. Rust's session
boundary holds one OS-backed writer lock per owned game until checkpoint
completion and donor teardown. Runtime/process/task handles are not saved.
The single live runtime is PLAYER-2 policy. Future worker slots may assign
these same saved sessions without changing their permanent identities;
no second process or IPC framework is implemented in this milestone.

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

Android reuses one `singleTask` player activity for launcher and notification
returns. This prevents a second NativeActivity thread from competing with a
still-open session; the process-wide core lease remains a refusal guard.
[Android launch modes](https://developer.android.com/guide/topics/manifest/activity-element#lmode)

`android_main` is the sole core/GL owner. The core is deliberately neither
Send nor Sync. A thread-local session callback pointer remains live through
unload/deinit. Audio callbacks consume a bounded atomic stereo FIFO and never
enter the core or perform file work. A separate import worker streams user
documents and discovery; it never owns emulation or GL. Menus, errors,
licenses, the layout editor and game controls are rendered in Rust through
the same GLES context. The system document/permission screens remain platform
UI. Shared menu/input geometry has no Android handles. Native accessibility
semantics for the custom interface still require release qualification.

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
Drawable dimensions are reconciled after EGL adopts a new buffer, including
when Android sends its final resize callback before that adoption. Insets,
cutouts and density arrive with their decor-view dimensions; another display's
insets cannot constrain the current drawable. NativeActivity's content rectangle
is not combined with dimensions from a different stage of the transition.

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

The ordinary action is to select a saved game from the local collection.
Adding games scans platform-authorized shared volumes, validates container
headers and CUE companions, then stores only selected entries in an atomic
local manifest. Game compatibility is still determined by the core on open.
Scanning is explicit, never repeated automatically at launch. See the broad
storage permission/release gate in [distribution.md](distribution.md).

Android content URIs are access
handles, not ordinary filesystem paths. Use system document pickers and keep
their access grants valid. An iOS file selection has its own scoped access
and persistence rules. Disc sets can require companion files;
support the chosen input deliberately and explain any missing inputs plainly.
Do not read whole discs into memory by default.

Keep memory-card persistence separate from optional save-state resume.
Persist ordinary in-game saves safely and prove them after process restart.
Do not let an old save state silently roll back newer memory-card progress.
Automatic checkpoints use the pinned core's public serialize/unserialize ABI,
between frames on the same execution/GL owner. A bounded worker hashes and
atomically writes one captured state; no worker calls emulation or GL. Periodic
writes do not block playback on storage sync. Background, explicit pause and
closure wait for durability. Resume validates core/disc inputs, payload integrity
and the contemporaneous card before entering the core, then restores the durable
card. See [formats.md](formats.md) for recovery and reset behavior.

## Everyday use

The first screen offers saved games and Add games. During play the cog opens
a native settings sheet over the running game; opening it releases input
without changing pause intent. Pause/play and speed have separate controls.
Tapping outside a sheet dismisses it after lift, without passing that touch
through to the game. Embedded navigation uses compact console panels,
framed highlights, small headers and explicit on/off indicators. The library's
Add and Settings icons live in its app bar; primary actions stay distinct
from back navigation. Controls occupy separate space by default;
the largest clear 4:3 picture is computed from their rectangular targets.
The default layout compares arrangements at the actual window dimensions.
On a sufficiently wide square display, shoulders and system buttons occupy
the space between thumb clusters. The dock adjusts its height while retaining
face-button sizes and directional response. Thumb clusters leave 32 logical
points below their lowest target before local camera avoidance. The compact band reduces vertical spread
while retaining face-button size and a 16-point gap between Triangle and Cross;
the largest clear picture is recomputed around those minimum touch bounds.
Defaults now use larger thumb wells and action caps. Landscape wells grow into
spare rail width before taking height from a full-height picture. A side camera
can compact the default well vertically and place it below the hole, using its
bottom clearance when necessary. Saved custom rectangles still take precedence.

Phone landscape is a primary layout. On Android 11 and later, side camera
bounds are local obstacles rather than full-height inset strips. System bars
remain reserved; portrait keeps its top camera margin. The shared layout moves
the whole face-button cluster together or shifts an obstructed control locally,
then finds the largest clear 4:3 picture around controls and camera rectangles.
The default picture centers on the full available display width when it fits.
Camera avoidance is derived for the current window without overwriting saved
control positions. The editor uses the same placement/defaults, and anchored
volume panels slide clear of camera bounds. Shoulder caps have simple symmetric
rounded faces. Landscape pairs sit level, with L1/R1 outside and L2/R2 inside;
Start/Select have smaller pill faces inside their retained touch targets, at
the inner ends of wide-display side rails. Rendering and hit areas remain distinct.

On Android 12 and later, `BEHAVIOR_DEFAULT` requests hidden navigation with
gesture navigation still available on the first swipe. Navigation's insets
ignoring visibility keep control targets out of the system gesture strip.
Older releases retain visible navigation. This supersedes transient-bars mode,
which previously needed a reveal swipe on the Fold. A single swipe from the
hidden-handle landscape surface reached Home on the Fold, and reopening reused
the same paused scene and process.
[Android system-bar behavior](https://developer.android.com/reference/android/view/WindowInsetsController#BEHAVIOR_DEFAULT)

That mode does not automatically hide revealed bars. While the app is focused
and visible, Rust observes their visibility at most twice per second and
reapplies per-window fullscreen after 1.2 seconds of revealed bars. It also
reapplies after actual drawable-size changes. Resize callbacks caused only by
bar visibility do not trigger an immediate hide request during a gesture.
Focus loss and document picking
cancel the timeout; a game without audio focus does not re-hide system UI.
This keeps the gesture strip reserved and the first Home gesture available,
without forcing device-wide settings or using a painted-over system handle.
The owner checked folding and rotation and reported that return-to-fullscreen
works. Revealed status bars now use a stable fullscreen layout: their temporary
visibility does not reduce picture height or move the toolbar into the play
area. Navigation's reserved strip, caption decoration and camera bounds remain
excluded. Visible-bar layout is retained during document picking or a game's
audio-focus interruption. Insets remain tagged to the actual drawable size.

The less-used system buttons keep their own separate row. Wide phone layouts use side rails,
including their system buttons, so a bottom row does not constrain picture
height. The small pause/play, speed, volume and settings toolbar uses a clear
part of an already excluded top margin when available, avoiding the actual
camera cutout rectangles. Active fullscreen retains that placement through
temporary status-bar reveals; outside fullscreen, visible decoration keeps
the toolbar inside safe content. This recovers picture space without placing touch
targets over the game. Devices without that space reserve it in the controller
rails or above the picture. Custom toolbar placement is retained. Android requests `fullUser`
orientation: all four rotations when
the user permits rotation, respecting their rotation lock.
[Android orientation contract](https://developer.android.com/guide/topics/manifest/activity-element#screen)

Intentional pause adds the PAUSED panel on the same presentation as the frozen
picture and changed toolbar; audio-focus interruption has its own label. These
state indicators do not fade. A toolbar pause presents before preparing its
checkpoint. One storage worker remains bounded; rapid toggles retain the latest
pending intent without waiting for an earlier write. Lifecycle interruptions
still wait for durable checkpoints. Opening settings preserves pause intent.

During an intentional foreground pause, the existing audio stream produces
silence and the consumer discards queued pre-pause samples. Resume reuses that
stream and its audio focus when still held. The producer never moves the
consumer's cursor; a pending discard boundary also handles pause/resume between
callbacks. Minimized, unfocused or detached playback still closes audio unless
the separate background-play contract permits continuing emulation.

Audio-focus denial before the first frame leaves the play controls visible
with an interruption message. Play retries focus; it never overrides another
app's audio ownership. Android's reported
[system-bar insets](https://developer.android.com/reference/android/view/WindowInsets#getInsets(int))
and [status-bar visibility](https://developer.android.com/reference/android/view/WindowInsets#isVisible(int))
determine toolbar placement, including transient bars that do not resize the
content area.
During audio-focus loss the toolbar stays inside safe content, covering
persistent call indicators that can remain visible outside the reported
system-bar insets. Focus transitions cancel touch and recalculate the layout.

The controller editor selects elements by tap, moves them by dragging, and
resizes them through corner grips, keeping the opposite corner anchored.
The action buttons select as one group by default. Dragging keeps their relative
positions; a corner scales their sizes and spacing together. Individual switches
to per-button editing, and Group returns to the cluster. Group Reset restores
just those four buttons; individual Reset retains its existing scope. The D-pad
remains one element. All edits use the existing per-display preset format.
Dragging lightly snaps nearby edges/centers to peers, defaults, screen margins
and mirrored horizontal centers. Visible guides explain each alignment. The
unsnapped drag origin is retained so slow movement can escape a guide; no grid
locks placement. These are draft changes until Done, with Cancel and Reset
unchanged. Existing saved custom positions are preserved.
The game picture keeps its aspect ratio. Reset appears only for a changed
selection and restores that element's draft until Done; Cancel discards it.
Controller settings offer a whole-layout reset only for a changed profile.
The D-pad captures its first finger until lift or cancellation, including
outside its visible bounds. Follow thumb is enabled by default: its logical
center follows overtravel, with a small neutral zone and directional
hysteresis. Direct taps on the arrows still work. A fixed-center option
also retains capture. Other fingers cannot steal that gesture or turn it
into a face-button press. The cross-shaped visual base and thumb marker ease
back on release and stay inside the reserved control well. This continues
to send eight-way digital PS1 input; analog and external controllers remain
later work.
Face buttons have square touch targets and a small margin in otherwise
unused control space. Nearest-button selection and a small preference for
the held button reduce misses and boundary chatter. Forgiveness does not
claim the game picture, the D-pad or another control. Independent fingers
still allow button combinations.
An explicit overlap option remains there. Tall, square and wide
profiles are persisted in logical coordinates. A display change cancels an
unfinished edit; committed profiles survive it. Bluetooth/USB controllers
will later feed the same logical input state; no transport UI is added yet.
Choose core defaults in the app. Ask about firmware or missing companion
files only when the selected game needs them, using an actionable message.
Keep navigation accessible and usable across display sizes. Treat reliable
reopening, sound interruptions and saved progress as part of the experience.

Tap the speaker control to mute/unmute. Holding for half a second opens a compact
volume popover with a live slider and mute, independent of system media volume.
The captured hold never also mutes on release; finger drift, display changes and
interruptions cancel the pending gesture. Zero mutes; raising the slider unmutes.
The mute action retains the last chosen listening level. The main settings
list has no duplicate Sound or Speed subpages. Shared Rust settings are
atomically persisted. The
real-time consumer applies a bounded gain while consuming the same FIFO;
muting never accumulates sound to replay later. Slider dragging changes the
audio gain immediately and retains pointer capture; cancellation keeps the
last listening level. Preference writes are debounced after interaction and
flushed on interruption.
The slider spans a perceptual -56 to 0 dB range, with zero silent. Migration
from the initial linear slider preserves its approximate loudness.

Tapping speed cycles 1x, 2x and 5x target emulation speed, without a menu.
Faster modes retain pitched sound unless muted. The fixed-rate audio consumer
averages source-frame groups at the selected multiplier, preserving stereo;
the bounded FIFO uses a proportionate fill target. Speed changes are adopted
at the next audio callback without reopening the device. Only the consumer
trims excess old-speed backlog to the new fill target. The libretro fast-forward query reflects
the selected mode, with video and audio still enabled. Emulation/rendering
runs on the sole owner thread; no emulation internals change. Frontend
presentation is bounded to 60 Hz above 1x. Achieved speed remains a measurement,
not a guarantee from the selected multiplier.

The FIFO has 16,384 source-frame slots. Foreground play targets about 25 ms
of output; opt-in minimized playback targets 50 ms to tolerate background
scheduling gaps. The consumer trims the extra backlog when returning, without
reopening audio. Fill targets scale with the selected multiplier. This is a
bounded host policy; its background audio quality still needs device validation.

Run in background is off by default. Minimizing pauses emulation, cancels input,
closes audio and checkpoints, without changing explicit pause intent. Opting in
starts Android's media-playback foreground service while the app is visible,
with an ongoing notification offering return and Pause. A callback-only generated
Service forwards lifecycle to Rust; it owns no core or UI logic. The existing
execution thread can continue on its retained EGL pbuffer when the window goes
away. Explicit pause and audio-focus loss still stop playback; there is no boot
receiver or automatic service resurrection. The service stops on pause, disabling
the option, closing the game or destroying the player. Notification permission is
requested only on opt-in. Android's service and audio-focus requirements are
documented in its [background audio contract](https://developer.android.com/about/versions/17/changes/bg-audio)
and [service types](https://developer.android.com/develop/background-work/services/fgs/service-types).

The launcher icon is original blue-Cross button artwork on grey plastic. Borrowed notices
remain in About and APK assets. The free startup path uses unmodified, separately
pinned OpenBIOS built with its documented `BOOT_MODE=fast` option. This omits
the shell rather than showing an intro cut short when a disc becomes ready.
Rust prepares the free image only when `system/openbios.bin` is absent; supplied
firmware remains authoritative. The donor core and its original embedded BIOS
remain unchanged. See [firmware provenance](../vendor/openbios/UPSTREAM.md).

The current donor serializes BIOS bytes in the machine checkpoint, so an existing
session restores its own firmware and continues without rebooting. New disc boots
use the new fallback. A fresh Crash boot reached its intro and title on the Fold;
timed startup latency and compatibility beyond the observed games remain unverified.

## Delivery boundary

A working first playable means actual gameplay, sound, controls, durable
saving and interruption handling on the target device. A built library or
synthetic output is a technical checkpoint.

Keep this design small. Choose dependencies and implementations based on
the first playable result. Broad platform abstraction, new emulation work,
enhancement UI and performance-superiority claims require their own evidence.

Multi-disc entries retain one card identity when discs are appended. The host
generates a private M3U from selected disc paths, registers the donor's actual
base/extended disk callbacks and selects the initial image before game load.
Runtime switching occurs while paused on the single core/GL thread, after a
card flush: eject, select image, close. Failure attempts to restore the old
disc and reports the result. The emulated machine is not reset by a swap.

## Primary references

- SwanStation: https://github.com/libretro/swanstation
- Selected source: https://github.com/libretro/swanstation/tree/e436e9b85374def58d211df7f78880f0f4a917b4
- Rust Android host: https://github.com/rust-mobile/android-activity

These sources justify the initial integration route; they do not prove this
particular application works on a phone.


## PLAYER-2 engine and launch boundary

`engine::Core` selects the two actual bindings from saved `System` metadata;
it is not a user-facing core selector. Legacy records default to PS1 without
renaming IDs, cards or profiles. ISO9660 `SYSTEM.CNF` BOOT/BOOT2 metadata can
distinguish the consoles; a CHD header alone cannot. Ambiguous new entries ask
once. PS3 has a reserved identifier and no nonfunctional library section.

The PS1 callback path above retains its single-thread TLS ownership. ARMSX2
has separate CPU/GS owners and creates its own EGL context sharing the host's
context. Its Rust callbacks use an immutable `Arc` binding, atomic input and
bounded audio, and synchronized output/metric snapshots. No mutable callback
reference crosses native threads. The binding and library remain alive until
native context destruction and CPU/GS teardown finish. Both bindings acquire
the same process core lease, inside the entry's durable writer lease.

PS2 accepts only actual GLES capability at or above the donor's required
version, 48 kHz stereo, bounded native-resolution output and a 68 MiB state
buffer. The adapter forwards supported CPU-owner pause, flush and speed
operations; timing and completed native performance metrics are distinct from
frontend calls. Rust disables automatic guest-clock tuning and cycle skipping
for qualification. On the observed ANGLE/PowerVR renderer, Rust selects the
donor's existing framebuffer-copy path because its EXT framebuffer-fetch
shader fails compilation. Other renderer defaults are retained. PS2 file
cards already flush page writes; the host does not interrupt its VM for the
PS1-only one-second save-RAM poll. Full flush/sync remains part of checkpoints
and interruptions. PS2 background play is not inherited from PS1's opt-in.
Dual-analog/digital touch and normalized editor profiles have separate PS2
storage, retaining all existing PS1 profile names and element positions.
PS2 defaults reuse the PS1 controls and cap styling, adding primary lower
thumb sticks and secondary stick-click keys; they do not change PS1 defaults.

Collection and platform links enter the same `OpenGame(GameId)` flow. Android
initial intents precede automatic resume. Warm intents enter a bounded ordered
queue that coalesces adjacent duplicate targets; different targets serialize.
The link carries no core, executable or writable save path. Missing entries
and files show a recovery sheet without deleting a saved session or stopping
another game. Relinking verifies the original content identity using its
original primary name, permitting directory and primary-file relocation.

Native pinned shortcuts use a stable entry ID, local artwork and explicit
Activity intent. The platform asks for launcher confirmation. An accepted
request is distinct from a confirmed pin; a callback-only receiver records the
latter. Existing pins receive title/local-art updates from the collection.
Removing a launcher icon does not mutate the collection or saved progress.
These are implemented contracts; device acceptance is tracked in PLAYER-2.
