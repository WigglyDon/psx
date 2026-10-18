# psx development

Read README.md, docs/architecture.md, docs/distribution.md and the active
GitHub issue before work.
Before the first issue exists, docs/first-playable.md is its prepared scope.
Keep the issue's progress and handback current; keep lasting design in the
small documents that own it.

## Product

Build a minimal, efficient native PS1 player. The first target is Android
ARM64 on a stock foldable; eventual Google Play and Apple App Store release
are product requirements. All new product and host logic is Rust. Start with
the pinned SwanStation C++ core candidate through a narrow libretro C ABI;
do not translate or redesign its emulation internals.

Use established Rust Android/NDK bindings. A minimal platform-generated or
callback shim may be needed where Android cannot expose a required callback
directly to Rust; document the need and keep application decisions in Rust.
Do not introduce a Kotlin, Java, C++ or Python application layer.

Keep reusable player behavior independent of Android handles. Use small
platform adapters for lifecycle, graphics, audio, file grants and input.
Preserve a stock-iOS route without JIT, debugger attachment or special user
setup. Verify the donor's distribution terms before treating it as the
permanent core for both stores; attribution alone does not settle this.
An iOS build or another app's store listing is not proof for this product.

Keep one core execution owner, explicit input, bounded output, correct
graphics-context ownership and durable saves. Follow the borrowed core's
actual interface contracts rather than importing another emulator's schedule.

## Work

Advance the complete first playable milestone autonomously. Inspect current
source and local state, preserve unknown work and make routine implementation
choices. Use focused validation for the actual changed behavior. Do not
recreate an emulator conformance programme or optimize core internals without
an observed blocker and an explicit product decision.

Do not grow a framework, multi-core selector, account service, game database
or settings catalog before first playable acceptance. The early iOS/store
feasibility check is in scope; a complete iOS port is a later milestone.
Document supported formats and failures honestly. Keep navigation, file
errors, touch targets and interruption behavior usable from the first build.

## Public source

This repository is intended to be public and free software. Original work
uses MIT. Bundled releases must also satisfy the borrowed core's applicable
GPL requirements; neither MIT nor FFI removes them. Preserve upstream licenses
and copyright, pin borrowed sources and record modifications. Retain the exact
upstream license and dependency notices when incorporating its tree.

Publish only project source and suitable evidence. Games, proprietary
firmware, saves, signing keys, credentials, device identifiers and private
logs remain local. A user game selected for runtime testing is not a public
test fixture. Do not copy private sibling-project files or history.

Repository creation and public project source were requested by the owner.
Normal project source integration is within this assignment, subject to the
local environment's access controls. This grants no unrelated account,
device, infrastructure or spending authority.

## Verification and reporting

Build success, synthetic tests, gameplay, sound, saving and sustained speed
are separate observations. Report what actually ran and any material limits.
Prepare without device custody; use the current stock-device procedure before
installation or testing. Preserve the owner's other apps and saved data.

