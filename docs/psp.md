# PSP-1

[Issue #5](https://github.com/WigglyDon/psx/issues/5) extends the existing Rust
player with PSP. Independent durable per-game resume is an acceptance gate.
Integration and device qualification are in progress. Preserve #2's accepted
PS1/PS2 artifacts and results; neither they nor package checks qualify PSP.

Candidate `f1b94b0` rendered Final Fantasy II's menus and opening battle, and
the owner confirmed audible working sound. A fresh process acknowledged native
restoration and showed the same upright paused battle picture/intent, with the
committed machine/picture fingerprints unchanged. Candidate `6f048e3` continued
that restored session through the opening battle into the next story scene.
Candidate `ac74cc0` completed PSP → PS1 → PS2 → PSP through the shared launch
operation. The matching PS2 saved scene resumed and accepted stick movement;
PSP returned to its paused story. Repeated delivery retained the same PSP
session. The same candidate subsequently saved through Final Fantasy II's own
Memory Stick menu. Explicit disc restart, process replacement at the title and
native Load Game recovered that ordinary slot and accepted D-pad walking.
Separately, a fresh process restored the same upright paused story with identical
committed machine/picture fingerprints; unpausing continued execution. Return
verification preserved normal data and the exact original installation.

These are selected-title observations. Stick input did not move this title's
character; analog gameplay and simultaneous physical touch are not qualified
by that attempt. Broader compatibility, lifecycle and release checks remain
separate. The subsequent shared full-display composition fits the complete
native-aspect picture before placing controls in the spare space. Insufficient
space requires an explicit alternative; it never silently crops, stretches or
reduces the picture. Its new hardware qualification remains separate from the
frozen tested candidate above.
The issue retains exact build bindings, failed trials and verified return observations.

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
PSP uses the shared picture-first placement, retaining only its supported inputs.
Its native aspect determines the maximum rectangle before controller packing.
The explicit keep-clear alternative retains the earlier shared analog placement,
camera rails and bottom-band budgeting. Existing custom element identities
and rectangles remain authoritative; changing defaults does not reset them.
Games do not select separate built-in presentation systems. Only an explicitly
enabled per-game controller profile overrides the shared defaults; each console's
native aspect and available inputs determine its fitted picture and capabilities.

The pinned Android adapter requests a GLES2-compatible hardware interface.
Rust accepts that PSP request on the player's existing GLES3 context and supplies
its normal framebuffer/procedure callbacks. Rejecting it makes the donor fall
back to software before the host refuses startup. Desktop GL and higher GLES
requests remain refused, and PS1 keeps its existing version negotiation.
The final blit uses the shared layout's solved rectangle, including PSP's
aspect ratio. Paused GL readback is normalized to the shared top-row-first CPU
picture format before publication; an already retained/restored CPU picture is
reused without another origin conversion. Live, paused and cold-restored output
therefore use the same presentation contract.
Rust explicitly disables the donor's default two-line 16:9 crop, so actual
output and checkpoint pictures retain all 480×272 pixels expected by that layout.

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
