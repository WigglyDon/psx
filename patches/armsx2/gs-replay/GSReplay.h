// SPDX-License-Identifier: MIT
// PSX GS-REPLAY-1 callback/build glue only. Rust owns bounds and stage policy.
// Included only by the separately built development capture/replay operands.
#pragma once
#ifdef PSX_GS_REPLAY1
#include <cstdint>
#include <atomic>
#include <string>
namespace GSReplay
{
using Callback = std::uint32_t (*)(std::uint32_t, std::uint64_t, std::uint64_t);
inline Callback callback = nullptr;
inline std::string path;
inline unsigned mode = 0;
inline std::uint64_t boundary = 0, recorded = 0;
inline std::atomic<bool> failed{false};
inline bool Emit(unsigned event, std::uint64_t a = 0, std::uint64_t b = 0)
{
    const bool allowed = !callback || callback(event, a, b) != 0;
    if (!allowed) failed = true;
    return allowed;
}
inline bool Capture() { return callback && mode == 1; }
inline bool Allowed() { return !callback || (!failed && Emit(14)); }
inline bool Raw(std::uint64_t bytes) { return !Capture() || (!failed && Emit(4, bytes)); }
inline bool Encoded(std::uint64_t bytes) { return !Capture() || Emit(5, bytes); }
inline void Error() { failed = true; Emit(9); }
inline bool Configure(unsigned selected, const char* output, Callback cb)
{
    if (callback || !cb || selected < 1 || selected > 3 || !output || !*output)
        return false;
    path = output;
    if (path.size() > 4096) return false;
    mode = selected; callback = cb; boundary = 0; recorded = 0; failed = false;
    return true;
}
// Caller must join CPU/GS owners before clearing the Rust callback.
inline void Clear() { callback = nullptr; mode = 0; path.clear(); }
}
#endif
