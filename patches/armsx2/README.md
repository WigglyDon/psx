# ARMSX2 adapter patch

Apply to exactly `83fe4e293f71ab5587ef81b06143725bf8c6a30b`.
The original submodule is unchanged. `cargo xtask ps2` stages it and applies
`0001-host-pause-and-card-flush.patch` before building.

The four exported adapter functions explicitly use C linkage; the donor's
version script exports only `retro_*`. Package verification requires each
defined public C symbol, catching a C++-mangled/hidden adapter before install.

The patch forwards four narrow operations through the donor's existing CPU
queue: readiness, pause, file-card flush and nominal playback speed. It releases
the GL/Vulkan frame handoff before a blocking pause, so the native CPU and GS
owners can settle without another frontend frame. The flush returns actual
`fflush` failure for regular file cards. Rust then syncs those files and their
directory before binding a checkpoint to their digest. Folder-card durability
is not supported by this host. Native thread scheduling, JIT and emulation
internals are unchanged.

Time-scale updates release the frame handoff before the blocking CPU callback
and update only the existing nominal scalar/limiter deadline, without a full
VM settings reload or pause/resume transition. Zero in this C ABI selects the
donor's unlimited limiter; Rust handles zero-time pause separately through
`retro_psx_set_paused`. Rust validates finite targets and coalesces slider
changes to at most 20 per second, pacing audio/frontend calls at the applied
rate. The native callback retains its current running/paused state and restores
the frame handoff only when running. Guest CPU/device clocks are unchanged.

The libretro checkpoint omits the unused thumbnail: psx restores and renders
the machine directly. This avoids a GS readback and PNG encoding while the VM
is stopped. Rust selects the existing uncompressed ZIP setting and retains
only the validated length-prefixed archive, rather than unused ABI padding.
Machine state remains complete and the existing native ZIP reader accepts
both compressed and stored entries. Device timing is qualified separately.

The observed clean-boot card-format failure also has a bounded diagnostic:
the first 32 PS2 writes and 16 erases report transfer geometry/checksums, without
guest bytes or private paths. This is evidence collection, not a card fix.
After both fresh JIT and IOP-interpreter boots failed before either trace
appeared, the next diagnostic records at most four occurrences of each SIO2
card opcode per process: port/slot, presence, auto-eject counter, terminator
and transfer lengths. It records no guest/authentication bytes. This distinguishes
probe/auth/setup failure from file writes without an unbounded protocol log.
Changing this native patch changes its strict checkpoint binding; an older PS2
snapshot is retained behind the explicit restart guard, with current cards kept.

Its existing `OnPerformanceMetricsUpdated` callback also forwards a five-float
completed sample (VPS, speed percent, average/minimum/maximum frame time) via
the private `0x20000` environment command. Rust copies it under the output
mutex; it never reads those native globals from the frontend thread. Frontend
call rate is not reported as achieved PS2 emulation speed.

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
other unsafe-setting diagnostics remain. Neither change touches emulated state,
card protocol or serialization. Rust permits only the exact prior `878bdd9e…`
patch to resume into this exact `92542cb6…` patch, retaining other input/card/
payload checks. A future native edit disables that explicit exception. New
checkpoint writes always record the complete new patch identity.

Original file licenses remain applicable to these modifications. No upstream
acceptance or device gameplay result is implied by this patch.
