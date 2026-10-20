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
