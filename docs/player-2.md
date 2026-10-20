# PLAYER-2

Active work: [issue #2](https://github.com/WigglyDon/psx/issues/2) and its
[Master dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6272753838).
The dispatch supersedes the old hold on issue #1's formal closure.

## Current UI qualification

The October 20 02:31–02:44 UTC window installed `a8e2a3d38c5f` after a fresh
current-data/APK backup. Darkstone's paused PS1 title restored; the analytic
controls, text, settings and time popover rendered on the Fold. Slider UI
reached zero/Paused and maximum/Uncapped, then returned to normal 1× with 2×
remembered. Manual pause remained independent; running-rate/audio acceptance
does not follow. Repeated USB loss prevented PS2 testing and final lock/screen-off
verification. Device access was released and the owner informed.

The subsequent owner-directed UI increment prepares sharp flat sheets, adaptive
game cards, fixed toolbar corner pairs, explicit visible-status-bar clearance
and larger inner-display picture candidates. Saved game-control geometry takes
precedence; generated targets stay at least 48 points, sticks 104 points. Host geometry,
input and actual GLES previews are distinct from device acceptance. The ordinary
PS2 card failure, gameplay/save/performance checks and Red Dead transfer remain
pending under issue #2.

The October 20 03:02–03:09 UTC window installed `f4ca71bb19e9` after a fresh
170 MiB current-data backup (games excluded). Resident Evil's automatic restore
was logged and resuming visibly returned to its intro. The sharp settings and
three-column library rendered, including local save artwork and multi-disc
details. Selecting Baldur's Gate reached the expected incompatible-checkpoint
guard; USB loss prevented a verified Restart or PS2 boot. The same psx process
survived reconnection. A bounded host kernel excerpt records actual USB
disconnect/re-enumeration events, including events before the first admitted
command; link/power/phone-stack cause remains undetermined. No server/USB reset
or USB configuration change was issued. Access was released with close/lock/
screen-off unverified, and the owner informed.

Owner feedback rejected the shrunken primary controls. The correction restores
larger thumb geometry, uses a smaller app toolbar and recovers clear pixels in
the existing top camera margin without moving saved controls. Sheets are wider
and taller. Compact OFL typography replaces the general-purpose UI face; normal
1× has no rate caption and other rates use larger green labels. Actual device
acceptance is still required for comfort across every display/orientation.

The October 20 03:22–03:55 UTC window installed `31753eef3fab` after a fresh
complete backup. The larger controls, four-column library, compact typography,
roomier settings and non-normal time label rendered. Resident Evil restored
its paused newspaper frame. The actual camera exposed left-biased picture
placement: the earlier host fixture underestimated its bounds. The owner
clarified that maximum edge-reaching size and high placement take precedence
over centering. The next correction retains that maximum, prefers the highest
clear placement and uses a 32-point-high toolbar with 40-point-wide tap targets,
testing the measured camera/navigation geometry without changing controllers.

Fresh Baldur's Gate boot still failed slot-1 card formatting. A verified,
temporary IOP-interpreter comparison and fresh restart produced the same
error; that setting was restored byte-for-byte and its temporary file removed.
No bounded protocol write/erase trace appeared. Both PS2 cards and all eight
PS1 cards, the library and PS1 profiles remained byte-equal to the fresh
initial backup. These observations do not establish a particular card-protocol
cause, controlled gameplay or ordinary saving. Own logs also identified a real
missing built-in patches resource, now included from the pinned donor's Android
assets; the fixed-API and framebuffer-copy warnings describe the selected
host/driver path and are not proof of an additional fault. Expansion lines
report renderer capabilities. Original diagnostics are retained.
The next native diagnostic adds a bounded SIO2 opcode/status trace before
write/erase, without card or authentication payloads. It is not a card fix.
The changed patch identity retains older PS2 checkpoints behind explicit
restart recovery; current cards and all PS1 checkpoint bindings are preserved.

PSX was closed and confirmed stopped before final backup/handback. The Fold
woke during the lock check and USB disappeared before retry; final lock and
screen-off were unverified. All clients/leases ended within the authorized
extension and the owner was informed. No system/USB/radio/security setting or
unrelated app was changed. There is no current device custody.

The October 20 04:16–04:39 UTC window installed `697b723099c5` after a fresh
full backup and a byte-verified actual installed APK backup. Automatic PS1
restoration logged; early pause/time/game-switch checks were confounded by
concurrent owner use until they explicitly yielded. Legend of Dragoon rendered.
Blank paused preview also appeared before the update. The owner's latest
clarification is horizontal centering with high placement, maximizing within
that clear space. The next solver enforces that horizontal center without a
vertical-centering step or any change to stored game-control rectangles.

The actual installed patches archive matches its pinned hash and the missing
resource warning is gone. Baldur formatting still fails. Bounded SIO2 records
show present=1/eject=0, authentication/setup/specs/reads and terminator `5A`;
format confirmation still produced no write/erase trace. This is evidence for
the next protocol investigation, not an ordinary-save fix. Red Dead Revolver
was transferred, hash-verified and selected through the normal scan/add flow:
10 games, 15 discs. It was not booted. All eight PS1 cards, both PS2 cards and
PS1 profiles remain byte-equal to the fresh initial backup. Final backup is
readable; psx was closed and confirmed stopped. After sleep verification again
showed awake/unlocked, the owner requested immediate release for personal use.
All device work and the lease ended at 04:39:04 UTC; locking/screen-off remained
unverified and was reported. No current custody or queued reconnect remains.

## Preserved baseline

The source baseline is `a1bdd2810e67ede3377383613b397776f966748c`.
The working installed PS1 runtime was `470fa617f249`, APK SHA-256
`e0969932623987e0e40647da587746805f4b77edabfa2f881b1ebf05a8d8de07`.
Its source, APK, original signing identity and historical private backup are
retained locally. Before any update, back up actual current device data;
the historical backup predates further owner play. Keep `com.machinerhythm.psx`
and its signing identity unchanged.

The first candidate (`22782b813eb4`) was installed in place on October 19 after
a fresh recoverable backup. It aborted during startup layout calculation on
Android's temporary 1-pixel surface, before loading a game. The working PS1
APK was restored in place and returned to the same paused FF7 scene. The
library, cards, controls and active-session files compared byte-for-byte equal
to the fresh backup. The phone was returned locked and screen-off. A focused
regression now covers saved PS1/PS2 controls through tiny transient surfaces;
the corrected candidate still requires device qualification.

The next candidate passed transient-surface startup, then refused session
ownership because `std::fs::File::try_lock` is unsupported on Android. A
bounded same-app-UID probe verified native `flock` succeeds on the Fold.
The Unix Rust adapter now calls descriptor-owned `flock` directly, also used
by the host competing-writer/process-death fixtures. Contention and underlying
I/O failures have distinct messages; ownership protection is retained.

Candidate `1c121dd47e7a` restored the same paused FF7 scene through that owner
on the Fold. All eight existing PS1 library records, five ordinary card files
and controller profiles compared unchanged against the fresh backup. PS2
firmware import and content-based scan/add succeeded. FF7's native pin
confirmation callback, actual launcher cold/warm entry, repeated same-game
intents and missing-entry recovery ran without replacing the paused scene.

The first PS2 launch then failed before emulation: the new adapter functions
lacked explicit C linkage and were hidden by the donor's `retro_*` export
map. The adapter now declares C linkage, and package verification rejects
missing adapter exports. This check rejected the failing artifact. The working
PS1 candidate was retained and the third
device window ended with verified locked/screen-off return.

Candidate `140092049f22` reached Baldur's Gate's memory-card menus using
ARM64 EE JIT and hardware GLES 3.2 through ANGLE/PowerVR, with 48 kHz output.
Its EXT framebuffer-fetch shaders initially failed, leaving text invisible;
selecting the donor's existing framebuffer-copy path restored readable text.
PS2 → FF7 → PS2 and repeated switches restored their paused scenes in one
process, including separate PS2 menu checkpoints. These are menu/state/switch
observations, not moving gameplay or an ordinary-save pass.

The game's first attempt to initialize its new isolated card reported an
access error. That card and checkpoint are retained privately; a clean disc
restart using the current card is the next diagnostic. Own-process logs also
exposed the host's unnecessary one-second PS2 pause/flush, now removed from
the PS1 RAM-card polling path. Menu samples before that repair commonly showed
94–95% native speed with audio underruns; neither useful gameplay performance
nor audible PS2 sound has been established. The fourth window ended October
19 at 21:17 UTC with verified locked/screen-off return and no psx process.

The latest Master engineering correction permits targeted core repairs and
Rust subsystem/boot work when justified. Continue the current integration;
additional engine/firmware evaluations are optional evidence, not a mandatory
sequence before fixing a concrete defect. BIOS-free default operation is
unqualified and must preserve existing firmware-bound sessions.

Candidate `60690ef96194` installed in place after a fresh private backup and
restored FF7. Its unavailable-state guard refused the old unbound PS2 snapshot
and retained it byte-for-byte until explicit Restart. Clean Baldur's Gate boot
reproduced the new card's format/access error; declining card use reached the
opening cinematic and interactive tavern dialogue. The owner confirmed audible
dialogue/music with breakup at hitches. D-pad selection and action confirmation
worked. A fresh process restored that dialogue scene. These observations do not
establish controlled locomotion, ordinary saving or gameplay resume acceptance.

After removing the one-second poll, menu samples commonly reached 59.9 VPS /
100% native speed. Opening-scene samples varied, including 31 VPS / 51.7%;
tavern dialogue commonly reached 58–60 VPS. Periodic checkpoint preparation took
about 400–750 ms with audio underruns. The next increment omits its unused
thumbnail, uses the donor's stored ZIP format and drops validated unused ABI
padding. One captured 68 MiB buffer contained only a 21.5 MB archive; its native
entries totalled 42.7 MB including the thumbnail. These source/size findings do
not establish the new device timing or fix the independent card error.

The owner found PS2 defaults used excessive unfolded height. Revised mirrored
defaults put the stick, system key and stick-click in a shared lower band,
digital clusters above, and shoulders in the remaining clear columns/rail.
The two sides share their geometry, with at least 48-point button targets and
104-point sticks. The clear-picture solver still excludes every touch target.
PS1 profiles and explicitly saved PS2 rectangles retain precedence. Device
ergonomics remain to be checked. Successful Restart also clears the stale Error
screen, and long text wraps to the actual embedded sheet width.

The fifth window ended at 23:26 UTC after checkpoint, fresh backup and closing
only psx. Locked state and no psx process were verified; USB disappeared before
screen-off verification, so screen-off handback was explicitly unverified.
No unrelated apps, system settings, packages or connectivity were changed.

Off-device, the unmodified OpenPS2BIOS candidate at
`43005227db73090a4a4b5701dee510a728d0e689` built twice to identical 4 MiB bytes,
SHA-256 `3ad7ab89b01eeedd61eb6afe39013da5a3d366c63d7c63b608c32526bb6ecbda`.
The tools were Fedora Clang 22.1.8 and the pinned NDK's LLD/objcopy, using
upstream Make/Python packaging unchanged in isolated private outputs. Both
that image and the owner-supplied image passed the Rust ROMDIR binding probe.
These are build/header observations only: the candidate is not bundled,
provenance clearance and gameplay/audio/save compatibility are not established,
and no existing session's firmware was substituted. Its upstream
[compatibility table](https://github.com/TamaribaEmu/OpenPS2BIOS/blob/43005227db73090a4a4b5701dee510a728d0e689/COMPATIBILITY.md)
claims menu access only. Further candidate work need not delay the current
gameplay/card investigation.

## Implemented, awaiting device qualification

Rust distinguishes a validated stable game ID, durable saved session and
new runtime assignment. One OS-backed per-entry writer lock spans card and
checkpoint work and donor teardown. A subprocess-death fixture verifies
lock release; another verifies reassigning an unloaded session without
changing its saved identity. One live runtime is this milestone's policy.
No second worker, general IPC or multi-game background toggle was added.

Collection taps, imports, automatic resume and Android intents use one
`OpenGame` flow. Content resolution precedes switching. Same-game return
retains pause intent. Initial shortcut requests precede old-session resume;
ordered warm requests coalesce adjacent duplicates. Local game actions use
native pin confirmation, stable IDs and local artwork, with confirmed pins
separate from accepted requests. Pins receive local title/icon updates.
Missing content offers verified relinking without changing saved identity.
Legacy library JSON and existing PS1 profiles remain compatible.

ARMSX2 is pinned at `83fe4e293f71ab5587ef81b06143725bf8c6a30b` with
all resolved dependency commits, nested trees and original notices retained.
The ARM64 libretro library linked through both the donor recipe and the Rust
build task. Standalone frontends and floating patch downloads are disabled.
The recorded adapter exposes existing native owner operations and forwards
completed native metric samples; it does not redesign emulation or JIT.
See [build.md](build.md), [THIRD_PARTY.md](../THIRD_PARTY.md) and
[adapter details](../patches/armsx2/README.md).

The host implements thread-safe callbacks, negotiated GLES, 48 kHz sound,
independent regular-file cards and revision/patch-bound 68 MiB checkpoints.
It syncs cards after native flush and again after shutdown before sealing.
Dual-analog/digital controls use the existing editor with independent PS2
profiles. PS2 does not inherit PS1 background play. Native resolution,
intended guest clocks and explicit target time scales remain distinguishable
from achieved native speed. The current PS2 JIT path requires 4 KiB pages.

The owner refined the time control: hourglass tap toggles normal time versus
the remembered scale; hold opens a live slider, with zero paused and maximum
uncapped. Fractional rates keep stereo audio; uncapped audio uses bounded queue
feedback. Manual pause/lifecycle gates stay independent. PS2 updates only its
existing limiter deadline and coalesces rapid requests, without reloading all
settings or pausing the VM for each rate change. Host checks cover preference
migration, toggle memory, fractional audio phase, zero/uncapped bounds and pause
gates. The signed candidate and actual Fold interaction/audio/performance still
require their own qualification; the owner has not released the Fold again.

The owner selected Baldur's Gate: Dark Alliance for first PS2 acceptance.
Local PS1/PS2 games and firmware were consolidated privately, retaining local
compatibility aliases. Duplicate archives were removed only after comparing
their game contents against canonical files. No games, firmware, saves,
signing material or raw device evidence are public source.

## Acceptance still required

Host fixtures and build/lint checks do not satisfy device observations.
Verify existing PS1 data/layouts; real PS2 moving gameplay with simultaneous
input and audible sound; ordinary save after a fresh process; separately
verified automatic resume/current-card consistency; both engine-switch
directions; cold/warm/repeated native shortcuts and missing-content recovery;
pause/focus/Home/fold/rotation; and a bounded measured gameplay interval.
Package/signature/ABI/notices and corresponding-source checks must use the
actual candidate. Record results and exact blockers on #2, leaving it open
for Master acceptance.

Carry #1's specific remaining checks from [verification.md](verification.md)
without repeating unrelated qualification. Stock iOS, store licensing and
16 KiB runtime/submission gates remain explicit in [distribution.md](distribution.md).

## Live controller editing

The owner's unsaved on-device arrangement was inspected without taps or
navigation and retained only as a private visual reference. Its intended order
puts primary sticks outside the inward digital clusters, with Start/Select
above and shoulder pairs toward the middle. Accidental sizes and asymmetry
are not accepted defaults. The unsaved draft was left open during handback;
no installation or app close was performed in that inspection window.

The editor now uses the real play surface with a live picture, full-size
controls, forgiving selection and a distinct corner-resize mode. Its floating
tools dock top/bottom and hide while moving/resizing. A visible optional grid,
alignment guides, explicit Mirror, bounded gesture Undo and a separate live
action-button spacing slider address precise placement without silently
linking controls. Spacing retains cap sizes and balances the diamond; the
existing per-display preset format persists its resulting positions. Draft
changes still require Done, with Cancel and contextual Reset retained. This
implementation needs its own actual Fold interaction check; host fixtures
and shared-renderer previews do not establish that acceptance.

The subsequent owner-approved window installed `15ca5ba4c71b` after a verified
current-data backup. The live editor, actual game image, grid, size readout,
group selection and spacing slider rendered over Red Dead Revolver. Full
drag/resize/spacing-gesture acceptance is still pending. At the owner's explicit
instruction, the mock square-display profile was reset through the app to our
defaults. All eight PS1 cards, all four current PS2 cards, the library and PS1
profiles compared unchanged against that fresh backup. Red Dead's tutorial scene
was visible after resume; no controlled locomotion/save/audio/performance pass
is inferred. Its core was gracefully closed before a final complete backup.

Actual folded-landscape tracing exposed a PS2 camera-layout failure: the picture
was 1145×858.7 pixels in a 2342×1021 content area. Camera avoidance pushed the
D-pad inward and narrowed the centered picture. A new generated arrangement
fits the camera between the secondary digital band and lower primary sticks,
retaining symmetry and the full 1021-pixel picture height in the measured host
fixture. It also swaps shoulder defaults to 2 outside/1 inside (1 above 2 when
stacked), and separates default circular action caps. Saved custom positions
retain precedence.

The next approved window installed `c27ce6f698c3` after a fresh, readable data
backup. Actual folded-landscape tracing confirmed the centered, top-aligned
1361.3×1021 picture at 490.3,0 in 2342×1021 content. Red Dead's tutorial scene
restored, with separate circular action caps and level shoulder pairs ordered
2 outside/1 inside. This is the host viewport; the game may draw borders inside
its own frame.

Actual editor interactions moved the D-pad independently and restored it with
Undo; a corner grip enlarged it from 147 to 160 points, with Undo restoring
the original. The action-group spacing slider increased its bounds from 147
to 174 points while keeping cap sizes unchanged; Undo restored the diamond.
A group corner grip enlarged all four caps together to a 160-point well.
Cancel discarded that final draft. The game picture remained visible and the
paused scene stayed unchanged. Mirror, grouped dragging, live-running editing,
Done persistence and fold/status transitions still need device checks.

The game closed gracefully before a final readable backup. All eight PS1 cards,
four PS2 cards, both controller-profile files and the library compare byte-equal
before/after. The session checkpoint was updated normally. Locking was confirmed
before the window ended; Android reported Dozing, not fully asleep. Ordinary
PS2 saving and the remaining gameplay/performance acceptance are unchanged.

## Next ergonomics and graphics correction

The owner reports Red Dead usable, PS1 behavior unchanged, and Baldur's Gate
unplayable at its bad points. This is owner feedback, not a controlled speed
or ordinary-save pass. The next Rust revision adds inward toolbar clearance,
adjacent inner-display thumb areas, larger folded sticks, and immediate
second-touch L3/R3 with a toggle and separate buttons retained. Custom control
rectangles keep precedence. Fold checking is still required.

Two rendering defects were found off-device. The captured GLSL enabled ARM but
emitted EXT syntax; enabling EXT makes that complete shader compile on Mesa,
while the missing-extension case fails. The PS2 core also fits its 640×448
intermediate before the host fits it again. Filling the intermediate leaves
final aspect fitting with Rust, removing redundant bars without cropping.
The shader/warning-only native patch has an exact checkpoint compatibility
transition; unknown builds/firmware and changed cards remain rejected. No PS2
speed/save improvement is claimed before testing.

The private host inventory has eleven PS1/PS2 titles. GTA: San Andreas is the
remaining consolidated title absent from the last Fold library; transfer/add
is pending a fresh safe device window. Other-console/PC downloads are not
presented as PS1/PS2 games. Games and firmware remain private.

## October 20 graphics candidate on the Fold

The owner granted a fresh window at 18:49 UTC. A complete readable current-data
backup preceded successful in-place installation of `867333a7449c`, retaining
the package and signing identity. GTA: San Andreas transferred with the full
image hash verified on the phone. The owner selected it; a temporary black
picture subsequently gave way to readable intro credits. This establishes
startup rendering, not controlled gameplay or ordinary saving.

The own-process log reports PowerVR EXT/PLS framebuffer fetch active, with both
ARM and EXT advertised and no shader compilation error in that startup capture.
Short native startup samples report 59.90–59.95 VPS and 99.9–100% speed; audio
underruns also increased. These are distinct from sustained gameplay and sound
acceptance. GTA's remaining warning says Basic blending is below its database's
recommended Medium level, which increases system requirements. No global
quality setting was raised and this advisory was not suppressed.

The new inner-display arrangement rendered with larger sticks, adjacent digital
clusters, central shoulders and lower central system/click keys. Red Dead's
automatic restore logged and visibly returned to its known tutorial scene.
Its image reached both sides of the host frame after removing the nested fit.
That frame was still only 1967.5 pixels wide in the 2076-pixel display: the owner
correctly distinguishes filling the frame from reaching the display edges.
The next default must spend less height on chrome and controls, with shoulder
pairs above the gap between each stick and digital cluster and click keys below.
Physical L3 gestures, current fold transitions and ordinary PS2 saving remain
unqualified.

After reconnection, both games closed gracefully to the 11-game/16-disc library.
A final complete readable backup preserves all eight PS1 cards, four existing
PS2 cards, both profiles and all ten prior library records unchanged. GTA is the
only new entry. Only psx was force-stopped afterward. The phone repeatedly woke
during locking; the owner confirmed waking it to inspect the game. USB vanished
before the final sleep action. The lease ended at 19:13:23 UTC before expiry,
with final locking/screen-off explicitly reported unverified. No unrelated app,
system-package, security, radio, USB configuration or server setting changed.

## Picture-first default follow-up

The next Rust default budgets the control band after a full-width 4:3 picture.
On the measured 2076×2152 inner portrait display, with its actual 160-pixel top
and 78-pixel bottom exclusions, the host fixture now reaches 2076 pixels wide
at the top of the clear content, compared with the installed 1967.5 pixels.
This adds about 11% picture area. Sticks remain about 130 points, secondary
caps at least 48 points and separate; all auxiliary keys retain 48-point targets.
Shoulder pairs bridge above each stick/digital gap, stick-click below, with
Start/Select in the lower middle. The toolbar is 28 points high, with 40-point
width and the existing corner clearance; non-normal time keeps 14-point text.

Folded camera rails retain the full-height picture and 128-point sticks,
with level shoulders between the upper digital well and lower stick. The
system/click keys stay on the inner side. Other surfaces use the largest clear
fit when the full-width picture plus minimum controls cannot fit. There is no
cropping, stretching, automatic overlap or change to saved custom rectangles.
PS1 thumb defaults and the native patch/state format remain unchanged.

All 101 active workspace tests and the deliberately ignored subprocess fixture
pass. Geometry tests cover actual picture edges, camera clearance, mirrored
keys, separate targets, status transitions and saved layouts. The independent
maximum-picture fixture now checks horizontal containment too: its old wider
candidate could leave the display once the real picture reached full width.
Writer fixtures use a per-run timestamp as well as PID to avoid sharing lock
directories across concurrent PID namespaces. Production ownership is unchanged.
Actual shared GLES previews, coverage and donor-sampler isolation pass. These
are host results; the prepared layout still requires installation and a physical
fold/rotation/gesture check under a fresh window.

## Picture and thumb-pair follow-up

The October 20 19:44–20:00 UTC window installed `337e767ac11a` with the
original signing identity after a fresh verified backup. GTA restored and
rendered. The owner approved resetting only the current Square PS2 profile.
The actual unfolded portrait picture reached 2076×1557 at 0,160; rotated inner
landscape remained 2027.7×1520.8 at 62.2,136. Folded landscape reached the
full 1021-pixel control surface, but still excluded a 59-pixel hidden navigation
strip from the 1080-pixel drawable. The latter is a frontend reservation, not
a game-rendered border. All eight PS1 and six PS2 cards, library and PS1
profiles remained unchanged; the authorized PS2 reset was the sole profile
change. Normal Close game preceded final backup and app shutdown. Locking was
verified, power reported Dozing, full screen-off unverified. Access was released
before the window expired; no current device custody.

The next Rust revision separates gesture-safe controller coordinates from the
picture canvas. Hidden navigation space may display the game while input stays
above its reserved strip. Caption/camera bounds remain honored. Transient bar
reveals move chrome around the existing picture and controls; the live editor
uses the same expanded picture without changing saved coordinate identities.
Generated PS1/PS2 side rails budget against that canvas. Inner PS2 thumb pairs
now have an eight-point clear gap; auxiliary keys use the center outside both
pairs. Folded camera rails keep auxiliary keys beside the primary well, outside
the thumb travel corridor. The toolbar is 24 points high with 40-point-wide
hit targets and existing rounded-corner clearance. Saved custom input rectangles
retain precedence. No native emulation, checkpoint identity or card-protocol
change accompanies this revision.

Do not promise four-edge filling: complete 4:3 output, camera clearance and
separate comfortable controls can constrain a nearly square rotated display.
The solver maximizes the centered, high clear picture. Game-drawn black regions
remain separate from host geometry; no automatic crop/stretch is introduced.
The new geometry and rendering checks require another installed-device pass;
they do not settle Baldur's card failure or PS2 gameplay/audio/performance.

## Shared presentation correction after actual PS1 inspection

The October 20 20:13–20:28 UTC window installed `beacac396af4`; FF7 restored
its existing scene and rendered. The owner correctly rejected the standard
unfolded PS1 view: 1804×1353 at 136,68.2 in a 2076×1914 control surface.
The old PS1 dock remained much deeper than the PS2 band. Several toolbar
glyphs also used fixed drawing dimensions, so reducing their rectangles did
not proportionally reduce what the owner saw. No actual new PS2 full-height
or L3 pass follows from this window. Normal Close game preceded final backup;
all eight PS1 cards, six PS2 cards, library and PS2 profiles remain unchanged.
PS1 profiles gained a default Tall entry and explicit default-valued preference,
with existing effective settings retained. USB disappeared after sleep;
locking/screen-off remained unverified. Access was released before expiry.

The next Rust revision composes play and editing through one console-independent
`PlayerViewport`. Digital and dual-stick controllers use the same shallow inner
band, common face-cap geometry, auxiliary-key metrics and picture policy. It
corrects generated PS1 defaults without rewriting saved manual rectangles.
Toolbar glyphs now scale with their available box; input bounds remain distinct.
L1/L2 and R1/R2 select as groups, supporting translation, proportional corner
resize, symmetry, contextual Reset and Undo. Single editing remains available;
the shape-spacing slider stays specific to action buttons. Existing serialized
element indices and save/session/native identities remain unchanged.

Read-only decoding of the current FF7 checkpoint separates another observation:
the guest display is 320×240, with eight all-black VRAM rows at both top and
bottom. Those source bands are independent of the incorrect outer host margins.
No automatic crop/stretch or speculative emulation change is introduced.
Actual installed-device checks remain required for the new shared presentation.
Presentation also rounds physical blit endpoints together, avoiding one-pixel
strips from truncating fractional density-scaled widths/heights. Shared GLES
probes check actual left/right inner-display and top/bottom folded-display
pixels, separately from geometric rectangle assertions. These remain host
rendering results until the candidate is installed and checked on the Fold.
