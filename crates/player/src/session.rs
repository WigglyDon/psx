//! Durable launch identities and temporary ownership. No platform handles enter
//! game identities or saved machines; this milestone assigns one live runtime.
use std::{
    collections::VecDeque,
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

/// Record only a platform-confirmed icon. A callback's ordinary files path
/// follows the same root policy as the player, including isolated test builds.
pub fn record_shortcut_confirmation(normal: &Path, id: &GameId) -> io::Result<()> {
    let root = crate::storage_root(normal)?;
    crate::saves::atomic_write(
        &root
            .join("shortcuts")
            .join(format!("{}.confirmed", id.as_str())),
        b"pinned\n",
    )
}

/// Ordered foreground requests. Coalesce repeated delivery, without losing a
/// later A in an A/B/A switch. Saturation is an explicit busy result.
#[derive(Default)]
pub struct LaunchQueue(VecDeque<Launch>);
impl LaunchQueue {
    pub fn push(&mut self, launch: Launch) -> Result<(), &'static str> {
        if self.0.back() == Some(&launch) {
            return Ok(());
        }
        if self.0.len() >= 8 {
            return Err("Several games are already opening. Please try again shortly.");
        }
        self.0.push_back(launch);
        Ok(())
    }
    pub fn pop(&mut self) -> Option<Launch> {
        self.0.pop_front()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
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
        try_writer_lock(&file).map_err(|e| {
            if e.kind() == io::ErrorKind::WouldBlock {
                io::Error::new(e.kind(), "This game's saved session already has a writer.")
            } else {
                io::Error::new(
                    e.kind(),
                    format!("This game's saved session could not be locked: {e}"),
                )
            }
        })?;
        Ok(Self { _file: file })
    }
}

// std::fs::File::try_lock is unsupported on Android. Use the same supported
// descriptor-owned flock on Unix hosts so the real adapter is exercised by
// competing-writer and process-death fixtures. Closing the file releases it.
#[cfg(unix)]
fn try_writer_lock(file: &File) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    // SAFETY: file owns a live descriptor for the duration of this call.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(unix))]
fn try_writer_lock(file: &File) -> io::Result<()> {
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => io::ErrorKind::WouldBlock.into(),
        fs::TryLockError::Error(error) => error,
    })
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
        if self.state == State::Unavailable {
            return;
        }
        self.state = if can_run {
            State::ForegroundRunning
        } else {
            State::PausedResident
        };
    }
    pub fn ensure_available(&self) -> Result<(), &'static str> {
        if self.state == State::Unavailable {
            Err("This session could not resume. Restart game to use its current card. The old checkpoint is kept until you choose Restart.")
        } else {
            Ok(())
        }
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
    #[cfg(not(feature = "graphics-observer"))]
    #[test]
    fn shortcut_confirmation_uses_player_storage_and_never_falls_back() {
        let parent = std::env::temp_dir().join(format!(
            "psx-shortcut-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let normal = parent.join("files");
        fs::create_dir_all(&normal).unwrap();
        fs::write(normal.join("owner-save"), b"normal progress").unwrap();
        let id = GameId::parse("abc123").unwrap();
        #[cfg(feature = "player-acceptance")]
        {
            assert!(record_shortcut_confirmation(&normal, &id).is_err());
            assert!(!normal.join("shortcuts").exists());
            let scratch = parent.join(crate::acceptance::DIRECTORY);
            fs::create_dir(&scratch).unwrap();
            fs::write(scratch.join("prepared-v1"), crate::acceptance::MARKER).unwrap();
        }
        let before = fs::metadata(&normal).unwrap();
        record_shortcut_confirmation(&normal, &id).unwrap();
        let root = crate::storage_root(&normal).unwrap();
        assert_eq!(
            fs::read(root.join("shortcuts/abc123.confirmed")).unwrap(),
            b"pinned\n"
        );
        assert_eq!(
            fs::read(normal.join("owner-save")).unwrap(),
            b"normal progress"
        );
        #[cfg(feature = "player-acceptance")]
        {
            assert_ne!(root, normal);
            assert!(!normal.join("shortcuts").exists());
            let after = fs::metadata(&normal).unwrap();
            assert_eq!(before.modified().unwrap(), after.modified().unwrap());
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                assert_eq!(
                    (before.dev(), before.ino(), before.mtime_nsec()),
                    (after.dev(), after.ino(), after.mtime_nsec())
                );
            }
        }
        #[cfg(not(feature = "player-acceptance"))]
        {
            let _ = before;
            assert_eq!(root, normal);
        }
        fs::remove_dir_all(parent).unwrap();
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
        let root = std::env::temp_dir().join(format!(
            "psx-session-death-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
    fn repeated_launches_coalesce_and_different_targets_keep_their_order() {
        let a = Launch::OpenGame(GameId::parse("abc").unwrap());
        let b = Launch::OpenGame(GameId::parse("def").unwrap());
        let mut queue = LaunchQueue::default();
        for request in [&a, &a, &b, &b, &a] {
            queue.push(request.clone()).unwrap();
        }
        assert_eq!(queue.pop(), Some(a.clone()));
        assert_eq!(queue.pop(), Some(b));
        assert_eq!(queue.pop(), Some(a));
        assert!(queue.is_empty());
        for i in 0..8 {
            queue
                .push(Launch::OpenGame(GameId::parse(&format!("{i:x}")).unwrap()))
                .unwrap();
        }
        assert!(queue
            .push(Launch::OpenGame(GameId::parse("9").unwrap()))
            .is_err());
    }
    #[test]
    fn saved_sessions_outlive_runtime_assignment_and_competing_writers_are_refused() {
        let root = std::env::temp_dir().join(format!(
            "psx-session-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
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
        assert!(first.ensure_available().is_ok());
        first.state = State::Unavailable;
        first.update(true);
        assert_eq!(first.state, State::Unavailable);
        assert!(first.ensure_available().is_err());
        assert!(Runtime::restoring(&root, a.clone()).is_err());
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
