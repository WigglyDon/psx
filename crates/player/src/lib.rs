pub mod acceptance;
pub mod artwork;
pub mod audio;
pub mod content;
pub mod controller_profiles;
pub mod engine;
pub mod files;
mod firmware;
pub mod graphics_observer;
#[cfg(feature = "gs-replay")]
pub mod gs_replay;
#[cfg(feature = "gs-replay")]
pub mod gs_replay_runner;
#[cfg(all(feature = "gs-replay",target_os="linux"))]
pub mod gs_replay_zstd;
pub mod input;
pub mod library;
#[cfg(feature = "producer-local-output-sync")]
pub mod minimal_host;
mod native_signals;
pub mod phone_buttons;
pub mod producer_failure;
pub mod ps2;
mod ps2_vulkan;
pub mod resume;
pub mod retro;
pub mod saves;
pub mod session;
pub mod settings;
pub mod ui;

/// Activities and platform callbacks must select the same build-owned data
/// root. Ordinary releases keep the supplied root without probing test storage.
pub fn storage_root(normal: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let root = acceptance::storage_root(normal)?;
    graphics_observer::storage_root(&root)
}

/// User intent survives interruptions. Platform focus alone never unpauses a game.
#[derive(Debug, Default)]
pub struct Activity {
    pub user_paused: bool,
    /// Zero time is a separate intent; changing it never clears manual pause.
    pub time_paused: bool,
    pub foreground: bool,
    pub focused: bool,
    pub surface: bool,
    pub audio_focus: bool,
    pub loading: bool,
    /// Explicit opt-in, effective only while a platform playback service owns
    /// background execution. Audio focus and pause intent still apply.
    pub background_playback: bool,
}

impl Activity {
    pub fn can_run(&self) -> bool {
        !self.user_paused
            && !self.time_paused
            && ((self.foreground && self.focused && self.surface) || self.background_playback)
            && self.audio_focus
            && !self.loading
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returning_to_foreground_preserves_intentional_pause() {
        let mut activity = Activity {
            foreground: true,
            focused: true,
            surface: true,
            audio_focus: true,
            ..Activity::default()
        };
        assert!(activity.can_run());
        activity.user_paused = true;
        activity.foreground = false;
        assert!(!activity.can_run());
        activity.foreground = true;
        assert!(!activity.can_run());
        activity.user_paused = false;
        activity.surface = false;
        assert!(!activity.can_run());
    }
    #[test]
    fn zero_time_preserves_manual_pause_and_all_interruption_gates() {
        let mut activity = Activity {
            foreground: true,
            focused: true,
            surface: true,
            audio_focus: true,
            user_paused: true,
            time_paused: true,
            ..Activity::default()
        };
        assert!(!activity.can_run());
        activity.time_paused = false;
        assert!(!activity.can_run());
        activity.user_paused = false;
        assert!(activity.can_run());
        activity.time_paused = true;
        activity.background_playback = true;
        assert!(!activity.can_run());
        activity.time_paused = false;
        activity.audio_focus = false;
        assert!(!activity.can_run());
    }
    #[test]
    fn background_play_requires_opt_in_service_focus_and_explicit_play_intent() {
        let mut activity = Activity {
            audio_focus: true,
            ..Activity::default()
        };
        assert!(!activity.can_run());
        activity.background_playback = true;
        assert!(activity.can_run());
        activity.user_paused = true;
        assert!(!activity.can_run());
        activity.user_paused = false;
        activity.audio_focus = false;
        assert!(!activity.can_run());
    }
}
