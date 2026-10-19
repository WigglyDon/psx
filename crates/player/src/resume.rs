//! Private, versioned automatic sessions. Memory cards remain authoritative.
use crate::{files::Game, saves::atomic_write};
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
    if state.is_empty() || state.len() > MAX_STATE_SIZE {
        return Err(io::Error::other("Invalid checkpoint size."));
    }
    let metadata = serde_json::to_vec(&Metadata {
        core: CORE.into(),
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
    if file.metadata()?.len() > (MAX_STATE_SIZE + 64 * 1024 + 12) as u64 {
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
        .map(|d| (&d.id, &d.path))
        .eq(game.disc_list().iter().map(|d| (&d.id, &d.path)));
    if metadata.core != CORE
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
    file.take(MAX_STATE_SIZE as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() > MAX_STATE_SIZE || digest(&bytes) != metadata.state {
        return Err(io::Error::other(
            "The saved session is damaged. Your memory card is preserved.",
        ));
    }
    Ok(Some(Checkpoint { metadata, bytes }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restart_round_trip_rejects_newer_cards_other_discs_and_damaged_state() {
        let root = std::env::temp_dir().join(format!("psx-resume-{}", std::process::id()));
        let game = Game {
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
        assert!(load(&root, &other, b"card").is_err());
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
