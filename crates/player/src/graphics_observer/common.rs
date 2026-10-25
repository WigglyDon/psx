//! Bounded observations of results already consumed by the native renderer.
//! Keys are transient owner-local addresses/names, never exported. Byte counts
//! describe requested storage, not physical GPU allocation or free device RAM.
use serde::Serialize;
use std::{
    cell::RefCell,
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};

pub const ENTER: u32 = 0;
pub const LEAVE: u32 = 1;
pub const ERROR: u32 = 2;
pub const ALLOCATE: u32 = 3;
pub const RELEASE: u32 = 4;
pub const MAP: u32 = 5;
pub const READBACK: u32 = 6;
pub const FENCE: u32 = 7;
pub const WAIT: u32 = 8;
pub const COMPILE: u32 = 9;
pub const LINK: u32 = 10;
pub const BINARY: u32 = 11;
pub const UPLOAD: u32 = 12;
pub const RANGE: u32 = 13;
pub const CONTEXT_DESTROYED: u32 = 14;
pub const UNSAFE: u32 = 15;
const RECENT: usize = 32;
const LIVE: usize = 512;

#[derive(Clone, Copy, Default, Serialize)]
pub struct Observation {
    pub sequence: u64,
    pub operation: u32,
    pub kind: u32,
    pub owner: u32,
    pub context: u32,
    pub thread: u32,
    pub phase: u32,
    pub resource_generation: u64,
    pub progress_frame: u64,
    pub micros: u64,
    pub values: [u64; 5],
}
#[derive(Clone, Default, Serialize)]
pub struct AllocationTotals {
    pub created: u64,
    pub failed: u64,
    pub deleted: u64,
    pub context_released: u64,
    pub live: u64,
    pub requested_live_bytes: u64,
    pub requested_byte_high_water: u64,
    pub unknown_byte_requests: u64,
}
#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub operations: [u64; 20],
    pub failed_operations: [u64; 20],
    pub phase_entries: [u64; 10],
    pub download_operations: [u64; 20],
    pub download_wait_results: [u64; 5],
    pub first_download_selection: Option<Observation>,
    /// Texture storage, stream GL buffers, download GL buffers, CPU staging.
    pub allocations: [AllocationTotals; 4],
    pub first_nonzero_error: Option<Observation>,
    pub first_probe_error: Option<Observation>,
    pub first_runtime_failure: Option<Observation>,
    pub first_dependency_failure: Option<Observation>,
    pub first_fatal: Option<Observation>,
    pub first_failed_request: Option<Observation>,
    pub first_readback_dependency: Option<Observation>,
    pub nonblocking_timeouts: u64,
    pub untracked_resources: u64,
    pub unknown_releases: u64,
    pub overflow: bool,
    pub native_context_destroyed: bool,
    pub frontend_context_destroyed: bool,
    pub events_omitted: u64,
    pub recent: VecDeque<Observation>,
}
struct Allocation {
    kind: u32,
    key: usize,
    generation: u64,
    bytes: u64,
    context: u32,
}
pub struct Ledger {
    progress: Progress,
    live: Vec<Allocation>,
    sequence: u64,
    generation: u64,
    destroyed_contexts: Vec<u32>,
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            progress: Progress {
                recent: VecDeque::with_capacity(RECENT),
                ..Default::default()
            },
            live: Vec::with_capacity(LIVE),
            sequence: 0,
            generation: 0,
            destroyed_contexts: Vec::with_capacity(8),
        }
    }
}
fn increment(value: &mut u64, overflow: &mut bool, n: u64) {
    if let Some(next) = value.checked_add(n) {
        *value = next;
    } else {
        *overflow = true;
    }
}
fn decrement(value: &mut u64, overflow: &mut bool, n: u64) {
    if let Some(next) = value.checked_sub(n) {
        *value = next;
    } else {
        *value = 0;
        *overflow = true;
    }
}
/// Exact requested texel/block storage for audited sized formats, including
/// mip levels. Unknown formats/invalid requests stay explicitly unestimated.
pub fn texture_bytes(w: u64, h: u64, format: u64, levels: u64) -> Option<u64> {
    if w == 0 || h == 0 || w > i32::MAX as u64 || h > i32::MAX as u64 || !(1..=32).contains(&levels)
    {
        return None;
    }
    let (bw, bh, bytes) = match format {
        0x8229 => (1, 1, 1),
        0x8234 => (1, 1, 2),
        0x8058 | 0x8059 | 0x822e | 0x8236 | 0x8cac => (1, 1, 4),
        0x805b | 0x881a | 0x8cad => (1, 1, 8),
        0x83f1 => (4, 4, 8),
        0x83f2 | 0x83f3 | 0x8e8c => (4, 4, 16),
        0x93b0..=0x93bd => {
            let (x, y) = [
                (4, 4),
                (5, 4),
                (5, 5),
                (6, 5),
                (6, 6),
                (8, 5),
                (8, 6),
                (8, 8),
                (10, 5),
                (10, 6),
                (10, 8),
                (10, 10),
                (12, 10),
                (12, 12),
            ][(format - 0x93b0) as usize];
            (x, y, 16)
        }
        _ => return None,
    };
    let (mut width, mut height, mut total) = (w, h, 0u64);
    for _ in 0..levels {
        let size = width
            .div_ceil(bw)
            .checked_mul(height.div_ceil(bh))?
            .checked_mul(bytes)?;
        total = total.checked_add(size)?;
        width = (width / 2).max(1);
        height = (height / 2).max(1);
    }
    Some(total)
}
impl Ledger {
    pub fn snapshot(&self) -> Progress {
        self.progress.clone()
    }
    pub fn observe(&mut self, key: usize, mut e: Observation) -> bool {
        if e.operation >= 20 {
            self.progress.overflow = true;
            return false;
        }
        increment(&mut self.sequence, &mut self.progress.overflow, 1);
        e.sequence = self.sequence;
        increment(
            &mut self.progress.operations[e.operation as usize],
            &mut self.progress.overflow,
            1,
        );
        if matches!(e.kind, 3 | 4) {
            increment(
                &mut self.progress.download_operations[e.operation as usize],
                &mut self.progress.overflow,
                1,
            );
            if e.operation == WAIT {
                let index = match e.values[0] {
                    0x911a => 0,
                    0x911c => 1,
                    0x911b => 2,
                    0x911d => 3,
                    _ => 4,
                };
                increment(
                    &mut self.progress.download_wait_results[index],
                    &mut self.progress.overflow,
                    1,
                );
            }
        }
        e.resource_generation = self
            .live
            .iter()
            .find(|v| v.key == key && v.kind == e.kind && (e.kind != 2 || v.context == e.context))
            .map_or(0, |v| v.generation);
        let [a, b, c, d, result] = e.values;
        let probe = e.phase == 8 || (e.operation == ERROR && c == 1);
        let fatal = e.operation == ERROR && matches!(a, 0x505 | 0x507);
        let failed = match e.operation {
            ERROR => a != 0,
            ALLOCATE => {
                result == 0
                    || (e.kind == 1
                        && (a == 0
                            || b == 0
                            || a > i32::MAX as u64
                            || b > i32::MAX as u64
                            || !(1..=32).contains(&d)))
            }
            MAP => a == 0 || b.checked_add(c).is_none_or(|end| end > d),
            FENCE | COMPILE | LINK | BINARY => a == 0,
            WAIT => !(matches!(a, 0x911a | 0x911c) || a == 0x911b && b == 0),
            RANGE => a.checked_add(b).is_none_or(|end| end > c),
            _ => false,
        };
        if e.operation == ENTER {
            if let Some(n) = self.progress.phase_entries.get_mut(a as usize) {
                increment(n, &mut self.progress.overflow, 1);
            } else {
                self.progress.overflow = true;
            }
        }
        if e.operation == ERROR && a != 0 {
            self.progress.first_nonzero_error.get_or_insert(e);
            if probe {
                self.progress.first_probe_error.get_or_insert(e);
            }
        }
        if fatal {
            self.progress.first_fatal.get_or_insert(e);
        }
        if failed {
            increment(
                &mut self.progress.failed_operations[e.operation as usize],
                &mut self.progress.overflow,
                1,
            );
            if !probe && e.phase != 9 {
                self.progress.first_runtime_failure.get_or_insert(e);
            }
            if matches!(e.operation, MAP | FENCE | WAIT | COMPILE | LINK | RANGE) && !probe {
                self.progress.first_dependency_failure.get_or_insert(e);
            }
            if matches!(e.operation, ALLOCATE | MAP) {
                self.progress.first_failed_request.get_or_insert(e);
            }
        }
        if e.operation == WAIT && a == 0x911b && b == 0 {
            increment(
                &mut self.progress.nonblocking_timeouts,
                &mut self.progress.overflow,
                1,
            );
        }
        if matches!(e.operation, READBACK | FENCE | WAIT) {
            self.progress.first_readback_dependency.get_or_insert(e);
        }
        if e.operation == ALLOCATE && (1..=4).contains(&e.kind) {
            let totals = &mut self.progress.allocations[(e.kind - 1) as usize];
            if result == 0 {
                increment(&mut totals.failed, &mut self.progress.overflow, 1);
            } else {
                increment(&mut totals.created, &mut self.progress.overflow, 1);
                let bytes = if e.kind == 1 {
                    texture_bytes(a, b, c, d)
                } else {
                    Some(d)
                };
                if bytes.is_none() {
                    increment(
                        &mut totals.unknown_byte_requests,
                        &mut self.progress.overflow,
                        1,
                    );
                }
                if key == 0
                    || self.live.len() == LIVE
                    || self.live.iter().any(|v| {
                        v.key == key && v.kind == e.kind && (e.kind != 2 || v.context == e.context)
                    })
                {
                    increment(
                        &mut self.progress.untracked_resources,
                        &mut self.progress.overflow,
                        1,
                    );
                } else {
                    increment(&mut self.generation, &mut self.progress.overflow, 1);
                    e.resource_generation = self.generation;
                    self.live.push(Allocation {
                        kind: e.kind,
                        key,
                        generation: self.generation,
                        bytes: bytes.unwrap_or(0),
                        context: e.context,
                    });
                    increment(&mut totals.live, &mut self.progress.overflow, 1);
                    increment(
                        &mut totals.requested_live_bytes,
                        &mut self.progress.overflow,
                        bytes.unwrap_or(0),
                    );
                    totals.requested_byte_high_water = totals
                        .requested_byte_high_water
                        .max(totals.requested_live_bytes);
                }
            }
        }
        if e.operation == RELEASE {
            if let Some(i) = self.live.iter().position(|v| {
                v.key == key && v.kind == e.kind && (e.kind != 2 || v.context == e.context)
            }) {
                // result=0 is context-owned reclamation pending, never a GL delete.
                if a != 0 {
                    let entry = self.live.remove(i);
                    let totals = &mut self.progress.allocations[(entry.kind - 1) as usize];
                    decrement(&mut totals.live, &mut self.progress.overflow, 1);
                    decrement(
                        &mut totals.requested_live_bytes,
                        &mut self.progress.overflow,
                        entry.bytes,
                    );
                    increment(&mut totals.deleted, &mut self.progress.overflow, 1);
                }
            } else {
                increment(
                    &mut self.progress.unknown_releases,
                    &mut self.progress.overflow,
                    1,
                );
            }
        }
        if e.operation == CONTEXT_DESTROYED && a == 1 {
            if e.owner == 1 && e.context != 0 {
                self.progress.native_context_destroyed = true;
                if !self.destroyed_contexts.contains(&e.context) {
                    if self.destroyed_contexts.len() < 8 {
                        self.destroyed_contexts.push(e.context);
                    } else {
                        self.progress.overflow = true;
                    }
                }
            } else if e.owner == 2 {
                self.progress.frontend_context_destroyed = true;
            }
            if self.progress.native_context_destroyed && self.progress.frontend_context_destroyed {
                let mut i = 0;
                while i < self.live.len() {
                    if self.live[i].kind != 4
                        && self.destroyed_contexts.contains(&self.live[i].context)
                    {
                        let v = self.live.remove(i);
                        let totals = &mut self.progress.allocations[(v.kind - 1) as usize];
                        decrement(&mut totals.live, &mut self.progress.overflow, 1);
                        decrement(
                            &mut totals.requested_live_bytes,
                            &mut self.progress.overflow,
                            v.bytes,
                        );
                        increment(&mut totals.context_released, &mut self.progress.overflow, 1);
                    } else {
                        i += 1;
                    }
                }
            }
        }
        if matches!(e.kind, 3 | 4)
            && (e.operation == MAP || (e.operation == ALLOCATE && e.values[4] != 0))
        {
            self.progress.first_download_selection.get_or_insert(e);
        }
        if self.progress.recent.len() == RECENT {
            self.progress.recent.pop_front();
            increment(
                &mut self.progress.events_omitted,
                &mut self.progress.overflow,
                1,
            );
        }
        self.progress.recent.push_back(e);
        (failed && !probe && e.phase != 9) || fatal
    }
}

static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
static RETIRING: AtomicBool = AtomicBool::new(false);
static COVERAGE_OVERFLOW: AtomicBool = AtomicBool::new(false);
static NEXT_THREAD: AtomicU32 = AtomicU32::new(1);
thread_local! {
    static PHASE: RefCell<([u32;16],usize,usize)> = const { RefCell::new(([0;16],0,0)) };
    static THREAD: u32 = NEXT_THREAD.fetch_update(Ordering::Relaxed,Ordering::Relaxed,|n| n.checked_add(1)).unwrap_or(0);
}
pub fn begin() {
    if let Ok(mut l) = LEDGER.get_or_init(|| Mutex::new(Ledger::default())).lock() {
        *l = Ledger::default();
    }
    RETIRING.store(false, Ordering::Release);
    COVERAGE_OVERFLOW.store(false, Ordering::Release);
}
pub fn retiring() {
    RETIRING.store(true, Ordering::Release);
}
pub fn phase() -> u32 {
    if RETIRING.load(Ordering::Acquire) {
        9
    } else {
        PHASE.with(|p| {
            let p = p.borrow();
            if p.2 == 0 {
                p.0[p.1]
            } else {
                0
            }
        })
    }
}
pub fn snapshot() -> Progress {
    let mut p = LEDGER
        .get()
        .and_then(|l| l.lock().ok().map(|l| l.snapshot()))
        .unwrap_or_default();
    p.overflow |= COVERAGE_OVERFLOW.load(Ordering::Acquire);
    p
}
pub fn existing_error(owner: u32, code: u32, site: u32, frame: u64) {
    if code == 0 {
        return;
    }
    record(
        ERROR,
        0,
        0,
        [code as u64, site as u64, 0, 0, 0],
        owner,
        frame,
    );
}
fn record(op: u32, kind: u32, key: usize, values: [u64; 5], owner: u32, frame: u64) -> bool {
    let context = if op == CONTEXT_DESTROYED && owner == 1 {
        super::context_token(key)
    } else {
        super::current_context_token()
    };
    let thread = THREAD.with(|id| *id);
    if thread == 0 {
        COVERAGE_OVERFLOW.store(true, Ordering::Release);
    }
    let e = Observation {
        operation: op,
        kind,
        owner,
        context,
        thread,
        phase: phase(),
        progress_frame: frame,
        micros: super::elapsed_micros(),
        values,
        ..Default::default()
    };
    let failed = LEDGER
        .get_or_init(|| Mutex::new(Ledger::default()))
        .lock()
        .map(|mut l| l.observe(key, e))
        .unwrap_or(true);
    if failed {
        super::request_stop(super::StopReason::Dependency);
    }
    if failed {
        super::native_result_event(
            owner,
            frame,
            [
                op as i64,
                kind as i64,
                e.phase as i64,
                values[0] as i64,
                values[1] as i64,
                0,
                0,
                0,
            ],
        );
    }
    failed
}
/// Same native result is returned unchanged by the native wrapper. No GL call,
/// error drain, formatting or file operation is performed here.
#[allow(clippy::too_many_arguments)]
pub extern "C" fn callback(
    op: u32,
    kind: u32,
    key: usize,
    a: u64,
    b: u64,
    c: u64,
    d: u64,
    e: u64,
) -> u32 {
    if op == UNSAFE {
        return u32::from(super::output_sync::context_unavailable());
    }
    // The existing native caller still receives its exact error flag. Forward
    // fatal results before requesting the diagnostic stop, once, through the
    // same typed latch as the original Rust error consumer.
    if op == ERROR && matches!(a, 0x505 | 0x507) {
        super::native_error(a as u32);
    }
    if op == ENTER {
        PHASE.with(|p| {
            let mut p = p.borrow_mut();
            if p.1 < 15 {
                p.1 += 1;
                let n = p.1;
                p.0[n] = a as u32;
            } else {
                p.2 = p.2.saturating_add(1);
                COVERAGE_OVERFLOW.store(true, Ordering::Release);
            }
        });
    }
    let result = record(op, kind, key, [a, b, c, d, e], 1, super::produced());
    if op == LEAVE {
        PHASE.with(|p| {
            let mut p = p.borrow_mut();
            if p.2 > 0 {
                p.2 -= 1;
            } else if p.1 > 0 {
                p.1 -= 1;
            } else {
                COVERAGE_OVERFLOW.store(true, Ordering::Release);
            }
        });
    }
    if COVERAGE_OVERFLOW.load(Ordering::Acquire) {
        super::request_stop(super::StopReason::Dependency);
    }
    u32::from(result)
}
pub fn frontend_destroyed(success: bool) {
    record(
        CONTEXT_DESTROYED,
        0,
        0,
        [success as u64, 0, 0, 0, 0],
        2,
        super::consumed(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    fn e(op: u32, kind: u32, phase: u32, v: [u64; 5]) -> Observation {
        Observation {
            operation: op,
            kind,
            phase,
            owner: 1,
            context: 1,
            values: v,
            ..Default::default()
        }
    }
    #[test]
    fn runtime_and_retirement_errors_do_not_overwrite_the_initiator() {
        let mut l = Ledger::default();
        assert!(!l.observe(0, e(ERROR, 0, 8, [0x500, 1, 1, 0, 0])));
        assert!(l.observe(1, e(MAP, 3, 3, [0, 0, 64, 128, 1])));
        assert!(l.observe(0, e(ERROR, 0, 9, [0x505, 2, 0, 0, 0])));
        let p = l.snapshot();
        assert_eq!(p.first_probe_error.unwrap().values[0], 0x500);
        assert_eq!(p.first_runtime_failure.unwrap().operation, MAP);
        assert_eq!(p.first_fatal.unwrap().phase, 9);
    }
    #[test]
    fn inactive_readback_and_a_valid_async_poll_are_distinct() {
        let mut l = Ledger::default();
        assert_eq!(l.snapshot().operations[READBACK as usize], 0);
        assert!(!l.observe(1, e(READBACK, 3, 3, [64, 128, 0, 0, 0])));
        assert!(!l.observe(1, e(WAIT, 3, 3, [0x911b, 0, 0, 0, 0])));
        assert!(l.observe(1, e(WAIT, 3, 3, [0x911d, 1, 0, 0, 0])));
        let p = l.snapshot();
        assert_eq!(p.nonblocking_timeouts, 1);
        assert_eq!(p.failed_operations[WAIT as usize], 1);
        assert!(p.first_readback_dependency.is_some());
    }
    #[test]
    fn requested_storage_and_lifetimes_survive_eviction_and_handle_reuse() {
        let mut l = Ledger::default();
        l.observe(9, e(ALLOCATE, 1, 1, [64, 32, 0x881a, 1, 1]));
        l.observe(9, e(RELEASE, 1, 9, [1, 0, 0, 0, 0]));
        l.observe(9, e(ALLOCATE, 1, 1, [64, 32, 0x881a, 1, 1]));
        let generation = l.snapshot().recent.back().unwrap().resource_generation;
        assert_eq!(generation, 2);
        for _ in 0..100 {
            l.observe(0, e(ENTER, 0, 1, [1, 0, 0, 0, 0]));
        }
        let p = l.snapshot();
        assert_eq!(p.allocations[0].created, 2);
        assert_eq!(p.allocations[0].live, 1);
        assert_eq!(p.allocations[0].requested_live_bytes, 16384);
        assert_eq!(p.recent.len(), 32);
        assert!(p.events_omitted > 0);
        l.observe(9, e(RELEASE, 1, 9, [0, 0, 0, 0, 0]));
        assert_eq!(l.snapshot().allocations[0].live, 1);
        l.observe(0, e(CONTEXT_DESTROYED, 0, 9, [1, 0, 0, 0, 0]));
        let mut frontend = e(CONTEXT_DESTROYED, 0, 9, [1, 0, 0, 0, 0]);
        frontend.owner = 2;
        l.observe(0, frontend);
        assert_eq!(l.snapshot().allocations[0].context_released, 1);
        assert_eq!(l.snapshot().allocations[0].live, 0);
    }
    #[test]
    fn overflow_and_bad_ranges_are_explicit_not_allocation_success_claims() {
        assert_eq!(texture_bytes(4, 4, 0x881a, 1), Some(128));
        assert_eq!(texture_bytes(5, 5, 0x83f1, 1), Some(32));
        assert!(texture_bytes(u64::MAX, 1, 0x8058, 1).is_none());
        assert!(texture_bytes(1, 1, 0, 1).is_none());
        let mut l = Ledger::default();
        assert!(l.observe(1, e(MAP, 2, 2, [1, u64::MAX, 2, 64, 1])));
        l.progress.operations[MAP as usize] = u64::MAX;
        l.observe(1, e(MAP, 2, 2, [1, 0, 4, 64, 1]));
        assert!(l.snapshot().overflow);
        let mut allocations = Ledger::default();
        allocations.observe(1, e(ALLOCATE, 2, 7, [0, 0, 0, u64::MAX, 1]));
        allocations.observe(2, e(ALLOCATE, 2, 7, [0, 0, 0, 1, 1]));
        allocations.observe(1, e(RELEASE, 2, 9, [1, 0, 0, 0, 0]));
        allocations.observe(2, e(RELEASE, 2, 9, [1, 0, 0, 0, 0]));
        assert!(allocations.snapshot().overflow);
        assert_eq!(allocations.snapshot().allocations[1].live, 0);
        assert!(serde_json::to_vec(&l.snapshot()).unwrap().len() < super::super::EXPORT_LIMIT / 4);
    }
    #[test]
    fn equal_buffer_names_in_distinct_contexts_do_not_merge_lifetimes() {
        let mut l = Ledger::default();
        l.observe(9, e(ALLOCATE, 2, 7, [0, 0, 0, 64, 1]));
        let mut other = e(ALLOCATE, 2, 7, [0, 0, 0, 128, 1]);
        other.context = 2;
        l.observe(9, other);
        assert_eq!(l.snapshot().allocations[1].live, 2);
        assert_eq!(l.snapshot().untracked_resources, 0);
        let mut release = e(RELEASE, 2, 9, [1, 0, 0, 0, 0]);
        release.context = 2;
        l.observe(9, release);
        assert_eq!(l.snapshot().allocations[1].requested_live_bytes, 64);
        l.observe(9, e(RELEASE, 2, 9, [1, 0, 0, 0, 0]));
        assert_eq!(l.snapshot().allocations[1].live, 0);
    }
    #[test]
    fn saturated_recent_evidence_and_explicit_omission_fit_the_existing_export_budget() {
        let mut l = Ledger::default();
        for _ in 0..2000 {
            let mut v = e(MAP, 2, 2, [0, u64::MAX, u64::MAX, u64::MAX, u64::MAX]);
            v.progress_frame = u64::MAX;
            v.micros = u64::MAX;
            v.thread = u32::MAX;
            l.observe(1, v);
        }
        let p = l.snapshot();
        assert_eq!(p.recent.len(), 32);
        assert_eq!(p.events_omitted, 1968);
        assert_eq!(p.failed_operations[MAP as usize], 2000);
        assert!(p.first_failed_request.is_some());
        assert!(serde_json::to_vec(&p).unwrap().len() < super::super::EXPORT_LIMIT / 8);
    }
}
