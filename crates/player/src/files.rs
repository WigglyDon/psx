use crate::saves::atomic_write;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum System {
    #[default]
    Ps1,
    Ps2,
    Psp,
    Ps3,
    Unknown,
}
impl System {
    pub fn is_ps1(&self) -> bool {
        *self == Self::Ps1
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Ps1 => "PlayStation",
            Self::Ps2 => "PlayStation 2",
            Self::Psp => "PSP",
            Self::Ps3 => "PlayStation 3",
            Self::Unknown => "Choose console",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    #[serde(default, skip_serializing_if = "System::is_ps1")]
    pub system: System,
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
    finish_import_for(root, staging, documents, None)
}

/// Check the selected original disc while it is still staged. A wrong choice
/// must not become an orphaned private import before its identity is refused.
pub fn finish_relink(
    root: &Path,
    staging: &Path,
    documents: &BTreeMap<String, String>,
    game: Game,
    index: usize,
) -> io::Result<Game> {
    finish_import_for(root, staging, documents, Some((game, index)))
}

fn finish_import_for(
    root: &Path,
    staging: &Path,
    documents: &BTreeMap<String, String>,
    recovery: Option<(Game, usize)>,
) -> io::Result<Game> {
    let primary: Vec<_> = documents
        .keys()
        .filter(|name| {
            matches!(
                extension(name).as_str(),
                "cue" | "chd" | "pbp" | "exe" | "iso" | "cso"
            )
        })
        .collect();
    if primary.len() != 1 {
        return Err(io::Error::other("Select one CHD, PBP, ISO, CSO or PS-EXE, or one CUE together with all its BIN tracks. Extract ZIP/7z archives first."));
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
    let recovery = recovery
        .map(|(game, index)| {
            crate::library::relink(game, index, &staging.join(name), &AtomicBool::new(false))
                .map(|game| (game, index))
        })
        .transpose()?;
    let mut hash = Sha256::new();
    for (name, digest) in &selected {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update(digest.as_bytes());
    }
    let id = format!("{:x}", hash.finalize());
    let directory = root.join("games").join(&id);
    fs::create_dir_all(directory.parent().unwrap())?;
    // Include any case-normalized CUE companions and their directory entries
    // in durability before publishing a newly created import directory.
    for name in selected.keys() {
        import_file(&staging.join(name))?.sync_all()?;
    }
    File::open(staging)?.sync_all()?;
    match fs::symlink_metadata(&directory) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.is_symlink() {
                return Err(io::Error::other(
                    "The existing game folder is unavailable. Your files and progress were kept.",
                ));
            }
            let mut missing = Vec::new();
            // Validate the complete existing set before restoring any missing
            // entry. A changed file or redirected path is never overwritten.
            for (name, expected) in &selected {
                let path = directory.join(name);
                match fs::symlink_metadata(&path) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => missing.push(name),
                    Err(error) => return Err(error),
                    Ok(_) => {
                        let mut file = import_file(&path)?;
                        let mut hash = Sha256::new();
                        let mut buffer = [0; 64 * 1024];
                        loop {
                            let count = file.read(&mut buffer)?;
                            if count == 0 {
                                break;
                            }
                            hash.update(&buffer[..count]);
                        }
                        if format!("{:x}", hash.finalize()) != *expected {
                            return Err(io::Error::other("The existing game copy differs from your selection. Existing files and saved progress were kept."));
                        }
                    }
                }
            }
            for name in missing {
                // Both directories are app-private on the same filesystem.
                // Linking publishes a complete synced file and refuses a raced
                // destination instead of replacing it or leaving a partial copy.
                fs::hard_link(staging.join(name), directory.join(name))?;
            }
            File::open(&directory)?.sync_all()?;
            fs::remove_dir_all(staging)?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            fs::rename(staging, &directory)?;
            File::open(directory.parent().unwrap())?.sync_all()?;
        }
        Err(error) => return Err(error),
    }
    if let Some((mut game, index)) = recovery {
        let path = directory.join(name);
        if index == 0 {
            game.path = path.clone();
        }
        if !game.discs.is_empty() {
            game.discs[index].path = path;
        }
        return Ok(game);
    }
    Ok(Game {
        system: crate::content::inspect(&directory.join(name))
            .map(|i| i.0)
            .unwrap_or(System::Unknown),
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

fn import_file(path: &Path) -> io::Result<File> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.is_symlink() {
        return Err(io::Error::other("A selected game file is redirected or unavailable. Existing files and progress were kept."));
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::other(
            "A selected game file changed during import.",
        ));
    }
    Ok(file)
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
    struct ImportFixture(PathBuf);
    impl ImportFixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "psx-import-recovery-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&root).unwrap();
            Self(root)
        }
        fn stage(&self, name: &str, bytes: &[u8]) -> (PathBuf, BTreeMap<String, String>) {
            let stage = self.0.join("importing");
            let sha = copy_document(bytes, &stage, name).unwrap();
            (stage, BTreeMap::from([(name.to_owned(), sha)]))
        }
    }
    impl Drop for ImportFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn psp_document() -> Vec<u8> {
        // Structural PSP PBP stand-in, never a proprietary game fixture.
        let mut bytes = vec![0u8; 100];
        bytes[..4].copy_from_slice(b"\0PBP");
        for offset in bytes[8..40].chunks_exact_mut(4) {
            offset.copy_from_slice(&40u32.to_le_bytes());
        }
        bytes[36..40].copy_from_slice(&64u32.to_le_bytes());
        bytes[40..44].copy_from_slice(b"~PSP");
        bytes
    }
    #[test]
    fn recovery_import_restores_a_missing_file_inside_an_existing_game_folder() {
        let fixture = ImportFixture::new();
        let bytes = psp_document();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        let original = finish_import(&fixture.0, &stage, &docs).unwrap();
        assert_eq!(original.system, System::Psp);
        fs::rename(&original.path, fixture.0.join("retained-original.pbp")).unwrap();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        let recovered = finish_import(&fixture.0, &stage, &docs).unwrap();
        assert_eq!(recovered.id, original.id);
        assert_eq!(fs::read(&recovered.path).unwrap(), bytes);
        assert_eq!(
            fs::read(fixture.0.join("retained-original.pbp")).unwrap(),
            bytes
        );
    }
    #[test]
    fn recovery_import_never_reuses_a_different_existing_payload() {
        let fixture = ImportFixture::new();
        let bytes = psp_document();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        let original = finish_import(&fixture.0, &stage, &docs).unwrap();
        fs::write(&original.path, b"different existing data").unwrap();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        assert!(finish_import(&fixture.0, &stage, &docs).is_err());
        assert_eq!(
            fs::read(&original.path).unwrap(),
            b"different existing data"
        );
        assert_eq!(fs::read(stage.join("Game.pbp")).unwrap(), bytes);
    }
    #[test]
    fn relink_refuses_the_wrong_game_before_creating_a_private_copy() {
        let fixture = ImportFixture::new();
        let bytes = psp_document();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        let original = finish_import(&fixture.0, &stage, &docs).unwrap();
        let mut wrong = bytes.clone();
        wrong[99] = 1; // Still a structurally valid PSP document, different game identity.
        let (stage, docs) = fixture.stage("Different.pbp", &wrong);
        assert!(
            finish_relink(&fixture.0, &stage, &docs, original.clone(), 0)
                .unwrap_err()
                .to_string()
                .contains("do not match")
        );
        assert_eq!(fs::read_dir(fixture.0.join("games")).unwrap().count(), 1);
        assert_eq!(fs::read(&original.path).unwrap(), bytes);
        assert_eq!(fs::read(stage.join("Different.pbp")).unwrap(), wrong);
    }
    #[test]
    fn renamed_relink_retains_entry_checkpoint_ordinary_save_and_custom_controller() {
        use crate::{controller_profiles, input::ControllerSettings, session::GameId};
        let fixture = ImportFixture::new();
        let bytes = psp_document();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        let mut original = finish_import(&fixture.0, &stage, &docs).unwrap();
        original.title = "My title".into();
        let id = GameId::parse(&original.id).unwrap();
        let stick = fixture.0.join("psp").join(id.as_str()).join("memstick");
        fs::create_dir_all(&stick).unwrap();
        fs::write(stick.join("save.bin"), b"ordinary progress").unwrap();
        let identity = crate::psp_saves::identity(&stick).unwrap();
        crate::resume::save(&fixture.0, &original, true, &identity, b"machine-state").unwrap();
        let state = fixture
            .0
            .join("resume")
            .join(format!("{}.state", original.id));
        let checkpoint = fs::read(&state).unwrap();
        let controls = ControllerSettings::default().with_psp();
        let mut profiles = controller_profiles::Settings::default();
        profiles
            .set_custom(&fixture.0, &id, true, &controls)
            .unwrap();
        let profile_path = fixture.0.join("controller-profiles.json");
        let profile = fs::read(&profile_path).unwrap();
        fs::rename(&original.path, fixture.0.join("retained.pbp")).unwrap();
        let (stage, docs) = fixture.stage("Renamed.pbp", &bytes);
        let recovered = finish_relink(&fixture.0, &stage, &docs, original.clone(), 0).unwrap();
        assert_eq!(recovered.id, original.id);
        assert_eq!(recovered.title, original.title);
        assert_eq!(recovered.system, System::Psp);
        assert_eq!(recovered.last_disc, original.last_disc);
        assert_eq!(fs::read(&recovered.path).unwrap(), bytes);
        assert_eq!(recovered.path.file_name().unwrap(), "Renamed.pbp");
        assert_eq!(fs::read(&state).unwrap(), checkpoint);
        assert_eq!(fs::read(&profile_path).unwrap(), profile);
        assert_eq!(
            fs::read(stick.join("save.bin")).unwrap(),
            b"ordinary progress"
        );
        assert!(controller_profiles::Settings::load(&fixture.0)
            .unwrap()
            .custom(&id));
        let loaded = crate::resume::load(&fixture.0, &recovered, &identity)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.bytes, b"machine-state");
        assert!(loaded.metadata.paused);
    }
    #[test]
    fn missing_cue_track_is_repaired_but_conflicting_payload_prevents_partial_repair() {
        let fixture = ImportFixture::new();
        let cue = b"FILE \"track.bin\" BINARY\n TRACK 01 MODE2/2352\n";
        let stage_tracks = || {
            let (stage, mut docs) = fixture.stage("Game.cue", cue);
            docs.insert(
                "TRACK.BIN".into(),
                copy_document(&b"synthetic track"[..], &stage, "TRACK.BIN").unwrap(),
            );
            (stage, docs)
        };
        let (stage, docs) = stage_tracks();
        let original = finish_import(&fixture.0, &stage, &docs).unwrap();
        let directory = original.path.parent().unwrap();
        fs::remove_file(directory.join("track.bin")).unwrap();
        let (stage, docs) = stage_tracks();
        let repaired = finish_import(&fixture.0, &stage, &docs).unwrap();
        assert_eq!(repaired.id, original.id);
        assert_eq!(
            fs::read(directory.join("track.bin")).unwrap(),
            b"synthetic track"
        );
        fs::remove_file(&original.path).unwrap();
        fs::write(directory.join("track.bin"), b"keep different track").unwrap();
        let (stage, docs) = stage_tracks();
        assert!(finish_import(&fixture.0, &stage, &docs).is_err());
        assert!(!original.path.exists());
        assert_eq!(
            fs::read(directory.join("track.bin")).unwrap(),
            b"keep different track"
        );
        assert!(stage.join("Game.cue").is_file());
    }
    #[cfg(unix)]
    #[test]
    fn redirected_existing_imports_are_refused_without_touching_their_targets() {
        let fixture = ImportFixture::new();
        let bytes = psp_document();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        let original = finish_import(&fixture.0, &stage, &docs).unwrap();
        let retained = fixture.0.join("retained.pbp");
        fs::rename(&original.path, &retained).unwrap();
        std::os::unix::fs::symlink(&retained, &original.path).unwrap();
        let (stage, docs) = fixture.stage("Game.pbp", &bytes);
        assert!(finish_import(&fixture.0, &stage, &docs).is_err());
        assert_eq!(fs::read(&retained).unwrap(), bytes);
        assert!(fs::symlink_metadata(&original.path).unwrap().is_symlink());
        assert!(stage.join("Game.pbp").is_file());
    }
    #[test]
    fn legacy_records_keep_their_ps1_identity_and_serialized_fields() {
        let old =
            r#"{"id":"abc123","title":"Existing game","path":"game.cue","discs":[],"last_disc":0}"#;
        let game: Game = serde_json::from_str(old).unwrap();
        assert_eq!(game.system, System::Ps1);
        assert_eq!(game.id, "abc123");
        assert_eq!(
            serde_json::to_value(&game).unwrap(),
            serde_json::from_str::<serde_json::Value>(old).unwrap()
        );
        let mut ps2 = game.clone();
        ps2.system = System::Ps2;
        let restored: Game = serde_json::from_slice(&serde_json::to_vec(&ps2).unwrap()).unwrap();
        assert_eq!(restored.system, System::Ps2);
        assert_eq!(restored.id, game.id);
    }
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
