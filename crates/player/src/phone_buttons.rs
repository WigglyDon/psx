//! Optional per-game phone keys. Platform delivery stays outside this module.
use crate::{saves::atomic_write, session::GameId};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum PhoneKey {
    VolumeUp,
    VolumeDown,
}
impl PhoneKey {
    pub fn label(self) -> &'static str {
        match self {
            Self::VolumeUp => "Volume up",
            Self::VolumeDown => "Volume down",
        }
    }
}

/// The shared libretro digital-button numbering, also used by touch input.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Button {
    Cross = 0,
    Square = 1,
    Select = 2,
    Start = 3,
    Up = 4,
    Down = 5,
    Left = 6,
    Right = 7,
    Circle = 8,
    Triangle = 9,
    L1 = 10,
    R1 = 11,
    L2 = 12,
    R2 = 13,
    L3 = 14,
    R3 = 15,
}
impl Button {
    pub const ALL: [Self; 16] = [
        Self::Cross,
        Self::Circle,
        Self::Square,
        Self::Triangle,
        Self::L1,
        Self::R1,
        Self::L2,
        Self::R2,
        Self::Select,
        Self::Start,
        Self::Up,
        Self::Down,
        Self::Left,
        Self::Right,
        Self::L3,
        Self::R3,
    ];
    pub fn mask(self) -> u16 {
        1 << self as u16
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Cross => "Cross ×",
            Self::Square => "Square □",
            Self::Select => "Select",
            Self::Start => "Start",
            Self::Up => "D-pad up",
            Self::Down => "D-pad down",
            Self::Left => "D-pad left",
            Self::Right => "D-pad right",
            Self::Circle => "Circle ○",
            Self::Triangle => "Triangle △",
            Self::L1 => "L1",
            Self::R1 => "R1",
            Self::L2 => "L2",
            Self::R2 => "R2",
            Self::L3 => "L3",
            Self::R3 => "R3",
        }
    }
    pub fn requires_sticks(self) -> bool {
        matches!(self, Self::L3 | Self::R3)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Bindings {
    volume_up: Option<Button>,
    volume_down: Option<Button>,
}
impl Bindings {
    pub fn get(self, key: PhoneKey) -> Option<Button> {
        match key {
            PhoneKey::VolumeUp => self.volume_up,
            PhoneKey::VolumeDown => self.volume_down,
        }
    }
    pub fn set(&mut self, key: PhoneKey, button: Option<Button>) {
        *match key {
            PhoneKey::VolumeUp => &mut self.volume_up,
            PhoneKey::VolumeDown => &mut self.volume_down,
        } = button;
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    games: BTreeMap<String, Bindings>,
}
impl Settings {
    pub fn load(root: &Path) -> io::Result<Self> {
        match fs::read(root.join("phone-buttons.json")) {
            Ok(bytes) => {
                let settings: Self = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
                for id in settings.games.keys() {
                    GameId::parse(id).map_err(io::Error::other)?;
                }
                Ok(settings)
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
    pub fn get(&self, id: &GameId) -> Bindings {
        self.games.get(id.as_str()).copied().unwrap_or_default()
    }
    /// Publish the live preference only after the atomic write and sync pass.
    /// A failed write leaves the in-memory binding unchanged.
    pub fn set(&mut self, root: &Path, id: &GameId, bindings: Bindings) -> io::Result<()> {
        let previous = if bindings == Bindings::default() {
            self.games.remove(id.as_str())
        } else {
            self.games.insert(id.as_str().into(), bindings)
        };
        let result = serde_json::to_vec(self)
            .map_err(io::Error::other)
            .and_then(|bytes| atomic_write(&root.join("phone-buttons.json"), &bytes));
        if result.is_err() {
            match previous {
                Some(old) => {
                    self.games.insert(id.as_str().into(), old);
                }
                None => {
                    self.games.remove(id.as_str());
                }
            }
        }
        result
    }
}

#[derive(Default)]
pub struct PhoneInput {
    // A cleared mask still owns repeat/up delivery until release, preventing
    // a held game key from becoming a system-volume repeat after interruption.
    held: BTreeMap<(i32, PhoneKey), u16>,
    pending: u16,
}
impl PhoneInput {
    /// Some means consumed; its mask contains only newly pressed controls.
    /// A repeat never starts a new press, including after focus/menu changes.
    pub fn down(
        &mut self,
        device: i32,
        key: PhoneKey,
        repeat: bool,
        bindings: Bindings,
        playing: bool,
    ) -> Option<u16> {
        let source = (device, key);
        if repeat {
            return self.held.contains_key(&source).then_some(0);
        }
        self.held.remove(&source); // A new down supersedes a lost old up.
        let button = bindings.get(key).filter(|_| playing)?;
        // Only two phone keys are supported. Bound source bookkeeping even
        // if a platform sends keys from many detached external input devices.
        if self.held.len() >= 16 {
            return None;
        }
        let pressed = button.mask() & !self.mask();
        self.held.insert(source, button.mask());
        self.pending |= pressed;
        Some(pressed)
    }
    pub fn up(&mut self, device: i32, key: PhoneKey) -> bool {
        self.held.remove(&(device, key)).is_some()
    }
    pub fn cancel(&mut self) {
        self.held.values_mut().for_each(|mask| *mask = 0);
        self.pending = 0;
    }
    pub fn mask(&self) -> u16 {
        self.held.values().fold(0, |all, mask| all | mask)
    }
    /// Preserve a complete short tap between two emulation input samples.
    pub fn frame_mask(&mut self) -> u16 {
        self.mask() | std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bindings() -> Bindings {
        Bindings {
            volume_up: Some(Button::R1),
            volume_down: Some(Button::L1),
        }
    }

    #[test]
    fn game_key_and_touch_compose_and_release_independently() {
        let mut keys = PhoneInput::default();
        let mut touch = crate::input::Touches::default();
        assert_eq!(
            keys.down(7, PhoneKey::VolumeUp, false, bindings(), true),
            Some(Button::R1.mask())
        );
        touch.update(1, Button::Square.mask());
        assert_eq!(
            keys.frame_mask() | touch.frame_mask(),
            Button::R1.mask() | Button::Square.mask()
        );
        touch.update(2, Button::R1.mask());
        assert!(!keys.up(8, PhoneKey::VolumeUp));
        assert!(keys.up(7, PhoneKey::VolumeUp));
        assert_eq!(
            keys.frame_mask() | touch.frame_mask(),
            Button::R1.mask() | Button::Square.mask()
        );
        touch.release(2);
        assert_eq!(
            keys.frame_mask() | touch.frame_mask(),
            Button::Square.mask()
        );
    }

    #[test]
    fn repeat_and_interruptions_cannot_latch_or_rearm_a_button() {
        let mut keys = PhoneInput::default();
        keys.down(7, PhoneKey::VolumeUp, false, bindings(), true);
        assert_eq!(
            keys.down(7, PhoneKey::VolumeUp, true, bindings(), true),
            Some(0)
        );
        keys.cancel();
        assert_eq!(keys.frame_mask(), 0);
        for playing in [false, true] {
            assert_eq!(
                keys.down(7, PhoneKey::VolumeUp, true, bindings(), playing),
                Some(0)
            );
            assert_eq!(keys.frame_mask(), 0);
        }
        assert!(keys.up(7, PhoneKey::VolumeUp));
        assert_eq!(
            keys.down(7, PhoneKey::VolumeUp, true, bindings(), true),
            None
        );
        assert_eq!(
            keys.down(7, PhoneKey::VolumeUp, false, bindings(), false),
            None
        );
        assert_eq!(
            keys.down(7, PhoneKey::VolumeUp, false, Bindings::default(), true),
            None
        );
        assert!(!keys.up(7, PhoneKey::VolumeDown));
    }

    #[test]
    fn short_taps_and_same_target_keys_are_sampled_without_stealing_ownership() {
        let mut keys = PhoneInput::default();
        let mut bindings = bindings();
        bindings.set(PhoneKey::VolumeDown, Some(Button::R1));
        keys.down(7, PhoneKey::VolumeUp, false, bindings, true);
        keys.up(7, PhoneKey::VolumeUp);
        assert_eq!(keys.frame_mask(), Button::R1.mask());
        assert_eq!(keys.frame_mask(), 0);
        keys.down(7, PhoneKey::VolumeUp, false, bindings, true);
        assert_eq!(
            keys.down(7, PhoneKey::VolumeDown, false, bindings, true),
            Some(0)
        );
        keys.up(7, PhoneKey::VolumeUp);
        assert_eq!(keys.frame_mask(), Button::R1.mask());
        keys.cancel();
        assert_eq!(keys.frame_mask(), 0);
    }

    #[test]
    fn preferences_are_per_game_durable_and_a_failed_write_rolls_back() {
        let root = std::env::temp_dir().join(format!("psx-phone-buttons-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let a = GameId::parse("abc").unwrap();
        let b = GameId::parse("def").unwrap();
        let mut settings = Settings::load(&root).unwrap();
        assert_eq!(settings.get(&a), Bindings::default());
        settings.set(&root, &a, bindings()).unwrap();
        let mut restored = Settings::load(&root).unwrap();
        assert_eq!(restored.get(&a), bindings());
        assert_eq!(restored.get(&b), Bindings::default());
        let not_directory = root.join("blocked");
        fs::write(&not_directory, b"fixture").unwrap();
        assert!(restored
            .set(&not_directory, &a, Bindings::default())
            .is_err());
        assert_eq!(restored.get(&a), bindings());
        restored.set(&root, &a, Bindings::default()).unwrap();
        assert_eq!(Settings::load(&root).unwrap().get(&a), Bindings::default());
        fs::remove_dir_all(root).unwrap();
    }
}
