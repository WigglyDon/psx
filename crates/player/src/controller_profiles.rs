//! App defaults and explicit per-game controller overrides, independent of saves.
use crate::{
    input::ControllerSettings,
    phone_buttons::{Bindings, Button, PhoneKey, Settings as LegacyBindings},
    saves::atomic_write,
    session::GameId,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct GameController {
    enabled: bool,
    // Legacy phone-only assignments continue inheriting existing display layouts.
    layouts: Option<ControllerSettings>,
    bindings: Bindings,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    version: u32,
    default_bindings: Bindings,
    games: BTreeMap<String, GameController>,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            default_bindings: Bindings::default(),
            games: BTreeMap::new(),
        }
    }
}
impl Settings {
    pub fn load(root: &Path) -> io::Result<Self> {
        let settings = match fs::read(root.join("controller-profiles.json")) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // Never rewrite legacy files. The first successful profile write
                // publishes the migration; a failure leaves the old data usable.
                let legacy = LegacyBindings::load(root)?;
                Self {
                    games: legacy
                        .legacy_assignments()
                        .map(|(id, bindings)| {
                            (
                                id.to_owned(),
                                GameController {
                                    enabled: true,
                                    layouts: None,
                                    bindings,
                                },
                            )
                        })
                        .collect(),
                    ..Self::default()
                }
            }
            Err(e) => return Err(e),
        };
        if settings.version != 1 {
            return Err(io::Error::other(
                "These controller profiles need a compatible app version.",
            ));
        }
        for id in settings.games.keys() {
            GameId::parse(id).map_err(io::Error::other)?;
        }
        Ok(settings)
    }
    pub fn custom(&self, id: &GameId) -> bool {
        self.games.get(id.as_str()).is_some_and(|g| g.enabled)
    }
    pub fn bindings(&self, id: Option<&GameId>, analog: bool) -> Bindings {
        self.selected_bindings(id).for_sticks(analog)
    }
    fn selected_bindings(&self, id: Option<&GameId>) -> Bindings {
        id.and_then(|id| self.games.get(id.as_str()))
            .filter(|g| g.enabled)
            .map_or(self.default_bindings, |g| g.bindings)
    }
    pub fn controls(
        &self,
        root: &Path,
        id: Option<&GameId>,
        analog: bool,
    ) -> io::Result<ControllerSettings> {
        if let Some(layouts) = id
            .and_then(|id| self.games.get(id.as_str()))
            .filter(|g| g.enabled)
            .and_then(|g| g.layouts.as_ref())
        {
            return Ok(layouts.clone().with_analog(analog));
        }
        let controls = if analog {
            ControllerSettings::load_ps2(&root.join("ps2-controls"))
        } else {
            ControllerSettings::load(root)
        }?;
        Ok(controls.with_analog(analog))
    }
    fn publish(&mut self, root: &Path, next: Self) -> io::Result<()> {
        atomic_write(
            &root.join("controller-profiles.json"),
            &serde_json::to_vec(&next)?,
        )?;
        *self = next;
        Ok(())
    }
    /// Re-enabling retains the previous custom arrangement. First enable copies
    /// all current display profiles and default bindings, never other games.
    pub fn set_custom(
        &mut self,
        root: &Path,
        id: &GameId,
        enabled: bool,
        current: &ControllerSettings,
    ) -> io::Result<()> {
        let mut next = self.clone();
        let game = next
            .games
            .entry(id.as_str().into())
            .or_insert_with(|| GameController {
                bindings: self.default_bindings,
                ..GameController::default()
            });
        if enabled && game.layouts.is_none() {
            game.layouts = Some(current.clone());
        }
        game.enabled = enabled;
        self.publish(root, next)
    }
    pub fn save_controls(
        &mut self,
        root: &Path,
        id: Option<&GameId>,
        controls: &ControllerSettings,
    ) -> io::Result<()> {
        if let Some(id) = id.filter(|id| self.custom(id)) {
            let mut next = self.clone();
            next.games.get_mut(id.as_str()).unwrap().layouts = Some(controls.clone());
            self.publish(root, next)
        } else {
            controls.save(&if controls.analog() {
                root.join("ps2-controls")
            } else {
                root.to_owned()
            })
        }
    }
    pub fn set_bindings(
        &mut self,
        root: &Path,
        id: Option<&GameId>,
        bindings: Bindings,
    ) -> io::Result<()> {
        let mut next = self.clone();
        if let Some(id) = id.filter(|id| self.custom(id)) {
            next.games.get_mut(id.as_str()).unwrap().bindings = bindings;
        } else {
            next.default_bindings = bindings;
        }
        self.publish(root, next)
    }
    pub fn assign(
        &mut self,
        root: &Path,
        id: Option<&GameId>,
        key: PhoneKey,
        button: Option<Button>,
    ) -> io::Result<()> {
        let mut bindings = self.selected_bindings(id);
        bindings.set(key, button);
        self.set_bindings(root, id, bindings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Preset;
    use std::{
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "psx-game-controls-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn id(c: char) -> GameId {
        GameId::parse(&c.to_string().repeat(64)).unwrap()
    }

    #[test]
    fn games_inherit_defaults_until_enabled_and_reenable_recovers_only_their_customization() {
        let temp = Temp::new();
        let root = &temp.0;
        let (a, b) = (id('a'), id('b'));
        let mut global = ControllerSettings::default();
        for (w, h) in [(360., 800.), (800., 360.), (800., 800.)] {
            global.set(
                w,
                h,
                Preset {
                    follow_thumb: false,
                    ..Preset::default()
                },
            );
        }
        global.save(root).unwrap();
        let mut settings = Settings::load(root).unwrap();
        assert!(!settings.custom(&a) && !settings.custom(&b));
        settings
            .assign(root, None, PhoneKey::VolumeUp, Some(Button::R1))
            .unwrap();
        assert_eq!(
            settings.bindings(Some(&b), false).get(PhoneKey::VolumeUp),
            Some(Button::R1)
        );
        settings.set_custom(root, &a, true, &global).unwrap();
        let mut custom = settings.controls(root, Some(&a), false).unwrap();
        custom.set(
            360.,
            800.,
            Preset {
                game_scale: 0.8,
                follow_thumb: false,
                ..Preset::default()
            },
        );
        settings.save_controls(root, Some(&a), &custom).unwrap();
        settings
            .assign(root, Some(&a), PhoneKey::VolumeUp, Some(Button::Square))
            .unwrap();
        global.set(360., 800., Preset::default());
        settings.save_controls(root, Some(&b), &global).unwrap();
        let global_bytes = fs::read(root.join("controls.json")).unwrap();
        assert!(
            settings
                .controls(root, Some(&b), false)
                .unwrap()
                .preset(360., 800.)
                .follow_thumb
        );
        assert_eq!(
            settings
                .controls(root, Some(&a), false)
                .unwrap()
                .preset(360., 800.)
                .game_scale,
            0.8
        );
        settings.set_custom(root, &a, false, &custom).unwrap();
        let mut settings = Settings::load(root).unwrap();
        assert_eq!(
            settings.bindings(Some(&a), false).get(PhoneKey::VolumeUp),
            Some(Button::R1)
        );
        assert!(
            settings
                .controls(root, Some(&a), false)
                .unwrap()
                .preset(360., 800.)
                .follow_thumb
        );
        settings.set_custom(root, &a, true, &global).unwrap();
        assert_eq!(
            settings.bindings(Some(&a), false).get(PhoneKey::VolumeUp),
            Some(Button::Square)
        );
        let restored = settings.controls(root, Some(&a), false).unwrap();
        assert_eq!(restored.preset(360., 800.).game_scale, 0.8);
        assert!(!restored.preset(800., 360.).follow_thumb);
        assert!(!restored.preset(800., 800.).follow_thumb);
        assert!(!settings.custom(&b));
        assert_eq!(fs::read(root.join("controls.json")).unwrap(), global_bytes);
    }
    #[test]
    fn legacy_phone_assignments_migrate_without_changing_existing_layouts_or_reactivating_after_disable(
    ) {
        let temp = Temp::new();
        let root = &temp.0;
        let a = id('a');
        let mut legacy = LegacyBindings::default();
        let mut binding = Bindings::default();
        binding.set(PhoneKey::VolumeDown, Some(Button::L1));
        legacy.set(root, &a, binding).unwrap();
        let original = fs::read(root.join("phone-buttons.json")).unwrap();
        let mut global = ControllerSettings::default();
        global.set(
            800.,
            800.,
            Preset {
                follow_thumb: false,
                ..Preset::default()
            },
        );
        global.save(root).unwrap();
        let controls = fs::read(root.join("controls.json")).unwrap();
        let mut settings = Settings::load(root).unwrap();
        assert!(settings.custom(&a));
        assert_eq!(settings.bindings(Some(&a), false), binding);
        assert!(
            !settings
                .controls(root, Some(&a), false)
                .unwrap()
                .preset(800., 800.)
                .follow_thumb
        );
        assert!(!root.join("controller-profiles.json").exists());
        settings.set_custom(root, &a, false, &global).unwrap();
        let settings = Settings::load(root).unwrap();
        assert!(!settings.custom(&a));
        assert_eq!(settings.bindings(Some(&a), false), Bindings::default());
        assert_eq!(fs::read(root.join("phone-buttons.json")).unwrap(), original);
        assert_eq!(fs::read(root.join("controls.json")).unwrap(), controls);
    }
    #[test]
    fn ps2_capabilities_survive_reload_and_basic_controller_edits_preserve_stick_bindings() {
        let temp = Temp::new();
        let root = &temp.0;
        let a = id('a');
        let mut ps2 = ControllerSettings::default().with_analog(true);
        ps2.set(
            800.,
            800.,
            Preset {
                allow_overlap: true,
                ..Preset::default()
            },
        );
        ps2.save(&root.join("ps2-controls")).unwrap();
        let global = fs::read(root.join("ps2-controls/controls.json")).unwrap();
        let mut settings = Settings::load(root).unwrap();
        settings
            .assign(root, None, PhoneKey::VolumeUp, Some(Button::L3))
            .unwrap();
        settings
            .assign(root, None, PhoneKey::VolumeDown, Some(Button::Start))
            .unwrap();
        assert_eq!(settings.bindings(None, false).get(PhoneKey::VolumeUp), None);
        assert_eq!(
            settings.bindings(None, true).get(PhoneKey::VolumeUp),
            Some(Button::L3)
        );
        settings.set_custom(root, &a, true, &ps2).unwrap();
        let settings = Settings::load(root).unwrap();
        let controls = settings.controls(root, Some(&a), true).unwrap();
        assert!(controls.analog() && controls.preset(800., 800.).analog);
        assert!(controls.preset(800., 800.).allow_overlap);
        assert!(
            !settings
                .controls(root, Some(&a), false)
                .unwrap()
                .preset(800., 800.)
                .analog
        );
        assert_eq!(
            fs::read(root.join("ps2-controls/controls.json")).unwrap(),
            global
        );
    }
    #[test]
    fn failed_publication_does_not_change_live_scope_or_bindings() {
        let temp = Temp::new();
        let root = &temp.0;
        let a = id('a');
        fs::create_dir(root.join("controller-profiles.json")).unwrap();
        let mut settings = Settings::default();
        assert!(settings
            .set_custom(root, &a, true, &ControllerSettings::default())
            .is_err());
        assert!(!settings.custom(&a));
        assert!(settings
            .assign(root, None, PhoneKey::VolumeUp, Some(Button::R1))
            .is_err());
        assert_eq!(settings.bindings(None, true), Bindings::default());
        assert!(Settings::load(root).is_err());
    }
    #[test]
    fn unknown_versions_invalid_ids_and_damaged_profiles_are_retained() {
        let temp = Temp::new();
        let file = temp.0.join("controller-profiles.json");
        for bytes in [
            r#"{"version":2}"#,
            r#"{"games":{"../other":{}}}"#,
            r#"{"games":"#,
        ] {
            fs::write(&file, bytes).unwrap();
            assert!(Settings::load(&temp.0).is_err());
            assert_eq!(fs::read_to_string(&file).unwrap(), bytes);
        }
    }
}
