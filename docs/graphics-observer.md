# Development PS2 graphics observation

[Master disposition](https://github.com/WigglyDon/psx/issues/2#issuecomment-6343717225)
authorizes this fixed full-path observer. It is an observation build, not a
rendering repair or performance option. The passing small probe is not repeated.

## Build and progress isolation

`PSX_GRAPHICS_OBSERVER=1 cargo xtask apk` selects Rust's `graphics-observer`
feature and applies `0002-development-graphics-observer.patch` after unchanged
`0001`. Donor pin, GLES backend, settings, synchronization, pacing and guest clocks
stay the same. Native additions are numeric call sites, frame-token carriage
and callback ABI glue; Rust owns sampling, tokens, aggregation and export.
Ordinary builds compile neither observation nor the second patch. No setting,
permission or network capability is added. Source validation checks the exact
combined diff through a separate index and rejects a diagnostic tree in an
ordinary build. A source-matched warm cache uses a 3 GiB incremental estimate
instead of the 8 GiB cold estimate; shared reserve/step checks remain mandatory.

The diagnostic app requires its own `observer-scratch-v1` root and exact
`prepared-v1` marker `PSX-OBSERVER-SCRATCH-1\n`; it never falls back to normal
progress. The admitted operator prepares a separate selected-game library,
regular copies of agreed settings/controller profiles, new card/checkpoint
paths and authorized read-only media references. Links outside scratch and hard
links are refused. Copy the selected firmware and its sidecars into scratch:
the donor can write NVRAM/mechacon sidecars beside its BIOS, so linking the normal
BIOS directory would not isolate progress. Internal resource/BIOS links stay
inside scratch. Resources install separately into scratch. Prove scratch
absent before preparation and retain its cleanup inventory. This development
contract is neither a user setup flow nor a new OS security boundary.

Diagnostic revision binding includes both patch hashes. Old-state compatibility
is not relaxed and the owner's only checkpoint is not moved to force a clean
boot. Normal progress, signer, APK and actual recoverable data remain the
rollback authority. Isolation/rollback must be proved before requesting use.

## Stages and numeric schema

Events contain monotonic microseconds, CPU insertion sequence, owner and opaque
context/surface/share-group/frame/texture-generation tokens, stage, dimensions,
32 state values and eight details. Private GL/EGL names are map keys only.
Context creation and surface attachment record separate generations. Tokens are
ephemeral and never saved game identity; CPU order is not GPU execution order.

| Stage | Meaning |
| --- | --- |
| 25 | Backbuffer allocation/texture generation |
| 1, 2, 3 | Producer pending state, fence/flush, publication |
| 4, 23 | Consume, or actual no-new/duplicate/read-lease-busy branch |
| 9 | Producer server wait issued, with fence still live for status |
| 5, 6, 7 | Native attachments, copy result, read-completion fence/flush |
| 8 | Producer return-wait issued, before live read fence deletion |
| 10 | Rust core-run return, hardware count and reported dimensions |
| 11 | Render admission, including absent surface and 1024×1024 guard |
| 15, 12 | Actual game-target→window blit inputs and post-blit observation |
| 13, 14 | UI completion and actual EGL swap return |
| 30 | Existing GL check, preserving its original typed result |
| 39, 40, 41 | Native retirement begins/ends; valid-context GL cleanup |
| 43, 44 | Actual frontend context/share-group and surface creation |

State 0–7: pending errors; 8/9: read/draw completeness (zero unsampled);
10/11: attachment tokens; 12/13: FBO tokens; 14–17: viewport; 18–21: scissor;
22: scissor enable; 23: sync status (zero absent/not requested, -1 invalid,
otherwise actual GL unsignaled/signaled enum); 24/25: attachment type;
26: fence presence. Other fields are reserved. Only already-bound targets are
queried, preserving bindings; an unused surfaceless producer default read FBO
is not treated as a failure. Fence queries precede deletion. Issuing a server
wait or seeing a valid unsignaled fence is not proof of completed GPU work.

Admission/post-blit details carry reported, fixed target and window dimensions
and the actual blit-guard result. Stage 15 carries the destination rectangle;
14 carries success and the returned EGL error. Detail slot 7=1 distinguishes an
error after observation queries from the pending-error sample; -1 means missing
current context, so no GL queries occurred. No pixels, paths or shader text are
exported. Existing shader/link/allocation diagnostics remain intact locally.

## Bounds, error recovery and qualification

A 256-event ring retains up to four initialization records, the first observed
failure and 32 preceding events. Capacity/contention drops are counted. Ordinary
samples use try-lock; failures retain their bounded copy under the short recorder
lock. No formatting or file I/O holds it. Multiple GL flags have no guaranteed
chronological order; the first observation is not necessarily the causal call.

Consumed fatal errors are retained for existing typed loss/OOM recovery once,
coalescing pending failures. Ordinary errors do not become device loss.
Retirement cannot requeue its old runtime's fault into the successor; context
replacement starts a clean pending-error epoch. Existing GL checks still return
their original fatal result, and EGL information comes from the actual operation.
No CPU finish, wait loop, pixel readback, screenshot, full command capture or
automatic black-screen detector is added. Sampling may perturb timing; clean
errors/FBOs/swaps do not establish correct pixels.

One observation writer formats/writes snapshots outside render/audio work; it
never enters VM or cards/checkpoints. At most three scratch files, each ≤256 KiB
(total ≤768 KiB), have first-failure, 45-second and lifecycle/retirement export
opportunities. A blocked frontend, process death or export failure can leave an
incomplete trace and must be reported. Host tests cover retention/bounds, owner/frame correlation,
fatal injection, cleanup/late errors, real incomplete FBO/binding preservation,
isolation and disabled execution. The patched-handoff fixture separately checks
token carriage/callback retirement using stubbed GL, not driver/game behavior.

After exact source/APK/native/symbol bindings pass, request one current custodian
admission through resource-discovery. It must explicitly include diagnostic APK
replacement and the already-selected private title/firmware, current APK/signer
and data rollback, and isolated scratch. One fresh-process clean boot only,
≤60 seconds from launch request, earlier stop on fatal failure/instability, plus
separate install/rollback/return reserve. No restored-state/configuration
permutation, second device, repeat, Fold access or automatic retry. Missing
isolation/admission/rollback stops use. Restore the agreed prior app/data and
verify fixture cleanup/normal resource return. Raw evidence stays private.

Report a sanitized boundary matrix on #2 and keep it open. Clean sampled stages
with wrong/unobserved pixels require a matched full-renderer or separately
reviewed private pixel observation, not another clear/error-only test. Apparent
non-reproduction is not a fix or gameplay/save/audio/performance acceptance.
