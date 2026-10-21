//! Check actual donor response framing without firmware, games or card data.
#![cfg(target_os = "linux")]

use libloading::Library;
use std::{fs, process::Command};

#[test]
fn queued_card_responses_preserve_packet_lengths_and_terminator_offsets() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let temp = std::env::temp_dir().join(format!("psx-card-packets-{}", std::process::id()));
    let relative = "pcsx2/SIO/Memcard/MemoryCardProtocol.cpp";
    fs::create_dir_all(temp.join("pcsx2/SIO/Memcard")).unwrap();
    fs::copy(
        repo.join("vendor/armsx2").join(relative),
        temp.join(relative),
    )
    .unwrap();
    let patch = Command::new("git")
        .arg("apply")
        .arg(format!("--include={relative}"))
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
    let start = source
        .find("void MemoryCardProtocol::The2bTerminator(size_t length)")
        .unwrap();
    let end = start + source[start..].find("\n}\n").unwrap() + 3;
    let method = &source[start..end];
    // Mechanical FIFO/card types and a callable bridge; framing is the actual
    // pinned GPL implementation plus the tracked repair, extracted verbatim.
    let fixture = format!(
        r#"
#include <cstddef>
#include <cstdint>
#include <deque>
#include <algorithm>
using u8 = std::uint8_t;
std::deque<u8> g_Sio2FifoOut;
struct Card {{ u8 term; }} card;
Card* mcd = &card;
struct MemoryCardProtocol {{ void The2bTerminator(size_t length); }} protocol;
{method}
extern "C" size_t reply(u8* out, size_t prefix, size_t length, u8 term) {{
    card.term = term;
    g_Sio2FifoOut.assign(prefix, 0xee);
    // Sio2::Memcard appends its two initial response bytes before dispatch.
    g_Sio2FifoOut.push_back(0); g_Sio2FifoOut.push_back(0);
    protocol.The2bTerminator(length);
    std::copy(g_Sio2FifoOut.begin(), g_Sio2FifoOut.end(), out);
    return g_Sio2FifoOut.size();
}}
"#
    );
    fs::write(temp.join("fixture.cpp"), fixture).unwrap();
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
    unsafe {
        let library = Library::new(temp.join("fixture.so")).unwrap();
        let reply = library
            .get::<unsafe extern "C" fn(*mut u8, usize, usize, u8) -> usize>(b"reply\0")
            .unwrap();
        for prefix in [0, 144, 288] {
            for length in [4, 5, 9, 14] {
                for term in [0x55, 0x5a] {
                    let mut out = [0xcc; 512];
                    let used = reply(out.as_mut_ptr(), prefix, length, term);
                    assert_eq!(used, prefix + length, "prefix={prefix} length={length}");
                    assert!(out[..prefix].iter().all(|byte| *byte == 0xee));
                    assert!(out[prefix..used - 2].iter().all(|byte| *byte == 0));
                    assert_eq!(&out[used - 2..used], &[0x2b, term]);
                    assert!(out[used..].iter().all(|byte| *byte == 0xcc));
                }
            }
        }
    }
    fs::remove_dir_all(temp).unwrap();
}
