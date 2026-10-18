use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// One core-thread producer, one real-time consumer. Fixed storage; no locks,
/// allocation, core calls, or file work on the audio thread.
pub struct AudioQueue {
    frames: Box<[AtomicU32]>,
    read: AtomicUsize,
    write: AtomicUsize,
    volume: AtomicU32,
    speed: AtomicUsize,
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
            volume: AtomicU32::new(65536),
            speed: AtomicUsize::new(1),
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

    pub fn set_volume(&self, percent: u8) {
        let gain = if percent == 0 {
            0
        } else {
            // A perceptual -56..0 dB range gives quiet listening useful travel.
            (10_f32.powf((percent.min(100) as f32 - 100.) * 0.56 / 20.) * 65536.).round() as u32
        };
        self.volume.store(gain, Ordering::Relaxed);
    }

    /// Change only while producer and consumer are stopped. Averaging each
    /// group resamples sped-up audio into the fixed device rate, with pitch.
    pub fn set_speed_stopped(&self, multiplier: u8) {
        assert!(matches!(multiplier, 1 | 2 | 5));
        self.speed.store(multiplier as usize, Ordering::Relaxed);
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
        let speed = self.speed.load(Ordering::Relaxed);
        let count = (output.len() / 2).min(available / speed);
        let volume = self.volume.load(Ordering::Relaxed) as i32;
        output.fill(0);
        for pair in output.chunks_exact_mut(2).take(count) {
            let mut left = 0;
            let mut right = 0;
            for _ in 0..speed {
                let packed = self.frames[read % self.frames.len()].load(Ordering::Relaxed);
                left += packed as i16 as i32;
                right += (packed >> 16) as i16 as i32;
                read = read.wrapping_add(1);
            }
            pair[0] = (left / speed as i32 * volume / 65536) as i16;
            pair[1] = (right / speed as i32 * volume / 65536) as i16;
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
    fn volume_and_mute_consume_audio_without_clipping_or_replaying_it() {
        let queue = AudioQueue::new(4);
        queue.set_volume(50);
        queue.push(&[i16::MIN, i16::MAX]);
        let mut output = [0; 2];
        queue.pop(&mut output);
        assert!((-1310..=-1300).contains(&output[0]));
        assert!((1300..=1310).contains(&output[1]));
        queue.set_volume(0);
        queue.push(&[1234, -1234]);
        queue.pop(&mut output);
        assert_eq!(output, [0, 0]);
        assert_eq!(queue.queued(), 0);
        queue.set_volume(100);
        queue.push(&[321, -321]);
        queue.pop(&mut output);
        assert_eq!(output, [321, -321]);
    }
    #[test]
    fn speed_resampling_preserves_stereo_order_and_consumes_the_faster_stream() {
        let queue = AudioQueue::new(16);
        queue.set_speed_stopped(2);
        queue.push(&[100, -100, 300, -300, 500, -500, 700, -700]);
        let mut out = [0; 4];
        queue.pop(&mut out);
        assert_eq!(out, [200, -200, 600, -600]);
        assert_eq!(queue.queued(), 0);
        queue.set_speed_stopped(5);
        queue.push(&[i16::MIN; 10]);
        queue.pop(&mut out[..2]);
        assert_eq!(&out[..2], &[i16::MIN; 2]);
        assert_eq!(queue.queued(), 0);
    }
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
