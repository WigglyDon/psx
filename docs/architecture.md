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

On Linux/Android, the PS2 runtime also owns its installed SIGSEGV/SIGBUS
actions. Rust captures the inherited handlers, masks and flags before native
initialization, then restores actions still belonging to that engine only
after its workers and native teardown finish, before releasing its library.
An action installed by another module is preserved. If the OS refuses a
restoration, the stopped library mapping is retained until process exit so a
remaining handler cannot enter unmapped code. This adds no guest fault handler
and changes no checkpoint or card format.

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

The native UI uses analytic GLES signed-distance coverage for rounded caps,
circles, rings, strokes and the molded D-pad cross. Screen derivatives keep
edge smoothing at physical-pixel scale across densities and custom sizes;
small polygon icons use an edge-only coverage fringe. A bounded 72-pixel
font atlas with mipmaps supplies text, with sampler binding isolated from
the donor. Flat charcoal sheets, small corners, fine boundaries and restrained
control shading are rendered in the existing UI batch; sheet clipping also
contains scrolling content and opaque header/footer masks. This requires no multisampled
game framebuffer or full-screen smoothing pass, and does not change native
game resolution, game filtering, controller hit targets or saved layouts.
Barlow Semi Condensed Medium supplies compact menu/control lettering, with
Noto Sans fallback for supported characters absent from that face. Both are
unmodified OFL fonts with pinned source/hash and exact notices. They share
the single bounded atlas. Menu headings use capitals; game titles retain
their spelling. The hourglass omits the normal 1× caption and uses a larger
green caption for other rates, with short `0×` and `max` endpoint labels.

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

Controller profiles distinguish app defaults from explicitly enabled per-game
overrides, keyed by the durable entry ID and independent of cards/checkpoints.
Legacy default display files remain in place for digital and analog capabilities.
Enabling a game's custom controller copies all current display profiles and
default phone bindings; subsequent edits affect that game only. Disabling it
inherits current app defaults while retaining its custom arrangement for later
re-enabling. Tall/square/wide profiles remain separate within either scope.
Versioned metadata is written atomically before publishing live preferences.
Existing per-game phone assignments migrate as enabled phone-only overrides,
inheriting existing display layouts until customized. The old file remains
untouched and is not re-imported after new metadata has been published.

Only Volume Up/Down enter the phone-key adapter; power and system-navigation
keys remain platform-owned. Unassigned keys and new presses in menus or outside
foreground gameplay return to Android. Rust merges captured digital presses
with touch masks before the existing engine input call, preserving independent
release and short taps at the host input-update boundary. Interruption clears
logical input and pending taps; captured repeats still delivered to the app
remain consumed until release and
cannot rearm on return.
No background listener, extra permission, runtime or managed callback is added.
Platform delivery and physical simultaneous-touch behavior still need device
validation; host tests establish the input/persistence contracts.

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
Automatic checkpoints use the pinned engine's state interfaces on the same
execution/GL owner. PS1 uses public serialize/unserialize. PS2 captures the donor's owned
byte/name entry list while paused, then transfers it to the existing bounded
Rust save worker for archive packaging. Packaging reads no live VM settings,
cards or graphics and retains the native library until the list is consumed.
Cancellation frees that list once. The worker hashes and
atomically writes one captured state; no worker calls emulation or GL. Periodic
writes do not block playback on storage sync. Background, explicit pause and
closure wait for durability. Resume validates core/disc inputs, payload integrity
and the contemporaneous card before entering the core, then restores the durable
card. See [formats.md](formats.md) for recovery and reset behavior.

ARM64 card/payload hashing uses the pinned SHA-256 implementation's runtime
CPU-feature detection and accelerated backend, retaining its portable fallback
and identical digests. This reduces host verification work without weakening
checkpoint/card checks or changing saved identities. Capture and card sync
remain synchronous; they are not made free by background archive packaging.

## Everyday use

The first screen offers saved games and Add games. During play the cog opens
a native settings sheet over the running game; opening it releases input
without changing pause intent. Pause/play and speed have separate controls.
Tapping outside a sheet dismisses it after lift, without passing that touch
through to the game. Embedded navigation uses compact flat panels, restrained
highlights and explicit switch positions. The library is an adaptive grid of
game cards with local artwork, two-line titles, console/disc details and a
separate 48-point options target. Embedded sheets may use up to 720 points of
width and 86% of available height to keep more content visible. Rendering, tap/scroll geometry and responsive
column counts share one Rust model. Its Add and Settings icons live in the app bar; primary actions stay distinct
from back navigation. Controls occupy separate space by default;
the largest clear 4:3 picture is computed from their rectangular targets.
Center the picture horizontally, maximize its clear 4:3 size and keep its
highest feasible position, leaving room below or beside it for controls.
There is no vertical-centering step. If custom controls block the entire
center column, retain a usable clear side region instead of an empty picture.
Generated controllers share the actual player canvas and input-capability
metrics. On inner displays, adjacent thumb wells use available headroom after
the maximum 4:3 picture; circular action caps retain clear separation and at
least 48-point targets. The digital controller targets 184-point thumb areas,
64-point shoulders and 80-point system keys, reflected exactly about the display
center. The analog controller uses the same 184-point thumb-area ceiling for
its four wells, bounded by actual width and headroom. Extra keys use the clear
center: taller bands use two columns, shorter bands use two rows, without
occupying the route between each stick and its adjacent digital well. First
shoulders are outside second shoulders, or below them when stacked. Stick-click
and system keys retain separate reachable targets; double-tap-and-hold remains
available for stick clicks. Thumb wells leave four points plus the system gesture
strip below them; center keys can use the remaining gesture-safe height. Other
display shapes compare clear docks/side rails. Wide digital controls solve both
thumb wells together around a side camera, keeping square proportions and equal
heights. Mirrored shoulder pairs and inward system keys sit directly above them.
No input is overlapped merely to imitate a physical controller. Owner-created
arrangements guide size and balance; defaults correct accidental asymmetry.
Saved custom rectangles still take precedence, so changing defaults does not
silently alter a saved layout. Each orientation is budgeted from its actual
dimensions instead of reusing a generated layout stretched along two axes.

The owner explicitly chose game-picture use of the complete display, including
camera pixels. A physical camera can occlude a corner of the image; it must not
reserve a broad band or shift the picture. This policy does not authorize
cropping the guest frame or stretching its aspect ratio. Controller targets,
chrome and popovers still avoid actual holes locally, without changing stored
input coordinates. A whole face-button cluster moves together when needed.
The live editor and play use the same placement and canvas.
Its camera guide is an outline of the input exclusion, leaving the game visible
inside the camera's bounding rectangle rather than painting that rectangle over it.

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
works. Fullscreen drawable dimensions remain independent of transient status
decoration. A callback-only platform shim forwards applied-inset changes to
Rust, which queries actual status visibility and its bounds ignoring visibility.
This catches transient bars that report zero occupied insets. Revealed status
decoration moves the toolbar over the stable game picture; it does not resize
the picture or controller geometry. Hiding it restores the normal thin strip.
The full picture canvas excludes caption decoration, while controller input
keeps its own gesture/camera-safe surface. Insets remain tagged to the actual
drawable size. Picking and audio-focus interruption retain safe system UI.

The less-used system buttons keep their own separate row. Wide phone layouts use side rails,
including their system buttons, so a bottom row does not constrain picture
height. Pause/time are anchored upper-left; sound/settings upper-right in every
orientation. Each pair can move inward around a camera while retaining its side.
The toolbar uses an 18-point-high strip with no extra gap below it; ordinary
targets remain 40 points wide, and time has room for a horizontal readable rate.
Glyphs use more of the strip's height to remain readable.
Its normal position uses the display's top edge, moving a corner pair inward
around a camera. Visible status bars push chrome below their actual bounds,
temporarily over the stable picture. Game-button targets remain separate from
the picture unless the owner enables overlap. This does not change controller
coordinates or select another saved profile.
These app-chrome anchors supersede legacy toolbar rectangles; stored controller
profiles remain intact. The controller editor edits game inputs and the picture,
over the live game at the same size and coordinates used for play. Android requests `fullUser`
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

The controller editor selects elements by tap and moves them by dragging at
actual size. Selection targets remain at least 48 points even for a smaller
custom cap. Resize mode exposes 48-point corner grips, keeping the opposite
corner anchored; Move mode keeps those grips from stealing selection. The
floating tools can dock above or below, and hide during a move/resize gesture.
The live game remains visible and retains its existing pause intent; editor
touches do not reach game input. Matching camera/status clearance keeps draft
geometry consistent with play. No miniature or extra controller margin is used.
The action buttons select as one group by default. Dragging keeps their relative
positions; a corner scales their sizes and spacing together. Individual switches
to per-button editing through Single, and Group returns to the cluster. A live
Button spacing slider redistributes the four centers into a balanced diamond
without changing cap sizes. Its available spread is bounded by the surface;
existing custom positions are untouched until that slider is used. Group Reset restores
just those four buttons; individual Reset retains its existing scope. The D-pad
remains one element. All edits use the existing per-display preset format.
Grid enables an eight-point grid centered about the display's horizontal
midline, with nearby edge/center alignment to peers, defaults, screen margins
and mirrored horizontal centers. It can be disabled for free placement. Visible
guides explain alignment; resizing also snaps width while preserving aspect.
Mirror explicitly copies the selected size and reflected position to its
opposite partner: sticks, matching shoulders, Start/Select, stick clicks or
D-pad/action-group bounds. It does not silently link later drags. Camera
collisions stop the selected object instead of pushing unselected face buttons.
The game alone still recomputes its available clear area as controls change. The
unsnapped drag origin is retained so slow movement can escape a guide; disabling
Grid permits free placement. A bounded 32-step draft Undo records one entry per gesture,
spacing adjustment, Mirror or Reset. These are draft changes until Done;
Cancel discards the editor's draft. Existing saved custom positions are preserved.
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

The hourglass toggles between normal time and the remembered time scale.
Holding it opens an anchored slider: zero pauses, finite settings span roughly
0.05x to 5x, and the maximum endpoint is uncapped. Moving the slider enables
the chosen scale; toggling it off retains that choice. Existing Normal/Double/
Quintuple preferences migrate without changing their active effect. The slider
changes wall-time pacing, preserving guest clock ratios and device timing.
Zero time is separate from manual pause and interruption gates. Restoring
normal time does not clear a manual pause; Play from a zero-time pause returns
to normal time. Background play cannot run at zero time.

The fixed-rate audio consumer uses fractional weighted source-frame groups,
preserving stereo and audible pitch changes without allocations or locks.
Slow rates retain at least one guest-frame source burst when trimming, so
their smaller output-fill target cannot discard most of a frame's sound.
Only the consumer advances its cursor or trims old-rate backlog. Uncapped
audio adapts to bounded queue fill; its bounded resampling work does not cap
guest execution. Presentation remains bounded to 60 Hz above 1x and for the
threaded PS2 engine at every rate. PS2 coalesces requested rate changes to at
most 20 per second and keeps audio on the applied rate. The native CPU owner
changes its existing limiter deadline
without a full settings reload or a pause/resume transition. Zero-time pause
uses the ordinary runtime pause path, rather than the native limiter's zero
(which means unlimited). Actual achieved speed and Fold audio/UI quality remain
separate device observations, not guarantees from a target setting.

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

The native GS handoff publishes one frame and waits for its consumption.
A narrow C-ABI notification wakes Rust's existing event-loop owner immediately;
an atomic pending bit coalesces signals and survives attaching the platform
waker after publication. Only that owner calls `retro_run`, consumes fences,
and uses the frontend GL context. The callback performs no GL, VM or file work,
and is retired under the handoff mutex before host/library teardown. Android's
stock `AndroidAppWaker` stays in its platform adapter. The portable binding uses
only `std::task::Waker` and ephemeral state, never durable platform handles.
PS2's native CPU limiter owns wall-time pacing; a second fixed frontend frame
clock must not make an otherwise ready producer wait another frame interval.
An 8 ms housekeeping bound services input/audio without waiting for a picture.
Duplicate service calls do not redraw the game; composition is separately
bounded to 60 Hz. Pending composition has its own wake deadline, so the cap
does not defer a ready picture until the next housekeeping tick.
Native emulation speed, service frequency and presentation
costs are measured separately. This scheduling repair needs physical timing
qualification; its host notification tests are not a gameplay/speed result.

PS2 accepts only actual GLES capability at or above the donor's required
version, 48 kHz stereo, bounded native-resolution output and a 68 MiB state
buffer. The adapter forwards supported CPU-owner pause, flush and speed
operations; timing and completed native performance metrics are distinct from
frontend calls. Rust disables automatic guest-clock tuning and cycle skipping
for qualification. On Android it enables the donor's existing public performance
hint session for registered native workers. The retained sampler reports active
work against the finite emulated frame deadline, excludes limiter sleep and
closes at VM teardown. Unsupported platform symbols leave hints inactive; this
does not force clocks or bypass Android thermal policy. Its effect on sustained
speed and heat needs device measurement. Completed GS-owner samples additionally
forward bounded CPU/GPU costs and draw/readback/barrier/blocking-wait counters;
Rust never reads those mutable native counters from its frontend thread.
The donor instruments blocking waits only on Vulkan: a zero in that field on
GLES is unavailable evidence, not proof that no GPU wait occurred. GLES analysis
uses its measured CPU/GPU times and implemented draw/readback/barrier counters.
The repaired GLSL header enables EXT for the PowerVR fetch
backend's `inout` syntax; Rust migrates the earlier ANGLE/PowerVR copy workaround
to fetch enabled. Other renderer defaults are retained. PS2 file
cards already flush page writes; the host does not interrupt its VM for the
PS1-only one-second save-RAM poll. Full flush/sync remains part of checkpoints
and interruptions. PS2 background play is not inherited from PS1's opt-in.
Per-entry firmware selection and settings changes occur under the same writer
as cards/checkpoints. The explicit donor BIOS option and resume identity use
that entry's pinned filename/hash; directory discovery cannot silently change
an existing session after another firmware import.
Failed checkpoint restoration leaves the owned runtime unavailable. Pause,
focus or repeated launch requests cannot make it runnable, and it cannot
write a replacement checkpoint. The recovery sheet offers explicit restart
with the current card; ordinary audio-interruption recovery is unaffected.
Dual-analog/digital touch and normalized editor profiles have separate PS2
storage, retaining all existing PS1 profile names and element positions.
PS2 defaults reuse the PS1 controls and cap styling, adding primary lower
thumb sticks and secondary stick-click keys; they do not overwrite PS1 profiles.
On wide inner displays a compact candidate puts digital clusters directly beside
the primary sticks, without a system/click column between the thumb areas.
With sufficient headroom below the maximum picture, 176-point primary sticks
sit outside raised 128-point digital wells, with separated 51-point action caps.
Level shoulder pairs and stick-click buttons sit below the digital wells;
Start/Select sit one key height above that row in the center. L1/R1 are outside
L2/R2. Shallower surfaces retain adjacent equal-sized wells and two compact
center rows; stacked fallbacks put 1 closer to the lower thumb areas.
It wins only when its clear 4:3 picture is larger. Narrower
layouts keep stacked digital/stick bands; landscape retains full-height picture
space between side rails.
Toolbar pairs sit 28 points inward where space permits, protecting them from
rounded display corners without adding a new row.
Generated button targets stay at least 48 points and sticks at least 104
points on usable displays. The clear-picture solver excludes them all, and
saved custom rectangles retain precedence. Transient tiny surfaces scale only
their presentation, without rewriting profiles.

For a wide surface with sufficient rail width, a side-camera candidate places
the digital clusters above the camera band and primary sticks below it.
Shoulders and a separate stick-click button sit beside each stick, with the
first shoulder closer to its thumb center. Both rails are solved symmetrically before camera avoidance;
the camera does not push one entire cluster into the centered picture. Selection
compares the unscaled clear picture and applies only to generated non-overlap
defaults, never saved custom rectangles. The measured 2342×1080 Fold with a
59-pixel navigation strip and 182×112 side camera retains a centered, top-aligned
1021-pixel-high 4:3 picture in the host fixture and in the installed Fold build
`c27ce6f698c3`. The actual viewport was 1361.3×1021 at 490.3,0 in
2342×1021 content. This measures the host viewport, not any black borders
drawn inside a game's own frame. The next rail defaults increase sticks from
104 to 128 points while retaining that full-height picture in the host fixture.
Separated action caps remain about 58 points there. Every generated cap remains at least
48 points. PS2 cap centers leave a four-point edge gap; sizing inside the well
replaces the previous overlapping diamond. Narrow PS1 wells grow to 128 points
to separate their minimum-size caps. Manual group spacing remains independent
of cap resizing and does not rewrite existing profiles automatically.

PS2 stick clicks also have an optional double-tap gesture, enabled by default:
a short stationary tap followed within 280 ms by another nearby touch engages
L3/R3 immediately on the second down. It stays pressed while that touch moves
the stick and releases on lift. Movement, long holds and interruptions do not
prime a click. Both sticks and separate buttons remain independent; no hold
timer delays ordinary analog input. Older presets default to the gesture.

The PS2 GS fills its intermediate 640×448 buffer. The Rust host alone applies
the final 4:3 fit; fitting at both stages adds redundant side bars. This does
not crop guest pixels or stretch the final picture. Game-drawn borders remain.
Large separate controls and a camera can still prevent an edge-to-edge rectangle;
the clear-picture solver keeps the largest high, centered picture around them.

The retained GL header repair matches the donor's existing backend policy:
non-Mali EXT syntax enables EXT even when ARM is also advertised. Previously
ANGLE rejected its `inout` output because only ARM was enabled. The paired
Rust setup removes our PowerVR copy workaround; Fold rendering/performance
still needs checking. Libretro omits only the inapplicable manual-API-choice
warning: its frontend negotiates the actual API. Other diagnostics remain.

PS2 checkpoints use the donor's existing uncompressed ZIP option and omit its
unused PNG thumbnail. Rust validates the native length prefix and stores only
the archive extent, within the same 68 MiB cap. The native reader accepts that
extent and the former padded buffers. This reduces synchronous preparation
work; improved device timing remains a separate observation. Exact core/patch
and firmware binding still applies before restoration. One explicit exception
accepts five named preceding adapters into the exact reviewed card-recovery and
performance-hint patch. These repairs change no state or archive fields.
All six patch hashes are named in Rust; a future edit disables this exception
automatically. Firmware, discs, current card and payload integrity must still
match; unknown and older incompatible bindings remain refused.

The donor state stream omits card-endpoint negotiation and transfer state. A
fresh runtime must therefore use its existing eject/reinsert/authentication
sequence after restoration even when card CRCs match; matching file bytes do
not restore the endpoint. This keeps the old archive readable and current cards
authoritative. SIO2 also refuses PS1/PocketStation commands on PS2 cards, matching
the SIO0 type check, so a failed PS2 probe cannot enter legacy 128-byte writes
on PS2 NAND. Focused Rust-driven fixtures reproduce both defects in the original
dispatcher/restore code and exercise the retained-source repairs. They do not
establish ordinary guest save/load success or repair earlier damaged card bytes.

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

### Player presentation and touch-controller geometry

The player viewport owns the available picture canvas and chrome clearance.
It has no console identifier. The controller preset supplies input geometry and
capabilities; it does not choose another presentation system. Android and the
live editor compose through the same Rust `PlayerViewport` operation. The
picture solver maximizes a complete, correctly proportioned image, places it
high and honors explicit non-overlap inputs/chrome. Camera pixels are included
by the owner's explicit choice; interactive elements still avoid holes. Gesture-safe
controller bounds remain distinct from clear drawable pixels usable by the
picture. Transient bars do not introduce a second canvas or a size preset.

Generated inner-display digital and dual-stick controllers share a shallow
thumb band and common cap, auxiliary-key and toolbar metrics. Additional PS2
inputs affect the input geometry, not the picture/chrome policy. Existing
per-display PS1/PS2 preset identities and normalized rectangles remain compatible;
custom geometry takes precedence. Future physical-controller support should
withdraw the touch overlay from this composition, retaining the same player
presentation. Physical-controller acquisition/routing is not yet implemented.

A host rectangle reaching a display edge is distinct from the visible pixels
inside a source frame. Do not infer that all black regions are frontend
letterboxing or crop dark content automatically. The latest FF7 checkpoint's
320×240 display contains eight all-black guest rows at each end, verified
read-only from its saved GPU/VRAM fields. Its larger outer margins were caused
by the old PS1 touch dock, independently of those source rows.
