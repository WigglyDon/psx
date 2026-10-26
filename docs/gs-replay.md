# GS-REPLAY-1

**Closed at the negative reproduction gate**, accepted by
[Master6427077508](https://github.com/WigglyDon/psx/issues/2#issuecomment-6427077508).
The suppressed pinned operand stays suppressed. The ongoing assignment is the
complete playable PS2 implementation loop; the historical bounds below do not
require a new Master dispatch after every product test or device return.
[Master6420075065](https://github.com/WigglyDon/psx/issues/2#issuecomment-6420075065)
owns the executed conditional sequence.
The earlier hold on capture/replay is superseded. Implementation and off-device
qualification precede physical admission. No ordinary player setting or release
execution path exposes this experiment. New orchestration, validation and bounds
are Rust; patch0008 is narrow retained-donor capture/replayer/build glue. Original
native licenses and the preceding seven patches remain applicable.

The sequence is one private corrected-minimal disc boot/capture, one modified
replay, then a pinned-runner replay only if the modified path reproduces the
relevant graphics fault with equivalent output coverage. Each is one fresh
custodian admission and at most30 seconds of guest/replay work, followed by
verified return. No Fold, normal-player replacement, restored checkpoint,
automatic loop, recapture, optional image batch or backend sweep is included.

## Observed boundary, 2026-10-26

The single capture reproduced the producer output-fence failure at output462.
Its stopped ledger does not assert clean codec closure; the retained original
bytes independently pass format/state, bounded decompression, framing, complete
trailer and VSync-end validation. Capture accepted/started at GS boundary1 and
stopped at504, retaining503VSyncs and1,330packets. These identities are distinct
from producer outputs. Original failed collection/return evidence was preserved,
then cleanup-only recovery verified the original copy, normal data and return.

The one modified replay consumed all1,330packets/503VSyncs, reached its EOF hook
and created/retired493creator-owned outputs without the matching fence failure.
A GPU submission warning was detected; the structured observer recorded no
nonzero error, fatal or dependency failure. Both contexts and native owners
retired. The positive reproduction gate refused the pinned-runner arm, which
was never requested or executed. This is not a clean-run or driver verdict.
Live EE/IOP/VU execution and pre-capture resource/scheduling history are absent
from replay; the route did not preserve the original reproduction.

The diagnostic sequence therefore stops here under the dispatch's negative
reproduction branch. Both physical stages have verified normal APK/data,
owned-package cleanup, display return and custody release. No repeat or new
comparison follows without a further disposition; PLAYER-2 remains open.
The frozen executable/source target is `d11d03e6d02c6c4293b972af9aadb8f639bd1184`;
later documentation revisions do not rebind those binaries. Exact APK, native,
symbol, signer and complete-source identities are in the
[verified package handback](https://github.com/WigglyDon/psx/issues/2#issuecomment-6424370490).
Streams, images, sidecars, input hashes and raw evidence remain private.

## Capture and validation

The existing GS-owner QueueSnapshot/VSync/ReadbackTextureCache/Freeze path owns
the initial state and packet recording. The first coherent GS boundary requests
capture; queue acceptance, successful state freeze, file-open start and codec closure have
separate records. Existing output generations are recorded alongside GS
boundaries and replay packet indices, never treated as the same frame identity.
During recording the existing register/VSync packet is written before its
presentation work, so a presentation fault cannot erase the triggering boundary.
This capture-only ordering change is declared; it creates no new packet or
post-fault GPU operation. Required target readback is retained; permission is rechecked before capture
work and after readback. A known first fault forbids new snapshot/readback work.
Owner retirement only finalizes existing CPU-side codec data.

The dedicated route uses the donor's zstd GS format and omits optional embedded
thumbnail, external screenshot, driver sidecar and ZIP duplication. Raw bytes
are checked before codec input growth and encoded bytes before file writes.
Caps are1024 recorded GS VSyncs and1GiB decoded data; encoded output reserves16MiB
inside the aggregate1GiB envelope for bounded logs/numeric sidecars. Failure,
incomplete recording and codec/file closure are distinct. Packet-boundary EOF
does not by itself prove a complete interval or relevant reproduction.

The Rust streaming validator checks the donor's modern header/version/ranges,
initial-state extent,8192-byte register snapshot, transfer paths/alignment,
FIFO allocation requests, packet framing and finite VSync/packet counts. It
rejects truncation instead of the native reader's permissive last-packet drop.
The pinned version9 freeze extent is425+4194304+84 bytes, with an internal
version word matching the outer header; native defrost must also succeed
before the first packet/hook executes. Host decoding uses the installed zstd
ABI with a64MiB window bound, no subprocess prefetch or decoded output file.
It rejects incomplete trailers, concatenation and trailing bytes without
producing beyond the decoded ceiling. Immutable encoded digest and complete
captured interval are separate checks. Game
state, embedded identifiers, images, private hashes/paths and raw logs stay
in private evidence, outside public fixtures/source archives/releases.

## Operand equivalence

| Boundary | Capture / modified replay | Pinned-runner requirement |
| --- | --- | --- |
| Source | Corrected83fe4e29, preceding seven patches plus declared capture/replay glue | Pinned runner Main/ladder/payload/RenderDoc target replaces libretro Main; matching repaired/instrumented renderer and creator output remain common, explicitly not untouched upstream |
| Guest work | Capture retains live EE/IOP/VU; replay uses actual donor GSDumpReplayer through VMManager | Same immutable dump initial state/packets, one finite pass; live CPU history is intentionally absent |
| Settings/resources | Existing minimal-host configuration, resources,1×GS/normal guest clocks/neutral input | Explicit GLES/OpenGL, matching effective GS/game-fix settings and resources; Auto/Vulkan and inherited defaults refused |
| Graphics | Existing minimal Android context, GLES/window/pbuffer geometry and creator-owned output dependency | Matched route and meaningful equivalent fence creation/validity/output observation; surfaceless completion alone cannot qualify |
| Progress/stop | GS boundary/packet/VSync/output correspondence; first fault wins; all owners joined | Actual initial-state/packet/output progress and equivalent observation required; exit0 is not PASS |
| Intentional differences | Replay removes live guest scheduling/resource history; capture adds required readback/state serialization | Runner frontend/bootstrap/thread/presentation differences must be recorded rather than hidden |

Both consumers must be prepared sufficiently to consume the format before
requesting capture. A parseable dump is only a candidate reproducer. If modified
replay is clean, incomplete, setup-failed or lacks the failing dependency,
the donor arm is suppressed. A clean runner omitting that dependency cannot
show that the original problem is fixed. Neither matched fault implies a
driver cause nor a differing result singles out one patch automatically.

## Retention and build

The modified native build tool uses only affected warm compiler/link recipes
with separate output objects; it preserves frozen/cache operands and checks
the composed source inventory, native exports, independent strip and symbols.
New native/Rust/APK/source bindings must be verified before physical admission.
The pinned runner may compile its necessary target; incompatible defines must
not borrow cached objects silently. Existing workspace locks, growth budgets,
reserve guards and low-expansion corresponding-source route remain required.

The runner's original Android frontend duplicates five JNI/file/sound stubs
already present with compatible `ENABLE_LIBRETRO` core objects. Its bridge uses
those definitions once without discarding the core's other platform stubs.
The fixed shared-context path supplies640×448 output and explicitly selects
OpenGL, restores admitted GS INI values after runner overrides, and sets the
native replayer's remaining-loop count to0 for one pass. Historical runner
command-line/default loop conventions are not trusted as a finite gate.
The common renderer/observer/output changes are intentionally shared to retain
the failing dependency. This compares native runner versus libretro frontend,
bootstrap and scheduling; it is not an untouched-renderer control.

Current qualification is recorded on issue2. This document dispatches no device
operation by itself. Normal APK/data verification, disposable package/input
cleanup, retained private capture verification before removal, Home/display
return and released clients/custody are mandatory at every physical stage.

The runner input adapter preserves the admitted GS settings and applies the
same three effective libretro post-load values: native-resolution upscale,
stretch presentation, and enabled hardware downloads. The saved base INI
precedes those libretro overrides and is therefore insufficient by itself.
Duplicate GS sections/keys or malformed entries refuse preparation.

The runner additionally installs SIGINT/SIGTERM handlers. Its Rust owner saves
and conditionally restores those alongside the existing crash-handler scope;
a newer foreign handler is preserved. Callback storage and native code remain
live until the graphics owners and runner thread have actually joined.

`gs-replay-build` reuses verified warm compiler recipes for the modified library
and the pinned runner target. `xtask gs-replay-apk` and `xtask gs-runner-apk`
package those libraries separately, checking source inventories, exports,
independent stripping, ARM64/ELF and ZIP alignment, resources and the existing
signing certificate. Failed native stages remain evidence. A corresponding
source archive must be completed and independently streamed before admission.
