#![cfg(feature = "gs-replay")]
use psx_player::gs_replay::{self as gs, Ledger, Mode, Phase};
use std::{fs, process::Command};

fn dump() -> Vec<u8> {
    let mut b = Vec::new();
    for v in [
        u32::MAX,
        44,
        9,
        gs::FREEZE_BYTES,
        36,
        0,
        123,
        0,
        0,
        36,
        0,
        0x564f5250,
        1,
    ] {
        b.extend(v.to_le_bytes());
    }
    b.extend(9u32.to_le_bytes());
    b.resize(b.len() + gs::FREEZE_BYTES as usize - 4 + 8192, 0);
    b.extend([0, 3]);
    b.extend(16u32.to_le_bytes());
    b.extend([7; 16]);
    b.extend([3]);
    b.extend([0; 8192]);
    b.extend([1, 0]);
    b
}
#[test]
fn strict_format_and_truncated_fields_payloads() {
    let b = dump();
    let report = gs::validate(&b[..], gs::BYTE_LIMIT).unwrap();
    assert_eq!((report.packets, report.transfers, report.vsyncs), (3, 1, 1));
    assert!(report.readback_provenance && report.ends_at_vsync);
    assert_eq!(report.decoded_bytes, b.len() as u64);
    assert_eq!(gs::validate(&b[..], b.len() as u64).unwrap(), report);
    assert!(gs::validate(&b[..], b.len() as u64 - 1).is_err());
    let state_end = 8 + 44 + gs::FREEZE_BYTES as usize;
    let registers_end = state_end + 8192;
    for n in (0..64)
        .chain([1024, state_end / 2])
        .chain(state_end - 4..state_end + 4)
        .chain(registers_end - 4..registers_end + 26)
        .chain(b.len() - 8..b.len())
    {
        assert!(
            gs::validate(&b[..n], gs::BYTE_LIMIT).is_err(),
            "truncated prefix{n}"
        );
    }
}
#[test]
fn rejects_header_ranges_packets_allocations_and_versions() {
    let valid = dump();
    for (offset, value) in [
        (0, 0),
        (4, u32::MAX),
        (8, 10),
        (12, u32::MAX),
        (16, u32::MAX),
        (20, u32::MAX),
        (28, u32::MAX),
        (36, u32::MAX),
    ] {
        let mut b = valid.clone();
        b[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        assert!(
            gs::validate(&b[..], gs::BYTE_LIMIT).is_err(),
            "offset{offset}"
        );
    }
    let packet_start = 8 + 44 + gs::FREEZE_BYTES as usize + 8192;
    for (offset, byte) in [
        (packet_start, 9),
        (packet_start + 1, 4),
        (packet_start + 2, 15),
    ] {
        let mut b = valid.clone();
        b[offset] = byte;
        assert!(gs::validate(&b[..], gs::BYTE_LIMIT).is_err());
    }
    let mut b = valid;
    b.extend([2]);
    b.extend(u32::MAX.to_le_bytes());
    assert!(gs::validate(&b[..], gs::BYTE_LIMIT).is_err());
    assert!(gs::validate(&[][..], gs::BYTE_LIMIT + 1).is_err());
}
#[test]
fn finite_vsync_limit_and_clean_prefix_are_distinct() {
    let mut b = dump();
    for _ in 1..gs::VSYNC_LIMIT {
        b.extend([1, 0]);
    }
    assert_eq!(
        gs::validate(&b[..], gs::BYTE_LIMIT).unwrap().vsyncs,
        gs::VSYNC_LIMIT
    );
    b.extend([1, 0]);
    assert!(gs::validate(&b[..], gs::BYTE_LIMIT).is_err());
    let mut b = dump();
    b.extend([0, 1]);
    b.extend(16u32.to_le_bytes());
    b.extend([0; 16]);
    let r = gs::validate(&b[..], gs::BYTE_LIMIT).unwrap();
    assert!(!r.ends_at_vsync); // Valid packet EOF is not a completed captured interval.
}
#[test]
fn queue_acceptance_does_not_imply_capture_started_and_caps_do_not_wrap() {
    let mut p = Ledger::new(Mode::Capture);
    assert_eq!(p.event(1, 1, 0), 2);
    assert_eq!(p.event(2, 1, 0), 1);
    assert_eq!(p.phase, Phase::Queued);
    assert_eq!(p.started_at, None);
    assert_eq!(p.event(3, u64::from(gs::FREEZE_BYTES), 0), 1);
    assert_eq!(p.started_at, None);
    assert_eq!(p.event(16, u64::from(gs::FREEZE_BYTES), 0), 1);
    assert_eq!(p.started_at, None);
    assert_eq!(p.event(15, 1, 0), 1);
    assert_eq!(p.phase, Phase::Recording);
    assert_eq!(p.event(4, gs::BYTE_LIMIT, 0), 1);
    assert_eq!(p.event(4, u64::MAX, 0), 0);
    assert_eq!(p.first_failure.as_deref(), Some("decoded cap"));
    assert_eq!(p.event(1, 2, 0), 0); // No new snapshot/GPU work after stop.
    assert_eq!(p.event(5, 9, 0), 1); // Existing CPU-side compression may finalize.
    assert_eq!(p.event(8, 1, 0), 1);
    assert!(p.codec_closed);
    assert_eq!(p.phase, Phase::Stopped);
    assert_eq!(p.first_failure.as_deref(), Some("decoded cap"));
}
#[test]
fn cancellation_startup_failure_replay_order_and_process_exit_are_not_passes() {
    let mut p = Ledger::new(Mode::Capture);
    p.stop("owner cancelled");
    assert_eq!(p.event(1, 1, 0), 0);
    assert_eq!(p.event(9, 1, 0), 0);
    assert_eq!(p.first_failure.as_deref(), Some("owner cancelled"));
    let mut p = Ledger::new(Mode::Capture);
    p.event(1, 1, 0);
    assert_eq!(p.event(2, 0, 0), 0);
    assert_eq!(p.started_at, None);
    let mut p = Ledger::new(Mode::ModifiedReplay);
    assert_eq!(p.event(12, 0, 0), 0); // Exit0 before state/packets proves nothing.
    let mut p = Ledger::new(Mode::ModifiedReplay);
    assert_eq!(p.event(10, 0, 0), 1);
    assert_eq!(p.event(11, 0, 0), 1);
    assert_eq!(p.event(11, 1, 1), 1);
    assert_eq!(p.event(11, 0, 0), 0); // A second loop is forbidden.
    let mut p = Ledger::new(Mode::PinnedReplay);
    p.event(10, 0, 0);
    p.event(11, 0, 1);
    assert_eq!(p.event(12, 1, 0), 1);
    assert_eq!(p.phase, Phase::Closed);
    assert_eq!(p.first_failure, None); // Still needs equivalent output/fault observations.
}

/// Actual retained codec, constructor and destructor against real host zstd.
/// Only file/header surroundings are stubbed; this tests EOF/trailer semantics,
/// not GPU behavior. Original empty-input end skips the trailer at an exact
/// chunk boundary; the narrow correction must complete it without more input.
#[test]
fn retained_zstd_close_after_exact_chunk_requires_the_empty_end_call() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repo.join("vendor/armsx2/pcsx2/GS/GSDump.cpp")).unwrap();
    let start = source.find("namespace\n{\n\tclass GSDumpZst").unwrap();
    let end = source[start..].find("} // namespace").unwrap() + start + "} // namespace".len();
    let original = &source[start..end];
    let dir = std::env::temp_dir().join(format!("psx-gs-codec-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let native = dir.join("pcsx2/GS/GSDump.cpp");
    fs::create_dir_all(native.parent().unwrap()).unwrap();
    fs::write(&native, &source).unwrap();
    let applied = Command::new("git")
        .args(["apply", "--include=pcsx2/GS/GSDump.cpp"])
        .arg(repo.join("patches/armsx2/0008-development-gs-replay.patch"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let patched = fs::read_to_string(native).unwrap();
    let start = patched.find("namespace\n{\n\tclass GSDumpZst").unwrap();
    let end = patched[start..].find("} // namespace").unwrap() + start + "} // namespace".len();
    let corrected = &patched[start..end];
    assert_ne!(corrected, original);
    let prefix = r#"
#include <zstd.h>
#include <vector>
#include <string>
#include <cstdint>
#include <cstring>
using u8=uint8_t;using u32=uint32_t;static constexpr size_t _1mb=1024*1024;
struct freezeData{};struct GSPrivRegSet{};
struct ConsoleT {template<class...A>void ErrorFmt(A...) {}};static ConsoleT Console;
static std::vector<u8> bytes;
class GSDumpBase {public:GSDumpBase(std::string){};virtual~GSDumpBase()=default;
virtual void AppendRawData(const void*,size_t)=0;virtual void AppendRawData(u8)=0;
void AddHeader(const std::string&,u32,u32,u32,const u32*,const freezeData&,const GSPrivRegSet*){}
void Write(const void*p,size_t n){auto b=(const u8*)p;bytes.insert(bytes.end(),b,b+n);}};
"#;
    let suffix = r#"
int main(int argc,char**) {
  {GSDumpZst dump("fixture","",0,0,0,nullptr,freezeData{},nullptr);
   auto* writer=static_cast<GSDumpBase*>(&dump);
   std::vector<u8> data(argc==1?_1mb:17,42);writer->AppendRawData(data.data(),data.size());}
  auto* d=ZSTD_createDStream();std::vector<u8> out(_1mb*2);
  ZSTD_inBuffer in{bytes.data(),bytes.size(),0};ZSTD_outBuffer dst{out.data(),out.size(),0};
  size_t left=ZSTD_decompressStream(d,&dst,&in);ZSTD_freeDStream(d);
  return ZSTD_isError(left)||left!=0||in.pos!=in.size?23:0;
}
"#;
    for (name, body, expected) in [("original", original, 23), ("corrected", corrected, 0)] {
        let cpp = dir.join(format!("{name}.cpp"));
        fs::write(&cpp, format!("{prefix}{body}{suffix}")).unwrap();
        let exe = dir.join(name);
        let o = Command::new("c++")
            .args(["-std=c++17", "-O0"])
            .arg(&cpp)
            .arg("-I")
            .arg(repo.join("vendor/armsx2/platforms/android/app/src/main/cpp/3rdparty/zstd/lib"))
            .arg("-l:libzstd.so.1")
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        assert_eq!(Command::new(&exe).status().unwrap().code(), Some(expected));
        assert!(Command::new(&exe).arg("short").status().unwrap().success());
    }
}

#[test]
fn decoder_status_and_complete_trailer_are_required_independently_of_packet_parse() {
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("psx-gs-decode-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let path = dir.join("synthetic.gs");
    fs::write(&path, dump()).unwrap();
    let child = |status: &str| {
        Command::new("sh")
            .args(["-c", "cat \"$1\"; exit \"$2\"", "fixture"])
            .arg(&path)
            .arg(status)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap()
    };
    assert!(gs::validate_decoder(child("0")).is_ok());
    assert!(gs::validate_decoder(child("7")).is_err());
}

#[test]
fn retained_owner_queue_dispatch_and_snapshot_start_are_separate() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("psx-gs-queue-{}", std::process::id()));
    let relative = "pcsx2/GS/Renderers/Common/GSRenderer.cpp";
    let native = dir.join(relative);
    fs::create_dir_all(native.parent().unwrap()).unwrap();
    fs::copy(repo.join("vendor/armsx2").join(relative), &native).unwrap();
    let applied = Command::new("git")
        .args([
            "apply",
            "--include=pcsx2/GS/Renderers/Common/GSRenderer.cpp",
        ])
        .arg(repo.join("patches/armsx2/0008-development-gs-replay.patch"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(
        applied.status.success(),
        "{}",
        String::from_utf8_lossy(&applied.stderr)
    );
    let s = fs::read_to_string(native).unwrap();
    let start = s.find("bool GSRenderer::QueueSnapshot(").unwrap();
    let end = s[start..]
        .find("static std::string GSGetBaseFilename")
        .unwrap()
        + start;
    let queue = &s[start..end];
    let start = s.find("void GSRenderer::VSync(u32 field").unwrap();
    let end = s[start..].find("\tif (GSConfig.ShouldDump").unwrap() + start;
    let dispatch = &s[start..end];
    let cpp = format!(
        r#"
#include <string>
#include <cstdint>
#include <cassert>
#include <memory>
#include "GSReplay.h"
#include "GSSnapshotPolicy.h"
using u32=uint32_t;
enum class GSDumpCompressionMethod {{Zstandard}};
struct {{GSDumpCompressionMethod GSDumpCompression;}} GSConfig;
struct {{unsigned GetFrame(){{return 7;}}}} g_perfmon;
namespace StringUtil {{bool EndsWithNoCase(const std::string&s,const char*){{return s.size()>=4&&s.substr(s.size()-4)==".png";}}}}
std::string GSGetBaseSnapshotFilename(){{return "fallback";}}
static unsigned gpu=0,recorded_at_gpu=999,recorded=0;
struct Dump {{bool VSync(u32,bool,void*){{++recorded;recorded_at_gpu=gpu;return false;}}}};
struct GSRenderer {{std::string m_snapshot;u32 m_snapshot_dump_frames=0;std::unique_ptr<Dump>m_dump;void*m_regs=nullptr;
bool QueueSnapshot(const std::string&,u32);void VSync(u32,bool,bool);}};
{queue}
{dispatch}
++gpu;
}}
static unsigned requests=0,accepted=0,started=0;static bool fault=false;
unsigned event(unsigned e,uint64_t a,uint64_t){{
 if(fault)return 0;
 if(e==1){{++requests;return requests==1?2:1;}}
 if(e==2){{accepted+=a;return a?1:0;}}
 if(e==3)++started;return 1;
}}
int main(){{
 assert(!GSReplay::Configure(0,"capture.png",event));
 assert(GSReplay::Configure(1,"capture.png",event));
 assert(!GSReplay::Configure(1,"other.png",event));
 GSRenderer r;r.VSync(0,false,false);
 assert(requests==1&&accepted==1&&started==0);
 assert(r.m_snapshot=="capture"&&r.m_snapshot_dump_frames==1024);
 assert(!r.QueueSnapshot("other.png",10));
 auto opening=SelectGSSnapshotAction(true,r.m_snapshot_dump_frames,false,0);
 assert(opening.open_dump&&!opening.record_vsync);
 assert(GSReplay::Emit(3,1024,0));assert(started==1);
 r.m_dump=std::make_unique<Dump>();r.VSync(0,false,false);
 assert(recorded==1&&recorded_at_gpu==1&&gpu==2);
 fault=true;r.VSync(0,false,false);assert(requests==2&&accepted==1&&recorded==1&&gpu==2);
 GSReplay::Clear();assert(!GSReplay::callback);
}}
"#
    );
    let path = dir.join("queue.cpp");
    fs::create_dir(dir.join("common")).unwrap();
    fs::write(
        dir.join("common/Pcsx2Types.h"),
        "#pragma once\n#include <cstdint>\nusing u32=uint32_t;\n",
    )
    .unwrap();
    fs::write(&path, cpp).unwrap();
    let exe = dir.join("queue");
    let o = Command::new("c++")
        .args(["-std=c++17", "-DPSX_GS_REPLAY1=1", "-I"])
        .arg(repo.join("patches/armsx2/gs-replay"))
        .arg("-I")
        .arg(&dir)
        .arg("-I")
        .arg(repo.join("vendor/armsx2/pcsx2/GS/Renderers/Common"))
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(Command::new(exe).status().unwrap().success());
}

#[test]
fn donor_gate_requires_positive_output_dependency_reproduction_and_retirement() {
    use psx_player::gs_replay::reproduction;
    let stream = gs::validate(&dump()[..], gs::BYTE_LIMIT).unwrap();
    let mut ledger = Ledger::new(Mode::ModifiedReplay);
    ledger.event(10, 0, 0);
    ledger.event(11, 0, 0);
    ledger.event(11, 1, 1);
    ledger.stop("existing first-fault/owner stop");
    let digest = "a".repeat(64);
    let snapshot = serde_json::json!({"first_failure":{"stage":16,"owner":1,"frame":2,"data":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1],"detail":[1,1,1,1,0,0,1,1]},"output_sync":{"created":2,"retired_on_creator":1,"invalid_names_released":1,"live":0,"untracked_creates":0,"frontend_sync_operations":0,"context_reclaim_pending":0,"native_joined":true,"first_ownership_failure":null,"overflow":false},"common":{"native_context_destroyed":true,"frontend_context_destroyed":true,"phase_entries":[0,0,1,0,1,1,1]}});
    assert!(reproduction(&stream, &ledger, &snapshot, &digest, &digest).is_ok());
    let mut missing_state = stream.clone();
    missing_state.readback_provenance = false;
    assert!(reproduction(&missing_state, &ledger, &snapshot, &digest, &digest).is_err());
    for path in [
        "/first_failure/stage",
        "/first_failure/data/26",
        "/output_sync/native_joined",
        "/common/phase_entries/4",
    ] {
        let mut bad = snapshot.clone();
        *bad.pointer_mut(path).unwrap() = serde_json::json!(0);
        assert!(
            reproduction(&stream, &ledger, &bad, &digest, &digest).is_err(),
            "{path}"
        );
    }
    let mut clean = snapshot.clone();
    clean["first_failure"] = serde_json::Value::Null;
    assert!(reproduction(&stream, &ledger, &clean, &digest, &digest).is_err());
    assert!(reproduction(&stream, &ledger, &snapshot, &digest, &"b".repeat(64)).is_err());
    ledger.mode = Mode::PinnedReplay;
    assert!(reproduction(&stream, &ledger, &snapshot, &digest, &digest).is_err());
}

#[test]
fn runner_ini_mirrors_actual_post_load_gs_answers_without_a_tuning_catalog() {
    let original="[EmuCore]\nEnableGameFixes = true\n[EmuCore/GS]\nAspectRatio = 4:3\nHWDownloadMode = 2\nupscale_multiplier = 3\nAccurateBlendingUnit = 1\n[Other]\nunchanged = yes\n";
    let matched = gs::matched_runner_ini(original).unwrap();
    assert!(matched.contains("AspectRatio = Stretch\nHWDownloadMode = 0\n"));
    assert!(matched.contains("AccurateBlendingUnit = 1\n"));
    assert!(matched.ends_with("[Other]\nunchanged = yes\n"));
    assert_eq!(matched, gs::matched_runner_ini(&matched).unwrap());
    for invalid in [
        "",
        "[EmuCore/GS]\na=1\na=2",
        "[EmuCore/GS]\n[EmuCore/GS]\n",
        "[EmuCore/GS]\nnot-an-entry",
    ] {
        assert!(gs::matched_runner_ini(invalid).is_err());
    }
}

#[cfg(target_os = "linux")]
#[test]
fn real_streaming_zstd_rejects_trailers_concatenation_and_decoded_overflow() {
    use psx_player::gs_replay_zstd::Decoder;
    let bytes = dump();
    let compressed = unsafe {
        let library = libloading::Library::new("libzstd.so.1").unwrap();
        let bound = library
            .get::<unsafe extern "C" fn(usize) -> usize>(b"ZSTD_compressBound\0")
            .unwrap();
        let compress = library
            .get::<unsafe extern "C" fn(
                *mut std::ffi::c_void,
                usize,
                *const std::ffi::c_void,
                usize,
                i32,
            ) -> usize>(b"ZSTD_compress\0")
            .unwrap();
        let error = library
            .get::<unsafe extern "C" fn(usize) -> u32>(b"ZSTD_isError\0")
            .unwrap();
        let mut output = vec![0u8; bound(bytes.len())];
        let n = compress(
            output.as_mut_ptr().cast(),
            output.len(),
            bytes.as_ptr().cast(),
            bytes.len(),
            1,
        );
        assert_eq!(error(n), 0);
        output.truncate(n);
        output
    };
    let expected = gs::validate(&bytes[..], gs::BYTE_LIMIT).unwrap();
    assert_eq!(
        gs::validate(
            Decoder::new(&compressed[..], gs::BYTE_LIMIT).unwrap(),
            gs::BYTE_LIMIT
        )
        .unwrap(),
        expected
    );
    for n in [0, 1, compressed.len() / 2, compressed.len() - 1] {
        assert!(gs::validate(
            Decoder::new(&compressed[..n], gs::BYTE_LIMIT).unwrap(),
            gs::BYTE_LIMIT
        )
        .is_err());
    }
    let mut trailing = compressed.clone();
    trailing.push(0);
    assert!(gs::validate(
        Decoder::new(&trailing[..], gs::BYTE_LIMIT).unwrap(),
        gs::BYTE_LIMIT
    )
    .is_err());
    trailing.pop();
    trailing.extend(&compressed);
    assert!(gs::validate(
        Decoder::new(&trailing[..], gs::BYTE_LIMIT).unwrap(),
        gs::BYTE_LIMIT
    )
    .is_err());
    let mut decoder = Decoder::new(&compressed[..], 1024).unwrap();
    assert!(gs::validate(&mut decoder, gs::BYTE_LIMIT).is_err());
    assert!(decoder.decoded_bytes() <= 1024);
}

#[test]
fn actual_native_reader_accepts_the_same_format_and_reveals_why_strict_eof_is_needed() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = fs::read_to_string(repo.join("vendor/armsx2/pcsx2/GS/GSLzma.cpp")).unwrap();
    let start = source
        .find("bool GSDumpFile::ReadFile(Error* error)")
        .unwrap();
    let end = source[start..]
        .find("/******************************************************************/")
        .unwrap()
        + start;
    let reader = &source[start..end];
    let header = fs::read_to_string(repo.join("vendor/armsx2/pcsx2/GS/GSDump.h")).unwrap();
    let start = header.find("#pragma pack(push, 4)").unwrap();
    let end = header.find("#pragma pack(pop)").unwrap() + "#pragma pack(pop)".len();
    let declarations = &header[start..end];
    let dir = std::env::temp_dir().join(format!("psx-gs-reader-{}", std::process::id()));
    fs::create_dir(&dir).unwrap();
    let bytes = dump();
    fs::write(dir.join("synthetic.gs"), &bytes).unwrap();
    let prefix = r#"
#include <cstdint>
#include <vector>
#include <string>
#include <cstring>
#include <algorithm>
#include <fstream>
#include <iterator>
using u8=uint8_t;using u32=uint32_t;using u64=uint64_t;static constexpr size_t _1mb=1024*1024;
namespace GSDumpTypes {enum class GSType:u8{Transfer=0,VSync=1,ReadFIFO2=2,Registers=3};enum class GSTransferPath:u8{Dummy=4};}
using namespace GSDumpTypes;
struct Error{template<class...A>static void SetString(A...){}template<class...A>static void SetStringFmt(A...){}};
struct ConsoleT{template<class...A>void Error(A...){}};static ConsoleT Console;
#define TRANSLATE_STR(c,m) m
#define TRANSLATE_FS(c,m) m
"#;
    let class = r#"
class GSDumpFile {public:
struct GSData{GSType id;const u8*data;size_t length;GSTransferPath path;};
std::vector<u8>m_state_data,m_regs_data,m_packet_data,fixture;std::vector<GSData>m_dump_packets;u32 m_crc=0;std::string m_serial;bool m_has_provenance=false,m_targets_read_back=false;size_t offset=0;
size_t Read(void*p,size_t n){n=std::min(n,fixture.size()-offset);memcpy(p,fixture.data()+offset,n);offset+=n;return n;}
bool IsEof(){return offset==fixture.size();}bool ReadFile(Error*);
};
"#;
    let main = r#"
int main(int argc,char**argv){std::ifstream file(argv[1],std::ios::binary);std::vector<u8>b((std::istreambuf_iterator<char>(file)),{});Error e;
GSDumpFile valid;valid.fixture=b;if(!valid.ReadFile(&e)||valid.m_dump_packets.size()!=3||!valid.m_targets_read_back)return 31;
GSDumpFile truncated;b.pop_back();truncated.fixture=b;
if(!truncated.ReadFile(&e)||truncated.m_dump_packets.size()!=2)return 32;
return 0;}
"#;
    let cpp = dir.join("reader.cpp");
    fs::write(&cpp, format!("{prefix}{declarations}{class}{reader}{main}")).unwrap();
    let exe = dir.join("reader");
    let o = Command::new("c++")
        .args(["-std=c++17", "-O0"])
        .arg(cpp)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(Command::new(exe)
        .arg(dir.join("synthetic.gs"))
        .status()
        .unwrap()
        .success());
    assert!(gs::validate(&bytes[..bytes.len() - 1], gs::BYTE_LIMIT).is_err());
}

#[test]
fn retained_replayer_restoration_failure_stops_before_packets_and_one_pass_waits_before_eof() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("psx-gs-step-{}", std::process::id()));
    let relative = "pcsx2/GSDumpReplayer.cpp";
    fs::create_dir_all(dir.join("pcsx2")).unwrap();
    fs::copy(
        repo.join("vendor/armsx2").join(relative),
        dir.join(relative),
    )
    .unwrap();
    let o = Command::new("git")
        .args(["apply", "--include=pcsx2/GSDumpReplayer.cpp"])
        .arg(repo.join("patches/armsx2/0008-development-gs-replay.patch"))
        .current_dir(&dir)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let s = fs::read_to_string(dir.join(relative)).unwrap();
    let start = s
        .find("static void GSDumpReplayerLoadInitialState()\n{")
        .unwrap();
    let end = s[start..]
        .find("static void GSDumpReplayerSendPacketToMTGS")
        .unwrap()
        + start;
    let load = &s[start..end];
    let start = s.find("void GSDumpReplayerCpuStep()\n{").unwrap();
    let end = s[start..]
        .find("void GSDumpReplayerExitExecution()")
        .unwrap()
        + start;
    let step = &s[start..end];
    let cpp = format!(
        r#"
#include <cstdint>
#include <cstring>
#include <vector>
#include <memory>
#include <cassert>
#include "GSReplay.h"
using u8=uint8_t;using u32=uint32_t;using u64=uint64_t;using s32=int32_t;
namespace GSDumpTypes{{enum class GSType{{Transfer,VSync,ReadFIFO2,Registers}};enum class GSTransferPath{{Path1Old,Path2,Path3,Path1New}};}}
struct GSDumpFile{{struct GSData{{GSDumpTypes::GSType id;const u8*data;size_t length;GSDumpTypes::GSTransferPath path;}};std::vector<u8>regs=std::vector<u8>(8192),state=std::vector<u8>(16);std::vector<GSData>packets;
auto&GetRegsData(){{return regs;}}auto&GetStateData(){{return state;}}auto&GetPackets(){{return packets;}}}};
static GSDumpFile file;static auto*s_dump_file=&file;static unsigned s_current_packet=0,s_dump_frame_number=0;static int s_dump_loop_count=0;static bool s_dump_running=false,s_needs_state_loaded=true;static u64 s_next_frame_time=0;
static void(*s_initial_state_hook)()=nullptr;static void(*s_packet_hook)(u32,bool)=nullptr;
static unsigned packets=0,vsyncs=0,waits=0,eofs=0,errors=0,starts=0;static int restore=0;
namespace Ps2MemSize{{constexpr u32 GSregs=8192;}}static u8 PS2MEM_GS[8192];
struct freezeData{{int size;u8*data;}};enum class FreezeAction{{Load}};
namespace MTGS{{struct FreezeData{{freezeData*data;int retval;}};void Freeze(FreezeAction,FreezeData&d){{d.retval=restore;}}void PostVsyncStart(bool){{++vsyncs;}}void InitAndReadFIFO(u8*,u32){{}}void WaitGS(){{++waits;}}}}
enum GIF_PATH{{GIF_PATH_1=1,GIF_PATH_2=2,GIF_PATH_3=3}};
static void GSDumpReplayerSendPacketToMTGS(GIF_PATH,const u8*,size_t){{}}
static void GSDumpReplayerUpdateFrameLimit(){{}}static void GSDumpReplayerFrameLimit(){{}}
static void GSDumpReplayerExitExecution(){{s_dump_running=false;}}
static u64 GetCPUTicks(){{return 0;}}
namespace VMManager{{namespace Internal{{void VSyncOnCPUThread(){{}}bool IsExecutionInterrupted(){{return false;}}}}}}
namespace Host{{template<class...A>void ReportFormattedErrorAsync(A...){{++errors;}}void RequestVMShutdown(bool,bool,bool){{}}void PumpMessagesOnCPUThread(){{}}}}
struct ConsoleT{{template<class...A>void Error(A...){{}}}};static ConsoleT Console;
{load}
{step}
unsigned event(unsigned e,uint64_t a,uint64_t){{if(e==9)++errors;if(e==12){{assert(waits==1&&vsyncs==1&&packets==1&&a==1);++eofs;}}return 1;}}
int main(){{
 u8 field=0;file.packets.push_back({{GSDumpTypes::GSType::VSync,&field,1,GSDumpTypes::GSTransferPath::Path1New}});
 assert(GSReplay::Configure(2,"fixture.gs.zst",event));s_initial_state_hook=[](){{++starts;}};s_packet_hook=[](u32 i,bool v){{assert(i==0&&v);++packets;}};
 GSDumpReplayerCpuExecute();assert(!s_dump_running&&starts==1&&packets==1&&vsyncs==1&&eofs==1&&s_current_packet==0);
 GSReplay::Clear();assert(GSReplay::Configure(2,"fixture.gs.zst",event));
 restore=-1;s_needs_state_loaded=true;starts=packets=vsyncs=waits=eofs=errors=0;
 GSDumpReplayerCpuExecute();assert(!s_dump_running&&starts==0&&packets==0&&vsyncs==0&&eofs==0&&errors>=1);
 GSReplay::Clear();
}}
"#
    );
    let path = dir.join("step.cpp");
    fs::write(&path, cpp).unwrap();
    let exe = dir.join("step");
    let o = Command::new("c++")
        .args(["-std=c++17", "-DPSX_GS_REPLAY1=1", "-I"])
        .arg(repo.join("patches/armsx2/gs-replay"))
        .arg(&path)
        .arg("-o")
        .arg(&exe)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    assert!(Command::new(exe).status().unwrap().success());
}
