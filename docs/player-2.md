# PLAYER-2

Active work: [issue #2](https://github.com/WigglyDon/psx/issues/2) and its
[Master dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6272753838).
The dispatch supersedes the old hold on issue #1's formal closure.

The source baseline is `a1bdd2810e67ede3377383613b397776f966748c`.
The working installed PS1 runtime was `470fa617f249`, APK SHA-256
`e0969932623987e0e40647da587746805f4b77edabfa2f881b1ebf05a8d8de07`.
Its source, APK, original signing identity and last verified private backup
are retained locally. Before a device update, take a fresh backup of its
actual current data; the older backup predates subsequent owner play.
Keep `com.machinerhythm.psx` and its signing identity unchanged.

## Current increment

The Rust host distinguishes a validated opaque game ID, a durable saved-session
identity and a temporary runtime binding. Its current single runtime holds an
OS-backed per-game writer lock through card/checkpoint work and core teardown.
Competing writers are refused; a fixture also checks release on worker death.
Opening an already running game retains its session and explicit pause intent.
Content resolution precedes closing another game. No runtime/process/task IDs
enter the existing saves or portable `machinerhythm-psx://play/<GameId>` target.
This does not add another worker process or claim resident multi-game playback.
Collection taps, automatic resume and imports use one Rust OpenGame operation;
multi-disc games reopen their remembered disc without a selection detour.

The selected private PS2 acceptance title is Baldur's Gate: Dark Alliance.
Local games/firmware were consolidated without changing existing phone paths.
Known duplicate local content is verified before removal. No game, proprietary
firmware, signing data, save or raw device evidence enters public source.

## Engine qualification in progress

ARMSX2 candidate `83fe4e293f71ab5587ef81b06143725bf8c6a30b` is checked out
privately for qualification. It includes the preceding nightly's fixes and
currently matches the specified candidate; release notes are not acceptance.
Its original Android dependency recipe has produced the required libraries;
the ARM64 libretro target is compiling, with resolved dependency revisions to
be locked before incorporation. No PS2 gameplay or save result is claimed yet.

Source inspection confirms donor-owned CPU/GS threads, an EGL shared-context
path, negotiated GLES 3.2/3.1/3.0 requests and a 68 MiB libretro state buffer.
The existing PS1 TLS callback, GLES negotiation and 64 MiB checkpoint assumptions
cannot simply be reused. File-owned PS2 cards require the donor's own flush
and shutdown ordering. Qualify that boundary before broader host changes.

## Acceptance still required

Implement and verify the full issue scope: PS2 moving gameplay, audible sound,
dual-analog/digital input, ordinary card save after a fresh process, separately
verified automatic resume/card consistency, both engine-switch directions,
native pinned-shortcut confirmation and cold/warm/repeated launch, same-entry
missing-content recovery and PS1 data/layout regression. Build/lint checks and
host fixtures do not satisfy these device observations.

Carry #1's remaining specific checks from [verification.md](verification.md)
without repeating unrelated release qualification. Stock iOS, store licensing,
16 KiB runtime and submission gates remain explicit in
[distribution.md](distribution.md). Keep #2 open for Master acceptance.
