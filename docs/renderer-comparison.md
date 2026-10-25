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
