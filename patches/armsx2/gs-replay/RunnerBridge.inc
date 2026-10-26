// SPDX-License-Identifier: MIT
// Compiled after the pinned runner Main.cpp. Narrow ABI/context/output bridge;
// the retained runner owns VM/replayer work, Rust owns lifetime and stopping.
#include "pcsx2/GS/Renderers/OpenGL/GLLibretro.h"
#include "pcsx2/GS/Renderers/OpenGL/GLStreamBuffer.h"
#define PSX_RUNNER_EXPORT extern "C" __attribute__((visibility("default")))
// Shared libretro-compatible input objects name this frontend hook. Replay
// has no live pad output; retain the native runner's inert rumble behavior.
void Host::SetPadVibration(u32, float, float) {}
PSX_RUNNER_EXPORT bool retro_psx_gs_replay_configure(unsigned mode, const char* path, GSReplay::Callback cb)
{ return mode == 3 && GSReplay::Configure(mode, path, cb); }
PSX_RUNNER_EXPORT void retro_psx_gs_replay_clear() { GSReplay::Clear(); }
PSX_RUNNER_EXPORT bool retro_psx_runner_prepare()
{
    Error error;
    if (!GSReplay::callback || !GLLibretro::CaptureFrontendContext(&error)) return false;
    GLLibretro::Active = true;
    GLLibretro::SetPacing(true);
    return true;
}
PSX_RUNNER_EXPORT void retro_psx_observer_set_callback(GLLibretro::Observer cb) { GLLibretro::SetObserver(cb); }
PSX_RUNNER_EXPORT void retro_psx_observer_set_output_callback(GLLibretro::OutputCallback cb) { GLLibretro::SetOutputCallback(cb); }
PSX_RUNNER_EXPORT void retro_psx_observer_set_result_callback(GSObservation::Callback cb) { GSObservation::SetCallback(cb); }
PSX_RUNNER_EXPORT void retro_psx_observer_set_stream_callback(GLStreamBuffer::StreamObserver cb) { GLStreamBuffer::SetStreamObserver(cb); }
PSX_RUNNER_EXPORT void retro_psx_runner_set_failure(GLStreamBuffer::FailureCallback cb) { GLStreamBuffer::SetFailureCallback(cb); }
PSX_RUNNER_EXPORT bool retro_psx_set_frame_callback(void (*cb)()) { GLLibretro::SetFrameReadyCallback(cb); return true; }
PSX_RUNNER_EXPORT unsigned retro_psx_runner_service()
{
    GLLibretro::Frame frame;
    if (!GLLibretro::ConsumeFrame(&frame)) return 0;
    return GLLibretro::AcknowledgeCreatorOutput(&frame) ? 1 : 2;
}
PSX_RUNNER_EXPORT bool retro_psx_runner_ready() { return VMManager::HasValidVM(); }
PSX_RUNNER_EXPORT void retro_psx_runner_stop()
{
    GLLibretro::AbortPacing();
    s_signal_stop_requested.store(true);
}
PSX_RUNNER_EXPORT void retro_psx_runner_retired()
{
    // Rust has joined the runner entry/CPU thread; graphics owner is current.
    GLLibretro::Shutdown();
    GLLibretro::Active = false;
    GLLibretro::ReleaseFrontendContext();
    GLLibretro::SetObserver(nullptr);
    GLLibretro::SetOutputCallback(nullptr);
    GSObservation::SetCallback(nullptr);
    GLStreamBuffer::SetStreamObserver(nullptr);
    GLStreamBuffer::SetFailureCallback(nullptr);
}
