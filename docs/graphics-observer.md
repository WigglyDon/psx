# Development PS2 graphics observation

The [matched full-renderer comparison](https://github.com/WigglyDon/psx/issues/2#issuecomment-6348349808)
is closed. The [producer-operation dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6390726958)
authorized a separate source repair and one bounded observation, now completed
as recorded below. The [output-sync ownership dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6393048944)
is completed, along with the [common-result dispatch](https://github.com/WigglyDon/psx/issues/2#issuecomment-6398308448)
recorded below. The next [full-renderer donor/host comparison](renderer-comparison.md)
is prepared off-device only. This is development observation, not PS2 acceptance
or a driver diagnosis. Do not repeat the generated probe or a closed comparison.

## Completed common-result observation

The one admitted run of `8e61eb2ccdf364cc4aa80e33dfc0796021c9dac1`
used the frozen candidate bound in [report6400177312](https://github.com/WigglyDon/psx/issues/2#issuecomment-6400177312).
Its external launch-to-stop interval was16.180878s, with two bounded numeric
exports. Exact normal APK/data rollback, scratch removal, Home/app stopped,
booted/physical screen OFF and free/inactive lease were verified. No Fold
operation or additional arm occurred. This is graphics diagnosis, not gameplay,
audio, ordinary saving or sustained-speed acceptance.

The first retained failure remains stage16, producer frame464, at12.176201s
on the observer clock: a non-null output fence failed both original validity
routes before flush/publication; sampled error flags remained zero. Owner
retirement began at12.314657s and native close completed at12.484416s.
The first common nonzero error/fatal was frontend stage41/phase9 retirement,
`GL_OUT_OF_MEMORY`, at12.565759s. This later observation does not establish
when an allocation failed, exhausted physical RAM or initiated the fence fault.
Two bounded task-log submission warnings were retained. Their relayed logging
thread and wall-clock timestamps do not identify the original native caller
or establish ordering against the observer clock.

New coverage actually executed:14 original native error reads returned zero;
the only nonzero error record was the later frontend retirement sample.
The audited results include1,352 maps,140 shader compile checks,70 links,
32 binary/cache results and1,573 range checks, with no recorded failure there.
Runtime/probe/dependency failures and failed-request fields remain empty.
The coarse render/upload/composition/donor-overlay/output phases were visited;
they do not add error queries or prove that every native command succeeded.

All download allocation/copy/map/Flush/Poll/wait counters and the readback phase
were zero; no CPU/PBO selection was recorded. That path was inactive for this
run and supplies no justification for another hypothetical wait repair.
The four audited allocation classes recorded39 texture objects and7 stream
buffers, no download objects/CPU staging, zero failed requests, and matching
recorded delete calls at retirement. Requested-byte high water was24,578,048
for textures and219,152,384 for stream buffers. These estimates neither cover
all allocations nor prove driver deletion/storage success or physical memory
health. No resource/counter overflow, unknown release or untracked audited
resource was reported. The32-event common tail omitted6,987 older events;
first-error/failure fields and totals were retained independently.

The existing output ledger closed with464 creates,463 creator retirements,
one known-invalid release, zero frontend sync operations, no live/untracked
remainder or recorded ownership failure, and actual native/frontend context
destruction. That accounting does not certify GPU execution. No earlier
demonstrated source defect was exposed by the added observations.

This observational route is closed. The [prepared comparison](renderer-comparison.md)
keeps the full renderer and exact frozen donor bytes while changing the surrounding
application integration. It is not built or admitted for hardware, and adds no
production workaround or new player setting.

## Build and isolation

`PSX_GRAPHICS_OBSERVER=1 cargo xtask apk` selects the normal observation arm.
Adding `PSX_GRAPHICS_OBSERVER_MODE=no-game-image-reader` selects the reference.
Unknown modes are refused. Both use the same source base and native binary:
adapter `0001`, historical observer `0002`, then separate matched
observer/ownership glue `0003`. The producer increment also includes repair
`0004` in ordinary and diagnostic builds, and observation `0005` only in
diagnostic builds. Rust owns the fixed build-mode decision, sampling,
aggregation, stop transport and final presentation. No setting, fallback,
permission, backend, shader, resolution or guest-clock change is introduced.
Ordinary builds compile neither observer nor comparison path. The stream repair
changes their failure handling and exact revision binding while preserving
successful synchronization scheduling. Diagnostic revisions include all patch
hashes and the mode. No checkpoint compatibility exception is added.
The builder derives the expected combined diff using a separate index and
refuses unknown native changes. Retain the historical executed artifacts.

Both arms run the actual title and full GS workload into ordinary donor
backbuffers, with existing CPU/frame pacing, loader/UID/window, UI and final
swap. Normal retains forward wait/delete, attachment, blit, read fence/flush,
and producer reverse wait/delete. Reference selects an explicit no-reader
acknowledgement through Rust's build constant. It submits **no frontend donor
texture attachment, sampling or blit**, no forward GPU wait and no reverse
fence. The host-owned 1024×1024 target is explicitly cleared to a fixed color
on its own GL owner, then used by the normal Rust final blit/UI/swap. A static
reference picture is expected; it is not successful game rendering.

Consume transfers the producer fence and grants an exclusive, monotonically
numbered reader lease. Reference acknowledgement checks texture/lease, retires
the transferred non-null fence exactly once on the frontend owner, and releases
the CPU lease. Null or invalid fences are failures, never the mode selector or
proof of GPU completion. Stale/duplicate acknowledgements fail without deleting
another frame's fence. The producer's ordinary GL context orders its own future
writes; acknowledgement asserts only that no frontend GPU read was submitted.
No fake signaled fence or implicit normal `CompleteFrame(null)` is used.
Scheduling and GPU load necessarily differ; matched inputs do not mean identical
timing. Existing live-context resource retirement and worker join remain intact.

The app refuses an unprepared `observer-scratch-v1` root. Its `prepared-v1`
marker must be exactly `PSX-OBSERVER-SCRATCH-2:MODE\n`, matching the built mode.
Normal progress is never a fallback. Prepare equivalent independent selected-game
libraries, copied controller/settings files, empty cards/checkpoints and regular
copies of authorized BIOS **and sidecars**. All mutable paths stay in scratch;
external links and hard links are refused. Existing selected media remains
read-only. Prove scratch absent, actual APK/signer and recoverable normal data
before install; preserve the normal checkpoint and strict revision checks.

## Boundary audit and schema 2

Native `GLsync` is pointer-sized and passed unchanged through the C callback.
C-layout metadata carries the current native EGL context and actual GLAD
creation/validity/status/error entry points only during the callback. It uses
the already-loaded `eglGetCurrentContext`, not context capture that reloads EGL.
Rust compares with the platform's actual current context and the entry points
captured by **its actual glow loader** on that owner. It independently compares
native and Rust validity queries on the same handle. Function-address equality
is reported, not assumed: different trampolines alone do not prove a defect.
Absent context, ABI mismatch, validity disagreement and pre/post handle/context/
dispatch changes request a diagnostic stop. No raw handle/address is exported.

Fence checks run immediately after creation and **before the existing flush**,
then after that flush, on the same owning thread. Producer frame publication is
later: the consumer cannot yet retire that frame's fence. The read fence follows
the same pre/post check. No status query follows a failed validity check. Null,
invalid, valid-unsignaled, query errors and absent context remain distinct.
Consumed OOM/context-lost errors still enter existing typed recovery once;
ordinary errors do not become device loss. Retirement cannot requeue an old
fault; actual frontend context replacement begins a clean error epoch.

Events contain sequence/time, opaque owner/context/surface/group/frame/texture
identity, dimensions, 32 state values and eight details. A 256-event ring, four
initialization records, first failure plus 32 preceding events, first terminal
observation and monotonic sampled stage milestones are bounded. Drops count
capacity/contention; this is not a complete GPU history or causal ordering.

| Stage | Meaning |
| --- | --- |
| 25 | Backbuffer allocation |
| 1, 16, 2, 3 | Producer state, new fence before flush, after flush, publication |
| 4, 23 | Consume or no-new/duplicate/reader-busy branch |
| 9 | Existing producer server wait issued, before deletion |
| 5, 6, 17, 7 | Native attachments/copy, new read fence before/after flush |
| 8 | Existing producer return wait, before read fence deletion |
| 51, 52, 53 | No-reader CPU acknowledgement, producer fence retirement, rejected lease |
| 10, 11 | Rust run return and presentation admission |
| 15, 12, 13, 14 | Final game-target blit inputs/result, UI, actual EGL swap result |
| 30 | Existing GL check with original typed result |
| 39, 40, 41 | Retirement begins/ends, valid-owner cleanup |
| 43, 44 | Actual frontend context/surface creation |

State 0–7: pending GL errors; 8/9: sampled read/draw completeness; 10/11:
attachment tokens; 12/13: FBO tokens; 14–17: viewport; 18–21: scissor; 22:
scissor enable; 23: sync status (0 absent/unqueried, -1 invalid, otherwise actual
GL status); 24/25: attachment type; 26: fence presence. Only already-bound targets
are queried. Unused surfaceless default-read incompleteness is not a failure.

Native callback detail: ABI valid, same current context, creator-entry equality,
validity-entry equality, native validity (-2 unqueried), pre/post identity
(0 pre, 1 matching post, -1 mismatch, -2 other stage), status-entry equality,
error-entry equality. Other Rust stages retain their existing detail schema.
An additional error event uses detail[7]=1; missing context has detail[7]=-1.
Stage 23 is a CPU-only empty/duplicate/busy handoff observation: it transfers no
frame or GPU ownership and can precede native GLAD/EGL initialization. Its state
is unqueried, with detail[7]=-2; unavailable native dispatch here is not a context
failure. Actual consume, producer/copy/fence and presentation stages still require
their current owning context and retain the existing diagnostic stop checks.
Post-query errors are collected separately and preserve fatal forwarding.
Successful FBO/swap samples are not proof of correct pixels.

## Stop and collection

Callbacks only retain bounded memory, latch a stop reason and wake the actual
application owner and independent writer. No callback formats/writes files,
waits for GPU completion or reenters teardown. Invalid/null required fence,
missing context, ownership/dispatch mismatch, fatal GL result or failed swap
requests stop. The application owner checks before further rendering and retires
through its existing core pause/unload/join, retaining completed writers but
starting no new checkpoint/flush for disposable faulted scratch.

The independent writer emits immutable `first-stop.json` (≤4 KiB) without
waiting for the frontend or a 45-second deadline. It also consumes the admitted
operator's `operator-stop` marker and wakes the same owner. Up to three numeric
snapshots (≤256 KiB each) cover first failure/terminal and lifecycle/retirement;
total export bound is 790,528 bytes. A blocked GL owner, process death or storage
failure can still prevent complete retirement/collection; report it explicitly.
There is no production automatic recovery or black-screen detector.

The actual retained Rust operator imports the reviewed pure `operator` module.
It recognizes the original GPU-command ioctl warning, submission/wait failures,
invalid/fatal stop signal and same-lease owner stop request. It delivers the stop
marker on the admitted lease, then externally retires before the 30-second cap.
Presence is checked separately from legacy `exec-out` file bytes. Each valid
export is saved independently; an absent optional or malformed later export
cannot erase accepted evidence. No PASS is inferred from launch, exit or swap.

Focused checks cover high-bit C/Rust forwarding, same-thread/context/dispatch
pairing, valid-unsignaled versus invalid/no-status-query, real host GLES
pre/post and deleted-sync observations, existing error forwarding, normal and
no-reader handoffs, stale/double acknowledgement, repeated teardown, disabled
ordinary builds, independent writer/owner stop delivery and the retained actual
warning/collector fixtures. These establish host contracts, not physical GPU
recovery, gameplay, saving or speed.

## Physical comparison and return

Publish exact source/patch/APK/native/symbol/signer/mode bindings before requesting
one current custodian admission through resource discovery. Previous release is
not current health clearance: reconcile the prior submission failure and
whole-screen flicker explicitly. The Fold is excluded.

At most two fresh-process clean boots, one per arm, reference first, each
externally capped at 30 seconds from launch, with separate installation/rollback
reserve. Same already-authorized title/firmware, fresh equivalent scratch,
unchanged display and input. Record observer time and full-GS/publication/
copy-or-ack/presentation milestones. Progress that never reaches the earlier
failure's workload interval is inconclusive. No screenshots/readback, generated
probe, repeats, permutations, system changes, reboot, clear-data or other apps.

At the first submission/wait failure, recorded invalid/fatal boundary, owner's
stop or whole-screen instability: stop and suspend the remainder for custodian
recovery/re-admission. A reproducing reference is useful; do not launch a normal
arm redundantly. Restore the exact baseline, verify all normal data, remove only
owned scratch, preserve Home, verify physical return and release through the
same procedure. Keep raw evidence private and publish sanitized results on #2,
which remains open for Master acceptance.

Reference failure removes the game-image read/copy as a necessary condition for
that reproduction; common GS/setup/command-stream/dispatch/remaining UI still
exist. Clean reference plus failing matched normal narrows transfer/ownership/
timing, not universal producer correctness. Both clean, unmatched progress or
interruption is inconclusive, not a fix or performance result.

## Historical actual full-path observation

Source `4277d4ca651a4c9c2e8e88113a0ef092e330f01c` ran one clean boot for 46.656s
with unchanged adapter `90b35554…` and historical observer `a08bf0c8…`.
First retained abnormality: producer stage2/frame465 at 11.435145s,
non-null fence invalid after flush, before publication. Initialization and the
preceding producer samples retain context4/group1; frontend context1/group1.
The earlier frame's copy FBOs, 640×448 admission and final swap samples succeeded.
That old observer did not carry the native/Rust dispatch comparison; its tokens
alone cannot rule out a dispatch defect. No handle truncation or deleting-owner
escape was demonstrated by source audit. No production repair follows.

The owner reported whole-screen/controls flicker and own-process submission
failure was retained. Two valid exports total145,144bytes; capacity/contention
drops10,828 and51,019 limit causal claims. The old operator missed the warning
and ran to its deadline; optional missing third export caused a late parser
error after both valid files were saved. Executed operator, receipts, APK/source,
native symbols and rollback remain frozen. Exact original APK/normal-data
restoration, Home, physicalOFF and original shared-lease release were verified.
This accepted localization increment is neither a compliant early-stop run nor
a driver/GPU-health certification. The new matched comparison must qualify its
own corrected observation/operator rather than reuse that trial as its control.

## Corrected no-reader observation

On 2026-10-24, application source `123e831e4afb6c7cec2d8e05a030e3cd82a72bba`
ran the real full-renderer reference, APK `44d21ffe…`, native `12a9aabc…`,
matched patch `020b4c2c…`. It acknowledged/presented/swapped through frame467
with copied-frame milestone0. First retained failure was producer stage16,
frame468, at11.788855s: a non-null newly created fence was invalid through both
native and Rust validity-query routes **before the existing flush and publication**.
ABI/current-context and all four dispatch comparisons matched. Sampled GL error
slots were zero; the previous frame's pre/post fence samples were valid. Stage2
retained the invalid result31microseconds later with matching pre/post identity.
These bounded observations do not identify a driver cause.

The reference thus reproduced a producer-fence failure without frontend game-image
attachment/sampling/copy or a reverse read fence. Those image reads are not a
necessary condition for this reproduction. Common GS execution, context/setup,
producer-sync retirement/CPU acknowledgement, observation and remaining UI/swap
still exist; the modes have different timing. Source123e831 includes552d7da's
ownership repair, but this reference does not exercise or accept the normal
frontend-read ordering path that repair protects.

The first-stop signal was reason1/frame468/consumed467. Actual-owner retirement
began about140ms after the failing sample; external retirement was verified
13.801s after launch, within30s. The normal arm was suppressed; no repeat ran.
Two numeric exports total144,942bytes, with10,705/10,715 dropped events, so this
is not a complete GPU history. No images were captured or visual-flicker claim
made. Exact original APK/full normal-data restoration, Home, app stopped,
physicalOFF/bootcomplete and shared-lease release were verified at22:10:29UTC;
whole custody lasted41.019s. This is a diagnostic result, not a production fix,
gameplay/saving/performance acceptance or PLAYER-2 completion. The issue handback
owns full artifact/operator bindings; raw evidence remains private.

## Producer stream investigation

The preceding comparison is closed; its suppressed normal arm must not run.
The next observation answers a separate producer-operation question. Historical
stream implementation selection was not recorded. The pinned factory chooses
persistent storage when supported and allocated, except its retained narrow
Adreno650 gate; otherwise it uses NVIDIA SubData or orphaning BufferData. The
logged BrokenBufferStreaming profile bit is not consulted by this factory.

Development patch0005 records these actual numeric operations without new GL
queries, fences or waits:

| Stage | Meaning |
| --- | --- |
| 60 | Selected implementation: 1 persistent, 2 SubData, 3 BufferData |
| 61 | Actual mapping request and current offset |
| 62 | Actual fence-creation non-null result and protected range |
| 63 | Actual client-wait result and protected range, before retirement |
| 64 | Persistent-storage unmap range/coherence, before the existing flush |

Each carries the sampled owner/context/frame, an opaque buffer token and
generation, target, offset, length and capacity. No native handle or guest bytes
are exported. Fixed counters retain every admitted stream operation; at most16
selected-resource records and the first stream failure survive tail eviction.
The existing export bounds remain unchanged. Failed wait/null creation requests
immediate owner stop with reason9, separately from the production failure latch.
Ordinary builds contain neither these call sites nor their additional fields.

Only after source/regression/lint/package validation and fresh custodian
admission may one new reference boot run: same full GS workload/title/settings,
no-game-image-reader, fresh isolated scratch, maximum30s, no screenshot/readback
or second arm. Stop at the first failed dependency or existing instability
boundary, then verify baseline data/APK restoration and resource return. Report
source proof, actual selected path, first operation/result and historical symptom
separately. Successful waits followed by a later invalid fence do not explain
the old failure; disappearance in one instrumented run does not establish a fix.

The one admitted producer run is now complete. It selected seven persistent
buffers and recorded one non-null stream-fence creation, but no client wait
before retirement. A later output fence was invalid before its existing flush.
The source wait-result repair therefore has not explained this reproduction;
there were no successful waits to report either. The creation's individual
range/frame event was evicted, while its counter and all selection records
survived. No repeated run or normal arm follows from this result. See
[verification](verification.md#player-2-admitted-producer-observation) and the
[sanitized handback](https://github.com/WigglyDon/psx/issues/2#issuecomment-6392928084).

## Creator-owned output-sync reference

The actual patched handoff audit exercised recycled opaque handles across
generations, stale/duplicate acknowledgments, unread replacement/withdrawal,
normal output wait/deletion, reverse-fence retirement and pacing cancellation.
It demonstrated no stale/double output-fence retirement in that contract.
In particular, consumption clears the shared fence slot before its producer
can overwrite it, and an unpublished generation cannot already be consumed.
The historical no-reader reference still deletes consumed output fences on
the frontend; that is an interaction to isolate, not proof of an invalid API use.

`PSX_GRAPHICS_OBSERVER_MODE=producer-local-output-sync` selects one fixed
development-only successor to the no-game-image-reader reference, adding
native patch0006. Normal and historical no-reader stacks remain available and
unchanged. No player setting or ordinary-release path selects it. It retains
real GS work, output-fence creation and pre/post existing-flush observations,
metadata pacing, the initialized host picture and Rust UI/final swap.

Rust owns a maximum-four-entry output ledger and monotonic generation IDs.
Raw handles are private keys, never generations or export fields. Creation,
publication, consume, acknowledgment, drop, cancellation and retirement are
validated against generation/frame/lease and actual creating thread/context.
Only metadata is copied into the shared frontend descriptor; its fence field
is cleared before publication. The selected frontend branch acknowledges that
metadata without output-sync validity/status/wait/delete operations and without
a donor-image read or reverse fence.

At creator-side backbuffer reuse and before the next output-fence creation,
the owner drains acknowledged/dropped/cancelled entries. Native glue deletes
the exact Rust-authorized sync once on that creator, then confirms retirement.
Deleting an owner fence needs no added wait: the creator's commands are ordered
in its own context and the frontend submitted no image read. CPU acknowledgment
and deletion do not prove GPU completion. Pending ownership is never silently
reused; capacity/generation overflow stops before another output creation.

Pacing cancellation withdraws metadata and clears the outstanding CPU lease;
the creator performs retirement while its context still exists. Known-invalid
names are not queried/deleted for accounting. On a reported context fault,
remaining names are held for share-group reclamation, not handed to GL.
Their accounting remains pending until donor join and successful final frontend
context destruction. The diagnostic writer joins after that actual graphics
destruction, retaining the existing maximum-three-export bound. Unverified
destruction or missing coverage remains explicit rather than being called clean.

The existing observer includes non-evicting lifecycle totals, first ownership
failure, first invalid generation and32 recent generation-tagged events.
Create/publish/consume/ack/creator-retire/invalid-name/context-release/live/high
water totals distinguish operations and owners; omission, untracked creation
and counter overflow are explicit. These are logical ownership records, not
GPU-completion or driver-health measurements. Existing typed errors and owner
stop remain; no new GL probe, fence, wait, finish or timing control is added.

Retirement timing and GPU load can differ from the earlier reference. One new
fresh-process boot is conditional on exact source/package/symbol verification
and fresh current custody: same stock target/private workload/settings/clocks,
independent scratch, maximum30s, immediate first-fault/warning/owner stop,
external retirement and separate exact rollback/return reserve. No other arm,
repeat, restored state, screenshot/readback or Fold use is authorized. Source
tests and any admitted result must be reported separately; this is not a PS2 fix.

The one admitted creator-owned run is now complete, with exact normal APK/data
rollback and verified physical return. The pre-flush output-fence invalidity
reproduced at generation466 despite zero frontend output-sync operations and
complete final lifecycle accounting:465 creator retirements, one invalid-name
release, no remaining names or ownership failure. See
[actual observations and limits](verification.md#player-2-admitted-creator-owned-output-sync-reference).
The earlier export was explicitly incomplete; only the post-join final export
closed its live ownership. This closes the cross-context output-sync branch
for this reproduction. Do not repeat the variant or infer a driver root cause.
Further off-device work moves to the common GS rendering/submission/context
path; no new physical scope or production workaround follows from this result.

## Common renderer result observation

Master6398308448 keeps the same creator-owned reference and adds native
patch0007. It is not a new handoff policy or player setting. Rust records
existing native results through the existing observer and its bounded writer.
The ordinary and historical native stacks exclude this extension.

The retained creator-owned run's first fence failure was at11.589871s.
Owner retirement began at11.727401s, native close was recorded at11.864115s,
and the frontend stage41 sample returned `GL_OUT_OF_MEMORY` at11.916541s,
after `Graphics` had attempted its UI-object cleanup. That is a later error
observation, not the time/cause of an allocation failure or evidence of exhausted
phone RAM. The same bounded application log retains two submission warnings.
Its wall-clock timestamps have no exact established mapping to the observer's
monotonic epoch. Native stdout is relayed by a logger thread; that logger's
thread identifier cannot identify the original graphics caller. Neither warning
order relative to individual observer events nor a causal command is inferred.

Source audit found six destructive error reads in the stream factories and
three sites in debug-group/optional remote-hardware paths. The new wrapper
returns exactly the flag each original read consumed, once; it does not read
another flag, suppress fallback or reinterpret a cache miss as required shader
failure. Factory/capability and binary-cache probes are labeled separately.
Existing Rust error samples also feed the record after their original fatal
forwarding. Native fatal flags use the same typed latch before requesting a
diagnostic stop. First runtime failure, first nonzero/probe error, first failed
request/dependency and first fatal result survive later retirement errors.

| Audited boundary | Added evidence; no new error query |
| --- | --- |
| `GSDeviceOGL::Create`, `CheckFeatures`, `DoRenderHW` | Initialization, probe and rendering entries/exits |
| `GSTextureOGL::DoUpdate`/`DoMap`/`Unmap` | Upload phase, requested shape/pitch and existing stream-map results |
| `PresentRect`, donor overlay call, output interval | Composition, overlay entry/exit and output phase; original fence samples retained |
| Stream constructors/destructors and mapped-write callers | Adopted GL capacity, actual map results, ranges and lifetime totals |
| `GSTextureOGL` storage request/destruction | Dimensions, sized format, levels, requested bytes and logical lifetime; GL storage calls return void |
| `GLProgram` compile/link/cache checks | Existing compile/link returns, with optional cache rejection distinct from required compilation |
| `GSDownloadTextureOGL` create/copy/flush/poll | CPU/PBO selection, actual map/fence/wait results and independent call/result totals |
| Owner retirement, existing native/frontend EGL destruction | Retirement phase and actual destruction results; no per-object deletion claim after a fatal flag |

Download counters and the first selection/dependency survive tail eviction.
A nonblocking timeout remains pending. Entry into a required copy is an attempt,
not proof that pixels were read successfully. No new readback, fence, wait,
finish, per-draw error poll or global interception is introduced. Non-null
names/maps do not prove completed GL storage. Missing paths remain unknown;
zero download entries can exclude that path only for an actual qualified run.

Accounting covers native texture requests, adopted stream GL buffers, download
GL buffers and download CPU staging. It excludes other CPU memory, programs,
framebuffers, samplers and driver allocations. Checked texel/block/mipmap sums
and explicit unknown formats describe requested bytes, not physical GPU usage
or free RAM. Live ownership is bounded to512 entries; native destruction keys
to its actual context, and context-based reclamation also needs successful
frontend destruction. CPU staging is never credited to EGL reclamation.
Untracked resources/releases, unknown byte estimates, overflow and omissions
remain explicit. Resource generations and opaque caller-thread/context tokens
replace private names/addresses. Generation0 marks a pre-adoption or untracked
operation. Progress tags identify the most recent output frame, not the exact
guest draw that issued a command.

Only32 recent result records are retained, alongside fixed non-evicting totals
and first failures; existing256KiB/three-export limits remain. After recorded
OOM/context loss, the diagnostic avoids subsequent ordinary object probing and
audited GL cleanup and records actual context destruction separately. Successful
paths and original native return values remain. These source checks do not
explain the prior failure; observation and retirement timing can change.

A single fresh admitted run is conditional on exact package/symbol/source
bindings and discriminating result coverage. If it exposes only the same output
fence failure, this observational route ends. Prepare the smallest source-matched
full-renderer donor/host comparison off-device with explicit equivalence limits
and cost; a new physical comparison needs its own disposition. Do not append
another ownership or single-fence variant. The Fold remains excluded.
