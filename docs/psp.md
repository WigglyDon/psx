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

Layout candidate `3568c3c` subsequently restored that PSP progress, accepted
D-pad walking and displayed 1080×612 at the top of a 1080×2280 portrait surface.
On a 2280×1080 landscape surface it displayed 1905.9×1080, centered horizontally;
the narrow remaining side space produced the explicit layout choice. Selecting
overlay in isolated test progress retained the maximum picture and survived
rotation and returning from PS2. PS2 used 1080×810 in portrait and 1440×1080 in
both landscape directions, with separate controls and working stick movement.
These are the measured lab surfaces, not new Fold or all-device qualification.
The original installation and normal data were restored and return verified twice.
The issue retains exact build bindings, failed trials and verified return observations.

The normal-use `d24342a` APK subsequently updated the owner-admitted Fold in
place. Its actual preceding APK and normal progress were backed up and verified;
normal data was byte-identical before first launch. The in-app scanner added
Final Fantasy II, its startup/intro rendered, and a new process restored the same
upright paused scene with an unchanged checkpoint fingerprint. PS1 title output
and returning to that PSP scene ran through the shared launch operation. Existing
controller profiles remained identical. An older PS2 checkpoint was correctly
refused for a different native binding and preserved with its cards; it was not
restarted or discarded. Final progress was backed up and psx stopped. USB loss
prevented the lock/display check; the owner explicitly accepted release as-is.
No new ordinary-save, listening, all-orientation or sustained-speed qualification
is inferred. The subsequent toolbar/control refinement below is separate source.

The separately admitted normal `619bab2` update then preserved current data before
launch and restored that same paused PSP checkpoint and custom profiles. Actual
folded portrait used 1080×612 at y=43.9 on a 1080×2342 drawable; unfolded used
2076×1176.4 at y=43.9 on a 2076×2152 drawable. The inspected inner controller had
level primary thumb areas and an adjacent D-pad. The narrow visible side strips
were also present in the retained 480×272 snapshot before final fitting; no
automatic crop/stretch was applied. Final progress was archived and read back,
then app stop, keyguard locking and both physical displays off were verified
under the held lease before release. The queued final whole-tar remote hash did
not execute after the work deadline and is not claimed. This adds the measured
layout/paused continuity observations, not new listening, gameplay input,
ordinary-save, all-orientation or sustained-speed qualification. The issue owns
the frozen APK/source/symbol bindings; later documentation does not rebind them.

The separately bound `619bab2` acceptance-storage APK then completed a two-PSP
lab check with Final Fantasy II and Pirates of the Caribbean: Dead Man's Chest.
Pirates reached gameplay and accepted left/right stick movement and reversal,
with movement stopped after release. Switching between the games restored each
independent paused point; fresh committed reads matched its machine, saved-file,
picture and pause metadata. Pirates also restored that point in a new Android
process and accepted further stick movement after unpausing. This qualifies
injected analog input in that title, not simultaneous physical touch.

An unpaused Final Fantasy II background/foreground check retained a stable
committed point during two background observations and returned in the same
process with the original intent, without a cold restore. It does not measure
exact guest-clock freezing or audible background behavior. Normal data/directory
identity, the original installation, Home and task-owned settings were preserved;
test progress was archived and read back, owned scratch/staging retired, and
physical-off return verified twice without cleanup errors. This campaign adds no
new ordinary-save, listening, launcher or sustained-speed result. A separately
prepared owner hands-on check retains its own admission and result boundaries.

The `441637f` acceptance-storage APK subsequently completed missing-original
recovery through Android's real document picker. A wrong game was refused
without an orphan import or changed progress; the verified original restored
the existing private import folder and exact library entry. Fresh reads matched
the saved machine, ordinary-file identity, core, picture and pause metadata,
with unchanged ordinary-file/controller manifests. A new process restored the
same paused point, then injected stick movement continued gameplay and a new
paused checkpoint committed. Exact original app/data/directory identity, Home
and task settings were restored; physical-off return was verified twice.
This adds no new in-game ordinary-save, listening or physical-touch result.
The [narrow handback](https://github.com/WigglyDon/psx/issues/5#issuecomment-6638608695)
retains exact tested artifact bindings and earlier failed-trial limits.

Normal application source `51fff5b` has now been packaged separately using the
same three frozen native engines and original signer. Packager/source `0943e83`
verifies every payload entry outside the normal Rust replacement and removal
of the acceptance marker. Its bounded host qualification and complete-source
readback pass. The
[package handback](https://github.com/WigglyDon/psx/issues/5#issuecomment-6640907638)
records its exact identities. Documentation updates do not rebind either APK.

That exact normal-use APK subsequently restored the preserved PSP point through
ordinary app storage, restored identical complete paused metadata in a new
process, and continued visible analog gameplay in portrait. A new paused point
committed; independent streaming readback matches its final archived checkpoint
and the exact validated current library entry, with ordinary-file identity kept.
The finite private recipe remains failed: its final checker compared the seed's
old file path with the intentionally relocated normal-storage path. Focused
retained-evidence checks explain that predicate failure without another device
run or a product repair. Linked cleanup proved only the original root directory's
modification time differed; correcting that timestamp reproduced the exact
pre-test data archive digest. Original installation, directory identity, Home,
owned-staging absence and physical-off return were verified. The
[actual result and limits](https://github.com/WigglyDon/psx/issues/5#issuecomment-6642286543)
keep the failed recipe, host checks and observed product behavior separate.
This adds no new in-game ordinary save, listening, simultaneous physical-touch,
cross-console, sustained-speed or release qualification.

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
The shared compact toolbar now owns its 18-point row above that rectangle,
without a spacer or any stretching/cropping. Roomy inner-screen defaults put
the stick and action cluster at the same height and the D-pad beside the stick;
narrow screens retain a stacked fallback. Existing custom controls are preserved.
This refinement follows the owner's native-proportion choice after the normal
update check; its new source qualification is separate from the tested APK above.
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
