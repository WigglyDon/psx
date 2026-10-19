use crate::saves::atomic_write;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub id: String,
    pub title: String,
    pub path: PathBuf,
    #[serde(default)]
    pub discs: Vec<Disc>,
    #[serde(default)]
    pub last_disc: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Disc {
    pub id: String,
    pub label: String,
    pub number: u32,
    pub path: PathBuf,
}

impl Game {
    pub fn disc_list(&self) -> Vec<Disc> {
        if self.discs.is_empty() {
            vec![Disc {
                id: self.id.clone(),
                label: "Disc 1".into(),
                number: 1,
                path: self.path.clone(),
            }]
        } else {
            self.discs.clone()
        }
    }

    pub fn launch_path(&self, root: &Path) -> io::Result<PathBuf> {
        if self.discs.len() < 2 {
            return Ok(self.path.clone());
        }
        let directory = root.join("playlists");
        fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{}.m3u", self.id));
        let mut content = String::new();
        for disc in &self.discs {
            let entry = disc
                .path
                .to_str()
                .ok_or_else(|| io::Error::other("This disc path is not valid Unicode."))?;
            if entry.contains(['\n', '\r']) {
                return Err(io::Error::other(
                    "Rename the disc file to remove its line break.",
                ));
            }
            content.push_str(entry);
            content.push('\n');
        }
        atomic_write(&path, content.as_bytes())?;
        Ok(path)
    }
}

pub fn safe_name(name: &str) -> io::Result<&str> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return Err(io::Error::other(
            "This file name is not supported. Rename the file and select it again.",
        ));
    }
    Ok(name)
}

pub fn copy_document(mut input: impl Read, staging: &Path, name: &str) -> io::Result<String> {
    safe_name(name)?;
    fs::create_dir_all(staging)?;
    let mut output = File::create(staging.join(name))?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        output.write_all(&buffer[..count])?;
        hash.update(&buffer[..count]);
    }
    output.sync_all()?;
    Ok(format!("{:x}", hash.finalize()))
}

pub fn cue_tracks(cue: &str) -> io::Result<Vec<String>> {
    let mut result = Vec::new();
    for line in cue.lines() {
        let line = line.trim();
        let Some((keyword, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !keyword.eq_ignore_ascii_case("FILE") {
            continue;
        }
        let rest = rest.trim_start();
        let name = if let Some(quoted) = rest.strip_prefix('"') {
            quoted.split_once('"').map(|(name, _)| name)
        } else {
            rest.split_whitespace().next()
        };
        let name = name.ok_or_else(|| io::Error::other("The CUE has an invalid FILE entry."))?;
        safe_name(name)?;
        result.push(name.to_owned());
    }
    if result.is_empty() {
        return Err(io::Error::other("This CUE does not name any disc tracks."));
    }
    Ok(result)
}

pub fn finish_import(
    root: &Path,
    staging: &Path,
    documents: &BTreeMap<String, String>,
) -> io::Result<Game> {
    let primary: Vec<_> = documents
        .keys()
        .filter(|name| {
            matches!(
                extension(name).as_str(),
                "cue" | "chd" | "pbp" | "exe" | "iso"
            )
        })
        .collect();
    if primary.len() != 1 {
        return Err(io::Error::other("Select one CHD, PBP, ISO or PS-EXE, or one CUE together with all its BIN tracks. Extract ZIP/7z archives first."));
    }
    let name = primary[0];
    let mut selected = BTreeMap::new();
    selected.insert(name.clone(), documents[name].clone());
    if extension(name) == "cue" {
        let path = staging.join(name);
        if fs::metadata(&path)?.len() > 1024 * 1024 {
            return Err(io::Error::other("The CUE file is too large."));
        }
        let text = fs::read_to_string(&path)?;
        for track in cue_tracks(&text)? {
            let actual=documents.keys().find(|key| key.eq_ignore_ascii_case(&track))
                .ok_or_else(||io::Error::other(format!("Missing track: {track}. Select the CUE and every named BIN track together.")))?;
            // Preserve the exact name requested by the core on case-sensitive storage.
            if actual != &track {
                fs::copy(staging.join(actual), staging.join(&track))?;
            }
            selected.insert(track, documents[actual].clone());
        }
    }
    let mut hash = Sha256::new();
    for (name, digest) in &selected {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(digest.as_bytes());
    }
    let id = format!("{:x}", hash.finalize());
    let directory = root.join("games").join(&id);
    fs::create_dir_all(directory.parent().unwrap())?;
    if directory.exists() {
        fs::remove_dir_all(staging)?;
    } else {
        fs::rename(staging, &directory)?;
        File::open(directory.parent().unwrap())?.sync_all()?;
    }
    Ok(Game {
        id,
        title: Path::new(name)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
        path: directory.join(name),
        discs: Vec::new(),
        last_disc: 0,
    })
}

pub fn extension(name: &str) -> String {
    Path::new(name)
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
}

/// Read-only migration for installations before the persistent library.
pub(crate) fn legacy_recent(root: &Path) -> Option<Game> {
    let game: Game = serde_json::from_slice(&fs::read(root.join("recent.json")).ok()?).ok()?;
    if game.path.is_file() {
        Some(game)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn companion_paths_cannot_escape_granted_import() {
        assert_eq!(
            cue_tracks("FILE \"Demo (Track 1).bin\" BINARY\n TRACK 01 MODE2/2352").unwrap(),
            ["Demo (Track 1).bin"]
        );
        for cue in [
            "FILE \"../secret\" BINARY",
            "FILE \"/secret\" BINARY",
            "FILE \"..\\secret\" BINARY",
            "TRACK 01 MODE2/2352",
        ] {
            assert!(cue_tracks(cue).is_err());
        }
    }
    #[test]
    fn missing_companion_is_actionable_and_never_committed() {
        let root = std::env::temp_dir().join(format!("psx-import-{}", std::process::id()));
        let stage = root.join("staging");
        let name = "Game.cue";
        let digest = copy_document(&b"FILE \"track.bin\" BINARY"[..], &stage, name).unwrap();
        let mut docs = BTreeMap::from([(name.to_owned(), digest)]);
        assert!(finish_import(&root, &stage, &docs)
            .unwrap_err()
            .to_string()
            .contains("Missing track"));
        let digest = copy_document(&b"private test stand-in"[..], &stage, "track.bin").unwrap();
        docs.insert("track.bin".into(), digest);
        let game = finish_import(&root, &stage, &docs).unwrap();
        assert!(game.path.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
