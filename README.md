# psx

A small native PlayStation 1 player from Machine Rhythm.

**Open a local game. Play.**

## Status

The Rust Android host launches an owner-selected PS1 disc on a
stock Fold through the system picker, with hardware-rendered gameplay and
owner-confirmed clear audio. Host tests and ARM64/signature/16 KB alignment
checks pass. The owner confirms simultaneous touch and discrete press haptics.
An ordinary in-game card save has loaded after a fresh process and APK update.
Updated fold/lifecycle acceptance is still being checked; see
[observations](docs/verification.md).
The Rust-rendered interface now includes a persistent game collection,
explicit storage scanning/selection, multi-disc menus, local thumbnails and
controller layout editing with separate picture/control space by default.
Corner resizing, contextual reset and layout persistence have run on the
Fold. The new play surface adds live console-style settings, a top toolbar,
volume popover, speed cycling and layouts for folded landscape. Those latest
toolbar/popover interactions have run on the Fold, including interruption
before the first game frame. Faster-mode sound/performance and the latest
layouts across display postures remain under device acceptance.
The first implementation target is a stock Android foldable.
The product is intended for eventual Google Play and Apple App Store release.
Store acceptance and iOS performance have not been established.

## Direction

- Rust owns the player, host, UI, input, file access, saves and runtime policy.
- [SwanStation](https://github.com/libretro/swanstation) is the initial Android
  core candidate, through its libretro C interface. Its upstream C++ stays
  intact. Cross-platform selection depends on the distribution checks below.
- A persistent local game collection and one play surface, with sensible
  defaults chosen by the application.
- Native execution and the core's existing rendering paths.
- Comfortable simultaneous touch input, stable sound and durable memory-card saves.
- Local game files: scan accessible storage once and choose games to keep.
  Add games repeats the scan; manual browsing is optional. No accounts or uploads.
- Shared Rust player behavior with small Android and iOS platform adapters.
- Clean everyday use: clear file errors, comfortable controls, dependable
  pause/resume and accessible navigation. Development diagnostics stay hidden
  from the ordinary play flow.

The active milestone is [FIRST-PLAYABLE-1 #1](https://github.com/WigglyDon/psx/issues/1).
[Build instructions](docs/build.md) produce the development APK.
[Files and saves](docs/formats.md) describe supported inputs and current limits.
[Architecture](docs/architecture.md) records the small ownership boundary.
[Distribution](docs/distribution.md) records the early store requirements,
including iOS execution without JIT and the borrowed core's licensing.
[AGENTS.md](AGENTS.md) gives the development entry point.

## Source and attribution

Original psx contributions are licensed under **MIT**, as provided
in [LICENSE](LICENSE). Third-party material retains its own copyright,
attribution and license. A distributed player containing SwanStation must
also satisfy the core's applicable GPL obligations; our MIT license does not
relicense the core or exempt the combined distribution.

SwanStation is a DuckStation-derived project maintained upstream. psx does
not claim authorship of those emulation engines. The exact initial core
revision is recorded in [upstream.lock.json](upstream.lock.json); attribution
and incorporation requirements are in [THIRD_PARTY.md](THIRD_PARTY.md).

The core is retained unchanged as a pinned Git submodule. License files from
the core and Rust dependencies are also packaged inside the development APK.
Game images and proprietary firmware are not part of this repository.

