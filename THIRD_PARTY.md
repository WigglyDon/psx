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

Barlow Semi Condensed Medium and fallback Noto Sans Regular are retained
unchanged under SIL Open Font License 1.1, Copyright 2017 The Barlow Project
Authors and Copyright 2018 The Noto Project Authors respectively. Their pinned
source revisions, hashes and exact upstream licenses are in
[assets/fonts](assets/fonts/SOURCE.md).
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


## ARMSX2 PS2 candidate

ARMSX2 derives from PCSX2. Its unmodified source is pinned at
`83fe4e293f71ab5587ef81b06143725bf8c6a30b` in `vendor/armsx2`.
The exact root `COPYING.GPLv3` has SHA-256
`8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903`;
individual files retain their original GPL and other terms. Credit the
PCSX2 and ARMSX2 contributors and each dependency's named authors.
The original tree retains all inherited copyright and dependency notices.
The host adapter changes and their purpose are recorded in
[patches/armsx2](patches/armsx2/README.md); they do not replace core internals.

The resolved Android dependency graph, including nested shaderc sources, is
locked in [revisions.tsv](vendor/armsx2-dependencies/revisions.tsv).
It includes libpng, libjpeg-turbo, Zstandard, LZ4, WebP, SDL3, FreeType/dlg,
PlutoVG/PlutoSVG, curl (without TLS), libpcap, shaderc, glslang, SPIR-V tools
and headers, Abseil, Effcee, GoogleTest and RE2. These names are an inventory,
not a replacement for their exact source notices. The original donor also
retains its in-tree `3rdparty` sources. Android links its NDK's zlib and
static C++ runtime. Matching fonts, shaders and native compatibility resources
come from the same core revision; no floating patch archive is downloaded.

The APK includes original core/dependency/resource notices. The corresponding
source bundle contains the exact donor, integration patch, complete resolved
dependency trees and Rust build recipe. Bundling this candidate adds its GPL
and dependency obligations to the combined distribution; the MIT Rust license
and ABI boundary do not remove them. Android qualification does not establish
stock-iOS execution, App Store rights or marketplace acceptance.
