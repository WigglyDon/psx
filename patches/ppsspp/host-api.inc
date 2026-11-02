// Minimal ABI bridge into retained PPSSPP owner operations. Rust owns policy.
// Included in the composed donor adapter; its inherited GPL terms apply.
#include "Core/HLE/sceIo.h"
#include "Core/HLE/sceUtility.h"
#include <cstdarg>
#include <cstdio>

extern "C" {
RETRO_API void psx_psp_auto_firmware(int enabled) {
   g_Config.bAutoUpgradeFirmware = enabled != 0;
}
RETRO_API int psx_psp_ready() {
   return gpu && PollBootState() == BootState::Complete && !Libretro::g_pendingBoot;
}
RETRO_API int psx_psp_pause(int paused) {
   psx_host_paused = paused != 0;
   if (!psx_psp_ready())
      return 0;
   if (Libretro::useEmuThread) {
      if (psx_host_paused)
         Libretro::EmuThreadPause();
      else
         Libretro::EmuThreadStart();
   }
   return 1;
}
RETRO_API int psx_psp_flush() {
   if (!psx_host_paused || !psx_psp_ready())
      return 0;
   __UtilityWaitForIO();
   __IoWaitForAsync();
   return 1;
}
typedef void (*psx_psp_log_callback)(unsigned, const char *);
static psx_psp_log_callback psx_log_callback = nullptr;
RETRO_API void psx_psp_set_log(psx_psp_log_callback callback) {
   psx_log_callback = callback;
}
RETRO_API void psx_psp_log(unsigned level, const char *format, ...) {
   char buffer[2048];
   va_list args;
   va_start(args, format);
   std::vsnprintf(buffer, sizeof(buffer), format, args);
   va_end(args);
   if (psx_log_callback)
      psx_log_callback(level, buffer);
}
}
