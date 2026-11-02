# PSP-1

[Issue #5](https://github.com/WigglyDon/psx/issues/5) extends the existing Rust
player with PSP. Independent durable per-game resume is an acceptance gate.
Current work is integration and off-device qualification, not a PSP gameplay
or distribution claim. Preserve #2's accepted PS1/PS2 artifacts and results.

PPSSPP is pinned at `fc9ca4d1f78a5d60ae03f3311c6b8ab49273f6a8` in the
`vendor/ppsspp` submodule with its recursively pinned dependencies. Its native
implementation, replacement assets and exact original licenses remain retained.
New product, session, file, controller and build policy is Rust.

## Runtime and progress

The existing `OpenGame(GameId)` and exclusive per-entry writer apply to PSP.
One live runtime remains the current policy; unloaded games retain independent
sessions. PSP ordinary saved files live in that entry's own Memory Stick tree,
separate from both other games and PlayStation cards. Explicit custom controller
profiles retain the same durable ID. PSP defaults have their own file and do not
overwrite the PS1/PS2 defaults.

`patches/ppsspp/compose.rs` makes bounded changes to the exact upstream adapter.
The narrow C ABI exposes its existing owner pause/ready/I/O operations, forwards
formatted native logs into Rust, and accepts Rust's firmware policy. Both public
serialization operations preserve a Rust-requested pause. A frame gate lets
initialization finish without executing the first guest frame before saved-file
validation and state restoration. Normal unpaused serialization retains the
donor's existing resume behavior. No native state layout is changed.

Rust chooses the HLE route and disables automatic extraction/upgrading of disc
firmware before `retro_load_game` launches the loader thread. PSP games need no
supplied firmware. This avoids
an unrecorded firmware transition in an existing saved session. The settings
expose no firmware/core/backend selection for PSP.

Capture settles donor asynchronous file operations and captures machine bytes
while held. A bounded sorted inventory binds saved-file names, directories,
sizes and contents; only the donor's reconstructible `PSP/SYSTEM/CACHE` is
excluded. The existing single checkpoint worker synchronizes retained read-only
file/directory descriptors before publication. It enters no VM or graphics API.
Current file identity is checked before restoring a checkpoint, preserving newer
ordinary saves and the old checkpoint on refusal. Engine/adapter and game inputs
also bind the state. A paused GL picture is retained in the existing atomic
picture/state format so cold paused restoration needs no guest frame.

PSP's GLES worker and callbacks retire before releasing their library. Inherited
fault handlers are restored after native retirement; a failed restoration keeps
the stopped native mapping alive. This extends the existing ownership contract
without a second runtime or an IPC framework.

## Presentation, content and validation

PSP uses its complete 480×272 picture and one stick, D-pad, shapes, L/R and
Start/Select. Play and live editing use the same geometry. Unsupported second
shoulders/stick clicks are absent; saved PS1/PS2 profiles retain their capabilities.
Phone bindings and per-game overrides use the existing shared policy.

ISO detection reads UMD/game metadata. PBP detection preserves PS1 EBOOT priority
even when it contains a PSP bootstrap. Structurally valid unresolved CHD/CSO
content requires a one-time console choice; an extension is not proof of system
or game compatibility. Supported formats and actual device findings belong in
the issue once qualified.

`cargo xtask psp` builds only the pinned PSP target. Packaging reuses verified
PS1/PS2 native inputs, checks the composed PSP source/native/symbol binding and
bundles matching support assets/notices. Complete corresponding source and
actual device save/load, resume, switching, input, sound and lifecycle results
remain separate required observations. Keep private games, progress, firmware,
signing material and raw device evidence local.
