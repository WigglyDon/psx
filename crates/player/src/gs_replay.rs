//! GS-REPLAY-1 development policy and strict validator for the retained donor
//! format. No game bytes or decoded state are retained in public evidence.
//! This validates a bounded stream; it does not replace the donor replayer.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Read};

pub const BYTE_LIMIT: u64 = 1024 * 1024 * 1024;
/// Reserve aggregate phone output for bounded logs, numeric observer exports
/// and the correspondence sidecar; never fill the entire envelope with a dump.
pub const ENCODED_LIMIT: u64 = BYTE_LIMIT - 16 * 1024 * 1024;
pub const VSYNC_LIMIT: u64 = 1024;
pub const PACKET_LIMIT: u64 = 1_048_576;
pub const STATE_VERSION: u32 = 9;
/// Pinned GSReplayPayload's audited version9 layout:425 register/transfer
/// bytes +4MiB local memory +84 GIF-path/trailing bytes. Native defrost is
/// still required and must report success before the first packet executes.
pub const FREEZE_BYTES: u32 = 425 + 4 * 1024 * 1024 + 84;
type Result<T> = std::result::Result<T, String>;

/// Mirror the three effective GS answers that the libretro adapter applies
/// after loading/saving its base INI. The native runner has no GET_VARIABLE
/// callback; copying the earlier on-disk INI alone would silently lose these.
/// Renderer is explicitly selected by the runner bridge, not a numeric enum
/// guessed here. All other sections and settings remain the admitted inputs.
pub fn matched_runner_ini(input: &str) -> Result<String> {
    use std::collections::BTreeSet;
    if input.len() > 256 * 1024 || input.contains('\0') {
        return Err("INI bound".into());
    }
    let mut output = String::new();
    let mut in_gs = false;
    let mut seen_section = false;
    let mut seen = BTreeSet::new();
    let add = |output: &mut String| {
        output.push_str("upscale_multiplier = 1\nAspectRatio = Stretch\nHWDownloadMode = 0\n");
    };
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if in_gs {
                add(&mut output);
            }
            in_gs = trimmed == "[EmuCore/GS]";
            if in_gs {
                if seen_section {
                    return Err("duplicate GS INI section".into());
                }
                seen_section = true;
            }
        }
        if in_gs
            && !trimmed.starts_with('[')
            && !trimmed.starts_with([';', '#'])
            && !trimmed.is_empty()
        {
            let (key, _) = trimmed.split_once('=').ok_or("invalid GS INI entry")?;
            let key = key.trim();
            if !seen.insert(key.to_owned()) {
                return Err("duplicate GS INI key".into());
            }
            if ["upscale_multiplier", "AspectRatio", "HWDownloadMode"].contains(&key) {
                continue;
            }
        }
        output.push_str(line);
        output.push('\n');
    }
    if in_gs {
        add(&mut output);
    }
    if !seen_section {
        return Err("missing GS INI section".into());
    }
    if output.len() > 256 * 1024 {
        return Err("matched INI bound".into());
    }
    Ok(output)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Stream {
    pub decoded_bytes: u64,
    pub decoded_sha256: String,
    pub state_bytes: u32,
    pub packets: u64,
    pub transfers: u64,
    pub vsyncs: u64,
    pub last_vsync_packet: u64,
    pub ends_at_vsync: bool,
    pub readback_provenance: bool,
}
struct Checked<R> {
    input: R,
    bytes: u64,
    hash: Sha256,
    limit: u64,
}
impl<R: Read> Checked<R> {
    fn read(&mut self, out: &mut [u8]) -> Result<usize> {
        // The one-byte EOF probe at the limit is allowed, but never accepted as
        // data. It prevents a stream exactly at the ceiling hiding more bytes.
        let remaining = self.limit.saturating_sub(self.bytes);
        let count = out.len().min(remaining.max(1) as usize);
        let n = self
            .input
            .read(&mut out[..count])
            .map_err(|e| e.to_string())?;
        self.bytes = self.bytes.checked_add(n as u64).ok_or("byte overflow")?;
        if self.bytes > self.limit {
            return Err("decoded byte limit".into());
        }
        self.hash.update(&out[..n]);
        Ok(n)
    }
    fn exact(&mut self, mut out: &mut [u8]) -> Result<()> {
        while !out.is_empty() {
            let n = self.read(out)?;
            if n == 0 {
                return Err("truncated dump".into());
            }
            out = &mut out[n..];
        }
        Ok(())
    }
    fn word(&mut self) -> Result<u32> {
        let mut b = [0; 4];
        self.exact(&mut b)?;
        Ok(u32::from_le_bytes(b))
    }
    fn skip(&mut self, mut count: u64) -> Result<()> {
        if count > self.limit.saturating_sub(self.bytes) {
            return Err("declared byte limit".into());
        }
        let mut b = [0; 65536];
        while count != 0 {
            let n = count.min(b.len() as u64) as usize;
            self.exact(&mut b[..n])?;
            count -= n as u64;
        }
        Ok(())
    }
}
/// Modern pinned GSDumpHeader (9 u32s), freeze and 8192-byte registers, then
/// the donor's transfer/VSync/FIFO/register packet stream. Reject rather than
/// emulate the donor reader's permissive final-packet dropping. The caller
/// must additionally validate the container/trailer and immutable file digest.
pub fn validate(input: impl Read, limit: u64) -> Result<Stream> {
    if limit == 0 || limit > BYTE_LIMIT {
        return Err("invalid decoded bound".into());
    }
    let mut r = Checked {
        input,
        bytes: 0,
        hash: Sha256::new(),
        limit,
    };
    if r.word()? != u32::MAX {
        return Err("unsupported legacy dump header".into());
    }
    let header_size = r.word()?;
    if !(36..=4 * 1024 * 1024).contains(&header_size) {
        return Err("header bound".into());
    }
    let mut header = vec![0; header_size as usize];
    r.exact(&mut header)?;
    let word = |i: usize| u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().unwrap());
    if word(0) != STATE_VERSION {
        return Err("unsupported GS state version".into());
    }
    let state_bytes = word(1);
    if state_bytes != FREEZE_BYTES {
        return Err("state bound".into());
    }
    let in_header = |offset: u32, len: u32| -> bool {
        offset >= 36 && u64::from(offset) + u64::from(len) <= u64::from(header_size)
    };
    if !in_header(word(2), word(3)) || !in_header(word(7), word(8)) {
        return Err("header subrange".into());
    }
    let pixels = u64::from(word(5))
        .checked_mul(u64::from(word(6)))
        .and_then(|v| v.checked_mul(4))
        .ok_or("image overflow")?;
    if pixels != u64::from(word(8)) || (word(5) == 0) != (word(6) == 0) {
        return Err("image dimensions disagree".into());
    }
    let provenance = (u64::from(word(7)) + u64::from(word(8))) as usize;
    let readback_provenance = header
        .get(provenance..provenance.saturating_add(8))
        .is_some_and(|b| {
            b[..4] == 0x564f5250u32.to_le_bytes()
                && u32::from_le_bytes(b[4..].try_into().unwrap()) & 1 != 0
        });
    if r.word()? != STATE_VERSION {
        return Err("initial state version disagrees".into());
    }
    r.skip(u64::from(state_bytes - 4))?;
    r.skip(8192)?;
    let (mut packets, mut vsyncs, mut transfers, mut last_vsync_packet, mut ends_at_vsync) =
        (0u64, 0u64, 0u64, 0u64, false);
    loop {
        let mut tag = [0];
        if r.read(&mut tag)? == 0 {
            break;
        }
        packets = packets
            .checked_add(1)
            .filter(|n| *n <= PACKET_LIMIT)
            .ok_or("packet bound")?;
        ends_at_vsync = tag[0] == 1;
        match tag[0] {
            0 => {
                let mut path = [0];
                r.exact(&mut path)?;
                let len = r.word()?;
                if path[0] > 3 || len == 0 || len % 16 != 0 || (path[0] == 0 && len > 16384) {
                    return Err("invalid GIF transfer".into());
                }
                r.skip(u64::from(len))?;
                transfers += 1;
            }
            1 => {
                let mut field = [0];
                r.exact(&mut field)?;
                if field[0] > 1 {
                    return Err("invalid VSync field".into());
                }
                vsyncs += 1;
                last_vsync_packet = packets - 1;
                if vsyncs > VSYNC_LIMIT {
                    return Err("VSync bound".into());
                }
            }
            2 => {
                let qwords = r.word()?;
                if qwords == 0 || u64::from(qwords) + 1 > 4 * 1024 * 1024 {
                    return Err("FIFO allocation bound".into());
                }
            }
            3 => r.skip(8192)?,
            _ => return Err("unsupported packet".into()),
        }
    }
    if transfers == 0 || vsyncs == 0 {
        return Err("no meaningful GS interval".into());
    }
    Ok(Stream {
        decoded_bytes: r.bytes,
        decoded_sha256: format!("{:x}", r.hash.finalize()),
        state_bytes,
        packets,
        transfers,
        vsyncs,
        last_vsync_packet,
        ends_at_vsync,
        readback_provenance,
    })
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Mode {
    Capture,
    ModifiedReplay,
    PinnedReplay,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Phase {
    Armed,
    Queued,
    Frozen,
    Recording,
    Replaying,
    Closed,
    Stopped,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ledger {
    pub mode: Mode,
    pub phase: Phase,
    pub boundaries: u64,
    pub accepted_at: Option<u64>,
    pub started_at: Option<u64>,
    pub ended_at: Option<u64>,
    pub vsyncs: u64,
    pub packets: u64,
    pub decoded_bytes: u64,
    pub encoded_bytes: u64,
    pub initial_state_bytes: u64,
    pub last_packet: Option<u64>,
    pub codec_closed: bool,
    pub first_failure: Option<String>,
    pub limit: u64,
}
impl Ledger {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            phase: Phase::Armed,
            boundaries: 0,
            accepted_at: None,
            started_at: None,
            ended_at: None,
            vsyncs: 0,
            packets: 0,
            decoded_bytes: 0,
            encoded_bytes: 0,
            initial_state_bytes: 0,
            last_packet: None,
            codec_closed: false,
            first_failure: None,
            limit: BYTE_LIMIT,
        }
    }
    pub fn stop(&mut self, reason: &'static str) -> u32 {
        self.first_failure.get_or_insert_with(|| reason.to_owned());
        self.phase = Phase::Stopped;
        0
    }
    fn add(value: &mut u64, n: u64, limit: u64) -> bool {
        if let Some(v) = value.checked_add(n).filter(|v| *v <= limit) {
            *value = v;
            true
        } else {
            false
        }
    }
    /// Narrow owner callback. 1 permits work, 2 requests the *first* queued
    /// capture, 0 refuses/stops. Closure and encoded CPU-only trailer writes
    /// remain possible after fault; no capture/readback/packet append does.
    pub fn event(&mut self, event: u32, a: u64, b: u64) -> u32 {
        if event == 8 {
            self.codec_closed = a == 1;
            self.ended_at = Some(self.boundaries);
            if a != 1 {
                return self.stop("codec/file closure failed");
            }
            if self.first_failure.is_none() {
                self.phase = Phase::Closed;
            }
            return 1;
        }
        if event == 5 {
            return if Self::add(&mut self.encoded_bytes, a, self.limit.min(ENCODED_LIMIT)) {
                1
            } else {
                self.stop("encoded cap")
            };
        }
        if self.first_failure.is_some() || matches!(self.phase, Phase::Closed | Phase::Stopped) {
            return 0;
        }
        match event {
            1 => {
                if a != self.boundaries + 1 {
                    return self.stop("GS boundary order");
                }
                self.boundaries = a;
                if self.mode == Mode::Capture && self.phase == Phase::Armed {
                    return 2;
                }
            }
            2 if self.phase == Phase::Armed && self.mode == Mode::Capture => {
                if a != 1 {
                    return self.stop("snapshot queue refused");
                }
                self.phase = Phase::Queued;
                self.accepted_at = Some(self.boundaries);
            }
            3 if self.phase == Phase::Queued => {
                if a != u64::from(FREEZE_BYTES) {
                    return self.stop("initial state bound");
                }
                self.initial_state_bytes = a;
            }
            16 if self.phase == Phase::Queued && a == self.initial_state_bytes && a != 0 => {
                self.phase = Phase::Frozen;
            }
            15 if self.phase == Phase::Frozen => {
                if a != 1 {
                    return self.stop("capture file open failed");
                }
                self.phase = Phase::Recording;
                self.started_at = Some(self.boundaries);
            }
            4 if self.phase == Phase::Recording => {
                if !Self::add(&mut self.decoded_bytes, a, self.limit) {
                    return self.stop("decoded cap");
                }
            }
            6 if self.phase == Phase::Recording => {
                if a > 3 || !Self::add(&mut self.packets, 1, PACKET_LIMIT) {
                    return self.stop("capture packet bound");
                }
            }
            7 if self.phase == Phase::Recording => {
                if !Self::add(&mut self.vsyncs, 1, VSYNC_LIMIT) {
                    return self.stop("capture VSync cap");
                }
            }
            9 => return self.stop("native capture/replay fault"),
            14 => {} // Permission check before GPU-dependent capture work.
            10 if self.mode != Mode::Capture && self.phase == Phase::Armed => {
                self.phase = Phase::Replaying;
            }
            11 if self.phase == Phase::Replaying => {
                if a != self.packets {
                    return self.stop("replay packet order or loop");
                }
                self.last_packet = Some(a);
                if !Self::add(&mut self.packets, 1, PACKET_LIMIT) {
                    return self.stop("replay packet cap");
                }
                if b == 1 && !Self::add(&mut self.vsyncs, 1, VSYNC_LIMIT) {
                    return self.stop("replay VSync cap");
                }
            }
            12 if self.phase == Phase::Replaying && self.packets == a && self.vsyncs > 0 => {
                self.phase = Phase::Closed;
            }
            _ => return self.stop("unexpected owner event"),
        }
        1
    }
}

/// Decompression is streamed by the admitted host; this ensures errors such as
/// a missing zstd trailer cannot be mistaken for a successfully ended payload.
pub fn validate_decoder(mut child: std::process::Child) -> Result<Stream> {
    let result = child
        .stdout
        .take()
        .ok_or_else(|| "decoder has no stream".to_owned())
        .and_then(|out| validate(out, BYTE_LIMIT));
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("decoder failed or incomplete container".into());
    }
    result
}

// Keep errors ordinary and allocation bounded; no game-specific fixture.
pub fn interrupted() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "GS stage stopped")
}

#[derive(Debug, Serialize)]
pub struct Reproduction {
    pub replayed_packets: u64,
    pub replayed_vsyncs: u64,
    pub producer_output: u64,
    pub failure_stage: u64,
}
/// Positive control only. A clean exit/missing fence path/partial startup
/// suppresses the donor arm. Custody, exact executable/settings/context and
/// immutable encoded input must also be independently verified by the operator.
pub fn reproduction(
    stream: &Stream,
    ledger: &Ledger,
    snapshot: &serde_json::Value,
    expected_digest: &str,
    replay_digest: &str,
) -> Result<Reproduction> {
    let refuse = || {
        "modified replay did not establish the relevant failure with complete coverage".to_owned()
    };
    if expected_digest.len() != 64
        || !expected_digest.bytes().all(|b| b.is_ascii_hexdigit())
        || expected_digest != replay_digest
        || !stream.readback_provenance
        || stream.state_bytes != FREEZE_BYTES
        || stream.transfers == 0
        || stream.vsyncs == 0
        || stream.vsyncs > VSYNC_LIMIT
        || ledger.limit != BYTE_LIMIT
        || ledger.first_failure.is_none()
        || ledger.mode != Mode::ModifiedReplay
        || ledger.started_at.is_some()
        || ledger.phase != Phase::Stopped
        || ledger.packets == 0
        || ledger.packets > stream.packets
        || ledger.vsyncs == 0
        || ledger.vsyncs > stream.vsyncs
        || ledger.last_packet != ledger.packets.checked_sub(1)
    {
        return Err(refuse());
    }
    let fault = &snapshot["first_failure"];
    if fault["stage"] != 16
        || fault["owner"] != 1
        || fault["data"][26] != 1
        || fault["detail"]
            .as_array()
            .is_none_or(|d| d != &vec![1, 1, 1, 1, 0, 0, 1, 1])
        || fault["frame"].as_u64().is_none_or(|f| f == 0)
    {
        return Err(refuse());
    }
    let sync = &snapshot["output_sync"];
    let common = &snapshot["common"];
    for key in [
        "live",
        "untracked_creates",
        "frontend_sync_operations",
        "context_reclaim_pending",
    ] {
        if sync[key] != 0 {
            return Err(refuse());
        }
    }
    if sync["native_joined"] != true
        || !sync
            .get("first_ownership_failure")
            .is_some_and(serde_json::Value::is_null)
        || sync["overflow"] != false
        || common["native_context_destroyed"] != true
        || common["frontend_context_destroyed"] != true
    {
        return Err(refuse());
    }
    for phase in [2, 4, 5, 6] {
        if common["phase_entries"][phase]
            .as_u64()
            .is_none_or(|n| n == 0)
        {
            return Err(refuse());
        }
    }
    let created = sync["created"].as_u64().ok_or_else(refuse)?;
    let retired = sync["retired_on_creator"].as_u64().ok_or_else(refuse)?;
    let invalid = sync["invalid_names_released"].as_u64().ok_or_else(refuse)?;
    if created == 0 || retired.checked_add(invalid) != Some(created) || invalid != 1 {
        return Err(refuse());
    }
    Ok(Reproduction {
        replayed_packets: ledger.packets,
        replayed_vsyncs: ledger.vsyncs,
        producer_output: fault["frame"].as_u64().ok_or_else(refuse)?,
        failure_stage: 16,
    })
}

/// One process-local stage; callback storage is kept until native CPU/GS join.
/// No framebuffer, decoded packet body, serial or image is copied here.
pub mod owner {
    use super::*;
    use std::sync::Mutex;
    #[derive(Clone, Serialize)]
    pub struct Point {
        pub event: u32,
        pub gs_boundary_or_packet: u64,
        pub source_vsync: u64,
        pub producer_output: u64,
        pub observer_micros: u64,
    }
    #[derive(Clone, Serialize)]
    pub struct Report {
        pub ledger: Ledger,
        pub correspondence: Vec<Point>,
    }
    static ACTIVE: Mutex<Option<Report>> = Mutex::new(None);
    pub fn begin(mode: Mode) -> Result<()> {
        let mut slot = ACTIVE.lock().map_err(|_| "GS stage lock")?;
        if slot.is_some() {
            return Err("GS stage already consumed".into());
        }
        *slot = Some(Report {
            ledger: Ledger::new(mode),
            correspondence: Vec::with_capacity(2064),
        });
        Ok(())
    }
    pub extern "C" fn callback(event: u32, a: u64, b: u64) -> u32 {
        let Ok(mut slot) = ACTIVE.lock() else {
            return 0;
        };
        let Some(report) = slot.as_mut() else {
            return 0;
        };
        if !matches!(event, 5 | 8) && crate::graphics_observer::stop_requested() {
            return report.ledger.stop("existing first-fault/owner stop");
        }
        if matches!(event, 1 | 3 | 8 | 10 | 11 | 12 | 15 | 16) && (event != 11 || b == 1) {
            if report.correspondence.len() >= 2064 {
                return report.ledger.stop("correspondence cap");
            }
            report.correspondence.push(Point {
                event,
                gs_boundary_or_packet: a,
                source_vsync: b,
                producer_output: crate::graphics_observer::producer_progress(),
                observer_micros: crate::graphics_observer::elapsed_micros(),
            });
        }
        let result = report.ledger.event(event, a, b);
        if result == 0 || matches!(report.ledger.phase, Phase::Closed) {
            crate::graphics_observer::request_stop(crate::graphics_observer::StopReason::Operator);
        }
        result
    }
    pub fn report() -> Option<Report> {
        ACTIVE.lock().ok()?.clone()
    }
    pub fn cancel(reason: &'static str) {
        if let Ok(mut s) = ACTIVE.lock() {
            if let Some(s) = s.as_mut() {
                s.ledger.stop(reason);
            }
        }
    }
}

/// Native callback registration does not launch a VM. Keep this handle until
/// the minimal host has joined all owners; it deliberately has no Drop cleanup
/// that could withdraw callback storage from a still-running worker.
pub struct Native {
    library: std::sync::Arc<libloading::Library>,
    clear: unsafe extern "C" fn(),
    joined: bool,
}
impl Native {
    /// # Safety
    /// Exact source-bound library on the graphics owner before native startup.
    /// Call finish_after_join only after Host::stop returns with joined owners.
    pub unsafe fn configure(
        library: &std::path::Path,
        mode: Mode,
        path: &std::path::Path,
    ) -> Result<Self> {
        let library =
            std::sync::Arc::new(libloading::Library::new(library).map_err(|e| e.to_string())?);
        type Configure = unsafe extern "C" fn(
            u32,
            *const std::ffi::c_char,
            extern "C" fn(u32, u64, u64) -> u32,
        ) -> bool;
        let configure = *library
            .get::<Configure>(b"retro_psx_gs_replay_configure\0")
            .map_err(|e| e.to_string())?;
        let clear = *library
            .get::<unsafe extern "C" fn()>(b"retro_psx_gs_replay_clear\0")
            .map_err(|e| e.to_string())?;
        let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|e| e.to_string())?;
        owner::begin(mode)?;
        if !configure(
            match mode {
                Mode::Capture => 1,
                Mode::ModifiedReplay => 2,
                Mode::PinnedReplay => 3,
            },
            path.as_ptr(),
            owner::callback,
        ) {
            owner::cancel("native stage registration refused");
            return Err("native GS registration refused".into());
        }
        Ok(Self {
            library,
            clear,
            joined: false,
        })
    }
    /// # Safety
    /// All native GS/CPU owners must already have retired and joined.
    pub unsafe fn finish_after_join(mut self) {
        (self.clear)();
        self.joined = true;
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        // A forgotten explicit finish retains code rather than unloading a live
        // native callback. Ordinary successful finish drops this extra handle
        // only after clearing. Arc is retained on uncompleted teardown below.
        if !self.joined {
            std::mem::forget(self.library.clone());
        }
    }
}
