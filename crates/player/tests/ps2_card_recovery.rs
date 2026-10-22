//! Actual retained SIO2 dispatch/restore policy with mechanical devices and
//! stream types. No firmware, game, saved card or successful-save stub.
#![cfg(target_os = "linux")]

use libloading::Library;
use std::{fs, path::PathBuf, process::Command};

fn fixture(name: &str) -> (PathBuf, Library) {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("psx-card-{name}-{}", std::process::id()));
    let relative = "pcsx2/SIO/Sio2.cpp";
    fs::create_dir_all(temp.join("pcsx2/SIO")).unwrap();
    fs::copy(
        repo.join("vendor/armsx2").join(relative),
        temp.join(relative),
    )
    .unwrap();
    let patch = Command::new("git")
        .args(["apply", "--include=pcsx2/SIO/Sio2.cpp"])
        .arg(repo.join("patches/armsx2/0001-host-pause-and-card-flush.patch"))
        .current_dir(&temp)
        .output()
        .unwrap();
    assert!(
        patch.status.success(),
        "{}",
        String::from_utf8_lossy(&patch.stderr)
    );
    let source = fs::read_to_string(temp.join(relative)).unwrap();
    let method = |signature: &str| {
        let start = source.find(signature).unwrap();
        let end = start + source[start..].find("\n}\n").unwrap() + 3;
        &source[start..end]
    };
    let header = fs::read_to_string(repo.join("vendor/armsx2/pcsx2/SIO/Sio2.h"))
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let commands = fs::read_to_string(repo.join("vendor/armsx2/pcsx2/SIO/SioTypes.h"))
        .unwrap()
        .replace("#pragma once", "");
    let cpp = format!(
        r#"
#include <array>
#include <deque>
#include <cstdint>
#include <cstring>
#include <algorithm>
using u8 = std::uint8_t; using u16 = std::uint16_t;
using u32 = std::uint32_t; using u64 = std::uint64_t;
{header}
{commands}
Sio2::Sio2() = default; Sio2::~Sio2() = default;
Sio2 g_Sio2;
std::deque<u8> g_Sio2FifoIn, g_Sio2FifoOut;
struct Card {{
    bool ps1 = false, present = true;
    u8 port = 0, slot = 0, term = 0x55;
    size_t autoEjectTicks = 0;
    u64 crc = 17;
    bool IsPSX() {{ return ps1; }}
    bool IsPresent() {{ return present; }}
    u64 GetChecksum() {{ return crc; }}
}} mcds[2][4];
Card* mcd = &mcds[0][0];
struct MultitapProtocol {{ unsigned GetMemcardSlot() {{ return 0; }} }};
std::array<MultitapProtocol, 2> g_MultitapArr;
struct ConsoleType {{
    template<class... T> void WriteLn(T...) {{}}
    template<class... T> void Warning(T...) {{}}
}} Console;
namespace Arcade {{ bool IsActive() {{ return false; }} }}
static unsigned ejects = 0, legacy_accesses = 0;
namespace AutoEject {{ void SetAll() {{
    ++ejects;
    for (auto& port : mcds) for (auto& card : port) {{
        card.autoEjectTicks = 60; card.term = 0x66;
    }}
}} }}
u32 sioLastFrameMcdBusy = 0;
struct StateWrapper {{
    bool writing = true;
    std::array<u64, 8> checksums{{}};
    bool DoMarker(const char*) {{ return true; }}
    template<class T> void Do(T*) {{}}
    void DoBytes(void* bytes, size_t length) {{
        if (writing) std::memcpy(checksums.data(), bytes, length);
        else std::memcpy(bytes, checksums.data(), length);
    }}
    bool IsWriting() {{ return writing; }}
    bool IsReading() {{ return !writing; }}
    bool IsGood() {{ return true; }}
}};
struct MemoryCardProtocol {{
    void ResetPS1State() {{}}
    u8 PS1Read(u8) {{ ++legacy_accesses; return 0x5a; }}
    u8 PS1Write(u8) {{ ++legacy_accesses; return 0x47; }}
    u8 PS1State(u8) {{ ++legacy_accesses; return 0; }}
    u8 PS1Pocketstation(u8) {{ ++legacy_accesses; return 0; }}
#define NOOP(name) void name() {{}}
    NOOP(Probe) NOOP(UnknownWriteDeleteEnd) NOOP(SetSector) NOOP(GetSpecs)
    NOOP(SetTerminator) NOOP(GetTerminator) NOOP(WriteData) NOOP(ReadData)
    NOOP(ReadWriteEnd) NOOP(EraseBlock) NOOP(UnknownBoot) NOOP(AuthXor)
    NOOP(AuthF3) NOOP(AuthF7)
#undef NOOP
}} g_MemoryCardProtocol;
void Sio2::SetCmdStat(u32 value) {{ CmdStat = value; }}
{dispatch}
{restore}
extern "C" size_t probe(bool ps1, u8 command, size_t length, u8* out) {{
    mcds[0][0] = Card{{}}; mcds[0][0].ps1 = ps1;
    g_Sio2.port = 0; g_Sio2.commandLength = length;
    g_Sio2FifoIn.assign(length - 1, 0);
    g_Sio2FifoIn.front() = command;
    g_Sio2FifoOut.assign(144, 0xee);
    legacy_accesses = 0;
    g_Sio2.Memcard();
    std::copy(g_Sio2FifoOut.begin(), g_Sio2FifoOut.end(), out);
    return legacy_accesses;
}}
extern "C" unsigned reload(bool changed, u8* term, u64* crc) {{
    for (auto& port : mcds) for (auto& card : port) card = Card{{}};
    // A guest has negotiated 0x5a, then the app process is replaced. The old
    // stream saves only CRCs, so the new card starts at power-on 0x55.
    mcds[0][0].term = 0x5a;
    StateWrapper sw;
    ejects = 0;
    g_Sio2.DoState(sw);
    mcds[0][0].term = 0x55;
    if (changed) mcds[0][0].crc++;
    sw.writing = false;
    g_Sio2.DoState(sw);
    *term = mcds[0][0].term; *crc = mcds[0][0].crc;
    return ejects;
}}
"#,
        dispatch = method("void Sio2::Memcard()"),
        restore = method("bool Sio2::DoState(StateWrapper& sw)"),
    );
    fs::write(temp.join("fixture.cpp"), cpp).unwrap();
    let build = Command::new("c++")
        .args([
            "-std=c++20",
            "-shared",
            "-fPIC",
            "fixture.cpp",
            "-o",
            "fixture.so",
        ])
        .current_dir(&temp)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let library = unsafe { Library::new(temp.join("fixture.so")).unwrap() };
    (temp, library)
}

#[test]
fn ps2_card_cannot_be_misdetected_or_written_through_the_ps1_protocol() {
    let (temp, library) = fixture("detection");
    unsafe {
        let probe = library
            .get::<unsafe extern "C" fn(bool, u8, usize, *mut u8) -> usize>(b"probe\0")
            .unwrap();
        for (command, length) in [(0x52, 3), (0x52, 140), (0x57, 138), (0x58, 5)] {
            let mut out = [0xcc; 512];
            let calls = probe(false, command, length, out.as_mut_ptr());
            assert_eq!(
                calls, 0,
                "PS2 card must not enter PS1 handler {command:02x}"
            );
            assert_eq!(out[146], 0, "PS2 card must not answer the PS1 0x5a probe");
            assert!(out[..144].iter().all(|b| *b == 0xee));
            assert!(out[144 + length..].iter().all(|b| *b == 0xcc));
        }
        let mut out = [0xcc; 512];
        assert_eq!(probe(true, 0x52, 3, out.as_mut_ptr()), 1);
        assert_eq!(out[146], 0x5a, "real PS1 card dispatch is retained");
    }
    drop(library);
    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn fresh_process_restore_renegotiates_the_card_endpoint_without_replacing_media() {
    let (temp, library) = fixture("restore");
    unsafe {
        let reload = library
            .get::<unsafe extern "C" fn(bool, *mut u8, *mut u64) -> u32>(b"reload\0")
            .unwrap();
        for changed in [false, true] {
            let mut term = 0;
            let mut crc = 0;
            assert_eq!(reload(changed, &mut term, &mut crc), 1);
            assert_eq!(
                term, 0x66,
                "guest must renegotiate the omitted card endpoint state"
            );
            assert_eq!(
                crc,
                17 + u64::from(changed),
                "current media is authoritative"
            );
        }
    }
    drop(library);
    fs::remove_dir_all(temp).unwrap();
}
