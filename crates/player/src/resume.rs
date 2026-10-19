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
const CORE: &str = "e436e9b85374def58d211df7f78880f0f4a917b4";
fn core(system: System) -> &'static str {
    if system == System::Ps2 {
        crate::ps2::revision()
    } else {
        CORE
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
}
pub struct Checkpoint {
    pub metadata: Metadata,
    pub bytes: Vec<u8>,
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
    if state.is_empty() || state.len() > max_size(game.system) {
        return Err(io::Error::other("Invalid checkpoint size."));
    }
    let metadata = serde_json::to_vec(&Metadata {
        core: core(game.system).into(),
        game: game.clone(),
        paused,
        card: digest(card),
        state: digest(state),
    })?;
    if metadata.len() > 64 * 1024 {
        return Err(io::Error::other("Checkpoint metadata is too large."));
    }
    let mut bytes = Vec::with_capacity(12 + metadata.len() + state.len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&metadata);
    bytes.extend_from_slice(state);
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
    if file.metadata()?.len() > (max_size(game.system) + 64 * 1024 + 12) as u64 {
        return Err(io::Error::other("The saved session is too large."));
    }
    let mut header = [0; 12];
    file.read_exact(&mut header)?;
    if &header[..8] != MAGIC {
        return Err(io::Error::other("The saved session format is unsupported."));
    }
    let length = u32::from_le_bytes(header[8..].try_into().unwrap()) as usize;
    if length > 64 * 1024 {
        return Err(io::Error::other("The saved session header is invalid."));
    }
    let mut metadata = vec![0; length];
    file.read_exact(&mut metadata)?;
    let metadata: Metadata = serde_json::from_slice(&metadata)?;
    let same_discs = metadata
        .game
        .disc_list()
        .iter()
        .map(|d| &d.id)
        .eq(game.disc_list().iter().map(|d| &d.id));
    if metadata.core != core(game.system)
        || metadata.game.system != game.system
        || metadata.game.id != game.id
        || !same_discs
        || metadata.game.last_disc != game.last_disc
    {
        return Err(io::Error::other("This session belongs to different core or disc inputs. Start the game again; your card is kept."));
    }
    if metadata.card != digest(card) {
        return Err(io::Error::other("Your memory card changed after this checkpoint. Start again to keep the newer card progress."));
    }
    let mut bytes = Vec::new();
    file.take(max_size(game.system) as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > max_size(game.system) || digest(&bytes) != metadata.state {
        return Err(io::Error::other(
            "The saved session is damaged. Your memory card is preserved.",
        ));
    }
    Ok(Some(Checkpoint { metadata, bytes }))
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
        save(
            root,
            game,
            checkpoint.metadata.paused,
            after,
            &checkpoint.bytes,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
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
