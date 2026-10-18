use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// One core-thread producer, one real-time consumer. Fixed storage; no locks,
/// allocation, core calls, or file work on the audio thread.
pub struct AudioQueue {
    frames: Box<[AtomicU32]>,
    read: AtomicUsize,
    write: AtomicUsize,
    pub underruns: AtomicU64,
    pub dropped: AtomicU64,
}

impl AudioQueue {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            frames: (0..capacity).map(|_| AtomicU32::new(0)).collect(),
            read: AtomicUsize::new(0),
            write: AtomicUsize::new(0),
            underruns: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn queued(&self) -> usize {
        self.write
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Acquire))
            .min(self.frames.len())
    }

    /// Called only by the producer. The core's batch is always consumed; excess
    /// samples are counted and discarded, rather than blocking emulation.
    pub fn push(&self, samples: &[i16]) {
        let mut write = self.write.load(Ordering::Relaxed);
        let free = self.frames.len() - write.wrapping_sub(self.read.load(Ordering::Acquire));
        let count = (samples.len() / 2).min(free);
        for pair in samples.chunks_exact(2).take(count) {
            let packed = pair[0] as u16 as u32 | ((pair[1] as u16 as u32) << 16);
            self.frames[write % self.frames.len()].store(packed, Ordering::Relaxed);
            write = write.wrapping_add(1);
        }
        self.write.store(write, Ordering::Release);
        self.dropped
            .fetch_add((samples.len() / 2 - count) as u64, Ordering::Relaxed);
    }

    /// Called only by the consumer, or with the stream closed.
    pub fn pop(&self, output: &mut [i16]) {
        let mut read = self.read.load(Ordering::Relaxed);
        let available = self.write.load(Ordering::Acquire).wrapping_sub(read);
        let count = (output.len() / 2).min(available);
        output.fill(0);
        for pair in output.chunks_exact_mut(2).take(count) {
            let packed = self.frames[read % self.frames.len()].load(Ordering::Relaxed);
            pair[0] = packed as i16;
            pair[1] = (packed >> 16) as i16;
            read = read.wrapping_add(1);
        }
        self.read.store(read, Ordering::Release);
        if count < output.len() / 2 {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Both producer and consumer must be stopped before clearing.
    pub fn clear_stopped(&self) {
        self.read.store(0, Ordering::Relaxed);
        self.write.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_wraps_bounds_latency_and_fills_silence() {
        let queue = AudioQueue::new(2);
        queue.push(&[1, -1, 2, -2, 3, -3]);
        let mut out = [0; 2];
        queue.pop(&mut out);
        assert_eq!(out, [1, -1]);
        queue.push(&[4, -4]);
        let mut out = [9; 6];
        queue.pop(&mut out);
        assert_eq!(out, [2, -2, 4, -4, 0, 0]);
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 1);
        assert_eq!(queue.queued(), 0);
    }
}
