//! Durable launch identities and temporary ownership. No platform handles enter
//! game identities or saved machines; this milestone assigns one live runtime.
use std::{
    fs::{self, File, OpenOptions},
    io,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct GameId(String);

impl GameId {
    pub fn parse(value: &str) -> Result<Self, &'static str> {
        // Existing imports use hexadecimal digests. Keep shorter legacy IDs
        // valid while rejecting paths, encoded separators and unbounded input.
        if value.is_empty() || value.len() > 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("This game link has an invalid identity.");
        }
        Ok(Self(value.into()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Launch {
    OpenGame(GameId),
}
impl Launch {
    pub fn from_url(value: &str) -> Result<Self, &'static str> {
        let id = value
            .strip_prefix("machinerhythm-psx://play/")
            .ok_or("This game link is not supported.")?;
        GameId::parse(id).map(Self::OpenGame)
    }
    pub fn url(&self) -> String {
        match self {
            Self::OpenGame(id) => format!("machinerhythm-psx://play/{}", id.as_str()),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SavedSession {
    pub game: GameId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    NotLoaded,
    Restoring,
    ForegroundRunning,
    PausedResident,
    CheckpointedUnloaded,
    Unavailable,
}

/// An advisory OS lock protects one game's cards and checkpoints even if a
/// future worker is in another process. Atomic replacement alone is not a lock.
pub struct Writer {
    _file: File,
}
impl Writer {
    pub fn acquire(root: &Path, id: &GameId) -> io::Result<Self> {
        let directory = root.join("session-locks");
        fs::create_dir_all(&directory)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join(format!("{}.lock", id.as_str())))?;
        file.try_lock().map_err(|e| {
            io::Error::other(format!(
                "This game's saved session already has a writer: {e}"
            ))
        })?;
        Ok(Self { _file: file })
    }
}

pub struct Runtime {
    pub saved: SavedSession,
    token: u64,
    pub state: State,
    // Keep this guard until donor teardown and all checkpoint writes finish.
    _writer: Writer,
}
static NEXT_RUNTIME: AtomicU64 = AtomicU64::new(1);

impl Runtime {
    pub fn restoring(root: &Path, saved: SavedSession) -> io::Result<Self> {
        let writer = Writer::acquire(root, &saved.game)?;
        Ok(Self {
            saved,
            token: NEXT_RUNTIME.fetch_add(1, Ordering::Relaxed),
            state: State::Restoring,
            _writer: writer,
        })
    }
    pub fn token(&self) -> u64 {
        self.token
    }
    pub fn update(&mut self, can_run: bool) {
        self.state = if can_run {
            State::ForegroundRunning
        } else {
            State::PausedResident
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    #[ignore = "subprocess fixture invoked by writer_lock_is_released_after_worker_death"]
    fn writer_child() {
        let Some(root) = std::env::var_os("PSX_SESSION_LOCK_FIXTURE") else {
            return;
        };
        let root = Path::new(&root);
        let _writer = Writer::acquire(root, &GameId::parse("abc123").unwrap()).unwrap();
        fs::write(root.join("ready"), b"ready").unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
    }
    #[test]
    fn writer_lock_is_released_after_worker_death() {
        let root = std::env::temp_dir().join(format!("psx-session-death-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let _ = fs::remove_file(root.join("ready"));
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args(["session::tests::writer_child", "--exact", "--ignored"])
                .env("PSX_SESSION_LOCK_FIXTURE", &root)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !root.join("ready").exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "Worker did not acquire the fixture lock"
            );
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "Worker exited before acquiring its lock"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let id = GameId::parse("abc123").unwrap();
        assert!(Writer::acquire(&root, &id).is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        let writer = Writer::acquire(&root, &id).unwrap();
        drop(writer);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn direct_links_accept_only_the_existing_opaque_identity_contract() {
        let id = GameId::parse("abc123").unwrap();
        let launch = Launch::OpenGame(id);
        assert_eq!(Launch::from_url(&launch.url()).unwrap(), launch);
        for bad in [
            "",
            "../abc",
            "%2fabc",
            "abc?core=x",
            "abc#state",
            "abc/def",
            "abc\n",
        ] {
            assert!(GameId::parse(bad).is_err());
            assert!(Launch::from_url(&format!("machinerhythm-psx://play/{bad}")).is_err());
        }
        assert!(GameId::parse(&"a".repeat(65)).is_err());
        assert!(Launch::from_url("https://play/abc").is_err());
    }
    #[test]
    fn saved_sessions_outlive_runtime_assignment_and_competing_writers_are_refused() {
        let root = std::env::temp_dir().join(format!("psx-session-{}", std::process::id()));
        let a = SavedSession {
            game: GameId::parse("abc123").unwrap(),
        };
        let b = SavedSession {
            game: GameId::parse("def456").unwrap(),
        };
        let mut first = Runtime::restoring(&root, a.clone()).unwrap();
        assert!(Runtime::restoring(&root, a.clone()).is_err());
        first.update(false);
        assert_eq!(first.state, State::PausedResident);
        let first_token = first.token();
        drop(first);
        let mut second = Runtime::restoring(&root, b).unwrap();
        second.update(true);
        assert_eq!(second.state, State::ForegroundRunning);
        drop(second);
        let restored = Runtime::restoring(&root, a.clone()).unwrap();
        assert_eq!(restored.saved, a);
        assert_ne!(restored.token(), first_token);
        drop(restored);
        fs::remove_dir_all(root).unwrap();
    }
}
