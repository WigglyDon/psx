# PS2 shared-context comparison

[PLAYER-2's focused graphics disposition](https://github.com/WigglyDon/psx/issues/2#issuecomment-6333844620)
requires separating a graphics failure from its later submission/wait symptoms.
The returned `552d7da` trial and its exact identities are recorded in
[the verified engineering handback](https://github.com/WigglyDon/psx/issues/2#issuecomment-6333961725).
That trial exercised native patch `90b35554…`; PS2 rendering remains unqualified.

## What the retained evidence establishes

Restored and clean contexts are distinct cases. The restored run's first recorded
GPU failure was a timestamp wait, followed by a submission failure. In each clean
process the first recorded GPU failure was a submission failure about ten seconds
after initialization. The clean no-fetch comparison explicitly reported fetch
disabled and still failed. The initialization markers precede these events;
shader/link warnings in the clean trace follow the first submission failure.
These are recorded log order, not an unbuffered trace of GPU command execution.

An earlier aspect-patch parse warning comes from `gsaspectratio=Stretch` in
bundled widescreen-group metadata. The parser accepts numeric ratios, whereas
`Stretch` is a valid GS **configuration** value. Parsing the database is separate
from enabling its groups. The retained settings disable widescreen overrides,
and the host explicitly selects Stretch, so the auto-aspect override application
branch is not taken. This warning does not identify a failed GL operation.

The packaged native library and retained symbols have the same ELF build ID.
There is no failing GL-call address, driver fault PC, per-call return trace,
producer/consumer fence status or framebuffer-completeness sample at the physical
failure. Thus symbols cannot identify the initiating draw, handoff or presentation
operation. Empty GL error queues and successful swaps do not establish valid
output. No driver root cause or new production defect is established by this
review. Do not manufacture a hardware-name gate, suppress errors or alter guest
clocks from these observations.

In the tested patch, `GSDeviceOGL::EndPresent` creates and flushes the producer fence
before publishing. The consumer waits/deletes it, rebinds the source attachment,
blits, then creates and flushes the read-completion fence before returning ownership.
The producer waits for that return before reuse, resize or destruction. This
normal-path review establishes no additional missing ownership step. Fence/API
failure handling at the physical failure was not sampled; a hypothetical null
fence is not an observed cause. The existing reported-loss recovery does not
detect every silent black-output failure.

## One prepared comparison

`psx-player`'s `ps2-graphics-probe` example uses generated quadrant patterns only.
It mechanically extracts the pinned donor's actual patched `GLLibretro` handoff
and the GLES consumer block from `Main.cpp`. The C++ portion is ABI/test glue
around those retained GPL bodies. Rust owns EGL, generation, scheduling,
observations and pixel verification. No game, firmware, card or checkpoint is
loaded, and the tool is not bundled into the application.

Run the same prepared helper and executable in separate fresh processes:

- `reference`: one GLES context produces and copies each generated frame.
- `shared`: a GS-like thread renders into three backbuffers in a context sharing
  the frontend's objects; the actual handoff, producer fence, consumer blit,
  read-completion fence and reuse/destruction wait are exercised.
- `negative-control`: an intentionally incomplete target verifies that the
  boundary/status/pixel oracle rejects a known invalid configuration. It is
  diagnostic validation, not a product defect or a candidate setting.

Both positive modes retain 24 separate 64×48 output targets, then compare every
pixel to a CPU oracle. Readbacks happen after submission, so they do not insert
a CPU finish between every producer/consumer pair. There is no `glFinish` call.
Each process needs an external 30-second timeout; in-process handoff waits are
bounded at two seconds. A driver call can still block, so the process deadline
is mandatory. This is a correctness comparison, never a performance measurement.

| Stage | Observation |
| --- | --- |
| 0 | Generated target allocation |
| 1 | Producer reuse/return-fence wait |
| 2 | Generated producer draw |
| 7 | Producer fence creation and flush |
| 3 | Consumer producer-fence wait/deletion |
| 4 | Read attachment and draw target, including both FBO statuses |
| 5 | Consumer blit |
| 6 | Read-completion fence/flush/return |
| 8 | Producer resource retirement |
| 9 | Final pixel readback |

EGL setup/current/swap failures retain their error result. The first observed
GL error/FBO failure is retained rather than silently drained. Pixel mismatches
include their frame and coordinate. Prepared manifests bind the probe source,
helper source/binary and native revision-plus-patch; mismatches refuse execution.
Original donor source/copyright and GPL text remain beside the generated helper.

## Build and off-device validation

Run the [storage preflight](storage.md) first; allow 1 GiB and serialize preparation
with the existing project build lock. These commands prepare/run only the host:

```sh
cargo build --locked --offline -p psx-player --example ps2-graphics-probe
# Choose a new project-owned output directory; preparation refuses an existing tree.
target/debug/examples/ps2-graphics-probe prepare build/graphics-comparison-host
LP_NUM_THREADS=2 LIBGL_ALWAYS_SOFTWARE=1 timeout -k 2s 30s \
  target/debug/examples/ps2-graphics-probe reference build/graphics-comparison-host/handoff.so
LP_NUM_THREADS=2 LIBGL_ALWAYS_SOFTWARE=1 timeout -k 2s 30s \
  target/debug/examples/ps2-graphics-probe shared build/graphics-comparison-host/handoff.so
# Expected nonzero exit, identifying the incomplete target:
LP_NUM_THREADS=2 LIBGL_ALWAYS_SOFTWARE=1 timeout -k 2s 30s \
  target/debug/examples/ps2-graphics-probe negative-control build/graphics-comparison-host/handoff.so
```

Mesa software GLES 3.2 produced identical positive-mode output digests:
`c76ffc3292ec362a31f9456a22c5abbe3b483b234944848838ddfab8c206c364`.
The negative control failed at stage 4 with an incomplete draw target, even with
`GL_NO_ERROR` at that boundary. This validates the oracle and the generated
handoff case on that implementation; it does not qualify the phone or donor GS.

For an Android **preparation only**, set `PSX_GL_PROBE_CXX` to the installed NDK's
`aarch64-linux-android28-clang++` and `PSX_GL_PROBE_ANDROID=1`, then run the host
`prepare` command with a fresh Android output directory. Cross-build the Rust
example for `aarch64-linux-android` with the usual NDK linker. Keep the helper,
`binding.json` and executable together; preserve their digests. No preparation
command admits a device, transfers a file or schedules execution.

## Interpretation and physical-test boundary

A reference pass and shared-only failure would isolate the added cross-context
handoff/visibility path for these commands. A failure in both modes would precede
that distinction. Two passes would rule out neither the full donor rendering
sequence nor payload-dependent, loader, window/UI or timing defects. An earlier
GL/FBO/fence failure would give a sharper boundary than a later driver message;
readback mismatch with no API error remains a failure, not a valid frame.

This is not an equivalent standalone-donor game comparison. It uses a pbuffer,
not NativeActivity's window; direct EGL sharing, not the donor platform loader;
simple generated clears, not GS shaders; and no application UI. Reference mode
necessarily disables the CPU pacing wait on its single owner; shared mode uses
that wait. Android shell execution would also differ from the application's UID.
Resize, actual game rendering, sound, saving and sustained speed are outside it.
The GLES sharing requirements are described by Khronos's
[Sync Object](https://wikis.khronos.org/opengl/Sync_Object) and
[Memory Model](https://wikis.khronos.org/opengl/Memory_Model) references.

No physical execution is authorized by this preparation. A later comparison
requires the current resource-discovery/custodian route and a separately admitted
stock lab session, with exact artifacts, current conflicts/rollback and a cleanup
reserve. Request at most three fresh processes, each ≤30 seconds, generated
memory under 16 MiB plus runtime/driver overhead, output under 64 KiB and no games,
firmware, screenshots, settings changes or unattended stress. Keep the personal
Fold out of that request. Any failure must retire only this fixture and preserve
the working application; no automatic retry or device/driver modification follows.
