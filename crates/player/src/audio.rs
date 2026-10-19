use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

/// One core-thread producer, one real-time consumer. Fixed storage; no locks,
/// allocation, core calls, or file work on the audio thread.
pub struct AudioQueue {
    frames: Box<[AtomicU32]>,
    read: AtomicUsize,
    write: AtomicUsize,
    volume: AtomicU32,
    speed: AtomicUsize,
    target: AtomicUsize,
    sample_rate: AtomicUsize,
    consumer_limit: AtomicUsize,
    paused: AtomicBool,
    discard_before: AtomicUsize,
    discard_pending: AtomicBool,
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
            target: AtomicUsize::new(1100),
            sample_rate: AtomicUsize::new(44100),
            consumer_limit: AtomicUsize::new(1100),
            paused: AtomicBool::new(false),
            discard_before: AtomicUsize::new(0),
            discard_pending: AtomicBool::new(false),
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

    /// The consumer adopts this at its next callback, without restarting the
    /// device. Averaging groups retains the faster stream's pitch.
    pub fn set_speed(&self, multiplier: u8) {
        assert!(matches!(multiplier, 1 | 2 | 5));
        self.speed.store(multiplier as usize, Ordering::Relaxed);
    }

    /// Background scheduling needs more headroom than interactive play. The
    /// queue stays bounded, and return discards the extra latency immediately.
    pub fn set_background(&self, background: bool) {
        let target = match (self.sample_rate.load(Ordering::Relaxed), background) {
            (48000, false) => 1200,
            (48000, true) => 2400,
            (_, true) => 2205,
            _ => 1100,
        };
        self.target.store(target, Ordering::Relaxed);
    }
    pub fn set_sample_rate(&self, sample_rate: usize) {
        assert!(matches!(sample_rate, 44100 | 48000));
        self.sample_rate.store(sample_rate, Ordering::Relaxed);
        self.set_background(false);
    }
    pub fn target_frames(&self) -> usize {
        self.target.load(Ordering::Relaxed) * self.speed.load(Ordering::Relaxed)
    }

    /// Foreground pause keeps the device stream warm but silent. Only the
    /// consumer moves its cursor, including discarding sound queued at pause.
    pub fn set_paused(&self, paused: bool) {
        if self.paused.load(Ordering::Relaxed) == paused {
            return;
        }
        if paused {
            self.discard_before
                .store(self.write.load(Ordering::Acquire), Ordering::Release);
            self.discard_pending.store(true, Ordering::Release);
        }
        self.paused.store(paused, Ordering::Release);
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
        let paused = self.paused.load(Ordering::Acquire);
        let mut read = self.read.load(Ordering::Relaxed);
        let mut available = self.write.load(Ordering::Acquire).wrapping_sub(read);
        if self.discard_pending.swap(false, Ordering::AcqRel) {
            let discard = self
                .discard_before
                .load(Ordering::Acquire)
                .wrapping_sub(read);
            if discard <= available {
                read = read.wrapping_add(discard);
                available -= discard;
            }
        }
        output.fill(0);
        if paused {
            self.read
                .store(read.wrapping_add(available), Ordering::Release);
            return;
        }
        let speed = self.speed.load(Ordering::Relaxed);
        let limit = self.target.load(Ordering::Relaxed) * speed;
        if self.consumer_limit.swap(limit, Ordering::Relaxed) != limit {
            // Only the consumer advances read. Discard excess old-speed
            // backlog, retaining at most the new mode's normal fill target.
            let skip = available.saturating_sub(limit);
            read = read.wrapping_add(skip);
            available -= skip;
        }
        let count = (output.len() / 2).min(available / speed);
        let volume = self.volume.load(Ordering::Relaxed) as i32;
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
        self.discard_before.store(0, Ordering::Relaxed);
        self.discard_pending.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn foreground_pause_is_silent_without_underruns_or_replaying_old_sound() {
        let queue = AudioQueue::new(32);
        queue.push(&[100, -100, 200, -200]);
        queue.set_paused(true);
        let mut out = [7; 8];
        queue.pop(&mut out);
        queue.pop(&mut out);
        assert_eq!(out, [0; 8]);
        assert_eq!(queue.queued(), 0);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
        queue.set_paused(false);
        queue.push(&[300, -300]);
        let mut frame = [0; 2];
        queue.pop(&mut frame);
        assert_eq!(frame, [300, -300]);
    }

    #[test]
    fn pause_and_resume_between_callbacks_discard_only_pre_pause_audio() {
        let queue = AudioQueue::new(32);
        queue.push(&[100, -100, 200, -200]);
        queue.set_paused(true);
        queue.set_paused(false);
        queue.push(&[300, -300]);
        let mut frame = [0; 2];
        queue.pop(&mut frame);
        assert_eq!(frame, [300, -300]);
        assert_eq!(queue.queued(), 0);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn returning_from_background_trims_extra_latency_without_restarting_sound() {
        let queue = AudioQueue::new(16384);
        queue.set_background(true);
        queue.set_speed(5);
        assert_eq!(queue.target_frames(), 11025);
        queue.push(&vec![1000; 24000]);
        let mut output = [0; 2];
        queue.pop(&mut output);
        assert_eq!(queue.queued(), 11020);
        queue.set_background(false);
        queue.pop(&mut output);
        assert_eq!(queue.queued(), 5495);
        assert_eq!(output, [1000; 2]);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
        assert_eq!(queue.dropped.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn engine_rate_changes_use_matching_bounded_headroom() {
        let queue = AudioQueue::new(16384);
        assert_eq!(queue.target_frames(), 1100);
        queue.set_sample_rate(48000);
        assert_eq!(queue.target_frames(), 1200);
        queue.set_background(true);
        assert_eq!(queue.target_frames(), 2400);
        queue.set_speed(5);
        assert_eq!(queue.target_frames(), 12000);
        queue.set_sample_rate(44100);
        assert_eq!(queue.target_frames(), 5500);
        queue.set_background(true);
        assert_eq!(queue.target_frames(), 11025);
    }
    #[test]
    fn live_speed_change_trims_backlog_without_stopping_or_resetting_the_fifo() {
        let queue = AudioQueue::new(8192);
        queue.set_speed(5);
        queue.push(&vec![1234; 11000]);
        let mut out = [0; 2];
        queue.pop(&mut out);
        assert_eq!(out, [1234; 2]);
        queue.set_speed(1);
        queue.pop(&mut out);
        assert_eq!(out, [1234; 2]);
        assert_eq!(queue.queued(), 1099);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
        queue.push(&[2345, -2345]);
        assert_eq!(queue.queued(), 1100);
    }
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
        queue.set_speed(2);
        queue.push(&[100, -100, 300, -300, 500, -500, 700, -700]);
        let mut out = [0; 4];
        queue.pop(&mut out);
        assert_eq!(out, [200, -200, 600, -600]);
        assert_eq!(queue.queued(), 0);
        queue.set_speed(5);
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
