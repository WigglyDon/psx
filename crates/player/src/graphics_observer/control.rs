//! Development-only stop transport. Observation callbacks only set atomics and
//! wake owners. One bounded writer handles all filesystem work away from GL.
use super::{
    enabled::{Recorder, Snapshot},
    EXPORT_LIMIT, MAX_EXPORTS,
};
use serde::Serialize;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
        Mutex, OnceLock,
    },
    task::Waker,
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Copy)]
#[repr(u32)]
pub enum StopReason {
    Fence = 1,
    Context = 2,
    Dispatch = 3,
    Handle = 4,
    Ownership = 5,
    Fatal = 6,
    Operator = 7,
    Swap = 8,
    Dependency = 9,
}
static STOP: AtomicU32 = AtomicU32::new(0);
static FORCE: AtomicUsize = AtomicUsize::new(0);
static SHUTDOWN: AtomicBool = AtomicBool::new(false);
static WRITER: OnceLock<std::thread::Thread> = OnceLock::new();
static OWNER: OnceLock<Waker> = OnceLock::new();
static HANDLE: Mutex<Option<JoinHandle<()>>> = Mutex::new(None);

pub fn request_stop(reason: StopReason) {
    if STOP
        .compare_exchange(0, reason as u32, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        if let Some(owner) = OWNER.get() {
            owner.wake_by_ref();
        }
    }
    notify();
}
pub fn stop_requested() -> bool {
    STOP.load(Ordering::Acquire) != 0
}
pub fn notify() {
    if let Some(writer) = WRITER.get() {
        writer.unpark();
    }
}

#[derive(Serialize)]
struct FirstStop {
    version: u32,
    reason: u32,
    frame: u64,
    consumed: u64,
    mode: &'static str,
}

pub fn start(root: &Path, recorder: &'static Mutex<Recorder>, waker: Waker) -> std::io::Result<()> {
    let mut handle = HANDLE
        .lock()
        .map_err(|_| std::io::Error::other("Observer writer unavailable"))?;
    if handle.is_some() || OWNER.set(waker).is_err() {
        return Err(std::io::Error::other("Observer already started"));
    }
    let root = root.to_owned();
    *handle = Some(
        std::thread::Builder::new()
            .name("psx-observer".into())
            .spawn(move || {
                let _ = WRITER.set(std::thread::current());
                let mut count = 0;
                let mut first_stop_written = false;
                let mut seen_force = 0;
                let mut terminal_exported = false;
                loop {
                    if root.join("operator-stop").is_file() {
                        request_stop(StopReason::Operator);
                    }
                    let reason = STOP.load(Ordering::Acquire);
                    if reason != 0 && !first_stop_written {
                        let signal = FirstStop {
                            version: 2,
                            reason,
                            frame: super::enabled::produced(),
                            consumed: super::enabled::consumed(),
                            mode: super::mode(),
                        };
                        if let Ok(bytes) = serde_json::to_vec(&signal) {
                            if bytes.len() <= 4096
                                && crate::saves::atomic_write(&root.join("first-stop.json"), &bytes)
                                    .is_ok()
                            {
                                first_stop_written = true;
                            }
                        }
                    }
                    let force = FORCE.load(Ordering::Acquire);
                    let shutdown = SHUTDOWN.load(Ordering::Acquire);
                    // Locks cover a bounded memory copy only, never formatting or I/O.
                    let snapshot = if count < MAX_EXPORTS {
                        recorder.lock().ok().and_then(|r| {
                            let mut snapshot = r.snapshot();
                            snapshot.dropped += super::enabled::dropped();
                            ((count == 0 && snapshot.first_failure.is_some())
                                || (!terminal_exported && snapshot.first_terminal.is_some())
                                || force != seen_force
                                || shutdown)
                                .then_some(snapshot)
                        })
                    } else {
                        None
                    };
                    if let Some(snapshot) = snapshot {
                        count += 1;
                        seen_force = force;
                        terminal_exported |= snapshot.first_terminal.is_some();
                        let _ =
                            write_snapshot(&root.join(format!("observer-{count}.json")), &snapshot);
                    }
                    if shutdown {
                        break;
                    }
                    std::thread::park_timeout(Duration::from_millis(20));
                }
            })?,
    );
    Ok(())
}
fn write_snapshot(path: &Path, snapshot: &Snapshot) -> std::io::Result<()> {
    let bytes = serde_json::to_vec(snapshot)?;
    if bytes.len() > EXPORT_LIMIT {
        return Err(std::io::Error::other("Observer export exceeds bound"));
    }
    crate::saves::atomic_write(path, &bytes)
}
pub fn service(_root: &Path, force: bool) {
    if force {
        FORCE.fetch_add(1, Ordering::AcqRel);
    }
    notify();
}
pub fn join_export() {
    SHUTDOWN.store(true, Ordering::Release);
    notify();
    if let Ok(mut handle) = HANDLE.lock() {
        if let Some(writer) = handle.take() {
            let _ = writer.join();
        }
    }
}
