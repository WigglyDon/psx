# Android development build

Run the [storage preflight and retention procedure](storage.md) before large
builds, backups or experiments. Writing xtask commands serialize project use
and check shared-host capacity. Completed source bundles are reused, and their
verified expanded stages are retired. Unfinished output is preserved for review.

Clone with `git clone --recurse-submodules https://github.com/WigglyDon/psx`.
For an existing checkout, run `git submodule update --init --recursive`.
The submodules must be the exact revisions in `upstream.lock.json`; the build
rejects a different revision, modified tracked core source or license hash.

The development build uses Rust 1.96.1 (pinned in `rust-toolchain.toml`), the
`aarch64-linux-android` Rust target, Android NDK 30.0.16248370, Android build
tools 37.0.0, compile platform `android-37.0`, Java with the `javac` module,
CMake 4.3, Ninja and the standard `zip`/`unzip` utilities. Android 9/API 28
is the minimum OS; the manifest targets API 36. Compile SDK and target SDK
are separate. The first build needs ordinary crates.io access; `Cargo.lock`
pins the resolved packages. SDK, NDK and JDK are build tools, not copied
from another application's source.

A clean PS2 dependency build also needs Flex, Bison and GNU M4. Put their
executables on `PATH`. With a local tool prefix, set `M4` to its absolute
executable and `BISON_PKGDATADIR` to that prefix's `share/bison` directory;
Bison can otherwise try its compiled-in system paths. These are host build
tools, not libraries shipped in the APK. `cargo run --release -p xtask -- apk`
uses the same build recipe with an optimized packager, which reduces the cost
of source-manifest hashing after restoring a cache.

```sh
export ANDROID_SDK_ROOT=/path/to/android-sdk
export ANDROID_NDK_HOME="$ANDROID_SDK_ROOT/ndk/30.0.16248370"
export ANDROID_BUILD_TOOLS=37.0.0
export ANDROID_PLATFORM=android-37.0
cargo fetch --locked
cargo test --workspace --locked
cargo xtask apk
```

The result is `build/psx-development.apk`. The default development build
includes the all-files scan permission. `PSX_SCOPED_STORAGE=1 cargo xtask apk`
omits that declaration and compiles without the storage-scan feature, keeping
document import. The build-generated manifest is ignored. This scoped variant
is a packaging route, not store approval; broader scanning still needs its
own policy basis. `cargo xtask verify` repeats its
signature, ZIP alignment, AArch64 ELF alignment and required-entry checks.
Raw build logs and APKs stay outside Git. Development signing material stays
under ignored `.local/signing/`; retain that key to update an installation
without losing its data. The development key is not a store signing key.

The fixed [PS2 observer](graphics-observer.md) uses `PSX_GRAPHICS_OBSERVER=1`
for an explicit diagnostic build and isolated scratch progress. It exposes no
player option and is excluded from ordinary builds. A candidate build grants
no device access; installation needs separate admission and actual rollback.
The matched reference additionally sets
`PSX_GRAPHICS_OBSERVER_MODE=no-game-image-reader`; both arms retain the same
native binary and require mode-matched scratch markers. Retain each APK and
unstripped libraries before building the other arm, then verify their exact
source/patch/mode/signature bindings.

The separately authorized creator-owned reference sets
`PSX_GRAPHICS_OBSERVER_MODE=producer-local-output-sync` with the observer flag.
It adds native patches0006/0007 and the matching Rust feature; the APK manifest binds
the ownership/result patches and Rust policies. Historical normal/no-reader binaries
remain frozen and are not controls to rerun without a new dispatch. This new
reference retains the real GS workload but transfers only metadata to the frontend.
Use its exact scratch marker, preserve symbols and verify complete matching
source before any separately admitted single run.

`xtask` is Rust. It builds the unmodified PS1 upstream CMake target, statically
links its C++ runtime, builds the Rust NativeActivity library, generates the
callback forwarding class described below, compiles it with Android's D8,
packages licenses, aligns the APK and signs it. No Gradle application layer,
managed UI renderer or native-core downloader is involved.

The native libraries use ELF LOAD alignment of at least 16 KB; the APK is
checked with `zipalign -c -P 16`. These checks establish alignment, not
16 KB runtime qualification. The donor's JIT guard and host-page constants
assume 4 KB. The PS1 host selects its existing cached interpreter on other
page sizes, preserving a path to testing without changing core internals.
Actual speed and complete memory mapping behavior on 16 KB Android remain
unverified until that environment runs the application.

## Free startup firmware

The normal build verifies and embeds the pinned 512 KB OpenBIOS fast-boot image
in the Rust host. Its unmodified upstream sources and exact MIT notice are in
`vendor/openbios`, with revision, image/source hashes and compiler image recorded
in `upstream.lock.json` and [UPSTREAM.md](../vendor/openbios/UPSTREAM.md).
It uses the existing `BOOT_MODE=fast` option to omit the shell/intro. Supplied
firmware and the pinned SwanStation sources are preserved.

On Linux amd64 with rootless Podman, `cargo xtask firmware` copies only the
vendored public firmware sources into a fresh marked `build/openbios-work`
directory, compiles with the pinned upstream compiler image and compares the
result against the bundled SHA-256. The compiler cache is private under
`.local/firmware-build-storage`; the compiler container has no network, root
filesystem writes or extra capabilities. Its first use can fetch the exact
compiler image. A clean rebuild has reproduced the bundled image. This is a
source reproduction result; the new startup still requires its hardware check.

## Android callback shims

`android-activity` supplies the NativeActivity lifecycle and input callbacks,
but Android's activity-result and audio-focus-listener
callbacks are Java virtual methods/interfaces. The build generates a small
`PsxActivity` class, extending NativeActivity and forwarding those signatures
to exported Rust JNI functions. It contains no picker, UI, file, save, input,
audio or emulation policy. All those decisions and SDK calls are Rust.
Its static `System.loadLibrary("psx")` associates the native library with the
class loader so Java can resolve those exported callbacks; NativeActivity's
own native loading does not establish that JNI association.
An applied-window-insets listener similarly delegates the platform's default
handling and forwards only a change notification to Rust. NativeActivity does
not expose that callback directly; Rust owns visibility/bounds queries, toolbar
clearance, layout and re-hide decisions. The bounded polling fallback remains.
This is the minimal platform callback exception allowed by `AGENTS.md`.

Android also requires a Service subclass for optional background playback.
The generated `PlaybackService` forwards start/destroy callbacks into Rust and
is unbound; notification construction, pause actions, permission handling and
all playback decisions remain Rust. It never owns or calls the core. The
manifest declares the media-playback foreground-service permissions and
notification permission. The service starts only for enabled, active play and
does not restart itself after termination. Background play is off by default.

The host explicitly links EGL and rejects unresolved native symbols at link
time. APK verification also checks its `libEGL.so` dependency. The Android
adapter accounts for system bars and display-cutout insets in its Rust layout.

The pinned `ndk` 0.9 audio wrapper reports positive buffer-size results as
errors. The Rust adapter accepts that API's positive actual frame count,
while retaining negative failures, matching the
[AAudio contract](https://developer.android.com/ndk/reference/group/audio).

## Device verification

Build before acquiring the device. Follow the current installed stock-device
procedure, its private target binding, standing preferences and exclusive
transaction. Do not select an arbitrary first ADB device or reinstall by
uninstalling an existing app. Install this APK as `com.machinerhythm.psx` and
preserve existing data and the signing key across updates.

Use Add games to scan storage, select candidates and retain the collection.
Grant shared-storage access through Android's own settings screen. Optional
Browse files imports a private copy, independent of later URI grants. Use
the scope and acceptance observations in
[FIRST-PLAYABLE-1](https://github.com/WigglyDon/psx/issues/1). Report actual
gameplay, audible output, simultaneous input, card saving after process
restart and fold/lifecycle transitions separately from host checks. The
`psx` log tag emits bounded ten-second playback counters outside the normal
UI. Keep raw device logs and media private.

An AAB/store package and physical iOS check are later delivery work; this APK
does not establish either store's acceptance.

## Narrow streaming source update

`source-update` is a Linux-only development host utility for the reviewed
`09e27cd` → `123e831` source increment. It uses the installed BSD-licensed
[libarchive 3 ABI](https://github.com/libarchive/libarchive/tree/v3.8.7), plus
the existing Rust dependencies, Git and gzip; its focused fixtures use GNU tar.
It requires `libarchive.so.13`, which is a host dependency and is not packaged
in Android. No new archive crate or dependency source expansion is needed.

After the normal storage check and coordinated slot:

```sh
cargo build --release --locked -p xtask --bin source-update
cargo test --locked -p xtask --bin source-update
target/release/source-update audit
target/release/source-update retire
# Commit/review the utility before canonical archive publication.
target/release/source-update update
target/release/source-update verify
```

This utility and its executable fixtures require focused runtime validation
before archive use; type checking alone is insufficient. Its archive target is
the fixed `123e831` commit, not the packaging utility's later revision. The
target's existing build recipes, full dependencies, resources and licenses
remain self-contained. The receipt separately records the utility revision,
predecessor and target hashes and complete-readback result. Frozen APKs and
symbols are reused without being relabeled or rebuilt. An interrupted output
stays explicitly incomplete; existing output is not silently overwritten.

For an admitted independent ext4 workspace, set `PSX_SOURCE_WORKSPACE` to its
existing private project directory, `PSX_SOURCE_VOLUME_UUID` and
`PSX_SOURCE_VOLUME_MOUNT` to the qualified binding, and
`PSX_SOURCE_CAPACITY_LEASE` to the custodian's existing external lock. Supply all
four or none. The tool acquires that lease, checks UUID/type/mount and private
ownership, and refuses missing or changed storage without an internal fallback.
Git and predecessor inputs stay anchored to the source checkout; new archives
and receipts use the admitted workspace. Growth checks measure that filesystem,
retaining the shared 12 GiB floor. Internal Btrfs policy is unchanged.

Compiler targets, `TMPDIR`, test fixtures and logs must also use the admitted
project directory, under the same capacity lease for each writing phase. The
tool itself owns that lease while its archive/audit operation runs; do not wrap
it in a second acquisition of the same lock. Cached compiler dependencies can
be read in place without rebuilding the accepted APK/native inputs. The actual
operator recipe and private volume binding belong to its local admission,
not public source. Normal internal commands above require internal admission.
When both the residual-stage retirement and source update are admitted, `finish`
performs those two existing operations under one lease and one predecessor
validation. Unmatched stage entries remain preserved.

## Release source bundle

From a clean, reviewed Git checkout, run `cargo xtask source`. This exports
the committed project and exact pinned SwanStation tree, includes the locked
Rust dependencies with their original notices, and writes
`build/psx-source-<revision>.tar.gz` plus its SHA-256 file. It exports Git
objects rather than copying the working directory; ignored games, firmware,
cards, captures, signing keys and local logs are excluded.

Extract the archive, enter its directory and follow the build steps above.
Its Cargo configuration uses the included Rust dependency sources, allowing
`cargo test --workspace --locked --offline` without downloading crates.
The Android SDK/NDK, Rust toolchain, JDK and other build tools are still
required separately. Without Git metadata, the build verifies the recorded
core inventory and exact upstream license instead of querying a submodule.
Each developer generates their own private development signing key.

Pair any distributed APK with the source bundle for its exact project
revision, required notices and build instructions. Source packaging is one
part of release preparation; it does not settle the store/distribution
questions in [distribution.md](distribution.md).

## PLAYER-2 engine build

`cargo xtask ps2` builds the pinned ARMSX2 libretro ARM64 candidate and its
locked source dependencies. `cargo xtask apk` includes that step and packages
both engines. Install host Flex, Bison and M4 as build tools. The first run
fetches the exact commits in `vendor/armsx2-dependencies/revisions.tsv`, including
nested repositories. Later runs verify those sources rather than updating
floating tags. `PSX_BUILD_JOBS` defaults to 2. Core sources stage under
`build/armsx2-source`; the original submodule remains unchanged.

The recipe disables Qt, standalone SDL UI, tests, unpinned patch downloads and
unrelated display backends. It uses GLES, native resolution, static/PIC
libraries, Android API 28 and the recorded 4 KiB host-page path. The PS2 host
checks actual GLES capability and refuses other page sizes with saved data
intact. The existing PS1 interpreter fallback remains separate; ELF alignment
alone does not qualify the PS2 JIT for a 16 KiB host.

Resource packaging includes `bin/resources` and the same pinned donor's Android
`platforms/android/app/src/main/assets/resources/patches.zip`. The latter is
outside the desktop resource tree; omitting it caused the real built-in-patches
warning on boot. Its exact SHA-256 is checked before packaging, its ZIP is
validated, and it enters the hash-checked runtime resource index. The original
archive and PNACH attribution remain in the corresponding source. This does
not enable optional widescreen/cheat patches or establish a game-specific fix.
No floating patch download or private game input is used.

`PSX_PS2_DEPENDENCY_SOURCES` can name an existing exact locked source cache.
Creating a new corresponding-source bundle requires each dependency's own Git
repository; a manifest-verified exported cache remains valid for building, but
may also contain native build output and is not re-exported recursively. Point
the override at the existing locked Git cache for source packaging. The source
preflight refuses this mismatch before creating another staging tree; it does
not remove or replace the build cache. Git export also refuses an ancestor
repository instead of accidentally archiving project files as a dependency.
`PSX_PS2_CORE_SOURCE` can name a qualification checkout only when its revision
and complete tracked diff equal the recorded core and patch. These are build
inputs, never runtime core-download or user-facing core-selection options.
A corresponding-source bundle includes dependency trees and source manifests
for offline builds without their Git metadata. Standard Android and Rust
build tools are still external requirements.

The Activity shim also forwards `onNewIntent`. A generated, non-exported
`ShortcutReceiver` forwards Android's confirmed-pin broadcast. Rust constructs
the intents, validates game IDs, requests native confirmation, updates local
icons and owns every launch decision. Neither shim implements application
logic. Missing confirmation is not reported as a successful pin.
