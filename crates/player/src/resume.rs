//! Private, versioned automatic sessions. Memory cards remain authoritative.
use crate::{
    files::{Game, System},
    saves::atomic_write,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
};

pub const MAX_STATE_SIZE: usize = 64 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"PSXRES01";
const PICTURE_MAGIC: &[u8; 8] = b"PSXRES02";
const MAX_PICTURE_SIZE: usize = 1024 * 1024 * 4;
const CORE: &str = "e436e9b85374def58d211df7f78880f0f4a917b4";
fn core(root: &Path, game: &Game) -> io::Result<String> {
    if game.system == System::Ps2 {
        let firmware = crate::ps2::firmware_identity(&root.join("ps2").join(&game.id))
            .map_err(io::Error::other)?;
        Ok(format!("{}+firmware:{firmware}", crate::ps2::revision()))
    } else {
        Ok(CORE.into())
    }
}
fn max_size(system: System) -> usize {
    if system == System::Ps2 {
        crate::ps2::MAX_STATE_SIZE
    } else {
        MAX_STATE_SIZE
    }
}

#[derive(Serialize, Deserialize)]
pub struct Metadata {
    core: String,
    pub game: Game,
    pub paused: bool,
    card: String,
    state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    picture: Option<PictureMetadata>,
}
#[derive(Serialize, Deserialize)]
struct PictureMetadata {
    width: u32,
    height: u32,
    sha256: String,
}
fn picture_size(width: u32, height: u32) -> io::Result<usize> {
    if width == 0 || height == 0 || width > 1024 || height > 1024 {
        return Err(io::Error::other(
            "The saved picture dimensions are invalid.",
        ));
    }
    Ok(width as usize * height as usize * 4)
}
pub struct Checkpoint {
    pub metadata: Metadata,
    pub bytes: Vec<u8>,
    pub picture: Option<crate::ps2::Pixels>,
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn path(root: &Path, id: &str) -> io::Result<PathBuf> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(io::Error::other("Invalid saved game identity."));
    }
    Ok(root.join("resume").join(format!("{id}.state")))
}
pub fn save(root: &Path, game: &Game, paused: bool, card: &[u8], state: &[u8]) -> io::Result<()> {
    save_with_picture(root, game, paused, card, state, None)
}
/// One atomic checkpoint owns both machine bytes and its displayed picture.
/// Legacy records omit the picture; native state bytes and card identity remain
/// independently verified, with no GPU or platform handle in durable storage.
pub fn save_with_picture(
    root: &Path,
    game: &Game,
    paused: bool,
    card: &[u8],
    state: &[u8],
    picture: Option<&crate::ps2::Pixels>,
) -> io::Result<()> {
    if state.is_empty() || state.len() > max_size(game.system) {
        return Err(io::Error::other("Invalid checkpoint size."));
    }
    let picture_metadata = picture
        .map(|picture| {
            if picture_size(picture.width, picture.height)? != picture.rgba.len() {
                return Err(io::Error::other("The saved picture length is invalid."));
            }
            Ok(PictureMetadata {
                width: picture.width,
                height: picture.height,
                sha256: digest(&picture.rgba),
            })
        })
        .transpose()?;
    let metadata = serde_json::to_vec(&Metadata {
        core: core(root, game)?,
        game: game.clone(),
        paused,
        card: digest(card),
        state: digest(state),
        picture: picture_metadata,
    })?;
    if metadata.len() > 64 * 1024 {
        return Err(io::Error::other("Checkpoint metadata is too large."));
    }
    let mut bytes =
        Vec::with_capacity(12 + metadata.len() + state.len() + picture.map_or(0, |p| p.rgba.len()));
    bytes.extend_from_slice(if picture.is_some() {
        PICTURE_MAGIC
    } else {
        MAGIC
    });
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&metadata);
    bytes.extend_from_slice(state);
    if let Some(picture) = picture {
        bytes.extend_from_slice(&picture.rgba);
    }
    atomic_write(&path(root, &game.id)?, &bytes)?;
    mark_active(root, game)
}
pub fn mark_active(root: &Path, game: &Game) -> io::Result<()> {
    atomic_write(
        &root.join("active-session.json"),
        &serde_json::to_vec(&game.id)?,
    )
}
pub fn active(root: &Path) -> io::Result<Option<String>> {
    match fs::read(root.join("active-session.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
pub fn clear_active(root: &Path) -> io::Result<()> {
    remove(&root.join("active-session.json"))
}
fn remove(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}
pub fn discard(root: &Path, game: &Game) -> io::Result<()> {
    remove(&path(root, &game.id)?)
}

pub fn load(root: &Path, game: &Game, card: &[u8]) -> io::Result<Option<Checkpoint>> {
    let mut file = match File::open(path(root, &game.id)?) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };
    if file.metadata()?.len() > (max_size(game.system) + MAX_PICTURE_SIZE + 64 * 1024 + 12) as u64 {
        return Err(io::Error::other("The saved session is too large."));
    }
    let mut header = [0; 12];
    file.read_exact(&mut header)?;
    if &header[..8] != MAGIC && &header[..8] != PICTURE_MAGIC {
        return Err(io::Error::other("The saved session format is unsupported."));
    }
    let length = u32::from_le_bytes(header[8..].try_into().unwrap()) as usize;
    if length > 64 * 1024 {
        return Err(io::Error::other("The saved session header is invalid."));
    }
    let mut metadata = vec![0; length];
    file.read_exact(&mut metadata)?;
    let metadata: Metadata = serde_json::from_slice(&metadata)?;
    if (&header[..8] == PICTURE_MAGIC) != metadata.picture.is_some() {
        return Err(io::Error::other(
            "The saved picture format is unsupported. Your memory card is preserved.",
        ));
    }
    let same_discs = metadata
        .game
        .disc_list()
        .iter()
        .map(|d| &d.id)
        .eq(game.disc_list().iter().map(|d| &d.id));
    let current_core = core(root, game)?;
    let same_core = metadata.core == current_core
        || (game.system == System::Ps2
            && crate::ps2::compatible_checkpoint_binding(&metadata.core, &current_core));
    if !same_core
        || metadata.game.system != game.system
        || metadata.game.id != game.id
        || !same_discs
        || metadata.game.last_disc != game.last_disc
    {
        return Err(io::Error::other("This session belongs to different core, firmware or disc inputs. Start the game again; your card is kept."));
    }
    if metadata.card != digest(card) {
        return Err(io::Error::other("Your memory card changed after this checkpoint. Start again to keep the newer card progress."));
    }
    let picture_len = metadata
        .picture
        .as_ref()
        .map(|p| picture_size(p.width, p.height))
        .transpose()?
        .unwrap_or(0);
    let mut bytes = Vec::new();
    file.take((max_size(game.system) + picture_len + 1) as u64)
        .read_to_end(&mut bytes)?;
    let state_len = bytes.len().checked_sub(picture_len).ok_or_else(|| {
        io::Error::other("The saved picture is truncated. Your memory card is preserved.")
    })?;
    let picture = if let Some(picture) = &metadata.picture {
        let rgba = bytes.split_off(state_len);
        if digest(&rgba) != picture.sha256 {
            return Err(io::Error::other(
                "The saved picture is damaged. Your memory card is preserved.",
            ));
        }
        Some(crate::ps2::Pixels {
            width: picture.width,
            height: picture.height,
            rgba: std::sync::Arc::new(rgba),
        })
    } else {
        None
    };
    if bytes.is_empty() || bytes.len() > max_size(game.system) || digest(&bytes) != metadata.state {
        return Err(io::Error::other(
            "The saved session is damaged. Your memory card is preserved.",
        ));
    }
    Ok(Some(Checkpoint {
        metadata,
        bytes,
        picture,
    }))
}

/// The PS2 donor stamps its change-detector on orderly close. Rebind only the
/// just-validated checkpoint, under the same Writer, after native ownership
/// ends. A crash before this succeeds leaves a retained incompatible state;
/// it never rolls back the durable card.
pub fn seal_closed_cards(root: &Path, game: &Game, before: &[u8], after: &[u8]) -> io::Result<()> {
    if before == after {
        return Ok(());
    }
    if let Some(checkpoint) = load(root, game, before)? {
        save_with_picture(
            root,
            game,
            checkpoint.metadata.paused,
            after,
            &checkpoint.bytes,
            checkpoint.picture.as_ref(),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_picture_is_atomic_checked_and_survives_card_sealing() {
        let root = std::env::temp_dir().join(format!("psx-picture-resume-{}", std::process::id()));
        let game = Game {
            system: System::Ps2,
            id: "abcdef123".into(),
            title: "Fixture".into(),
            path: "fixture.iso".into(),
            discs: vec![],
            last_disc: 0,
        };
        fs::create_dir_all(root.join("ps2").join(&game.id)).unwrap();
        fs::write(
            root.join("ps2").join(&game.id).join("firmware.json"),
            format!(r#"{{"file":"fixture.bin","sha256":"{}"}}"#, "a".repeat(64)),
        )
        .unwrap();
        let picture = crate::ps2::Pixels {
            width: 2,
            height: 1,
            rgba: std::sync::Arc::new(vec![1, 2, 3, 255, 4, 5, 6, 255]),
        };
        save_with_picture(
            &root,
            &game,
            true,
            b"card",
            b"exact native bytes",
            Some(&picture),
        )
        .unwrap();
        let state_path = path(&root, &game.id).unwrap();
        let original = fs::read(&state_path).unwrap();
        assert_eq!(&original[..8], PICTURE_MAGIC);
        let restored = load(&root, &game, b"card").unwrap().unwrap();
        assert_eq!(restored.bytes, b"exact native bytes");
        assert!(restored.metadata.paused);
        let restored_picture = restored.picture.unwrap();
        assert_eq!((restored_picture.width, restored_picture.height), (2, 1));
        assert_eq!(restored_picture.rgba, picture.rgba);
        assert!(load(&root, &game, b"newer card").is_err());
        assert_eq!(fs::read(&state_path).unwrap(), original);
        for bad in [
            crate::ps2::Pixels {
                width: u32::MAX,
                ..picture.clone()
            },
            crate::ps2::Pixels {
                rgba: std::sync::Arc::new(vec![0; 7]),
                ..picture.clone()
            },
        ] {
            assert!(save_with_picture(&root, &game, true, b"card", b"state", Some(&bad)).is_err());
            assert_eq!(fs::read(&state_path).unwrap(), original);
        }
        let mut corrupt = original.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        fs::write(&state_path, &corrupt).unwrap();
        assert!(load(&root, &game, b"card").is_err());
        assert_eq!(fs::read(&state_path).unwrap(), corrupt);
        fs::write(&state_path, &original[..original.len() - 5]).unwrap();
        assert!(load(&root, &game, b"card").is_err());
        let metadata_len = u32::from_le_bytes(original[8..12].try_into().unwrap()) as usize;
        let mut corrupt_state = original.clone();
        corrupt_state[12 + metadata_len] ^= 1;
        fs::write(&state_path, &corrupt_state).unwrap();
        assert!(load(&root, &game, b"card").is_err());
        assert_eq!(fs::read(&state_path).unwrap(), corrupt_state);
        let mut wrong_version = original.clone();
        wrong_version[..8].copy_from_slice(MAGIC);
        fs::write(&state_path, &wrong_version).unwrap();
        assert!(load(&root, &game, b"card").is_err());
        fs::write(&state_path, &original).unwrap();
        seal_closed_cards(&root, &game, b"card", b"closed card").unwrap();
        let sealed = load(&root, &game, b"closed card").unwrap().unwrap();
        assert_eq!(sealed.bytes, b"exact native bytes");
        assert_eq!(sealed.picture.unwrap().rgba, picture.rgba);
        save(&root, &game, true, b"card", b"legacy native bytes").unwrap();
        assert_eq!(&fs::read(&state_path).unwrap()[..8], MAGIC);
        let legacy = load(&root, &game, b"card").unwrap().unwrap();
        assert!(legacy.picture.is_none());
        assert_eq!(legacy.bytes, b"legacy native bytes");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn native_revision_change_keeps_old_bytes_and_requires_the_exact_binding() {
        let root = std::env::temp_dir().join(format!("psx-fetch-upgrade-{}", std::process::id()));
        let game = Game {
            system: System::Ps2,
            id: "def987".into(),
            title: "Fixture".into(),
            path: "fixture.iso".into(),
            discs: vec![],
            last_disc: 0,
        };
        fs::create_dir_all(root.join("ps2/def987")).unwrap();
        let firmware = root.join("ps2/def987/firmware.json");
        fs::write(
            &firmware,
            format!(r#"{{"file":"fixture.bin","sha256":"{}"}}"#, "a".repeat(64)),
        )
        .unwrap();
        save(&root, &game, true, b"current card", b"complete machine").unwrap();
        let state_path = path(&root, &game.id).unwrap();
        let bytes = fs::read(&state_path).unwrap();
        let size = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let mut metadata: Metadata = serde_json::from_slice(&bytes[12..12 + size]).unwrap();
        let suffix = metadata.core.split_once("+firmware:").unwrap().1.to_owned();
        let prior = format!(
            "{}+878bdd9e47f6cb750c44ab6fca6413c8088b92a12c0e15e14a099e3ea6144ef6+firmware:{suffix}",
            crate::ps2::REVISION,
        );
        let current = metadata.core.clone();
        for (binding, allowed) in [
            (current, true),
            (prior.clone(), false),
            (
                prior.replace(
                    "878bdd9e47f6cb750c44ab6fca6413c8088b92a12c0e15e14a099e3ea6144ef6",
                    "92542cb62995f80d750a88f0a4f8209ed1e6804b5ebba60827319128c0edc83c",
                ),
                false,
            ),
            (
                prior.replace(
                    "878bdd9e47f6cb750c44ab6fca6413c8088b92a12c0e15e14a099e3ea6144ef6",
                    "23a0b217e21a9442dde06b009aa6911f782c083a5ff5c37593b415d615309765",
                ),
                false,
            ),
            (
                prior.replace(
                    "878bdd9e47f6cb750c44ab6fca6413c8088b92a12c0e15e14a099e3ea6144ef6",
                    "9f5bf1a2a6d6aa228d7789eb0e38bc7cb62c94c98ca12e6a88d36fd8899c01ac",
                ),
                false,
            ),
            (prior.replacen("878bdd9e", "00000000", 1), false),
            (prior.replace(&suffix, "other firmware"), false),
        ] {
            metadata.core = binding;
            let header = serde_json::to_vec(&metadata).unwrap();
            let mut old = MAGIC.to_vec();
            old.extend_from_slice(&(header.len() as u32).to_le_bytes());
            old.extend_from_slice(&header);
            old.extend_from_slice(&bytes[12 + size..]);
            fs::write(&state_path, &old).unwrap();
            assert_eq!(load(&root, &game, b"current card").is_ok(), allowed);
            assert_eq!(fs::read(&state_path).unwrap(), old);
            if allowed {
                let restored = load(&root, &game, b"current card").unwrap().unwrap();
                assert!(restored.metadata.paused);
                assert_eq!(restored.bytes, b"complete machine");
                assert!(load(&root, &game, b"newer card").is_err());
            }
        }
        assert!(!crate::ps2::compatible_checkpoint_binding(
            &prior,
            "unknown future build"
        ));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn ps2_bound_and_close_sealing_keep_current_cards_and_retain_failed_checkpoints() {
        let root = std::env::temp_dir().join(format!("psx-ps2-resume-{}", std::process::id()));
        let game = Game {
            system: System::Ps2,
            id: "def123".into(),
            title: "Fixture".into(),
            path: "fixture.iso".into(),
            discs: vec![],
            last_disc: 0,
        };
        let state = vec![9; MAX_STATE_SIZE + 1];
        fs::create_dir_all(root.join("ps2/def123")).unwrap();
        fs::write(
            root.join("ps2/def123/firmware.json"),
            format!(r#"{{"file":"fixture.bin","sha256":"{}"}}"#, "a".repeat(64)),
        )
        .unwrap();
        save(&root, &game, true, b"flushed card", &state).unwrap();
        assert_eq!(
            load(&root, &game, b"flushed card")
                .unwrap()
                .unwrap()
                .bytes
                .len(),
            state.len()
        );
        seal_closed_cards(&root, &game, b"flushed card", b"closed card").unwrap();
        assert!(
            load(&root, &game, b"closed card")
                .unwrap()
                .unwrap()
                .metadata
                .paused
        );
        let before = fs::read(path(&root, &game.id).unwrap()).unwrap();
        let binding = root.join("ps2/def123/firmware.json");
        let original_firmware = fs::read(&binding).unwrap();
        fs::write(
            &binding,
            format!(r#"{{"file":"fixture.bin","sha256":"{}"}}"#, "b".repeat(64)),
        )
        .unwrap();
        assert!(load(&root, &game, b"closed card").is_err());
        assert_eq!(fs::read(path(&root, &game.id).unwrap()).unwrap(), before);
        fs::write(&binding, original_firmware).unwrap();
        assert!(load(&root, &game, b"newer card").is_err());
        assert!(seal_closed_cards(&root, &game, b"wrong card", b"other card").is_err());
        assert_eq!(fs::read(path(&root, &game.id).unwrap()).unwrap(), before);
        // Make the target directory unusable in an isolated fixture. No quota
        // exhaustion or mutations to an owner's card are needed to test this.
        let bad = root.join("unavailable");
        fs::create_dir_all(&bad).unwrap();
        fs::write(bad.join("resume"), b"blocked").unwrap();
        assert!(save(&bad, &game, false, b"current card", b"new state").is_err());
        assert_eq!(fs::read(path(&root, &game.id).unwrap()).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn restart_round_trip_rejects_newer_cards_other_discs_and_damaged_state() {
        let root = std::env::temp_dir().join(format!("psx-resume-{}", std::process::id()));
        let game = Game {
            system: crate::files::System::Ps1,
            id: "abc123".into(),
            title: "Fixture".into(),
            path: "fixture.iso".into(),
            discs: vec![],
            last_disc: 0,
        };
        save(&root, &game, true, b"card", b"machine").unwrap();
        assert_eq!(active(&root).unwrap(), Some(game.id.clone()));
        let loaded = load(&root, &game, b"card").unwrap().unwrap();
        assert_eq!(loaded.metadata.core, CORE);
        assert!(loaded.metadata.paused);
        assert_eq!(loaded.bytes, b"machine");
        assert!(load(&root, &game, b"newer card").is_err());
        let mut other = game.clone();
        other.path = "other.iso".into();
        // Library relinking verifies the original content before changing paths.
        assert!(load(&root, &other, b"card").unwrap().is_some());
        other.id = "def456".into();
        save(&root, &other, false, b"card", b"other machine").unwrap();
        let mut wrong_disc = game.clone();
        wrong_disc.discs = vec![crate::files::Disc {
            id: other.id,
            label: "Other".into(),
            number: 1,
            path: other.path,
        }];
        assert!(load(&root, &wrong_disc, b"card").is_err());
        let mut data = fs::read(path(&root, &game.id).unwrap()).unwrap();
        *data.last_mut().unwrap() ^= 1;
        fs::write(path(&root, &game.id).unwrap(), data).unwrap();
        assert!(load(&root, &game, b"card").is_err());
        clear_active(&root).unwrap();
        assert!(active(&root).unwrap().is_none());
        discard(&root, &game).unwrap();
        assert!(load(&root, &game, b"card").unwrap().is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
