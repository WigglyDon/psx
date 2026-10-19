# OpenBIOS free startup firmware

Upstream: https://github.com/pcsx-redux/nugget
Revision: `08f1b3e1b3289b27fda955f6105fc9e6afd9ffe6`.
License: exact upstream MIT text in `source/LICENSE`; retain per-file notices.
Local source modifications: **none**.

`source/` contains the exact upstream `openbios`, `common`, `shell`, `modplayer`
and `monitor` directories and root `LICENSE`, `common.mk`, `nooverlay.ld`.
The shell is built to satisfy upstream's link inputs; `BOOT_MODE=fast` omits
its execution and the linker discards the unused shell data. No console logo,
boot animation or game is added by psx.

Build flags: `BOOT_MODE=fast OPENBIOS_SHELL=shell BUILD=Release -j4`.
Builder: Linux amd64, upstream compiler image
`ghcr.io/grumpycoders/pcsx-redux-build@sha256:d9ae6cbb23a5d0d98d8c3702cc6f512698ee1670bf8d59b236b2baabe99a66e4`.

`openbios-fast.bin` is a 524,288-byte firmware image, SHA-256
`c13794bc14de130c363786f45710d6e472c092070c0524ddede50f41f948bc45`.
The source inventory SHA-256 is
`407256c55987dcb098fe8dfb25cf2c0d41836be57d14c39ea0a1d266550e089d`.
The inventory algorithm is the sorted relative path, NUL, and each file's
hex SHA-256, hashed together by Rust `xtask`. It includes the exact license.

Run `cargo xtask firmware` to rebuild from a fresh generated tree using Podman
and compare the result byte for byte by SHA-256. The build mounts only that
tree at `/project`, with no container network or extra capabilities. The
normal APK build verifies the bundled image and source inventory; it does
not require recompiling firmware. The matching psx corresponding-source
archive includes this entire tree.

Rust places the free image at `system/openbios.bin` only when that path is
absent. Existing user firmware is preserved, and the donor's configured
regional firmware keeps precedence. The existing pinned core also serializes
BIOS bytes in its machine state, preserving the firmware used by an existing
checkpoint. A restored session need not boot again to adopt new branding.

This uses the firmware's documented fast-boot build mode, rather than trying
to patch unknown firmware addresses, time a cover over game frames or alter
SwanStation internals. Hardware startup/compatibility checks remain distinct
from successful compilation and source reproduction.
