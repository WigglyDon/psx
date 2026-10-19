use crate::saves::atomic_write;
use serde::{Deserialize, Serialize};
use std::{fs, io, path::Path};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Speed {
    #[default]
    Normal,
    Double,
    Quintuple,
}
impl Speed {
    pub fn multiplier(self) -> u8 {
        match self {
            Self::Normal => 1,
            Self::Double => 2,
            Self::Quintuple => 5,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
pub struct PlaybackSettings {
    pub volume: u8,
    pub muted: bool,
    pub speed: Speed,
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
            speed: Speed::Normal,
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
                let mut settings: Self =
                    serde_json::from_slice(&bytes).map_err(io::Error::other)?;
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
        for speed in [Speed::Double, Speed::Quintuple] {
            settings.speed = speed;
            assert_eq!(settings.output_volume(), 65);
        }
        settings.speed = Speed::Normal;
        assert_eq!(settings.output_volume(), 65);
        settings.muted = true;
        assert_eq!(settings.output_volume(), 0);
        settings.muted = false;
        settings.run_in_background = true;
        let restored: PlaybackSettings =
            serde_json::from_slice(&serde_json::to_vec(&settings).unwrap()).unwrap();
        assert_eq!(restored.output_volume(), 65);
        assert_eq!(restored.speed.multiplier(), 1);
        assert!(restored.run_in_background);
    }
}
