# Development PS2 graphics observation

The [active Master disposition](https://github.com/WigglyDon/psx/issues/2#issuecomment-6348349808)
authorizes a matched full-renderer comparison. This is development observation,
not PS2 acceptance, a production rendering repair, or a driver diagnosis. Do not
repeat the passing generated probe.

## Build and isolation

`PSX_GRAPHICS_OBSERVER=1 cargo xtask apk` selects the normal observation arm.
Adding `PSX_GRAPHICS_OBSERVER_MODE=no-game-image-reader` selects the reference.
Unknown modes are refused. Both use the same source base and native binary:
unchanged adapter `0001`, historical observer `0002`, then separate matched
observer/ownership glue `0003`. Rust owns the fixed build-mode decision, sampling,
aggregation, stop transport and final presentation. No setting, fallback,
permission, backend, shader, resolution or guest-clock change is introduced.
Ordinary builds compile neither observer nor comparison path; their PS2 revision
binding and rendering behavior remain unchanged. Diagnostic revisions include
all patch hashes and the mode. No checkpoint compatibility exception is added.
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
