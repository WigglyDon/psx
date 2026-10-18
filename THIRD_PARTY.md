# Third-party source

## SwanStation

- Upstream: https://github.com/libretro/swanstation
- Pinned candidate revision: `e436e9b85374def58d211df7f78880f0f4a917b4`
- Revision URL: https://github.com/libretro/swanstation/commit/e436e9b85374def58d211df7f78880f0f4a917b4
- Planned role: existing PS1 emulation core, to be integrated through its
  libretro C ABI.
- Lineage: SwanStation is an independent hard fork of DuckStation. Credit the
  SwanStation contributors, the DuckStation authors/contributors and the
  owners named in their retained source notices.
- Upstream root license: GNU General Public License version 3; retain the
  exact upstream text and per-file notices. Do not infer an "or later"
  permission merely from the license document's example application text.
- Exact retained license copy: [SwanStation-GPL-3.0.txt](LICENSES/SwanStation-GPL-3.0.txt).
- License blob SHA-1: `94a9ed024d3859793618152ea559a168bbcbb5e2`.
- License SHA-256: `8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903`.
- Local source modifications at foundation: none; the core implementation
  has not yet been incorporated.

The pinned upstream tree vendors additional dependencies under `dep/`.
When incorporating the tree, preserve their license/copyright files and
record which inputs enter the delivered build. This file does not relicense
those dependencies or claim authorship of borrowed work.

Google Play and Apple App Store distribution are intended product goals.
SwanStation is not yet cleared here as a donor for both stores. Before that
decision, record how the actual combined distribution satisfies all inherited
terms and store conditions, or obtain applicable additional permissions.
Do not assume marketplace permission granted to a different application
extends to psx. The Rust/C ABI boundary does not remove license obligations.
See [distribution.md](docs/distribution.md).

## Rust Android integration

`android-activity` is the current host candidate:
https://github.com/rust-mobile/android-activity

It is not yet a declared or incorporated dependency in this foundation.
The implementation must select and pin the actual Rust/Android dependencies
and retain their applicable licenses before distribution.

## Original contributions

Original psx source and project documentation use MIT under the repository
LICENSE, preserving flexibility for future distribution. Reused material
retains its original ownership and applicable terms. A shipped combination
must satisfy all applicable core and dependency obligations; the original
host's MIT grant does not relicense SwanStation. Generated bindings should
identify their source headers.

