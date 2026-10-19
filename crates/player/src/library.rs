//! Local discovery and persistent choices. Scanning never enters the core and
//! never reads unrelated file contents. Android supplies authorized roots.
use crate::{
    files::{self, Disc, Game, System},
    saves::atomic_write,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Default, Serialize, Deserialize)]
pub struct Library {
    pub games: Vec<Game>,
}

impl Library {
    pub fn load(root: &Path) -> io::Result<Self> {
        match fs::read(root.join("library.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                let library = Self {
                    games: files::legacy_recent(root).into_iter().collect(),
                };
                if !library.games.is_empty() {
                    library.save(root)?;
                }
                Ok(library)
            }
            Err(e) => Err(e),
        }
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
        self.games
            .sort_by_key(|g| (g.system, g.title.to_lowercase()));
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
    pub system: System,
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

/// Fast structural checks, not a claim of complete game compatibility.
pub fn inspect_disc(path: &Path) -> io::Result<String> {
    crate::content::inspect(path).map(|(_, format)| format)
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
    let mut groups: BTreeMap<(System, String), FoundGame> = BTreeMap::new();
    for path in candidates {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if referenced.contains(&path) {
            continue;
        }
        match crate::content::inspect(&path) {
            Ok((system, format)) => {
                let (title, number) = disc_name(&path);
                let key = title.to_lowercase();
                let game = groups
                    .entry((system, key.clone()))
                    .or_insert_with(|| FoundGame {
                        system,
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
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("Missing disc name"))?
        .to_string_lossy();
    disc_id_as(path, &name, cancel)
}
fn disc_id_as(path: &Path, identity_name: &str, cancel: &AtomicBool) -> io::Result<String> {
    let mut selected = BTreeMap::new();
    selected.insert(identity_name.to_owned(), digest(path, cancel)?);
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

/// Resolve the same saved entry after directory or primary-file relocation.
/// Verify using its original import identity, never a matching title alone.
pub fn relink(mut game: Game, index: usize, path: &Path, cancel: &AtomicBool) -> io::Result<Game> {
    inspect_disc(path)?;
    let mut discs = game.disc_list();
    let disc = discs
        .get_mut(index)
        .ok_or_else(|| io::Error::other("This disc is no longer in the entry."))?;
    let name = disc
        .path
        .file_name()
        .ok_or_else(|| io::Error::other("The original disc name is unavailable."))?
        .to_string_lossy();
    if disc_id_as(path, &name, cancel)? != disc.id {
        return Err(io::Error::other("These files do not match the missing disc. Select the original disc contents; this entry's progress is kept."));
    }
    disc.path = path.to_owned();
    if index == 0 {
        game.path = path.to_owned();
    }
    if !game.discs.is_empty() {
        game.discs = discs;
    }
    Ok(game)
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
                    || (game.system == found.system
                        && disc_name(&game.path).0.to_lowercase() == found.key)
            })
            .cloned();
        let mut game = existing.unwrap_or_else(|| Game {
            system: found.system,
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
    fn adding_ps2_keeps_existing_ps1_order_and_every_saved_identity() {
        let mut library = Library::default();
        for (id, title, system) in [
            ("aa22", "Final Fantasy VII", System::Ps1),
            ("bb11", "Baldur's Gate", System::Ps2),
            ("aa11", "Crash", System::Ps1),
        ] {
            library.put(Game {
                id: id.into(),
                title: title.into(),
                system,
                path: format!("{id}.iso").into(),
                discs: vec![],
                last_disc: 0,
            });
        }
        assert_eq!(
            library
                .games
                .iter()
                .map(|g| g.id.as_str())
                .collect::<Vec<_>>(),
            vec!["aa11", "aa22", "bb11"]
        );
        assert_eq!(library.games[0].title, "Crash");
        assert_eq!(library.games[1].title, "Final Fantasy VII");
        assert_eq!(library.games[2].system, System::Ps2);
    }
    #[test]
    fn legacy_recent_migrates_once_and_never_repopulates_a_saved_library() {
        let root =
            std::env::temp_dir().join(format!("psx-library-migration-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("fixture.iso");
        fs::write(&path, b"synthetic migration fixture").unwrap();
        let game = Game {
            system: System::Ps1,
            id: "abc123".into(),
            title: "Fixture".into(),
            path,
            discs: vec![],
            last_disc: 0,
        };
        fs::write(root.join("recent.json"), serde_json::to_vec(&game).unwrap()).unwrap();
        assert_eq!(Library::load(&root).unwrap().games.len(), 1);
        Library::default().save(&root).unwrap();
        assert!(Library::load(&root).unwrap().games.is_empty());
        assert!(root.join("recent.json").is_file());
        fs::remove_dir_all(root).unwrap();
    }
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
        bin[start + 6] = 1;
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
        let renamed = root.join("relocated.cue");
        fs::copy(&cue, &renamed).unwrap();
        let relinked = relink(extra.clone(), 0, &renamed, &cancel).unwrap();
        assert_eq!(relinked.id, extra.id);
        assert_eq!(relinked.discs[0].id, extra.discs[0].id);
        assert_eq!(relinked.last_disc, extra.last_disc);
        let mut changed = fs::read(root.join("track.bin")).unwrap();
        changed[0] ^= 1;
        fs::write(root.join("track.bin"), changed).unwrap();
        assert!(relink(extra.clone(), 0, &renamed, &cancel)
            .unwrap_err()
            .to_string()
            .contains("do not match"));
        fs::remove_dir_all(root).unwrap();
    }
}
