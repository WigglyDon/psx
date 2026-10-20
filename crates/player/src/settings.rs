use crate::saves::atomic_write;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

/// Wall-time pacing only. Zero stops execution; MAX removes the pacing cap.
/// Integer hundredths keep persistence, slider steps and audio ratios exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Speed(u16);
impl Speed {
    pub const PAUSED: Self = Self(0);
    pub const NORMAL: Self = Self(100);
    pub const DOUBLE: Self = Self(200);
    pub const UNLIMITED: Self = Self(u16::MAX);
    pub fn from_percent(percent: u16) -> Option<Self> {
        (percent <= 500 || percent == u16::MAX).then_some(Self(percent))
    }
    pub fn percent(self) -> u16 {
        self.0
    }
    pub fn is_paused(self) -> bool {
        self == Self::PAUSED
    }
    pub fn is_fast(self) -> bool {
        self.0 > 100
    }
    pub fn multiplier(self) -> Option<f64> {
        (self != Self::UNLIMITED).then_some(self.0 as f64 / 100.)
    }
    pub fn from_slider(value: u8) -> Self {
        match value.min(100) {
            100 => Self::UNLIMITED,
            99 => Self(500),
            value => Self(value as u16 * 5),
        }
    }
    pub fn slider(self) -> u8 {
        if self == Self::UNLIMITED {
            100
        } else {
            (self.0 / 5).min(99) as u8
        }
    }
    pub fn label(self) -> String {
        match self {
            Self::PAUSED => "Paused".into(),
            Self::UNLIMITED => "Uncapped".into(),
            _ => format!(
                "{}×",
                format!("{:.2}", self.0 as f32 / 100.)
                    .trim_end_matches('0')
                    .trim_end_matches('.')
            ),
        }
    }
}
impl Default for Speed {
    fn default() -> Self {
        Self::NORMAL
    }
}
impl<'de> Deserialize<'de> for Speed {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Percent(u16),
            Legacy(String),
        }
        let speed = match Stored::deserialize(deserializer)? {
            Stored::Percent(value) => Self::from_percent(value),
            Stored::Legacy(value) => match value.as_str() {
                "Normal" => Some(Self::NORMAL),
                "Double" => Some(Self::DOUBLE),
                "Quintuple" => Some(Self(500)),
                _ => None,
            },
        };
        speed.ok_or_else(|| serde::de::Error::custom("Invalid time scale"))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackSettings {
    pub volume: u8,
    pub muted: bool,
    pub speed: Speed,
    pub timescale_enabled: bool,
    pub run_in_background: bool,
    #[serde(default)]
    volume_curve: u8,
    last_volume: u8,
}
impl Default for PlaybackSettings {
    fn default() -> Self {
        Self {
            volume: 50,
            muted: false,
            speed: Speed::DOUBLE,
            timescale_enabled: false,
            run_in_background: false,
            volume_curve: 1,
            last_volume: 50,
        }
    }
}
impl PlaybackSettings {
    pub fn load(root: &Path) -> io::Result<Self> {
        match fs::read(root.join("playback.json")) {
            Ok(bytes) => {
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                let legacy_speed = value.get("timescale_enabled").is_none();
                let has_speed = value.get("speed").is_some();
                let mut settings: Self = serde_json::from_value(value).map_err(io::Error::other)?;
                if legacy_speed {
                    settings.timescale_enabled = has_speed && settings.speed != Speed::NORMAL;
                    if !settings.timescale_enabled {
                        settings.speed = Speed::DOUBLE;
                    }
                }
                settings.volume = settings.volume.min(100);
                if settings.volume_curve == 0 {
                    // Preserve loudness when migrating the initial linear slider.
                    if settings.volume > 0 {
                        settings.volume = (100.
                            + 20. * (settings.volume as f32 / 100.).log10() / 0.56)
                            .round()
                            .clamp(1., 100.) as u8;
                    }
                    settings.volume_curve = 1;
                    settings.save(root)?;
                }
                if settings.volume > 0 {
                    settings.last_volume = settings.volume;
                }
                Ok(settings)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
    pub fn save(&self, root: &Path) -> io::Result<()> {
        atomic_write(&root.join("playback.json"), &serde_json::to_vec(self)?)
    }
    pub fn output_volume(&self) -> u8 {
        if self.muted {
            0
        } else {
            self.volume.min(100)
        }
    }
    pub fn effective_speed(&self) -> Speed {
        if self.timescale_enabled {
            self.speed
        } else {
            Speed::NORMAL
        }
    }
    pub fn toggle_timescale(&mut self) {
        self.timescale_enabled = !self.timescale_enabled;
    }
    pub fn set_timescale(&mut self, slider: u8) {
        self.speed = Speed::from_slider(slider);
        self.timescale_enabled = true;
    }
    pub fn set_volume(&mut self, value: u8) {
        self.volume = value.min(100);
        self.muted = self.volume == 0;
        if self.volume > 0 {
            self.last_volume = self.volume;
        }
    }
    pub fn toggle_mute(&mut self) {
        if self.muted || self.volume == 0 {
            self.muted = false;
            if self.volume == 0 {
                self.volume = self.last_volume.clamp(1, 100);
            }
        } else {
            self.muted = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn migrating_linear_volume_preserves_quiet_loudness_across_restarts() {
        let root = std::env::temp_dir().join(format!(
            "psx-volume-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("playback.json"),
            br#"{"volume":4,"muted":false,"speed":"Normal"}"#,
        )
        .unwrap();
        let migrated = PlaybackSettings::load(&root).unwrap();
        assert_eq!(migrated.volume, 50);
        assert!(!migrated.run_in_background);
        assert_eq!(migrated.volume_curve, 1);
        assert_eq!(PlaybackSettings::load(&root).unwrap().volume, 50);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn speed_modes_keep_volume_and_mute_independent() {
        let mut settings = PlaybackSettings {
            volume: 65,
            ..PlaybackSettings::default()
        };
        for speed in [Speed::DOUBLE, Speed::from_slider(99)] {
            settings.speed = speed;
            assert_eq!(settings.output_volume(), 65);
        }
        settings.speed = Speed::NORMAL;
        assert_eq!(settings.output_volume(), 65);
        settings.muted = true;
        assert_eq!(settings.output_volume(), 0);
        settings.muted = false;
        settings.run_in_background = true;
        let restored: PlaybackSettings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.output_volume(), 65);
        assert_eq!(restored.effective_speed(), Speed::NORMAL);
        assert!(restored.run_in_background);
    }
    #[test]
    fn timescale_toggle_keeps_the_selected_rate_including_pause_and_uncapped() {
        let mut settings = PlaybackSettings::default();
        assert_eq!(settings.effective_speed(), Speed::NORMAL);
        settings.toggle_timescale();
        assert_eq!(settings.effective_speed(), Speed::DOUBLE);
        for value in [0, 1, 10, 20, 25, 40, 98, 99, 100] {
            settings.set_timescale(value);
            let selected = settings.speed;
            assert_eq!(selected.slider(), value);
            settings.toggle_timescale();
            assert_eq!(settings.effective_speed(), Speed::NORMAL);
            let mut restored: PlaybackSettings =
                serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
            assert_eq!(restored.speed, selected);
            restored.toggle_timescale();
            assert_eq!(restored.effective_speed(), selected);
        }
        assert!(Speed::from_slider(0).is_paused());
        assert_eq!(Speed::from_slider(25).multiplier(), Some(1.25));
        assert_eq!(Speed::UNLIMITED.multiplier(), None);
        assert_eq!(Speed::from_slider(25).label(), "1.25×");
        assert!(serde_json::from_str::<Speed>("501").is_err());
        assert!(serde_json::from_str::<Speed>("-1").is_err());
    }
    #[test]
    fn legacy_speed_preferences_keep_their_effect_and_gain_a_remembered_toggle() {
        let root = std::env::temp_dir().join(format!("psx-speed-migration-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        for (legacy, enabled, selected) in [
            ("Normal", false, Speed::DOUBLE),
            ("Double", true, Speed::DOUBLE),
            ("Quintuple", true, Speed::from_slider(99)),
        ] {
            fs::write(
                root.join("playback.json"),
                format!(r#"{{"speed":"{legacy}","volume_curve":1}}"#),
            )
            .unwrap();
            let settings = PlaybackSettings::load(&root).unwrap();
            assert_eq!(settings.timescale_enabled, enabled);
            assert_eq!(settings.speed, selected);
            settings.save(&root).unwrap();
            assert_eq!(
                PlaybackSettings::load(&root).unwrap().effective_speed(),
                settings.effective_speed()
            );
        }
        fs::remove_dir_all(root).unwrap();
    }
}
