//! ELF checks shared by full builds and frozen repackaging.
use crate::Result;

pub fn elf(headers: &str, symbols: &str, name: &str, observer: Option<&str>) -> Result<()> {
    if !headers.contains("AArch64") || headers.contains("libc++_shared.so") {
        return Err(format!("Wrong ABI or unpackaged C++ runtime in {name}").into());
    }
    let loads: Vec<_> = headers
        .lines()
        .filter(|l| l.trim_start().starts_with("LOAD"))
        .collect();
    if loads.is_empty() {
        return Err("ELF has no LOAD segments".into());
    }
    for line in loads {
        let alignment = line
            .split_whitespace()
            .last()
            .ok_or("Missing ELF alignment")?;
        if u64::from_str_radix(alignment.trim_start_matches("0x"), 16)? < 16384 {
            return Err(format!("{name} LOAD alignment below16KiB").into());
        }
    }
    let exported = |required: &str| {
        symbols.lines().any(|line| {
            let c: Vec<_> = line.split_whitespace().collect();
            c.len() >= 8
                && c[3] == "FUNC"
                && c[4] == "GLOBAL"
                && c[5] == "DEFAULT"
                && c[6] != "UND"
                && c[7] == required
        })
    };
    if name == "libpsx.so" {
        if !headers.contains("Shared library: [libEGL.so]") {
            return Err("Missing Rust EGL dependency".into());
        }
        for required in [
            "ANativeActivity_onCreate",
            "Java_com_machinerhythm_psx_PsxActivity_documentResult",
            "Java_com_machinerhythm_psx_PsxActivity_audioFocusChanged",
            "Java_com_machinerhythm_psx_PsxActivity_gameIntent",
            "Java_com_machinerhythm_psx_PsxActivity_insetsChanged",
            "Java_com_machinerhythm_psx_ShortcutReceiver_confirmed",
            "Java_com_machinerhythm_psx_PlaybackService_started",
            "Java_com_machinerhythm_psx_PlaybackService_stopped",
        ] {
            if !exported(required) {
                return Err(format!("Missing defined export {required}").into());
            }
        }
    }
    if name == "libarmsx2.so" {
        for (required, expected) in [
            ("retro_psx_observer_set_stream_callback", observer.is_some()),
            (
                "retro_psx_observer_set_output_callback",
                observer == Some("producer-local-output-sync"),
            ),
            (
                "retro_psx_observer_set_result_callback",
                observer == Some("producer-local-output-sync"),
            ),
        ] {
            if exported(required) != expected {
                return Err(format!("PS2 capability mismatch: {required}").into());
            }
        }
        for required in [
            "retro_psx_ready",
            "retro_psx_set_paused",
            "retro_psx_flush_cards",
            "retro_psx_set_speed",
            "retro_psx_set_frame_callback",
            "retro_psx_capture_state",
            "retro_psx_pack_state",
            "retro_psx_free_state",
        ] {
            if !exported(required) {
                return Err(format!("Missing PS2 C ABI export {required}").into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incomplete_wrong_abi_small_alignment_and_missing_runtime_refuse() {
        let good = "Machine: AArch64\n LOAD 0 0 0 0 0 R 0x4000\n";
        assert!(elf(good, "", "libppsspp.so", None).is_ok());
        for bad in [
            good.replace("AArch64", "x86"),
            good.replace("0x4000", "0x1000"),
            "AArch64".into(),
            format!("{good}libc++_shared.so"),
        ] {
            assert!(elf(&bad, "", "libppsspp.so", None).is_err());
        }
    }
    #[test]
    fn actual_export_parser_requires_defined_functions_and_matching_capabilities() {
        let headers = "AArch64\n LOAD 0 0 0 0 0 R 0x4000\n";
        let names = [
            "retro_psx_ready",
            "retro_psx_set_paused",
            "retro_psx_flush_cards",
            "retro_psx_set_speed",
            "retro_psx_set_frame_callback",
            "retro_psx_capture_state",
            "retro_psx_pack_state",
            "retro_psx_free_state",
        ];
        let normal = names
            .map(|n| format!("1: 0 1 FUNC GLOBAL DEFAULT 1 {n}\n"))
            .concat();
        assert!(elf(headers, &normal, "libarmsx2.so", None).is_ok());
        assert!(elf(
            headers,
            &normal.replace("DEFAULT 1", "DEFAULT UND"),
            "libarmsx2.so",
            None
        )
        .is_err());
        assert!(elf(headers, &normal, "libarmsx2.so", Some("normal")).is_err());
        let extra = format!(
            "{normal}1: 0 1 FUNC GLOBAL DEFAULT 1 retro_psx_observer_set_stream_callback\n"
        );
        assert!(elf(headers, &extra, "libarmsx2.so", None).is_err());
        assert!(elf(headers, &extra, "libarmsx2.so", Some("normal")).is_ok());
    }
}
