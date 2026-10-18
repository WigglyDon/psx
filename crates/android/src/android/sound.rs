use ndk::audio::{
    AudioCallbackResult, AudioContentType, AudioDirection, AudioFormat, AudioPerformanceMode,
    AudioSharingMode, AudioStream, AudioStreamBuilder, AudioUsage,
};
use psx_player::audio::AudioQueue;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub struct Sound {
    stream: AudioStream,
    pub failed: Arc<AtomicBool>,
}
impl Sound {
    pub fn start(queue: Arc<AudioQueue>) -> Result<Self, String> {
        let failed = Arc::new(AtomicBool::new(false));
        let error = failed.clone();
        let stream = AudioStreamBuilder::new()
            .map_err(|e| e.to_string())?
            .direction(AudioDirection::Output)
            .channel_count(2)
            .sample_rate(44100)
            .format(AudioFormat::PCM_I16)
            .usage(AudioUsage::Game)
            .content_type(AudioContentType::Music)
            .sharing_mode(AudioSharingMode::Shared)
            .performance_mode(AudioPerformanceMode::LowLatency)
            .data_callback(Box::new(move |_, data, frames| {
                if frames > 0 && !data.is_null() {
                    let output = unsafe {
                        std::slice::from_raw_parts_mut(data as *mut i16, frames as usize * 2)
                    };
                    queue.pop(output);
                }
                AudioCallbackResult::Continue
            }))
            .error_callback(Box::new(move |_, _| {
                error.store(true, Ordering::Release);
            }))
            .open_stream()
            .map_err(|e| format!("Sound could not start: {e}"))?;
        if stream.sample_rate() != 44100
            || stream.channel_count() != 2
            || stream.format() != AudioFormat::PCM_I16
        {
            return Err("The audio device did not provide the requested sound format.".into());
        }
        stream
            .set_buffer_size_in_frames(stream.frames_per_burst() * 3)
            .map_err(|e| e.to_string())?;
        stream.request_start().map_err(|e| e.to_string())?;
        Ok(Self { stream, failed })
    }
}
impl Drop for Sound {
    fn drop(&mut self) {
        let _ = self.stream.request_stop();
    }
}
