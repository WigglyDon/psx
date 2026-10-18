//! Local discovery and persistent choices. Scanning never enters the core and
//! never reads unrelated file contents. Android supplies authorized roots.
use crate::{
    files::{self, Disc, Game},
    saves::atomic_write,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Default, Serialize, Deserialize)]
pub struct Library {
    pub games: Vec<Game>,
}

impl Library {
    pub fn load(root: &Path) -> io::Result<Self> {
        let mut library = match fs::read(root.join("library.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other)?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(e),
        };
        if let Some(game) = files::recent(root) {
            if !library.games.iter().any(|g| g.id == game.id) {
                library.games.push(game);
            }
        }
        Ok(library)
    }

    pub fn save(&self, root: &Path) -> io::Result<()> {
        atomic_write(&root.join("library.json"), &serde_json::to_vec(self)?)
    }

    pub fn put(&mut self, game: Game) {
        if let Some(existing) = self.games.iter_mut().find(|g| g.id == game.id) {
            *existing = game;
        } else {
            self.games.push(game);
        }
        self.games.sort_by_key(|g| g.title.to_lowercase());
    }
}

#[derive(Debug, Clone)]
pub struct FoundDisc {
    pub path: PathBuf,
    pub number: u32,
    pub format: String,
}
#[derive(Debug, Clone)]
pub struct FoundGame {
    pub title: String,
    pub key: String,
    pub discs: Vec<FoundDisc>,
}
#[derive(Default)]
pub struct Scan {
    pub games: Vec<FoundGame>,
    pub visited: usize,
    pub unreadable: usize,
    pub rejected: usize,
    pub archives: usize,
    pub limited: bool,
}

/// A common filename convention is a grouping hint, never a game database.
/// Retain region and revision so differently named releases are not combined.
pub fn disc_name(path: &Path) -> (String, u32) {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    // ASCII markers must retain byte offsets into the original Unicode name.
    let lower = stem.to_ascii_lowercase();
    for marker in ["(disc ", "(disk ", "[disc ", "[disk "] {
        if let Some(start) = lower.rfind(marker) {
            let tail = &stem[start + marker.len()..];
            let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
            let rest = &tail[digits.len()..];
            if let Ok(number @ 1..=99) = digits.parse::<u32>() {
                if let Some(end) = rest.find([')', ']']) {
                    let title = format!("{}{}", &stem[..start], &rest[end + 1..])
                        .trim()
                        .to_owned();
                    return (title, number);
                }
            }
        }
    }
    (stem.into_owned(), 1)
}

fn read_at(file: &mut File, offset: u64, size: usize) -> io::Result<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))?;
    let mut bytes = vec![0; size];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn ps1_volume(path: &Path) -> io::Result<bool> {
    let mut file = File::open(path)?;
    for (sector, header) in [(2048, 0), (2352, 24), (2352, 16), (2336, 8)] {
        if let Ok(bytes) = read_at(&mut file, 16 * sector + header, 40) {
            if bytes[0] == 1 && &bytes[1..6] == b"CD001" && bytes[8..40].starts_with(b"PLAYSTATION")
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Fast structural checks, not a claim of complete game compatibility.
pub fn inspect_disc(path: &Path) -> io::Result<String> {
    let extension = files::extension(&path.to_string_lossy());
    if extension == "cue" {
        if fs::metadata(path)?.len() > 1024 * 1024 {
            return Err(io::Error::other("The CUE is too large."));
        }
        let text = fs::read_to_string(path)?;
        let tracks = files::cue_tracks(&text)?;
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("The CUE has no directory."))?;
        let mut ps1 = false;
        for track in tracks {
            let track = parent.join(track);
            if !track.is_file() {
                return Err(io::Error::other("A CUE track is missing."));
            }
            ps1 |= ps1_volume(&track)?;
        }
        if ps1 {
            return Ok("PS1 CUE".into());
        }
    } else if matches!(extension.as_str(), "iso" | "bin") {
        if ps1_volume(path)? {
            return Ok("PS1 disc image".into());
        }
    } else {
        let mut file = File::open(path)?;
        let header = read_at(&mut file, 0, 40)?;
        match extension.as_str() {
            "chd"
                if &header[..8] == b"MComprHD"
                    && (3..=5)
                        .contains(&u32::from_be_bytes(header[12..16].try_into().unwrap())) =>
            {
                return Ok("CHD container; compatibility checked on open".into())
            }
            "exe" if &header[..8] == b"PS-X EXE" && file.metadata()?.len() >= 2048 => {
                return Ok("PS-X EXE".into())
            }
            "pbp" if &header[..4] == b"\0PBP" => {
                let offset = u32::from_le_bytes(header[36..40].try_into().unwrap());
                let psar = read_at(&mut file, offset as u64, 16)?;
                if psar.starts_with(b"PSISOIMG0000") || psar.starts_with(b"PSTITLEIMG000000") {
                    return Ok("PS1 PBP".into());
                }
            }
            _ => {}
        }
    }
    Err(io::Error::other(
        "The file does not have a supported PS1 image header.",
    ))
}

pub fn scan(roots: Vec<PathBuf>, cancel: &AtomicBool) -> Scan {
    let mut result = Scan::default();
    let mut stack: Vec<_> = roots.into_iter().map(|p| (p, 0)).collect();
    let mut seen = BTreeSet::new();
    let mut candidates = BTreeSet::new();
    while let Some((directory, depth)) = stack.pop() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if depth > 64 || result.visited >= 1_000_000 || candidates.len() >= 10_000 {
            result.limited = true;
            break;
        }
        if !seen.insert(directory.clone()) {
            continue;
        }
        let entries = match fs::read_dir(&directory) {
            Ok(e) => e,
            Err(_) => {
                result.unreadable += 1;
                continue;
            }
        };
        for entry in entries {
            if cancel.load(Ordering::Relaxed) {
                return result;
            }
            let Ok(entry) = entry else {
                result.unreadable += 1;
                continue;
            };
            result.visited += 1;
            if result.visited >= 1_000_000 {
                result.limited = true;
                break;
            }
            let Ok(kind) = entry.file_type() else {
                result.unreadable += 1;
                continue;
            };
            let path = entry.path();
            if kind.is_dir() {
                // Protected app areas cannot contain user-granted game files.
                if directory.file_name().is_some_and(|n| n == "Android")
                    && matches!(entry.file_name().to_str(), Some("data" | "obb"))
                {
                    continue;
                }
                stack.push((path, depth + 1));
            } else if kind.is_file() {
                match files::extension(&path.to_string_lossy()).as_str() {
                    "cue" | "iso" | "bin" | "chd" | "pbp" | "exe" => {
                        candidates.insert(path);
                    }
                    "zip" | "7z" => result.archives += 1,
                    _ => {}
                }
            }
        }
    }
    let mut referenced = BTreeSet::new();
    for path in &candidates {
        if files::extension(&path.to_string_lossy()) == "cue"
            && fs::metadata(path).is_ok_and(|m| m.len() <= 1024 * 1024)
        {
            if let Ok(text) = fs::read_to_string(path) {
                if let Ok(tracks) = files::cue_tracks(&text) {
                    for track in tracks {
                        referenced.insert(path.parent().unwrap().join(track));
                    }
                }
            }
        }
    }
    let mut groups: BTreeMap<String, FoundGame> = BTreeMap::new();
    for path in candidates {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if referenced.contains(&path) {
            continue;
        }
        match inspect_disc(&path) {
            Ok(format) => {
                let (title, number) = disc_name(&path);
                let key = title.to_lowercase();
                let game = groups.entry(key.clone()).or_insert_with(|| FoundGame {
                    title,
                    key,
                    discs: Vec::new(),
                });
                game.discs.push(FoundDisc {
                    path,
                    number,
                    format,
                });
            }
            Err(_) => result.rejected += 1,
        }
    }
    result.games = groups.into_values().collect();
    for game in &mut result.games {
        game.discs.sort_by_key(|d| (d.number, d.path.clone()));
    }
    result
}

fn digest(path: &Path, cancel: &AtomicBool) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other("Adding games was cancelled."));
        }
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn disc_id(path: &Path, cancel: &AtomicBool) -> io::Result<String> {
    let mut selected = BTreeMap::new();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    selected.insert(name, digest(path, cancel)?);
    if files::extension(&path.to_string_lossy()) == "cue" {
        for track in files::cue_tracks(&fs::read_to_string(path)?)? {
            selected.insert(
                track.clone(),
                digest(&path.parent().unwrap().join(track), cancel)?,
            );
        }
    }
    // Same identity as the original private import. Existing cards survive.
    let mut hash = Sha256::new();
    for (name, digest) in selected {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(digest.as_bytes());
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Only selected games are hashed. Save once after successful completion;
/// cancelling or a failed addition leaves the on-disk library intact.
pub fn add(root: &Path, selected: Vec<FoundGame>, cancel: &AtomicBool) -> io::Result<Library> {
    let mut library = Library::load(root)?;
    for found in selected {
        let mut discs = Vec::new();
        for disc in found.discs {
            inspect_disc(&disc.path)?;
            let id = disc_id(&disc.path, cancel)?;
            if !discs.iter().any(|d: &Disc| d.id == id) {
                if discs.iter().any(|d: &Disc| d.number == disc.number) {
                    return Err(io::Error::other(format!("More than one different copy of disc {} was found for {}. Keep one copy in scanned storage, or use Browse files to choose one.",disc.number,found.title)));
                }
                discs.push(Disc {
                    id,
                    label: format!("Disc {}", disc.number),
                    number: disc.number,
                    path: disc.path,
                });
            }
        }
        if discs.is_empty() {
            continue;
        }
        let existing = library
            .games
            .iter()
            .find(|game| {
                game.disc_list()
                    .iter()
                    .any(|d| discs.iter().any(|new| new.id == d.id))
                    || disc_name(&game.path).0.to_lowercase() == found.key
            })
            .cloned();
        let mut game = existing.unwrap_or_else(|| Game {
            id: discs[0].id.clone(),
            title: found.title.clone(),
            path: discs[0].path.clone(),
            discs: Vec::new(),
            last_disc: 0,
        });
        let active_id = game.disc_list().get(game.last_disc).map(|d| d.id.clone());
        for old in game.disc_list() {
            if !discs.iter().any(|d| d.id == old.id) {
                discs.push(old);
            }
        }
        discs.sort_by_key(|d| d.number);
        game.last_disc = active_id
            .and_then(|id| discs.iter().position(|d| d.id == id))
            .unwrap_or(0);
        game.title = found.title;
        game.path = discs[0].path.clone();
        game.discs = discs;
        library.put(game);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(io::Error::other("Adding games was cancelled."));
    }
    library.save(root)?;
    Ok(library)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disc_grouping_keeps_region_and_uses_numeric_order() {
        assert_eq!(
            disc_name(Path::new("Game (USA) (Disc 12).cue")),
            ("Game (USA)".into(), 12)
        );
        assert_ne!(
            disc_name(Path::new("Game (Europe) (Disc 1).cue")).0,
            disc_name(Path::new("Game (USA) (Disc 1).cue")).0
        );
        assert_eq!(
            disc_name(Path::new("İ Example (DISC 2).cue")),
            ("İ Example".into(), 2)
        );
    }
    #[test]
    fn scan_checks_headers_and_companions_and_keeps_card_identity() {
        let root = std::env::temp_dir().join(format!("psx-scan-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let cue = root.join("Example (Disc 1).cue");
        fs::write(
            &cue,
            "FILE \"track.bin\" BINARY\n TRACK 01 MODE2/2352\n INDEX 01 00:00:00\n",
        )
        .unwrap();
        let cancel = AtomicBool::new(false);
        assert!(scan(vec![root.clone()], &cancel).games.is_empty());
        let mut bin = vec![0; 2352 * 18];
        let start = 2352 * 16 + 24;
        bin[start..start + 6].copy_from_slice(b"\x01CD001");
        bin[start + 8..start + 19].copy_from_slice(b"PLAYSTATION");
        fs::write(root.join("track.bin"), bin).unwrap();
        fs::write(root.join("fake.iso"), b"not a game").unwrap();
        let initial = scan(vec![root.clone()], &cancel);
        assert_eq!(initial.games.len(), 1);
        assert_eq!(initial.games[0].discs.len(), 1);
        assert_eq!(initial.rejected, 1);
        let library = add(&root, initial.games, &cancel).unwrap();
        let id = library.games[0].id.clone();
        assert_eq!(Library::load(&root).unwrap().games[0].id, id);
        let second = root.join("Example (Disc 2).cue");
        fs::copy(&cue, &second).unwrap();
        let found = scan(vec![root.clone()], &cancel);
        let added = add(&root, found.games, &cancel).unwrap();
        assert_eq!(added.games.len(), 1);
        let extra = &added.games[0];
        assert_eq!(extra.discs.len(), 2);
        assert!(extra.launch_path(&root).unwrap().is_file());
        assert_eq!(extra.id, id);
        fs::remove_dir_all(root).unwrap();
    }
}
