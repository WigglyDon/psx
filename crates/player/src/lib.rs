pub mod artwork;
pub mod audio;
pub mod files;
pub mod input;
pub mod library;
pub mod retro;
pub mod saves;
pub mod ui;

/// User intent survives interruptions. Platform focus alone never unpauses a game.
#[derive(Debug, Default)]
pub struct Activity {
    pub user_paused: bool,
    pub foreground: bool,
    pub focused: bool,
    pub surface: bool,
    pub audio_focus: bool,
    pub loading: bool,
}

impl Activity {
    pub fn can_run(&self) -> bool {
        !self.user_paused
            && self.foreground
            && self.focused
            && self.surface
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
}
