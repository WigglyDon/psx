//! Fixed diagnostic policy: metadata crosses owners, output syncs do not.
//! Raw names are private keys and never identify a generation by themselves.
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Mutex, OnceLock},
    thread::{self, ThreadId},
};

pub const LIMIT: usize = 4;
const RECENT: usize = 32;
pub const CREATE: u32 = 1;
pub const PUBLISH: u32 = 2;
pub const CONSUME: u32 = 3;
pub const ACK: u32 = 4;
pub const CANCEL: u32 = 5;
pub const DROP: u32 = 6;
pub const NEXT: u32 = 7;
pub const RETIRED: u32 = 8;
pub const ALLOWED: u32 = 9;
pub const REJECT: u32 = 10;
pub const CONTEXT_FAULT: u32 = 11;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Command {
    pub version: u32,
    pub size: u32,
    pub op: u32,
    pub owner: u32,
    pub generation: u64,
    pub frame: u64,
    pub lease: u64,
    pub raw: usize,
    pub context: usize,
    pub texture: u32,
    pub reason: u32,
}
impl Command {
    pub fn new(op: u32, owner: u32) -> Self {
        Self {
            version: 1,
            size: std::mem::size_of::<Self>() as u32,
            op,
            owner,
            generation: 0,
            frame: 0,
            lease: 0,
            raw: 0,
            context: 0,
            texture: 0,
            reason: 0,
        }
    }
}
#[derive(Clone, Copy, Serialize)]
pub struct Event {
    pub sequence: u64,
    pub operation: u32,
    pub owner: u32,
    pub generation: u64,
    pub frame: u64,
    pub lease: u64,
    pub reason: u32,
}
#[derive(Clone, Default, Serialize)]
pub struct Progress {
    pub created: u64,
    pub published: u64,
    pub consumed: u64,
    pub acknowledged: u64,
    pub retired_on_creator: u64,
    pub invalid_names_released: u64,
    pub released_with_share_group: u64,
    pub context_reclaim_pending: usize,
    pub cancelled: u64,
    pub cancelled_acks: u64,
    pub cancelled_lease_omissions: u64,
    pub dropped: u64,
    pub live: usize,
    pub high_water: usize,
    pub creator_observations: u64,
    pub frontend_sync_operations: u64,
    pub untracked_creates: u64,
    pub overflow: bool,
    pub native_joined: bool,
    pub first_ownership_failure: Option<Event>,
    pub first_invalid_generation: Option<u64>,
    pub recent: VecDeque<Event>,
    pub events_omitted: u64,
}
#[derive(Clone, Copy, PartialEq)]
enum State {
    Created,
    Published,
    Leased,
    Ready,
    Retiring,
    ContextHeld,
}
struct Entry {
    generation: u64,
    frame: u64,
    lease: u64,
    raw: usize,
    texture: u32,
    state: State,
    invalid: bool,
}
pub struct Ledger {
    entries: Vec<Entry>,
    next: u64,
    sequence: u64,
    creator: Option<(ThreadId, usize)>,
    frontend: Option<ThreadId>,
    context_fault: bool,
    progress: Progress,
    cancelled_leases: VecDeque<(u64, u64)>,
}
impl Default for Ledger {
    fn default() -> Self {
        Self {
            entries: Vec::with_capacity(LIMIT),
            next: 0,
            sequence: 0,
            creator: None,
            frontend: None,
            context_fault: false,
            progress: Progress {
                recent: VecDeque::with_capacity(RECENT),
                ..Default::default()
            },
            cancelled_leases: VecDeque::with_capacity(LIMIT),
        }
    }
}
impl Ledger {
    fn count(value: &mut u64, overflow: &mut bool) {
        if let Some(n) = value.checked_add(1) {
            *value = n;
        } else {
            *overflow = true;
        }
    }
    fn event(&mut self, c: &Command, failure: bool) {
        Self::count(&mut self.sequence, &mut self.progress.overflow);
        let e = Event {
            sequence: self.sequence,
            operation: c.op,
            owner: c.owner,
            generation: c.generation,
            frame: c.frame,
            lease: c.lease,
            reason: c.reason,
        };
        if failure && self.progress.first_ownership_failure.is_none() {
            self.progress.first_ownership_failure = Some(e);
        }
        if self.progress.recent.len() == RECENT {
            self.progress.recent.pop_front();
            Self::count(
                &mut self.progress.events_omitted,
                &mut self.progress.overflow,
            );
        }
        self.progress.recent.push_back(e);
    }
    fn fail(&mut self, c: &Command, reason: u32) -> u32 {
        let mut c = *c;
        c.reason = reason;
        self.event(&c, true);
        0
    }
    fn creator(&self, c: &Command) -> bool {
        c.owner == 1 && c.context != 0 && self.creator == Some((thread::current().id(), c.context))
    }
    pub fn snapshot(&self) -> Progress {
        let mut p = self.progress.clone();
        p.live = self.entries.len();
        p.context_reclaim_pending = self
            .entries
            .iter()
            .filter(|e| e.state == State::ContextHeld)
            .count();
        p
    }
    /// Return1 authorizes only the exact command. NEXT additionally returns2
    /// for an invalid/context-held name for which no GL call is permitted.
    pub fn apply(&mut self, c: &mut Command) -> u32 {
        if c.version != 1 || c.size as usize != std::mem::size_of::<Command>() {
            return self.fail(c, 1);
        }
        if c.op == ALLOWED {
            if self.entries.len() == LIMIT || self.next == u64::MAX {
                self.progress.overflow |= self.next == u64::MAX;
                return self.fail(c, 13);
            }
            return u32::from(
                !self.context_fault && self.progress.first_ownership_failure.is_none(),
            );
        }
        if c.op == CREATE {
            if c.owner != 1 || c.context == 0 {
                return self.fail(c, 2);
            }
            let who = (thread::current().id(), c.context);
            if self.creator.is_some_and(|v| v != who) {
                return self.fail(c, 3);
            }
            self.progress.overflow |= self.next == u64::MAX;
            if self.entries.len() == LIMIT
                || self.next == u64::MAX
                || self.entries.iter().any(|e| c.raw != 0 && e.raw == c.raw)
            {
                Self::count(
                    &mut self.progress.untracked_creates,
                    &mut self.progress.overflow,
                );
                return self.fail(c, 4);
            }
            self.creator = Some(who);
            self.next += 1;
            c.generation = self.next;
            self.entries.push(Entry {
                generation: c.generation,
                frame: c.frame,
                lease: 0,
                raw: c.raw,
                texture: c.texture,
                state: State::Created,
                invalid: c.raw == 0,
            });
            Self::count(&mut self.progress.created, &mut self.progress.overflow);
            self.progress.high_water = self.progress.high_water.max(self.entries.len());
            self.event(c, false);
            return 1;
        }
        if c.op == CANCEL {
            for e in &mut self.entries {
                if e.state == State::Leased {
                    if self.cancelled_leases.len() == LIMIT {
                        self.cancelled_leases.pop_front();
                        Self::count(
                            &mut self.progress.cancelled_lease_omissions,
                            &mut self.progress.overflow,
                        );
                    }
                    self.cancelled_leases.push_back((e.generation, e.lease));
                }
                if !matches!(e.state, State::Retiring | State::ContextHeld) {
                    e.state = State::Ready;
                }
            }
            Self::count(&mut self.progress.cancelled, &mut self.progress.overflow);
            self.event(c, false);
            return 1;
        }
        if matches!(c.op, ACK | REJECT)
            && c.lease != 0
            && self.cancelled_leases.contains(&(c.generation, c.lease))
        {
            Self::count(
                &mut self.progress.cancelled_acks,
                &mut self.progress.overflow,
            );
            self.event(c, false);
            return 0;
        }
        if c.op == CONTEXT_FAULT {
            self.context_fault = true;
            self.event(c, false);
            return 1;
        }
        if c.op == NEXT {
            if self.entries.is_empty() && c.owner == 1 {
                return 0;
            }
            let creator_thread =
                c.owner == 1 && self.creator.is_some_and(|v| v.0 == thread::current().id());
            if !(self.creator(c) || self.context_fault && creator_thread) {
                return self.fail(c, 5);
            }
            let Some(index) = self.entries.iter().position(|e| {
                e.state == State::Ready && (c.texture == 0 || c.texture == e.texture)
            }) else {
                return 0;
            };
            let e = &mut self.entries[index];
            c.generation = e.generation;
            c.frame = e.frame;
            c.lease = e.lease;
            c.texture = e.texture;
            if self.context_fault {
                e.state = State::ContextHeld;
                c.reason = 2;
                c.raw = 0;
                self.event(c, false);
                return 2;
            }
            if e.invalid {
                self.entries.remove(index);
                Self::count(
                    &mut self.progress.invalid_names_released,
                    &mut self.progress.overflow,
                );
                c.reason = 1;
                c.raw = 0;
                self.event(c, false);
                return 2;
            }
            e.state = State::Retiring;
            c.raw = e.raw;
            self.event(c, false);
            return 1;
        }
        if c.op == REJECT {
            return self.fail(c, 6);
        }
        let Some(index) = self.entries.iter().position(|e| {
            e.generation == c.generation && e.frame == c.frame && e.texture == c.texture
        }) else {
            return self.fail(c, 7);
        };
        if matches!(c.op, PUBLISH | DROP | RETIRED) && !self.creator(c) {
            return self.fail(c, 8);
        }
        if matches!(c.op, CONSUME | ACK) {
            if c.owner != 2
                || c.raw != 0
                || c.context != 0
                || self.frontend.is_some_and(|t| t != thread::current().id())
            {
                return self.fail(c, 9);
            }
            self.frontend = Some(thread::current().id());
        }
        let e = &mut self.entries[index];
        match c.op {
            PUBLISH if e.state == State::Created => {
                e.state = State::Published;
                Self::count(&mut self.progress.published, &mut self.progress.overflow);
            }
            // A concurrent cancellation withdraws creation without publication.
            PUBLISH if e.state == State::Ready => return 0,
            CONSUME if e.state == State::Published && c.lease != 0 => {
                e.state = State::Leased;
                e.lease = c.lease;
                Self::count(&mut self.progress.consumed, &mut self.progress.overflow);
            }
            ACK if e.state == State::Leased && e.lease == c.lease && c.lease != 0 => {
                e.state = State::Ready;
                Self::count(&mut self.progress.acknowledged, &mut self.progress.overflow);
            }
            DROP if matches!(e.state, State::Created | State::Published) => {
                e.state = State::Ready;
                Self::count(&mut self.progress.dropped, &mut self.progress.overflow);
            }
            RETIRED if e.state == State::Retiring && e.raw == c.raw => {
                self.entries.remove(index);
                Self::count(
                    &mut self.progress.retired_on_creator,
                    &mut self.progress.overflow,
                );
            }
            _ => return self.fail(c, 10),
        }
        self.event(c, false);
        1
    }
    pub fn observed(&mut self, frame: u64, raw: usize, valid: bool, context_fault: bool) {
        self.context_fault |= context_fault;
        if frame != 0
            && self
                .creator
                .is_some_and(|who| who.0 != thread::current().id())
        {
            let mut c = Command::new(REJECT, 1);
            c.frame = frame;
            self.fail(&c, 14);
            return;
        }
        if let Some(e) = self
            .entries
            .iter_mut()
            .find(|e| e.frame == frame && e.raw == raw)
        {
            Self::count(
                &mut self.progress.creator_observations,
                &mut self.progress.overflow,
            );
            if !valid {
                e.invalid = true;
                self.progress
                    .first_invalid_generation
                    .get_or_insert(e.generation);
            }
        }
    }
    pub fn native_joined(&mut self) {
        self.progress.native_joined = true;
        if self.entries.iter().any(|e| e.state != State::ContextHeld) {
            let c = Command::new(REJECT, 0);
            self.fail(&c, 11);
        }
    }
    /// Called only after successful destruction of the last frontend context,
    /// after donor join; no GL operation or completion claim follows.
    pub fn share_group_destroyed(&mut self) {
        if !self.progress.native_joined {
            return;
        }
        let count = self
            .entries
            .iter()
            .filter(|e| e.state == State::ContextHeld)
            .count() as u64;
        self.entries.retain(|e| e.state != State::ContextHeld);
        if let Some(n) = self.progress.released_with_share_group.checked_add(count) {
            self.progress.released_with_share_group = n;
        } else {
            self.progress.overflow = true;
        }
    }
}
static LEDGER: OnceLock<Mutex<Ledger>> = OnceLock::new();
pub fn begin() -> Result<(), &'static str> {
    let mut l = LEDGER
        .get_or_init(|| Mutex::new(Ledger::default()))
        .lock()
        .map_err(|_| "Output owner unavailable")?;
    if !l.entries.is_empty() {
        return Err("Prior output ownership is unresolved");
    }
    *l = Ledger::default();
    Ok(())
}
pub fn snapshot() -> Progress {
    LEDGER
        .get()
        .and_then(|l| l.lock().ok().map(|l| l.snapshot()))
        .unwrap_or_default()
}
pub fn context_unavailable() -> bool {
    LEDGER
        .get()
        .and_then(|l| l.lock().ok().map(|l| l.context_fault))
        .unwrap_or(false)
}

/// # Safety
/// `command` points to one writable ABI command for this synchronous callback.
pub unsafe extern "C" fn callback(command: *mut Command) -> u32 {
    let Some(c) = command.as_mut() else {
        super::request_stop(super::StopReason::Ownership);
        return 0;
    };
    if matches!(c.op, ALLOWED | PUBLISH) && super::stop_requested() {
        return 0;
    }
    let (result, failure) = LEDGER
        .get()
        .and_then(|l| {
            l.lock().ok().map(|mut l| {
                let result = l.apply(c);
                (result, l.progress.first_ownership_failure.is_some())
            })
        })
        .unwrap_or((0, true));
    if failure {
        super::request_stop(super::StopReason::Ownership);
    }
    result
}
pub fn observed(frame: u64, raw: usize, valid: bool, context_fault: bool) {
    if let Some(l) = LEDGER.get() {
        if let Ok(mut l) = l.lock() {
            l.observed(frame, raw, valid, context_fault);
        }
    }
}
pub fn unexpected_frontend(frame: u64) {
    if let Some(l) = LEDGER.get() {
        if let Ok(mut l) = l.lock() {
            let p = &mut l.progress;
            Ledger::count(&mut p.frontend_sync_operations, &mut p.overflow);
            let mut c = Command::new(REJECT, 2);
            c.frame = frame;
            l.fail(&c, 12);
        }
    }
    super::request_stop(super::StopReason::Ownership);
}
pub fn native_joined() {
    if let Some(l) = LEDGER.get() {
        if let Ok(mut l) = l.lock() {
            l.native_joined();
        }
    }
}
pub fn share_group_destroyed() {
    if let Some(l) = LEDGER.get() {
        if let Ok(mut l) = l.lock() {
            l.share_group_destroyed();
        }
    }
    super::control::notify();
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make(l: &mut Ledger, raw: usize, frame: u64) -> Command {
        let mut c = Command::new(CREATE, 1);
        c.raw = raw;
        c.frame = frame;
        c.texture = 1;
        c.context = 100;
        assert_eq!(l.apply(&mut c), 1);
        c
    }
    fn op(l: &mut Ledger, created: Command, operation: u32, lease: u64) -> Command {
        let mut c = created;
        c.op = operation;
        c.lease = lease;
        if matches!(operation, CONSUME | ACK) {
            c.owner = 2;
            c.raw = 0;
            c.context = 0;
        }
        assert_eq!(l.apply(&mut c), 1);
        c
    }
    fn retire(l: &mut Ledger) -> Command {
        let mut c = Command::new(NEXT, 1);
        c.context = 100;
        assert_eq!(l.apply(&mut c), 1);
        c.op = RETIRED;
        assert_eq!(l.apply(&mut c), 1);
        c
    }
    #[test]
    fn reused_values_are_separate_generations_and_metadata_ack_never_authorizes_a_gl_call() {
        let mut l = Ledger::default();
        let old = make(&mut l, 50, 1);
        op(&mut l, old, PUBLISH, 0);
        op(&mut l, old, CONSUME, 7);
        op(&mut l, old, ACK, 7);
        assert_eq!(retire(&mut l).generation, old.generation);
        let new = make(&mut l, 50, 2);
        assert!(new.generation > old.generation);
        op(&mut l, new, PUBLISH, 0);
        op(&mut l, new, CONSUME, 8);
        let mut stale = old;
        stale.op = ACK;
        stale.owner = 2;
        stale.context = 0;
        stale.raw = 0;
        stale.lease = 7;
        assert_eq!(l.apply(&mut stale), 0);
        assert_eq!(l.snapshot().live, 1);
        op(&mut l, new, ACK, 8);
        assert_eq!(retire(&mut l).generation, new.generation);
        assert_eq!(l.snapshot().retired_on_creator, 2);
        assert_eq!(l.snapshot().frontend_sync_operations, 0);
    }
    #[test]
    fn dropped_and_cancelled_leases_retire_only_on_creator_without_waiting_for_frontend() {
        let mut l = Ledger::default();
        let a = make(&mut l, 1, 1);
        op(&mut l, a, PUBLISH, 0);
        op(&mut l, a, DROP, 0);
        retire(&mut l);
        let b = make(&mut l, 1, 2);
        op(&mut l, b, PUBLISH, 0);
        op(&mut l, b, CONSUME, 9);
        let mut cancel = Command::new(CANCEL, 0);
        assert_eq!(l.apply(&mut cancel), 1);
        retire(&mut l);
        let mut late = b;
        late.op = REJECT;
        late.owner = 2;
        late.raw = 0;
        late.context = 0;
        late.lease = 9;
        assert_eq!(l.apply(&mut late), 0);
        assert_eq!(l.snapshot().cancelled_acks, 1);
        l.native_joined();
        assert_eq!(l.snapshot().live, 0);
        assert!(l.snapshot().first_ownership_failure.is_none());
    }
    #[test]
    fn wrong_thread_or_context_cannot_retire_and_duplicate_retirement_preserves_next_generation() {
        let mut l = Ledger::default();
        let a = make(&mut l, 7, 1);
        op(&mut l, a, PUBLISH, 0);
        op(&mut l, a, CONSUME, 3);
        op(&mut l, a, ACK, 3);
        let (mut l, result) = std::thread::spawn(move || {
            let mut c = Command::new(NEXT, 1);
            c.context = 100;
            let result = l.apply(&mut c);
            (l, result)
        })
        .join()
        .unwrap();
        assert_eq!(result, 0);
        assert_eq!(l.snapshot().live, 1);
        let mut wrong = Command::new(NEXT, 1);
        wrong.context = 200;
        assert_eq!(l.apply(&mut wrong), 0);
        let mut done = retire(&mut l);
        assert_eq!(l.apply(&mut done), 0);
        assert_eq!(l.snapshot().retired_on_creator, 1);
    }
    #[test]
    fn invalid_names_are_not_returned_for_delete_and_context_loss_requires_real_group_retirement() {
        let mut l = Ledger::default();
        let a = make(&mut l, 7, 1);
        op(&mut l, a, PUBLISH, 0);
        l.observed(1, 7, false, false);
        let mut cancel = Command::new(CANCEL, 0);
        l.apply(&mut cancel);
        let mut next = Command::new(NEXT, 1);
        next.context = 100;
        assert_eq!(l.apply(&mut next), 2);
        assert_eq!(next.raw, 0);
        assert_eq!(l.snapshot().invalid_names_released, 1);
        assert_eq!(l.snapshot().retired_on_creator, 0);
        let b = make(&mut l, 8, 2);
        op(&mut l, b, PUBLISH, 0);
        l.observed(0, 0, false, true);
        l.apply(&mut cancel);
        next = Command::new(NEXT, 1);
        assert_eq!(l.apply(&mut next), 2);
        assert_eq!(next.raw, 0);
        assert_eq!(l.snapshot().context_reclaim_pending, 1);
        l.share_group_destroyed();
        assert_eq!(l.snapshot().live, 1);
        l.native_joined();
        l.share_group_destroyed();
        assert_eq!(l.snapshot().live, 0);
        assert_eq!(l.snapshot().released_with_share_group, 1);
    }
    #[test]
    fn bounded_pending_recent_events_overflow_and_first_failure_are_explicit() {
        let mut l = Ledger::default();
        for i in 1..=LIMIT {
            make(&mut l, i, i as u64);
        }
        let mut allowed = Command::new(ALLOWED, 1);
        assert_eq!(
            l.apply(&mut allowed),
            0,
            "Capacity refusal must precede another GL creation"
        );
        let mut c = Command::new(CREATE, 1);
        c.context = 100;
        c.raw = 99;
        c.frame = 99;
        assert_eq!(l.apply(&mut c), 0);
        let first = l.snapshot().first_ownership_failure.unwrap().sequence;
        for _ in 0..100 {
            let mut reject = Command::new(REJECT, 2);
            l.apply(&mut reject);
        }
        let p = l.snapshot();
        assert_eq!(p.live, LIMIT);
        assert_eq!(p.recent.len(), RECENT);
        assert!(p.events_omitted > 0);
        assert_eq!(p.first_ownership_failure.unwrap().sequence, first);
        let mut l = Ledger {
            next: u64::MAX,
            ..Ledger::default()
        };
        assert_eq!(l.apply(&mut c), 0);
        assert_eq!(l.snapshot().untracked_creates, 1);
    }
}
