# PLAYER-2

Active work: [issue #2](https://github.com/WigglyDon/psx/issues/2), its
[Master dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6272753838)
and the later
[first-party engineering correction](https://github.com/WigglyDon/psx/issues/2#issuecomment-6277288148).
Continue without waiting for #1's formal closure. The issue owns current
build/device status, chronological evidence and the full Master handback;
this document retains the implementation contract and outstanding acceptance.

## Continuity and ownership

Preserve the accepted PS1 player, package `com.machinerhythm.psx`, signing
identity, collection IDs, ordinary cards, automatic checkpoints and controller
profiles. The original source baseline is `a1bdd2810e67`; its APK, exact source
and signing identity remain recoverable locally. Back up actual current data
before an update. An old APK or historical backup alone is not a data rollback.

A validated `GameId` identifies a durable entry and saved session. Runtime
assignment is ephemeral; Android task/process handles and graphics objects
never enter permanent IDs or save formats. One OS-backed per-entry writer
lease spans card/checkpoint work and native teardown. Unix uses descriptor-owned
`flock`, including Android, where the standard-library lock was unsupported.
Contention is distinct from underlying I/O failure. Host fixtures cover
competing owners, killed-worker lock release and reassignment of an unloaded
session without changing its identity.

One live runtime is this milestone's policy, not a permanent session limit.
Native CPU/GS threads and graphics work end before switching engines. No second
worker process, general IPC framework or multi-game background toggle is part
of PLAYER-2. Future resident sessions can reuse the same durable ownership
boundary without bypassing one core/graphics owner in each process.

## Engine and persistence

ARMSX2 is pinned at `83fe4e293f71ab5587ef81b06143725bf8c6a30b`, with its
resolved dependency/nested commits and exact inherited notices. Standalone
frontends and floating patch downloads are disabled. The recorded adapter
exposes existing owner operations and metric samples; substantive new host
logic is Rust. Targeted retained-donor repairs and justified Rust subsystem
work are permitted by the engineering correction, without requiring an engine
replacement or authorizing a blanket rewrite.

The GS handoff now notifies Rust's frontend owner when a frame is published,
bypassing its former fixed frame-check cadence. The native CPU limiter owns
emulation time; Rust drains available frames promptly, services input/audio
within 8 ms and separately bounds composition to 60 Hz. The platform adapter uses
Android's stock looper waker; the reusable binding carries only a portable
waker and coalesced pending bit. Callback retirement precedes native teardown.
Focused Rust and patched-native handoff tests cover publication, late attachment,
one-consumer ownership and interruption. GPU ordering and actual speed/audio
still require device validation. Presentation mean/max duration is reported
separately from frontend calls and native achieved emulation speed.

The host implements thread-safe worker callbacks, negotiated shared GLES,
48 kHz audio, dual-analog/digital input, isolated file-owned cards and bounded
revision/firmware-bound checkpoints. PS2 does not inherit PS1's synchronous
stepping, RAM-card polling or background-play assumptions. Native card flush
precedes file synchronization, with another sync after shutdown. Complete
checkpoint inputs and current cards are validated before restore; embedded
old cards cannot replace newer durable progress. Incompatible states are kept
and explained until explicit disc restart with the current card.

The 68 MiB serialization ABI has a smaller validated ZIP payload; the host
retains exact archive bytes without unused ABI padding. Stored ZIP entries
and omission of an unused thumbnail reduce checkpoint work. Periodic captures
still produce observed hundreds-of-milliseconds stalls; this is a remaining
performance cost, not a claim of seamless capture. See
[formats.md](formats.md) and [adapter details](../patches/armsx2/README.md).

The PowerVR shader repair enables the EXT framebuffer-fetch language contract
when its backend emits EXT `inout`, preserving the donor's Mali/ARM preference
and ARM-only fallback. Actual GTA/Red Dead scenes rendered with EXT fetch
active. Checkpoints from the two named prior adapters can enter the exact
shader/frame-notification build, which changes no emulated state or archive
fields. This transition is guarded by complete patch identities; future edits
disable that exception automatically. No serializer/card-protocol change is
implied by a host layout edit.

The matching donor Android `patches.zip` resource is bundled and hash-checked;
the missing-resource warning was reproduced and repaired. Genuine renderer
and compatibility diagnostics remain. The Basic/Medium blending advisory
recommends graphical accuracy at increased system requirements; it is not a
speed setting or proof of a stutter cause. Native resolution, intended guest
clocks, requested time scale and achieved emulation speed remain distinct.
The PS2 JIT currently requires 4 KiB host pages; native-library alignment does
not qualify 16 KiB runtime support.

## Direct-game launch

Collection taps, imported games, automatic resume and platform links enter one
validated `OpenGame(GameId)` operation. Resolve accessible content before
switching. Same-game return preserves the runtime and pause intent; initial
shortcut requests precede old-session automatic resume. Ordered warm requests
coalesce adjacent duplicates and serialize different targets.

Game actions use Android native pin confirmation, stable IDs and local artwork.
An accepted pin request is separate from a confirmed launcher callback. Pins
receive local title/icon updates. Missing entries/files offer recovery and
verified relinking without deleting progress or resolving to another game.
No link supplies an arbitrary core/executable or writable save destination.
The portable `machinerhythm-psx://play/<GameId>` contract is retained for a later
iOS adapter; a full iPhone port is deferred until Apple hardware is available.

## Shared presentation and touch controls

One console-independent Rust `PlayerViewport` composes play and live editing.
The controller supplies input capabilities and geometry; it does not select a
second player layout. PS2 adds primary sticks and click inputs to the shared
control styling/metrics. Future physical-controller routing should withdraw
the touch overlay while retaining this presentation; that routing is not yet
implemented.

The owner explicitly chose picture use of the complete display, including
camera pixels. Keep the complete unstretched 4:3 frame, maximize the centered
clear picture and place it high. The physical camera may hide a corner; it
must not reserve a broad band or move the picture sideways. Gesture-safe
input coordinates remain separate from the full drawable canvas, preserving
saved profiles. Controls, toolbar and popovers still avoid actual holes.
Default controls remain separate from the picture; overlap is explicit.

Pause/time stay upper-left and sound/settings upper-right in every orientation.
The normal toolbar is an 18-point strip without an extra gap below it, with
40-point-wide ordinary targets. Non-normal time uses a horizontal readable
caption beside its hourglass. Visible OS status pushes toolbar controls down
over the stable picture; it does not resize the game or controller geometry.
Hiding decoration restores the normal strip. Physical blit endpoints round
together to avoid density-truncation strips. Play and editing use the same
actual canvas, cutouts and status state.

Inner defaults budget after a maximum picture, with mirrored adjacent thumb
wells and separated circular caps of at least 48 points. The digital controller
targets 184-point thumb areas, 64-point shoulders and 80-point system keys.
The analog controller uses the same thumb-area ceiling for four wells, limited
by actual headroom and width. Auxiliary keys occupy clear center space in two
columns or two rows as space permits; they do not occupy the route between
primary and secondary thumb areas. First shoulders are outside or below second.
Separate stick-click and system keys remain available. Thumb wells leave four
points plus the OS gesture strip beneath them; center keys use the available
gesture-safe height. Wide digital controls use equal square wells around either
side camera, with mirrored shoulder pairs and inward system keys directly above.
Other surfaces use clear docks/side rails. Owner references guide size and balance;
opposite sides use exact mirrored geometry. Saved manual rectangles retain
precedence; applying new defaults is
explicit. Generated sizing uses each orientation's actual dimensions.

Do not infer that all black regions are host margins. Read-only decoding of
the actual FF7 checkpoint found a 320×240 guest display with eight all-black
VRAM rows at both ends. Those are source pixels, separate from the confirmed
old frontend margins. No automatic dark-content crop is introduced.

## Editing and time control

The live editor retains gameplay size and pause intent while preventing editor
touches from entering the game. Tap selection has forgiving targets; separate
Move/Resize modes and 48-point corner grips keep the opposite corner anchored.
Floating tools dock above/below and hide during a gesture. Optional grid/peer/
center guides, explicit Mirror and bounded gesture Undo support accurate
placement without silently linking subsequent drags. Camera input exclusions
are outlined, preserving the same visible picture as play.

Action buttons and each shoulder pair select as groups for translation,
proportional resize, Mirror, contextual Reset and Undo. Single editing remains.
A live action-spacing slider balances the diamond without changing cap sizes.
Done persists the draft; Cancel discards it. Existing per-display PS1/PS2
identities and normalized element indices remain compatible.

Hourglass tap toggles normal 1× versus the remembered rate; hold opens a live
slider with zero paused and maximum uncapped. Manual pause and interruption
gates remain independent. Fractional audio stays stereo; uncapped delivery uses
a bounded queue. PS2 updates its limiter deadline and coalesces requests without
reloading all settings or pausing the VM for each rate change. Host checks cover
migration, rate memory, fractional phase and bounds; UI interaction is distinct
from running-rate/sound/performance acceptance.

## Observations and remaining acceptance

The accepted PS1 experience includes real gameplay, clear sound, simultaneous
touch, ordinary fresh-process card loading and automatic scene restoration.
Updates preserved existing PS1 collection/card/profile identities. FF7 native
pin confirmation, cold/warm launcher return, repeated same-game delivery and
missing-entry recovery ran without replacing its paused scene. Remaining
shortcut and lifecycle cases stay on #2.

PS2 reached Baldur's Gate's card menus, opening cinematic and interactive tavern
dialogue. The owner heard music/dialogue breaking up at hitches. Separate PS2
checkpoints and PS2→PS1→PS2 scene switches restored. Red Dead and GTA scenes
also rendered/restored; owner feedback calls Red Dead usable. None establishes
ordinary PS2 saving or general game compatibility. GTA's later short 1× scene
sample ran below full speed with audio underruns; it is not a controlled
regression baseline.

Baldur's Gate: Dark Alliance remains the selected acceptance target. Clean boot
reproduces its card format/access error. Traces show present/eject/specs/read/
auth operations, but no write/erase at failed format. A temporary IOP-interpreter
comparison reproduced the same error and was restored exactly. No protocol
cause or card repair is established; severe hitches remain. Ordinary PS2
save→fresh process→load is still required, independently of automatic resume.

Still required: controlled simultaneous analog/digital/action gameplay with
audible sound, physical L3/R3, remaining fold/status/lifecycle cases, missing-file
relink and another-active-game/update-retention shortcuts, plus a repeatable
speed/pacing/audio/memory/thermal interval. Preserve the working increments
and report a concrete blocker if qualification fails. Keep #2 open for Master
acceptance; host fixtures and package success are separate from device results.
Carry #1's remaining checks from [verification.md](verification.md) without
repeating unrelated qualification.

## Firmware and distribution

The unmodified OpenPS2BIOS candidate at
`43005227db73090a4a4b5701dee510a728d0e689` reproduced twice to identical
4 MiB output, SHA256 `3ad7ab89b01eeedd61eb6afe39013da5a3d366c63d7c63b608c32526bb6ecbda`.
Its header passed the Rust ROMDIR binding probe. This is isolated build/header
evidence, not provenance clearance or gameplay/audio/save qualification. It is
not bundled and no existing firmware-bound session was substituted. A BIOS-free
default remains a product goal; further donors are not compulsory prerequisites
before a justified repair. No proprietary firmware is published.

Local PS1/PS2 resources are consolidated privately; all eleven selected games
and sixteen discs are in the Fold collection. Duplicate archives were removed
only after content verification. Games, proprietary firmware, saves, raw device
evidence and signing material remain private. The complete signed APK must
pair with exact corresponding source and inherited notices. See
[build.md](build.md), [THIRD_PARTY.md](../THIRD_PARTY.md) and
[distribution.md](distribution.md). Original MIT does not erase borrowed GPL
obligations. Stock-iOS performance/rights, 16 KiB runtime, accessibility and
store packaging/permission eligibility remain separate release gates.
