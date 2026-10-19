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

The libretro checkpoint omits the unused thumbnail: psx restores and renders
the machine directly. This avoids a GS readback and PNG encoding while the VM
is stopped. Rust selects the existing uncompressed ZIP setting and retains
only the validated length-prefixed archive, rather than unused ABI padding.
Machine state remains complete and the existing native ZIP reader accepts
both compressed and stored entries. Device timing is qualified separately.

The observed clean-boot card-format failure also has a bounded diagnostic:
the first 32 PS2 writes and 16 erases report transfer geometry/checksums, without
guest bytes or private paths. This is evidence collection, not a card fix.

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

Original file licenses remain applicable to these modifications. No upstream
acceptance or device gameplay result is implied by this patch.
