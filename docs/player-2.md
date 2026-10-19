# PLAYER-2

Active work: [issue #2](https://github.com/WigglyDon/psx/issues/2) and its
[Master dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6272753838).
The dispatch supersedes the old hold on issue #1's formal closure.

## Preserved baseline

The source baseline is `a1bdd2810e67ede3377383613b397776f966748c`.
The working installed PS1 runtime was `470fa617f249`, APK SHA-256
`e0969932623987e0e40647da587746805f4b77edabfa2f881b1ebf05a8d8de07`.
Its source, APK, original signing identity and historical private backup are
retained locally. Before any update, back up actual current device data;
the historical backup predates further owner play. Keep `com.machinerhythm.psx`
and its signing identity unchanged. No PLAYER-2 device update has occurred yet.

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
intended guest clocks and explicit 1×/2×/5× playback remain distinguishable
from achieved native speed. The current PS2 JIT path requires 4 KiB pages.

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
