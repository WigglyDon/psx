use crate::settings::Speed;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};

// Half-percent units retain the existing resampling ratios and allow small
// audio-side clock correction without floating point on the real-time thread.
const FRAME_PHASE: usize = 200;

/// One core-thread producer, one real-time consumer. Fixed storage; no locks,
/// allocation, core calls, or file work on the audio thread.
pub struct AudioQueue {
    frames: Box<[AtomicU32]>,
    read: AtomicUsize,
    write: AtomicUsize,
    volume: AtomicU32,
    speed: AtomicUsize,
    /// Fraction of the current source frame, owned only by the consumer.
    phase: AtomicUsize,
    target: AtomicUsize,
    sample_rate: AtomicUsize,
    consumer_limit: AtomicUsize,
    core_clock: AtomicBool,
    background: AtomicBool,
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
            speed: AtomicUsize::new(100),
            phase: AtomicUsize::new(0),
            target: AtomicUsize::new(1100),
            sample_rate: AtomicUsize::new(44100),
            consumer_limit: AtomicUsize::new(1100),
            core_clock: AtomicBool::new(false),
            background: AtomicBool::new(false),
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
    /// device. Fractional box resampling retains changes in playback pitch.
    pub fn set_speed(&self, speed: Speed) {
        self.speed
            .store(speed.percent() as usize, Ordering::Relaxed);
    }

    /// A threaded core owns its emulation clock. Keep normal-speed audio
    /// buffered across bounded captures and refill it with small audio-side
    /// drift correction; never adjust the core's requested time scale.
    pub fn set_core_clock(&self, owns_clock: bool) {
        self.core_clock.store(owns_clock, Ordering::Relaxed);
    }

    fn base_target(&self, requested: usize) -> usize {
        if self.core_clock.load(Ordering::Relaxed) && requested == 100 {
            let milliseconds = if self.background.load(Ordering::Relaxed) {
                120
            } else {
                80
            };
            (self.sample_rate.load(Ordering::Relaxed) * milliseconds)
                .div_ceil(1000)
                .min(self.frames.len())
        } else {
            self.target.load(Ordering::Relaxed)
        }
    }

    /// Background scheduling needs more headroom than interactive play. The
    /// queue stays bounded, and return discards the extra latency immediately.
    pub fn set_background(&self, background: bool) {
        self.background.store(background, Ordering::Relaxed);
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
        let speed = self.speed.load(Ordering::Relaxed);
        let target = self.base_target(speed);
        if speed == Speed::UNLIMITED.percent() as usize {
            target
        } else {
            (target * speed).div_ceil(100).max(1)
        }
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
        let mut phase = self.phase.load(Ordering::Relaxed);
        let mut available = self.write.load(Ordering::Acquire).wrapping_sub(read);
        if self.discard_pending.swap(false, Ordering::AcqRel) {
            let discard = self
                .discard_before
                .load(Ordering::Acquire)
                .wrapping_sub(read);
            if discard <= available {
                read = read.wrapping_add(discard);
                available -= discard;
                phase = 0;
            }
        }
        output.fill(0);
        if paused {
            self.read
                .store(read.wrapping_add(available), Ordering::Release);
            self.phase.store(0, Ordering::Relaxed);
            return;
        }
        let requested = self.speed.load(Ordering::Relaxed);
        if requested == 0 {
            self.read
                .store(read.wrapping_add(available), Ordering::Release);
            self.phase.store(0, Ordering::Relaxed);
            return;
        }
        let target = self.base_target(requested);
        let unlimited = requested == Speed::UNLIMITED.percent() as usize;
        // In uncapped mode source production varies with actual achieved speed.
        // Queue fill provides bounded feedback without clocks, locks or a second
        // producer. The audio ratio is bounded; it never caps core execution.
        let mut speed = FRAME_PHASE / 100
            * if unlimited {
                (available * 100 / target).clamp(100, 3200)
            } else {
                requested
            };
        if self.core_clock.load(Ordering::Relaxed) && requested == 100 {
            // An initial reserve alone drains on every capture. Replenish it
            // between captures with at most0.5% pitch/rate correction, using a
            // dead band to avoid reacting to ordinary guest-frame bursts.
            if available * 10 < target * 9 {
                speed -= 1;
            } else if available * 10 > target * 11 {
                speed += 1;
            }
        }
        let limit = if unlimited {
            self.frames.len()
        } else {
            // The retention limit follows the requested mode, not the small
            // feedback correction; changing it per callback would trim audio.
            let target = (target * requested).div_ceil(100).max(1);
            if requested < 100 {
                // Sources arrive in whole guest-frame bursts. At slow rates
                // a 25 ms output target can be smaller than one such burst;
                // trimming it would discard most of that frame's sound.
                target.max(self.sample_rate.load(Ordering::Relaxed).div_ceil(45))
            } else {
                target
            }
        };
        if self.consumer_limit.swap(limit, Ordering::Relaxed) != limit {
            // Only the consumer advances read. Discard excess old-speed
            // backlog, retaining at most the new mode's normal fill target.
            let skip = available.saturating_sub(limit);
            read = read.wrapping_add(skip);
            available -= skip;
            if skip > 0 {
                phase = 0;
            }
        }
        let count = (output.len() / 2).min((available * FRAME_PHASE).saturating_sub(phase) / speed);
        let volume = self.volume.load(Ordering::Relaxed) as i32;
        for pair in output.chunks_exact_mut(2).take(count) {
            let mut left = 0;
            let mut right = 0;
            let mut remaining = speed;
            while remaining > 0 {
                let packed = self.frames[read % self.frames.len()].load(Ordering::Relaxed);
                let weight = remaining.min(FRAME_PHASE - phase);
                left += packed as i16 as i32 * weight as i32;
                right += (packed >> 16) as i16 as i32 * weight as i32;
                remaining -= weight;
                phase += weight;
                if phase == FRAME_PHASE {
                    read = read.wrapping_add(1);
                    phase = 0;
                }
            }
            pair[0] = (left / speed as i32 * volume / 65536) as i16;
            pair[1] = (right / speed as i32 * volume / 65536) as i16;
        }
        self.phase.store(phase, Ordering::Relaxed);
        self.read.store(read, Ordering::Release);
        if count < output.len() / 2 {
            self.underruns.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Both producer and consumer must be stopped before clearing.
    pub fn clear_stopped(&self) {
        self.read.store(0, Ordering::Relaxed);
        self.write.store(0, Ordering::Relaxed);
        self.phase.store(0, Ordering::Relaxed);
        self.discard_before.store(0, Ordering::Relaxed);
        self.discard_pending.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_clock_audio_refills_between_observed_checkpoint_gaps() {
        fn trial(core_clock: bool) -> u64 {
            let queue = AudioQueue::new(16384);
            queue.set_sample_rate(48000);
            queue.set_core_clock(core_clock);
            let samples: Vec<i16> = (0..192).flat_map(|_| [1000, -1000]).collect();
            let mut output = [0; 384];
            queue.pop(&mut output); // Adopt the consumer mode before observation.
            let initial: Vec<i16> = (0..1472).flat_map(|_| [1000, -1000]).collect();
            queue.push(&initial);
            let before = queue.underruns.load(Ordering::Relaxed);
            for _ in 0..3000 {
                //12s of the independently clocked producer.
                queue.push(&samples);
                queue.pop(&mut output);
            }
            for _ in 0..3 {
                // Actual99 trace:1472queued frames and about52ms capture+hash.
                // A larger initial reserve alone cannot survive repeated gaps;
                // the real queue must replenish it before the next checkpoint.
                for _ in 0..13 {
                    queue.pop(&mut output);
                    if core_clock {
                        assert!(output.chunks_exact(2).all(|s| s == [1000, -1000]));
                    }
                }
                for _ in 0..7500 {
                    //30s until the next snapshot.
                    queue.push(&samples);
                    queue.pop(&mut output);
                    if core_clock {
                        assert!(output.chunks_exact(2).all(|s| s == [1000, -1000]));
                    }
                }
            }
            assert_eq!(queue.dropped.load(Ordering::Relaxed), 0);
            queue.underruns.load(Ordering::Relaxed) - before
        }
        assert!(
            trial(false) > 0,
            "The retained uncorrected queue must reproduce starvation"
        );
        assert_eq!(trial(true), 0);
    }
    #[test]
    fn core_clock_reserve_leaves_ps1_and_other_time_scales_unchanged() {
        let queue = AudioQueue::new(16384);
        assert_eq!(queue.target_frames(), 1100);
        queue.set_sample_rate(48000);
        queue.set_core_clock(true);
        assert_eq!(queue.target_frames(), 3840);
        queue.set_background(true);
        assert_eq!(queue.target_frames(), 5760);
        queue.set_speed(Speed::DOUBLE);
        assert_eq!(queue.target_frames(), 4800);
        queue.set_background(false);
        assert_eq!(queue.target_frames(), 2400);
        queue.set_speed(Speed::UNLIMITED);
        assert_eq!(queue.target_frames(), 1200);
        queue.set_speed(Speed::NORMAL);
        queue.set_core_clock(false);
        assert_eq!(queue.target_frames(), 1200);
        queue.set_sample_rate(44100);
        assert_eq!(queue.target_frames(), 1100);
    }
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
        queue.set_speed(Speed::from_percent(500).unwrap());
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
        queue.set_speed(Speed::from_percent(500).unwrap());
        assert_eq!(queue.target_frames(), 12000);
        queue.set_sample_rate(44100);
        assert_eq!(queue.target_frames(), 5500);
        queue.set_background(true);
        assert_eq!(queue.target_frames(), 11025);
    }
    #[test]
    fn live_speed_change_trims_backlog_without_stopping_or_resetting_the_fifo() {
        let queue = AudioQueue::new(8192);
        queue.set_speed(Speed::from_percent(500).unwrap());
        queue.push(&vec![1234; 11000]);
        let mut out = [0; 2];
        queue.pop(&mut out);
        assert_eq!(out, [1234; 2]);
        queue.set_speed(Speed::NORMAL);
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
        queue.set_speed(Speed::DOUBLE);
        queue.push(&[100, -100, 300, -300, 500, -500, 700, -700]);
        let mut out = [0; 4];
        queue.pop(&mut out);
        assert_eq!(out, [200, -200, 600, -600]);
        assert_eq!(queue.queued(), 0);
        queue.set_speed(Speed::from_percent(500).unwrap());
        queue.push(&[i16::MIN; 10]);
        queue.pop(&mut out[..2]);
        assert_eq!(&out[..2], &[i16::MIN; 2]);
        assert_eq!(queue.queued(), 0);
    }
    #[test]
    fn fractional_audio_keeps_its_phase_across_callbacks_and_slow_motion() {
        let queue = AudioQueue::new(32);
        queue.set_speed(Speed::from_percent(150).unwrap());
        queue.push(&[
            0, 0, 300, -300, 600, -600, 900, -900, 1200, -1200, 1500, -1500,
        ]);
        let mut frame = [0; 2];
        for expected in [100, 500, 1000, 1400] {
            queue.pop(&mut frame);
            assert_eq!(frame, [expected, -expected]);
        }
        assert_eq!(queue.queued(), 0);
        queue.set_speed(Speed::from_percent(50).unwrap());
        queue.push(&[100, -100, 300, -300]);
        for expected in [100, 100, 300, 300] {
            queue.pop(&mut frame);
            assert_eq!(frame, [expected, -expected]);
        }
        assert_eq!(queue.queued(), 0);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn slow_motion_keeps_the_source_frame_burst_when_adopting_its_rate() {
        let queue = AudioQueue::new(16384);
        queue.set_sample_rate(48000);
        queue.set_speed(Speed::from_percent(5).unwrap());
        queue.push(&vec![1234; 1600]);
        let mut out = [0; 400];
        for _ in 0..80 {
            queue.pop(&mut out);
            assert_eq!(out, [1234; 400]);
        }
        assert_eq!(queue.queued(), 0);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
    }
    #[test]
    fn uncapped_audio_is_bounded_and_zero_is_silent_without_dividing_by_zero() {
        let queue = AudioQueue::new(16384);
        queue.set_speed(Speed::UNLIMITED);
        queue.push(&vec![1234; 22000]);
        let mut out = [0; 4];
        queue.pop(&mut out);
        assert_eq!(out, [1234; 4]);
        assert_eq!(queue.queued(), 10980);
        queue.set_speed(Speed::NORMAL);
        queue.pop(&mut out);
        assert_eq!(queue.queued(), 1098);
        queue.set_speed(Speed::PAUSED);
        queue.pop(&mut out);
        assert_eq!(out, [0; 4]);
        assert_eq!(queue.queued(), 0);
        assert_eq!(queue.underruns.load(Ordering::Relaxed), 0);
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
