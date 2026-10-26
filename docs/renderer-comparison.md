# Full-renderer comparison preparation

The implementation follows [Master6400483784](https://github.com/WigglyDon/psx/issues/2#issuecomment-6400483784), after preparation under Master6398308448 and the single
common-result run of application/source `8e61eb2ccdf364cc4aa80e33dfc0796021c9dac1`.
The new dispatch authorizes a conditional comparison after qualified bindings
and fresh custody; no device operation follows from this document alone. The stream-wait, output-sync ownership
and common-result observational routes are closed. Preserve their repairs and
frozen artifacts. Do not append another output-fence variant.

## Question and smallest useful reference

Does the same complete donor renderer fail when driven by an independent,
minimal Rust Android libretro host, without the player's UI, compositing,
collection/session machinery and checkpoint worker?

The separate development-only NativeActivity harness uses the exact
frozen libarmsx2 bytes from 8e61, rather than rebuilding or changing the donor.
The reference still boots the real guest through the full EE/IOP/VU/GS path.
It is not a generated GL probe or a replacement product architecture. Keep the
current creator-owned/no-game-image-reader behavior byte-identical in both
hosts; there is no additional sync-ownership policy. The contrast is the
surrounding application, not a new producer/consumer protocol.

The independent Rust entrypoint supplies only required libretro callbacks,
an EGL bootstrap context/surface, paced `retro_run`, neutral input, bounded
audio consumption, scratch file roots, existing observer callbacks and the
existing owner-aware stop/join contract. It creates no player UI, text,
controller geometry or presentation shaders, and no checkpoint/import/runtime
reuse worker. Retain necessary platform callback glue only. Use a verified absent
task-owned package and an isolated private progress root; never migrate or clear
the normal player's data. No selector appears in ordinary builds.

Source anchors at pinned donor `83fe4e293f71ab5587ef81b06143725bf8c6a30b`:

- `pcsx2-libretro/Main.cpp`: `retro_load_game` selects the real boot filename;
  hardware-context reset captures the frontend context; `retro_run` consumes
  actual GS output. Use the same seven-patch stack and frozen library.
- `pcsx2/GS/Renderers/OpenGL/GSDeviceOGL.cpp`: `DoRenderHW`, `PresentRect`,
  `RenderImGui` and `EndPresent` remain full donor execution. Removing the
  player's UI does not remove the donor overlay or guest rendering.
- `pcsx2/GS/Renderers/OpenGL/GLLibretro.cpp`: the same captured EGL share group,
  producer context, pacing and metadata acknowledgement remain in force.
- Product `crates/player/src/ps2.rs::environment` and
  `crates/android/src/android/graphics.rs::Graphics` identify the callback,
  framebuffer and platform answers that must be matched, rather than silently
  inheriting another frontend's defaults.

## Equivalence gate before calling it a comparison

| Boundary | Implementation and evidence | Difference/limit |
| --- | --- | --- |
| Entry/service | `crates/android/src/minimal.rs`, selected only by build-script `PSX_MINIMAL_RENDERER_HOST=1`; ordinary `android` module is excluded | Own package/NativeActivity, lifecycle loop and scheduling/resource pressure |
| Native runtime | `crates/player/src/minimal_host.rs::Host`; dynamically loads exact frozen DSO; no `ps2::Core`, engine/session/runtime or checkpoint owner | The instrumented donor, seven patches, adapter and platform remain common |
| Callbacks/options | Independent implementations; fixture calls actual frozen `ps2.rs::environment`, comparing accepted/refused answers, pointers, hardware negotiation and all explicit Android overrides | Test-only hook follows the byte-identical frozen player implementation; source hashes pin that prefix and its graphics bootstrap. Linux fixture normalizes only the Android performance-hint value |
| Frame/input/time | Atomic coalesced notification, owner wake,8ms fallback after readiness, neutral digital/analog input; ready→pause→1×→resume | No touch/UI interpretation, audio output device or checkpoint worker. Native guest clocks and audio generation remain active |
| Graphics | Same RGBA8/window+pbuffer/GLES3 selector and client-version request; valid1024² RGBA8/depth24-stencil8 target, actual proc lookup, normal Android window/swap | Only constant-color window clear remains. No game-image copy, UI shaders, fonts, controls, vertex buffers or compositing. The1² pbuffer is bootstrap-only |
| Actual selection | Captures selected EGL configuration attributes, actual context client version/GLES version, surface geometry and host page. Executes the frozen selector again on the same display as a preboot equality witness | This is query/source equivalence, not inspection of another process's context or a claim of identical dynamic addresses. No additional context, GL error poll, readback, fence or wait is added by the witness |
| Error/retirement | Existing observer/output ownership and native-signal/failure contracts; bounded frontend fatal drain before/after minimal presentation; detach notifications→pause/unload/join→destroy→deinit→clear storage | Existing observer writer, ABI, signal helper and atomic export remain shared. Required FBO objects are deleted only before a known fatal/context loss; actual context destruction is separate |
| Files | New task-owned package, independent cards/cache/firmware-sidecar roots; packet supplies frozen resources and matching qualification INI | No product library/profile/resume state is read or migrated into the harness |

The harness never instantiates the product Android `Player`, `Graphics::Objects`,
text/controller drawing, audio device, collection/import/session layer or
save-state worker. Linking the existing player crate for narrow helpers is not
evidence of independence by itself; the separate entrypoint, actual call graph,
excluded module and absent product JNI exports establish the removed boundary.

Bind the frozen native library, symbols, donor revision and seven patch hashes
independently in both APKs. Match effective renderer/capabilities, core options,
firmware/content bytes, fresh boot/no state, neutral input, game/output geometry,
EGL client API/config/context attributes, cadence and guest clocks. Record
required environment answers, returned GL dispatch and actual owner/context
selection in a bounded build-only fixture. An unsupported callback or different
effective configuration is an equivalence failure, not a successful reference.

Use the same baseline host surface geometry and request the same output target;
do not substitute a tiny pbuffer and claim equivalent workloads. Scratch cards,
firmware sidecars, caches and input scripts are independent and matched. No new
pixel capture/readback, render option, fence or wait is needed. Validate callback
negotiation, startup/stop, fatal precedence, real native dispatch and missing
dependency refusal off-device before requesting hardware.

The separation is deliberately limited. A reference failure would retain common
donor/libretro code, EGL bootstrap requirements and platform/driver behavior as
candidates; it would not establish a driver cause or reproduce a fully native
donor frontend. A reference success with a failing matched player would implicate
the removed application integration or its scheduling/resource pressure, without
identifying a particular UI draw or proving sustained gameplay. Timing and
allocation changes are part of that contrast and must be reported. Existing
observations at differing guest progress do not replace a freshly admitted
matched comparison.

## Feasibility and cost

The exact native library, unstripped symbols, source archive, NDK, Rust target and
minimal callback packaging route already exist. This requires a small independent
Rust entrypoint and build-only packaging target; no native dependency rebuild,
expanded source bundle or second live runtime is required. The executable
implementation and targeted fixtures are in this repository; build/device
qualification is reported separately on #2. Budget2GiB peak for cached Rust
outputs, wrapper/symbol/package verification and bounded evidence, with a
separate0.5GiB streaming-source phase and the existing12GiB reserve. This is an estimate,
not measured growth or a storage grant; preflight the actual build plan.

The pinned `pcsx2-gsrunner` is a genuine alternative full-renderer executable,
but its command-line validation rejects ordinary discs and requires a GS dump.
No source-matched failing dump is retained for this reproduction. Its Android
surfaceless route enables `GLContext::AllowHeadless` and may use `GSHeadless`
presentation instead of libretro publication; these introduce additional
workload/context differences. Building/running it now would neither test the
same fresh disc boot nor preserve the existing output contract. Obtaining a
private failing dump and qualifying replay in both hosts is a separate,
larger capture/build task. Do not substitute it without a concrete disposition.

## Input route and conditional execution

The independent package has no storage permission or inherited URI grant.
The admitted operator may copy exactly the previously authorized selected game
with `run-as` from its existing owner into the verified-new diagnostic package,
through a device-local pipe. This is not automatic cross-package access by the
application. Declare at most4GiB transient phone payload, check free space with
at least1GiB remaining margin, verify source size/hash and destination hash,
and remove that owned copy with the diagnostic package on return. The game
never needs another laptop copy. No library scan, broad grant, normal-progress
migration or other media is part of this route. Firmware and sidecars use the
existing private authorized input packet; frozen resources/notices remain bundled.
The exact private input identities and current access/capacity checks stay local.

The development operator stages the packet through `adb shell -T`, with the
payload opened only after the existing identity/lease preflight. It explicitly
prepares the app's `files` directory, refuses an existing or redirected scratch
root, and requires the expected manifest, remote success status and exact
completion marker before checking individual input hashes. A missing manifest
ends preparation before any guest launch. The narrow Rust command/completion
helper and generated host fixtures are in `xtask/src/android_packet.rs`.

Legacy `exec-in` is insufficient for this check: the
[ADB client implementation](https://android.googlesource.com/platform/packages/modules/adb/+/refs/heads/main/client/commandline.cpp)
copies input to its raw connection and returns without collecting the remote
command's exit status or stderr. Host fixtures separately reproduce extraction
failure when the destination directory is absent. These prove staging/reporting
defects, not why a particular phone extraction failed or any graphics root cause.
The failed zero-launch contact and its return remain separate evidence; neither
this tool correction nor passing fixtures authorize another physical trial.
Tool source/binary bindings are separate from the unchanged diagnostic APK and
its complete corresponding-source archive.

`minimal_host::qualification::reference` is the control-arm gate. It requires
the source/engine binding, matched window/EGL/page/settings, successful hardware
negotiation, ready/normal-speed transition, generated audio, at least20s bounded
guest interval, at least512 produced outputs and480 actual frontend notifications,
hardware callbacks/services/acknowledgements, real render/composition/donor-overlay
phase visits, no recorded fault, and complete native/frontend retirement. It
does not require deliberately removed UI/copy/presentation observer stages.
A successful process exit, missing export or merely waiting20s cannot pass.

The bounded runner admits at most two sequential fresh boots: reference first,
each capped externally at30s with separate installation/rollback/return reserve.
Any reference fault, missing equivalence/progress or unsettled recovery suppresses
the frozen-player arm. Only a clean qualified reference permits the frozen8e61
player as a fresh contemporaneous control. The control retains the same native
progress/boot observations; a new early/unrelated fault is inconclusive, not a
matching reproduction. Stop immediately on required dependency/fence/fatal/context/
submission warning, background/window loss, instability or owner request. No
retry, other game, restored state, pixel capture or Fold operation follows.

Fresh resource-discovery/Android-device/current-custodian admission remains
mandatory, including this exact input route/budget and conditional player
replacement. Verify actual normal APK/data before and after any replacement;
preserve normal progress and original Home, remove only the newly owned package/
scratch, retire clients, verify booted/screen OFF and released custody. A faulted
reference closes this integration branch without automatically adding another
UI variant or inferring a driver cause. #2 remains open for Master acceptance.

## Observed minimal-host result and next common comparison

The single admitted reference boot on 2026-10-26 used application source
`e9908161fc097189093d8531cb27104f2ac69d11`, the frozen native binary above,
and the corrected development operator. The independently staged input hashes
matched. Actual window was1080×2280 with matched edge-to-edge setup, GLES3.2,
4KiB pages and the same selected EGL attributes. All11 effective option answers
matched the frozen settings; initialization, load, reset, ready and normal-speed
transitions were observed. Input was queried and audio generated/consumed without
rejections. The audio output device and successful platform clock hints were
not established as equal: output was deliberately absent, and the native log
reported its performance-hint session request failed.

The first retained failure was a non-null output fence rejected by both existing
validity routes on its creating graphics owner, before flush/publication:
stage16, producer463 at10.790506s on the observer clock. Recorded dispatch and
context comparisons matched; sampled GL errors were zero. The host had462
hardware callbacks/notifications/metadata acknowledgements and404,352 generated
audio frames. Real GS rendering, composition and donor overlay phases executed.
Required download/readback remained inactive. The external runner retired the
arm12.083352s after launch and suppressed the full-player control.

This is a faulted reference, not the clean20s/512-output qualification. It reached
the same failure class near the historical player's producer464, without the
ordinary UI/compositing/session/checkpoint layers. Those removed layers are not
necessary for this reproduction; the modified native engine/adapter, shared
observer, bootstrap and platform remain candidates. It does not isolate a driver
or particular native call, prove upstream failure or establish gameplay/saving.
The contemporaneous full-player arm did not run and historical evidence does not
replace it. Native/frontend context destruction and complete creator retirement
were recorded; this run recorded no common GL OOM, including retirement.

Close this integration branch. The next distinct common-renderer comparison
to prepare for Master is replay of the same private failing GS command stream
through the pinned donor's existing `pcsx2-gsrunner` path and the modified native
path, rather than another product-shell/ownership/observer variation. Immutable
donor83fe4e29 source checks establish these concrete boundaries:

- `pcsx2-gsrunner/Main.cpp` validates `VMManager::IsGSDumpFileName`, then uses
  `CPUThreadInitialize`, `GSDumpReplayer::SetIsDumpRunner` and
  `VMManager::Initialize`. It is not a disc-boot substitute.
- Its OpenGL surfaceless path opts into `GLContext::AllowHeadless`; windowed
  setup and `GSHeadless` presentation must be audited before treating either
  context/presentation route as equivalent. Both operands must use a declared
  matched route and retain the real GS command stream, renderer settings and
  native output/error evidence. Replayed GS work alone does not retain the live
  EE/IOP/VU, libretro scheduling or output-fence contract of the disc boot.
- No source-matched failing GS dump is retained. A private capture, bounded
  storage/output budget, ARM64 runner build/packaging and actual replay support
  need separate concrete scope and admission. A donor-clean/modified-fault result
  would localize that changed set only; both-fault would retain common renderer,
  capture/bootstrap/platform candidates; no reproduction would leave the removed
  guest/scheduling/publication differences unresolved.

This is off-device preparation, not authority to capture, rebuild the frozen
engine, run another phone trial or replace the product. Preserve all frozen
operands, symbols, private evidence and normal progress while Master chooses
the next native/common boundary. No speculative production workaround follows.

## Coherent mapping repair and corrected minimal-host result

Master6406832718 deferred GS capture/replay to correct the active persistent
upload contract first. Exact seven-patch composition confirmed coherent storage
with noncoherent mapping access and a skip-flush Unmap branch. Repair58a18f2 adds
`GL_MAP_COHERENT_BIT` to coherent mapping access. The noncoherent explicit-flush
branch, synchronization and earlier failure handling remain intact. Rust-owned
fixtures execute retained Create/Map/Unmap through EXT and core/ARB dispatch:
the predecessor fails the coherent access assertion, both corrected patch
compositions pass, and exact explicit-flush ranges remain covered. This proves
the source contract, not actual GPU cache visibility.

The one admitted corrected minimal-host boot used source
`a1fc211112801c27132e53e62772034ea4d5bcec`, APK SHA256
`57ac4549f7a8fdae3ea246859a981715c665c53053b59b46068b1d1a3f12bb39`
and native SHA256
`b6bff788bc7676d67417beb89c5880590cd47038f6d3d3b2d235220addf5d9cf`.
Exact symbols, source inventory and streamed complete-source bindings are in
issue2 prerequisite report6407942129. This document's later commit is not the
executed APK/source target. Frozen preceding operands remain unchanged.

At producer463/10.991186s, stage16 again recorded a non-null output fence invalid
on both creator-side validity routes before flush/publication. Recorded dispatch
and context comparisons matched; sampled errors were zero. Seven persistent
BufferStorage selections and946 map/unmap operations were recorded, with coherent
Unmap active and zero stream waits. Real GS rendering/composition/overlay ran;
the host received461 hardware callbacks and462 notifications. Two bounded numeric
exports retained the final ledger. The external stop completed13.391298s after
launch. The unchanged clean20s/512-output gate failed; no player arm ran.

The retained application log also contains two submission deadlock warnings.
No common error or OOM was recorded, including retirement; download/readback
remained inactive. Native performance-hint session creation failed as in the
preceding minimal run. Audio output remained intentionally absent; equal dynamic
clock behavior, gameplay, sound, saving and sustained performance are not claimed.
All463 fences were accounted for (462 creator retirements and one invalid-name
release), with zero live/untracked/frontend sync operations, native joined, and
both graphics contexts destroyed.

Return was verified at2026-10-26T04:14:36Z: diagnostic package/input removed,
actual normal APK and complete normal-data digests unchanged, Home preserved,
normal player stopped, screen OFF and custody released without pending recovery.
Private logs, workload and custody receipts remain local. The Fold was untouched.

The source repair stands independently; this reproduction does not establish
its historical causal role or a driver diagnosis. Close this corrected minimal
run without another ownership/UI/wait variant. Retain the native/common runner
boundaries above for a concrete next Master disposition; capture/replay has not
been authorized or executed. Issue2 remains open for Master acceptance.
