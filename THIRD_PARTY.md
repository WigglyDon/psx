# Third-party source

## SwanStation

- Upstream: https://github.com/libretro/swanstation
- Pinned candidate revision: `e436e9b85374def58d211df7f78880f0f4a917b4`
- Revision URL: https://github.com/libretro/swanstation/commit/e436e9b85374def58d211df7f78880f0f4a917b4
- Role: existing PS1 emulation core, integrated through its libretro C ABI
  at the pinned `vendor/swanstation` Git submodule.
- Lineage: SwanStation is an independent hard fork of DuckStation. Credit the
  SwanStation contributors, the DuckStation authors/contributors and the
  owners named in their retained source notices.
- Upstream root license: GNU General Public License version 3; retain the
  exact upstream text and per-file notices. Do not infer an "or later"
  permission merely from the license document's example application text.
- Exact retained license copy: [SwanStation-GPL-3.0.txt](LICENSES/SwanStation-GPL-3.0.txt).
- License blob SHA-1: `94a9ed024d3859793618152ea559a168bbcbb5e2`.
- License SHA-256: `8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903`.
- Local source modifications: none. CMake's generated, upstream-ignored
  `src/libretro/version.h` is a build output, not an emulation change.

The pinned upstream tree vendors additional dependencies under `dep/`.
The complete tree and notices are retained. The Android target builds core,
common, GLAD, the Vulkan loader, libretro-common, libchdr (including LZMA,
miniz and Zstandard inputs), stb, xxHash, VIXL, the embedded OpenBIOS from
PCSX-Redux (MIT) and the NDK static C++ runtime. Exact per-file terms remain
in those source directories, including embedded header notices where no
standalone license file exists. `cargo xtask notices` copies the upstream
license/notice files into APK assets without rewriting them. This file does not relicense
those dependencies or claim authorship of borrowed work.

The free startup fallback additionally uses unmodified PCSX-Redux OpenBIOS from
`pcsx-redux/nugget` revision `08f1b3e1b3289b27fda955f6105fc9e6afd9ffe6`, built
with its supported `BOOT_MODE=fast` option. Exact source, MIT license, image,
source inventory and reproduction details are retained under
[`vendor/openbios`](vendor/openbios/UPSTREAM.md) and in `upstream.lock.json`.
The APK retains that source's exact license and provenance in its notices.
No new C/C++ firmware logic is authored here; all new host policy remains Rust.
The pinned SwanStation core and its included original OpenBIOS are unchanged.

Google Play and Apple App Store distribution are intended product goals.
SwanStation is not yet cleared here as a donor for both stores. Before that
decision, record how the actual combined distribution satisfies all inherited
terms and store conditions, or obtain applicable additional permissions.
Do not assume marketplace permission granted to a different application
extends to psx. The Rust/C ABI boundary does not remove license obligations.
See [distribution.md](docs/distribution.md).

## Rust Android integration

`android-activity` 0.6.1 supplies the NativeActivity host:
https://github.com/rust-mobile/android-activity

It is MIT OR Apache-2.0; the build selects the MIT option where offered.
Other direct Rust dependencies are `ndk` 0.9.0, JNI, khronos-egl, glow,
android_logger, log, libloading, ab_glyph, libc, serde/serde_json and sha2.
`Cargo.lock` pins all versions and registry checksums. The packaged
`assets/notices/INDEX.txt` lists dependency versions, declared license terms
and source links; retained license files are alongside it. The index also
includes build-only and other-platform Cargo dependencies for completeness.
The build retains Rust toolchain and NDK notices for linked runtime material.

## UI typography

Noto Sans Regular is retained unchanged under SIL Open Font License 1.1,
Copyright 2018 The Noto Project Authors. Its pinned source revision, file
hash and exact upstream license are in [assets/fonts](assets/fonts/SOURCE.md).
The same license and source notice are packaged in the APK. The Rust
`ab_glyph` rasterizer retains its own MIT/Apache notices through the Cargo
dependency notice collection. These font assets are not original MIT work.

## Original contributions

Original psx source and project documentation use MIT under the repository
LICENSE, preserving flexibility for future distribution. Reused material
retains its original ownership and applicable terms. A shipped combination
must satisfy all applicable core and dependency obligations; the original
host's MIT grant does not relicense SwanStation. Generated bindings should
identify their source headers.

