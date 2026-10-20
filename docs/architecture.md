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
The default layout compares arrangements at the actual window dimensions.
On a sufficiently wide square display, shoulders and system buttons occupy
the space between thumb clusters. Main thumb geometry sets the dock's minimum
height before maximizing the clear picture: square-display PS1 wells retain
200 points of height and 84-point face targets. Thumb clusters leave 32 logical points below
their lowest target before local camera avoidance. A 16-point gap separates Triangle and Cross;
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
The picture search includes actual camera bounds while growing about the
display's horizontal center. This prevents a corner camera from silently
shifting the picture sideways; saved game-control rectangles remain unchanged.
Camera avoidance is derived for the current window without overwriting saved
control positions. The editor uses the same placement/defaults, and anchored
volume/time panels slide clear of camera bounds. Shoulder caps have simple symmetric
faces with small corners. Landscape pairs sit level, with L2/R2 outside and L1/R1 inside;
Stacked pairs put L1/R1 above L2/R2. Start/Select have smaller faces inside their retained touch targets, at
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
works. Fullscreen drawable dimensions remain independent of transient status
decoration. A callback-only platform shim forwards applied-inset changes to
Rust, which queries actual status visibility and its bounds ignoring visibility.
This catches transient bars that report zero occupied insets. Toolbar clearance
and the largest unobstructed picture are recomputed when decoration changes;
hiding the bar restores the previous maximum. Navigation's reserved strip,
caption decoration and camera bounds remain excluded. Visible-bar layout is
retained during picking or audio-focus interruption. Insets remain tagged to
the actual drawable size.

The less-used system buttons keep their own separate row. Wide phone layouts use side rails,
including their system buttons, so a bottom row does not constrain picture
height. Pause/time are anchored upper-left; sound/settings upper-right in every
orientation. Each pair can move inward around a camera while retaining its side.
A clear camera margin can host them only while OS decoration is absent; visible
status bars push them below the actual bar. App chrome uses 40×32-point targets,
while game-button targets retain their 48-point minimum. Clear pixels below
that smaller toolbar in the camera margin also become picture candidates;
actual camera rectangles and every target remain excluded. This does not
change content coordinates or select another saved controller profile.
Targets never cover the game.
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
guest execution. Presentation remains bounded to 60 Hz above 1x. PS2 coalesces
requested rate changes to at most 20 per second and keeps audio/frontend pacing
on the applied rate. The native CPU owner changes its existing limiter deadline
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
The mirrored PS2 lower band shares height between each stick and an inner
system/click column. On wide inner displays a compact candidate places secondary
digital clusters inside and slightly above the primary sticks, with shoulders
in the middle. It wins only when its clear 4:3 picture is larger. Narrower
layouts keep stacked digital/stick bands; landscape retains full-height picture
space between side rails.
The compact inner-display candidate gives primary sticks up to 128 points and
digital/action wells at least 160, fitting less-used shoulders into the clear
middle. It does not shrink the main clusters to chase a full-width result.
Generated button targets stay at least 48 points and sticks at least 104
points on usable displays. The clear-picture solver excludes them all, and
saved custom rectangles retain precedence. Transient tiny surfaces scale only
their presentation, without rewriting profiles.

For a wide surface with sufficient rail width, a side-camera candidate places
the digital clusters above the camera band, shoulders beside it, and primary
sticks below it. Both rails are solved symmetrically before camera avoidance;
the camera does not push one entire cluster into the centered picture. Selection
compares the unscaled clear picture and applies only to generated non-overlap
defaults, never saved custom rectangles. The measured 2342×1080 Fold with a
59-pixel navigation strip and 182×112 side camera retains a centered, top-aligned
1021-pixel-high 4:3 picture in the host fixture and in the installed Fold build
`c27ce6f698c3`. The actual viewport was 1361.3×1021 at 490.3,0 in
2342×1021 content. This measures the host viewport, not any black borders
drawn inside a game's own frame. Primary sticks retain 104 points; separated action
caps are about 58 points in that particular constrained camera layout, versus
roughly 64–74 in the square PS2 wells. Every generated cap remains at least
48 points. PS2 cap centers leave a four-point edge gap; sizing inside the well
replaces the previous overlapping diamond. Narrow PS1 wells grow to 128 points
to separate their minimum-size caps. Manual group spacing remains independent
of cap resizing and does not rewrite existing profiles automatically.

PS2 checkpoints use the donor's existing uncompressed ZIP option and omit its
unused PNG thumbnail. Rust validates the native length prefix and stores only
the archive extent, within the same 68 MiB cap. The native reader accepts that
extent and the former padded buffers. This reduces synchronous preparation
work; improved device timing remains a separate observation. Exact core/patch
and firmware binding still applies before restoration.

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
