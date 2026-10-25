# ARMSX2 adapter patch

Apply to exactly `83fe4e293f71ab5587ef81b06143725bf8c6a30b`.
The original submodule is unchanged. `cargo xtask ps2` stages it and applies
`0001-host-pause-and-card-flush.patch` before building.

The exported adapter functions explicitly use C linkage; the donor's
version script exports only `retro_*`. Package verification requires each
defined public C symbol, catching a C++-mangled/hidden adapter before install.

The patch forwards four narrow operations through the donor's existing CPU
queue: readiness, pause, file-card flush and nominal playback speed. It releases
the GL/Vulkan frame handoff before a blocking pause, so the native CPU and GS
owners can settle without another frontend frame. The flush returns actual
`fflush` failure for regular file cards. Rust then syncs those files and their
directory before binding a checkpoint to their digest. Folder-card durability
is not supported by this host. JIT and guest clocks remain unchanged; the
targeted card-protocol repairs below are retained donor modifications.

`retro_psx_set_frame_callback` additionally exposes a non-reentrant notification
from the existing GLES frame handoff. It only wakes Rust's frontend owner;
frame consumption, input/audio service and presentation policy stay in Rust.
Registering against an already published frame also notifies. The callback
is serialized by the existing handoff mutex, so clearing it retires in-flight
calls before the Rust binding/platform waker or library can be destroyed.
It never moves GL work onto the callback thread or changes native frame/fence
storage, CPU limiter, guest clocks or serialization. This GLES extension does
not qualify a future Vulkan host. A focused Linux test applies this patch to
the pinned donor and compiles its actual handoff with mechanical GL stubs;
it checks notification, consumption and shutdown, not GPU execution or speed.

The GLES backbuffer ring also needs reverse ordering. `ConsumeFrame` releases
the producer's pacing wait before the frontend has queued its texture blit;
three buffers alone do not prevent reuse before that read completes. The
frontend now flushes a completion fence after the blit. Before overwriting,
resizing or deleting that particular backbuffer, the GS owner waits for the
frontend to publish its fence, then queues a server-side wait and retires it.
Other backbuffers remain available. With pacing disabled, an unread frame is
withdrawn before its texture is overwritten. The existing handoff mutex owns
these transfers; fences remain bounded by the three retained backbuffers.
No per-frame CPU `glFinish`, new rendering backend or emulated-state change is
introduced. The actual-helper fixture verifies delayed completion, different
backbuffer use, exactly-once wait/deletion and unread-frame withdrawal. Device
graphics stability and performance remain unverified for this repair.
See [Khronos synchronization guidance](https://wikis.khronos.org/opengl/Synchronization).

Time-scale updates release the frame handoff before the blocking CPU callback
and update only the existing nominal scalar/limiter deadline, without a full
VM settings reload or pause/resume transition. Zero in this C ABI selects the
donor's unlimited limiter; Rust handles zero-time pause separately through
`retro_psx_set_paused`. Rust validates finite targets and coalesces slider
changes to at most 20 per second, pacing audio/frontend calls at the applied
rate. The native callback retains its current running/paused state and restores
the frame handoff only when running. Guest CPU/device clocks are unchanged.

The libretro checkpoint omits the unused thumbnail: psx restores and renders
the machine directly. `retro_psx_capture_state` forwards the donor's existing
complete `SaveState_DownloadState` through its paused CPU owner, transferring
the owned byte/name `ArchiveEntryList` to Rust. `retro_psx_pack_state` consumes
that list on the existing Rust save worker through `SaveState_ZipToBuffer`;
`retro_psx_free_state` handles cancellation. These are ABI glue around retained
capture/ZIP implementation. Packaging performs no VM, GL, card or CPU-queue work.
Its explicit stored-ZIP argument bypasses live compression settings, so the
immutable list can be packaged independently of a resumed or torn-down runtime.
Rust retains the loaded library until pack/free returns, with no cloned state
owner; it joins the one checkpoint writer before runtime teardown/switching.

The length-prefixed archive and emulated fields are unchanged; the existing
reader accepts stored and compressed ZIP entries. Rust retains only the exact
validated extent, not unused ABI padding. Capture/card verification stay on the
paused owner and remain synchronous costs. Packaging/hashing/writing move off
that wait; separate timings and device checks must establish the actual gain.
A Linux fixture compiles the actual patched capture/pack exports with mechanical
VM/ZIP stubs, checking post-resume packaging, unchanged frozen bytes, cancellation
and failed-pack ownership. It is not full serialization or speed acceptance.

The observed clean-boot card-format failure also has a bounded diagnostic:
the first 32 PS2 writes and 16 erases report transfer geometry/checksums, without
guest bytes or private paths. This is evidence collection, not a card fix.
After both fresh JIT and IOP-interpreter boots failed before either trace
appeared, the next diagnostic records at most four occurrences of each SIO2
card opcode per process: port/slot, presence, auto-eject counter, terminator
and transfer lengths. It records no guest/authentication bytes. This distinguishes
probe/auth/setup failure from file writes without an unbounded protocol log.

A separate off-device reproduction found that `The2bTerminator` incorrectly
uses the total output FIFO length when padding one reply. The retained DMA11
input loop can enqueue several responses before DMA12 drains them. After a
144-byte earlier response, 5/9/14-byte replies contain only four bytes and their
acknowledgement/terminator moves. Count this reply's two existing header bytes
instead; prior queued bytes remain untouched. A Rust-driven fixture applies the
tracked patch and compiles the actual helper with mechanical FIFO/card types.
It fails before the repair and checks reply extents, zero padding and both
terminators after zero, one and two queued DMA blocks. It does not execute guest
firmware or prove the cause/repair of the observed game-format error. No card
preformatting, successful-response stub, auth bypass or state field is added.

Two further retained SIO2 repairs address card-type and restore coherence.
The dispatcher refuses PS1/PocketStation opcodes on PS2 cards, matching SIO0's
existing type check. It appends dead air for the current packet without entering
legacy 128-byte read/write handlers or disturbing queued earlier replies.
The original state stream stores CRCs but omits the card endpoint's negotiated
terminator and transfer state. After loading it, use normal eject/reinsert/auth
even when CRCs match; a fresh process otherwise resumes guest MCMAN against
power-on endpoint state. Current media and the archive layout stay unchanged.
Rust-driven fixtures apply the actual patch, compile its dispatch/restore
methods with mechanical devices/stream types, and fail before both changes.
They verify the type gate, retained genuine PS1 dispatch and reinsertion for
matching/changed media. They do not execute guest filesystem saving, restore
omitted protocol fields or repair bytes damaged by earlier legacy writes.

Its existing `OnPerformanceMetricsUpdated` callback also forwards a five-float
completed sample (VPS, speed percent, average/minimum/maximum frame time) via
the private `0x20000` environment command. Rust copies it under the output
mutex; it never reads those native globals from the frontend thread. Frontend
call rate is not reported as achieved PS2 emulation speed.

A separate nine-float GS-owner sample uses private command `0x20001`: EE/GS/GPU
usage, GS/GPU milliseconds and draws/readbacks/barriers/blocking waits. Rust
copies finite nonnegative values before returning, under the same output mutex.
It reports these at its existing bounded diagnostic interval. No new thread,
GPU query, readback or public telemetry is introduced.
The donor increments `GpuBlockingWaits` only on Vulkan. Its zero on GLES is
not an observed absence of waits; use the implemented GLES counters and timing.

Rust's Android host option `psx_performance_hints` enables the donor's retained
ADPF implementation at startup. This is glue around its existing thread
registration, finite frame-work sampler, runtime capability checks and teardown;
it neither forces system clocks nor changes guest timing. Other frontends keep
their existing opt-in policy. This requires real device speed/thermal comparison;
a successful compile does not establish a performance gain.

The donor writes its card change-detector on close. Rust holds the entry's
writer lock through native teardown, syncs the final files and seals only the
checkpoint that matched the pre-close cards. A failed/interrupted seal retains
an incompatible checkpoint and current card; it never restores older cards.
The PS2 checkpoint binding includes both upstream revision and this patch's
SHA-256. Host policy and input/launch/session ownership remain Rust.

The observed ANGLE failure enabled ARM in the GLSL header while the existing
PowerVR shader path emitted EXT `inout` syntax. A small header repair enables
EXT for that path, retaining Mali's ARM preference and the ARM-only fallback.
The complete captured shader fails without EXT and compiles with EXT on Mesa;
this compiler regression observation does not establish Fold performance.
Rust removes its earlier copy workaround only for the affected ANGLE/PowerVR
renderer. Device rendering/audio/speed validation remains necessary.

Libretro omits the generic manual-API-choice warning: the frontend grants the
specific context, so this is not an unsafe user override. Actual renderer and
other unsafe-setting diagnostics remain. Those rendering repairs, frame
notification, capture/packaging and the reply-padding correction change no
emulated state or archive fields. The card repairs do change wire/restore
behavior. Rust permits only the exact prior `878bdd9e…`, `92542cb6…`,
`23a0b217…`, `9f5bf1a2…`, `510a3f5e…` and `4650bdf4…` patches to resume into this exact
`90b35554…` patch, retaining other input/card/payload checks. A future native edit disables
that explicit exception. New
checkpoint writes always record the complete new patch identity.

Original file licenses remain applicable to these modifications. No upstream
acceptance or device gameplay result is implied by this patch.

`PSX_GRAPHICS_OBSERVER=1` additionally applies
`0002-development-graphics-observer.patch`. Its native additions are numeric
call sites, a frame token and callback glue; observation, error retention and
bounded export live in Rust. Ordinary builds reject this second patch. It
changes the diagnostic checkpoint binding without relaxing compatibility.
See [the observer contract](../../docs/graphics-observer.md) for isolation,
limitations and the separately admitted real-workload procedure.

`0004-stream-buffer-failure.patch` is a retained-core failure-handling repair.
The actual stream helper previously discarded every `glClientWaitSync` result,
deleted the dependency and authorized wrapped CPU writes. Only
`GL_ALREADY_SIGNALED` and `GL_CONDITION_SATISFIED` now permit reuse. Timeout,
failed/unrecognized waits and null fence creation latch a first failure; failed
allocation grants no pointer or rewind. Every mapped-write caller checks the
mapping before copying; draw/publish entry points stop after the latch. All
remaining fences, including the wrapped portion, stay owned until teardown.
There is no additional GL wait, error read, timeout change or retry.

The private `0x50535801` callback copies two numeric words into a bounded Rust
first-failure latch and wakes the application owner. It does not tear down
reentrantly. Rust refuses further runs/new checkpoint capture and uses existing
owner retirement. It also frees a captured list if the callback failed while
capture was servicing native work. Cards and preceding checkpoints are retained.
This native edit has its own revision binding and disables the old checkpoint exception.
Controlled-return tests compile the pinned helper and actual upload callers;
these are source-level regression evidence, not GPU or historical-cause proof.

Observer builds apply patches in order `0001`, `0004`, `0002`, `0003`, `0005`.
The development-only `producer-local-output-sync` mode additionally applies
`0006-development-creator-output.patch`. It adds necessary retained-native
ownership/C-ABI glue, with generation/lease policy and bounded lifecycle
recording in Rust. Creator output fences remain on the GS owner; the shared
frontend descriptor contains metadata and a null, non-actionable fence field.
Acknowledgment does not mean GPU completion. The creator drains ready entries
at backbuffer reuse/before the next output creation and during cancellation/
resource destruction. Known-invalid names are not deleted for bookkeeping;
context-fault leftovers remain accounted until actual share-group destruction.
The full GS/output-create/pre/post-flush path stays in place. Ordinary and
historical diagnostic modes do not apply0006; no player option is introduced.
The extended fixture exercises the actual patched handoff/selected Main branch
with the real Rust ownership policy, including recycled handle values and
creator-only retirement. Source tests do not establish a GPU cause or fix.
`0005-development-stream-observer.patch` alone adds implementation selection,
buffer generation and real create/wait/map/unmap observations. Its callback,
instance fields and call sites are absent from ordinary builds. All retained
donor copyright and license terms apply to these modifications.

The same creator-owned reference now also applies
`0007-development-common-results.patch` under Master6398308448. It forwards
existing error reads and operation results, coarse phase scopes, audited
allocation/release requests and required download/readback selection/results
to the existing Rust observer. No extra GL error read, pixel readback, fence,
wait, finish, driver layer or player option is added. It preserves the value
returned to each original native caller, including optional binary-cache
rejection and nonblocking poll timeout. Native fault guards skip audited GL
cleanup after a reported OOM/context loss; actual EGL destruction is recorded
separately. Names/maps are not proof of completed storage and requested byte
estimates are not physical GPU usage. The source fixture compiles the retained
error-consuming factory check, download Flush/Poll and retirement method with
controlled results. Runtime selection and the historical cause remain separate
questions. Ordinary/historical stacks exclude0007. The necessary ABI remains
in the retained handoff header; substantive recording/policy stays Rust.
