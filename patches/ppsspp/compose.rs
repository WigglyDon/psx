// Exact, bounded changes to the pinned donor adapter; shared with state binding.
pub fn adapter(upstream: &str, bridge: &str) -> Result<String, &'static str> {
    let declaration = "using namespace Libretro;";
    if upstream.matches(declaration).count() != 1 {
        return Err("Pinned PPSSPP owner declaration changed");
    }
    let mut source = upstream.replace(
        declaration,
        "using namespace Libretro;\nstatic bool psx_host_paused = false;",
    );
    let resume = "   if (useEmuThread)\n      EmuThreadStart();";
    if source.matches(resume).count() != 2 {
        return Err("Pinned PPSSPP serialization resume sites changed");
    }
    source = source.replace(
        resume,
        "   if (useEmuThread && !psx_host_paused)\n      EmuThreadStart();",
    );
    let input = "   // Process input.\n   retro_input();";
    if source.matches(input).count() != 1 {
        return Err("Pinned PPSSPP frame gate changed");
    }
    source = source.replace(input,
        "   // The Rust owner may boot/restore without advancing the guest.\n   if (psx_host_paused)\n      return;\n\n   // Process input.\n   retro_input();");
    source.push('\n');
    source.push_str(bridge);
    Ok(source)
}
