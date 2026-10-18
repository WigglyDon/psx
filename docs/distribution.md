# Distribution direction

Reviewed 2026-10-18. The goal is a public, free-source native player that can
eventually ship through Google Play and the Apple App Store. Android Fold
gameplay comes first. Store acceptance is not established by this foundation;
recheck current requirements when preparing an actual submission.

## Core decision

SwanStation remains the pinned Android prototype candidate. Before committing
to it for both stores, settle two concrete questions: practical stock-iOS
execution without JIT, and a documented basis for the combined distribution
under the donor's license and the store's terms. Credit and a source link
alone do not answer the second question. Keep the exact inherited licenses.

Our original Rust host and project documentation use MIT. This preserves
options for our work while leaving all borrowed-code obligations intact.
A bundled SwanStation release must satisfy its applicable GPL requirements.

Libretro's own iOS documentation distinguishes its App Store core selection
from sideload builds: it includes cores maintained in-house or with explicit
upstream marketplace approval. That is evidence to verify our own rights,
not a transferable permission or a blanket conclusion about every GPL app.
[Source](https://docs.libretro.com/guides/install-ios/)

If this donor cannot meet the product goal without major core work, prefer
another established donor with suitable rights and measured performance.
Retain the small Rust/libretro boundary; avoid a multi-core product UI.

## Apple route

Apple's App Review Guidelines expressly address retro console emulators in
section 4.7. This permits the category subject to the remaining rules; it
does not grant rights to games, firmware or borrowed source.
[Source](https://developer.apple.com/app-store/review/guidelines/)

Apple restricts executable writable memory through special entitlements.
Plan the normal iOS release around execution without JIT or a debugger. Check
the exact donor build, execution mode, graphics backend and real gameplay
with audio on a physical device. An ARM64 build or Android recompiler result
is insufficient. [Apple runtime security](https://support.apple.com/en-ca/guide/security/sec15bfe098e/web)
and [Libretro iOS guidance](https://docs.libretro.com/guides/install-ios/)

Bundle the core into the signed app. Use public platform APIs and a native
document picker. Keep session behavior, input and saves in shared Rust with
small platform bindings; document any indispensable callback shim. Prove
the actual iOS renderer instead of assuming Android Vulkan means Metal.

## Android route

The current phone/tablet submission requirement is target API 36 or higher;
minimum supported Android version is a separate product choice.
[Source](https://support.google.com/googleplay/android-developer/answer/11926878?hl=en)

Support 16 KB host pages across the Rust host, C++ core and native dependencies.
Verify packaging/ELF alignment and runtime memory mappings. For this emulator,
that includes checking host executable buffers and protection boundaries;
emulated PS1 memory constants are separate. Test the complete runtime in a
16 KB environment before claiming support. A recent NDK alone is not proof.
[Source](https://developer.android.com/guide/practices/page-sizes)

Prepare an Android App Bundle for Play release. Bundle the pinned native core
and update it through app releases; do not add a native-core downloader.
[App Bundles](https://developer.android.com/guide/app-bundle) and
[Play executable-code policy](https://support.google.com/googleplay/android-developer/answer/16559646?hl=en)

Use the system document picker and persist the granted document access when
needed. Resolve companion disc files with explicit user-selected access;
a document grant does not imply sibling-file access. Avoid broad storage
access for a file-picker product.
[Source](https://developer.android.com/training/data-storage/shared/documents-files)

## Product quality

Open game, play, pause and return reliably. Choose technical defaults in the
application, with short actionable messages for missing inputs. Provide
comfortable touch controls and accessible navigation. Keep save durability,
audio interruption handling, display changes and clear recovery in the first
playable scope. Ordinary users should not need developer setup to use a
store release.

Ship original product identity and accurate store screenshots. Include clear
source/license access and a privacy description matching actual behavior.
The product direction is local files with no accounts, analytics or uploads
required for play. Games and proprietary firmware remain user-provided local
inputs. Publishing the project source does not publish those inputs.

## First implementation feasibility finding

The exact pinned source includes `Interpreter` and `CachedInterpreter` CPU
modes. `src/core/cpu_code_cache.cpp` allocates executable code only when
`IsUsingRecompiler()` is true. The candidate stock-iOS route is therefore
the existing cached interpreter, with no JIT privileges. The actual current
hardware host contract is OpenGL ES 3.0 through libretro; an iOS adapter would
need a signed bundled core and a real EAGL presentation check. There is no
implemented Metal adapter. These are source findings and an integration
candidate, not an iOS execution or performance result.

This implementation session has no authorized Mac/Xcode and physical-iOS
test environment. Non-JIT iOS gameplay/audio and presentation remain
unverified. No full iOS port, new CPU engine or graphics-backend rewrite is
authorized by that missing evidence.

The root core license and inherited dependency files are retained unchanged
in the submodule. The development APK also carries original license files
under `assets/notices`, including OpenBIOS (MIT), VIXL, libchdr and Rust
dependency notices. GPL obligations apply to the bundled combination;
publication of the MIT host does not settle store terms. No additional
marketplace permission for this application has been obtained. Accordingly
SwanStation is still a prototype candidate for the two-store product.

Rechecked primary store/platform guidance on 2026-10-18: Google requires
target API 36 for new phone apps; Android requires both native alignment and
runtime testing for 16 KB support. Apple 4.7 addresses emulator apps, while
Libretro documents its own marketplace-specific upstream approvals. The
linked guidance above supports keeping these as explicit release gates.
