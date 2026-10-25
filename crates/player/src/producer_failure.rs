//! Bounded first-failure delivery from the GS producer to its Rust owner.
use std::sync::atomic::{AtomicU64, Ordering};

pub const ENVIRONMENT_COMMAND: u32 = 0x50535801;
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: u32,
    pub result: u32,
}
#[derive(Default)]
pub struct Latch(AtomicU64);
impl Latch {
    /// Reporting never waits for the runtime owner or performs teardown.
    pub fn record(&self, failure: Failure) -> bool {
        let value = (u64::from(failure.kind) << 32) | u64::from(failure.result);
        value != 0
            && self
                .0
                .compare_exchange(0, value, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }
    pub fn get(&self) -> Option<Failure> {
        let value = self.0.load(Ordering::Acquire);
        (value != 0).then_some(Failure {
            kind: (value >> 32) as u32,
            result: value as u32,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_worker_failure_survives_until_owner_retirement() {
        let latch = std::sync::Arc::new(Latch::default());
        let worker = latch.clone();
        assert!(std::thread::spawn(move || worker.record(Failure {
            kind: 1,
            result: 0x911d
        }))
        .join()
        .unwrap());
        assert!(!latch.record(Failure { kind: 2, result: 0 }));
        assert_eq!(
            latch.get(),
            Some(Failure {
                kind: 1,
                result: 0x911d
            })
        );
        assert_eq!(latch.get(), latch.get());
        assert_eq!(Latch::default().get(), None);
    }
}
