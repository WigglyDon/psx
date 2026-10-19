# ARMSX2 Android dependency inputs

`revisions.tsv` records every resolved source repository, including nested
FreeType, PlutoSVG and shaderc inputs. The Rust build task follows the original
pinned donor's Android source-build operations using these commits rather
than floating tags or shaderc's network dependency updater. NDK zlib and the
static C++ runtime come from the selected Android NDK.

`cargo xtask source` exports each tree at its exact revision, with its original
licenses and notices, under `vendor/armsx2-dependencies/source`. It adds source
manifests for builds without Git metadata. The source bundle includes the
unmodified donor, the recorded integration patch, original dependency trees
and the Rust build recipe. The APK includes dependency and resource notices.
No prebuilt core, online patch download, proprietary firmware or game is an
input supplied by this repository.

The qualification toolchain is Rust 1.96.1, NDK 30.0.16248370, Android API 28,
CMake/Ninja, and host Flex/Bison/M4. Android ELF alignment is checked separately
from runtime page-size/JIT support; this PS2 candidate targets a 4 KiB host.
