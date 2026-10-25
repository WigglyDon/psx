# Full-renderer comparison preparation

This is off-device preparation under Master6398308448, following the single
common-result run of application/source `8e61eb2ccdf364cc4aa80e33dfc0796021c9dac1`.
It authorizes no additional physical run. The stream-wait, output-sync ownership
and common-result observational routes are closed. Preserve their repairs and
frozen artifacts. Do not append another output-fence variant.

## Question and smallest useful reference

Does the same complete donor renderer fail when driven by an independent,
minimal Rust Android libretro host, without the player's UI, compositing,
collection/session machinery and checkpoint worker?

Prepare a separate development-only NativeActivity harness, using the exact
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
expanded source bundle or second runtime is required. That harness is not
implemented or qualified yet. Budget one host implementation/validation
increment, with a provisional 1 GiB peak workspace admission for the wrapper,
symbols and packaging, plus the existing 12 GiB reserve. This is an estimate,
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

A new physical comparison needs its own Master disposition, exact built
bindings and fresh current custodian admission. It is not authorized by the
completed one-run increment. Preserve scratch isolation, first-fault stopping,
external bounds, exact rollback and verified return; keep the Fold excluded.
