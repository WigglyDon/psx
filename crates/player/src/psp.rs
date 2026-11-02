//! PSP owner adapter. Durable identity and launch policy stay in the player.
use crate::{
    ps2::{Output, Pixels},
    psp_saves,
    retro::{self, HostConfig},
    session::GameId,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const MAX_STATE_SIZE: usize = 128 * 1024 * 1024;
pub const UPSTREAM: &str = "fc9ca4d1f78a5d60ae03f3311c6b8ab49273f6a8";
type Result<T> = std::result::Result<T, String>;

pub fn revision() -> String {
    format!(
        "{UPSTREAM}+host:{:x}",
        Sha256::digest(
            concat!(
                include_str!("../../../patches/ppsspp/host-api.inc"),
                include_str!("../../../patches/ppsspp/compose.rs"),
                include_str!("../../../patches/ppsspp/host.cmake")
            )
            .as_bytes()
        )
    )
}
pub fn prepare(root: &Path, id: &GameId) -> Result<PathBuf> {
    let path = root.join("psp").join(id.as_str());
    // Canonical per-entry writer must be held by the caller before preparation.
    for part in [root.join("psp"), path.clone(), path.join("memstick")] {
        match fs::symlink_metadata(&part) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => (),
            Ok(_) => {
                return Err("PSP saved progress is redirected. Its files were preserved.".into())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir_all(&part).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(path)
}

pub fn install_resource(root: &Path, name: &str, expected: &str, bytes: &[u8]) -> Result<()> {
    use std::path::Component;
    let relative = Path::new(name);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || bytes.len() > 16 * 1024 * 1024
        || expected != format!("{:x}", Sha256::digest(bytes))
    {
        return Err("A bundled PSP resource is invalid.".into());
    }
    let base = root.join("system").join("PPSSPP");
    let target = base.join(relative);
    let mut parent = root.to_owned();
    for component in target
        .parent()
        .ok_or("Missing PSP resource parent")?
        .strip_prefix(root)
        .map_err(|e| e.to_string())?
        .components()
    {
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(m) if m.is_dir() && !m.file_type().is_symlink() => (),
            Ok(_) => return Err("PSP support storage is redirected.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&parent).map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    if let Ok(m) = fs::symlink_metadata(&target) {
        if !m.is_file() || m.file_type().is_symlink() {
            return Err("A PSP support file is redirected.".into());
        }
        if m.len() <= 16 * 1024 * 1024
            && fs::read(&target).is_ok_and(|b| format!("{:x}", Sha256::digest(b)) == expected)
        {
            return Ok(());
        }
    }
    crate::saves::atomic_write(&target, bytes).map_err(|e| e.to_string())
}

pub struct Core {
    inner: retro::Core,
    ready_fn: unsafe extern "C" fn() -> i32,
    pause_fn: unsafe extern "C" fn(i32) -> i32,
    flush_fn: unsafe extern "C" fn() -> i32,
    memstick: PathBuf,
    paused: Option<bool>,
    picture: Option<Pixels>,
}
pub(crate) struct Captured {
    pub state: Vec<u8>,
    pub files: psp_saves::Pending,
    pub captured_in: Duration,
    pub cards_in: Duration,
}
impl Core {
    /// # Safety
    /// The pinned core and supplied GLES context stay on this sole owner until
    /// all native workers, callbacks and library objects have retired.
    pub unsafe fn load(library: &Path, game: &Path, config: HostConfig<'_>) -> Result<Self> {
        let memstick = config.save.to_owned();
        let inner = retro::Core::load_psp(library, game, config)?;
        let pause_fn: unsafe extern "C" fn(i32) -> i32 = inner.extension(b"psx_psp_pause\0")?;
        let core = Self {
            ready_fn: inner.extension(b"psx_psp_ready\0")?,
            flush_fn: inner.extension(b"psx_psp_flush\0")?,
            inner,
            pause_fn,
            memstick,
            paused: None,
            picture: None,
        };
        // Hold the first guest frame. Boot may finish during subsequent polls;
        // no saved-data validation or restore is raced against guest execution.
        (core.pause_fn)(1);
        Ok(core)
    }
    pub fn ready(&self) -> bool {
        unsafe { (self.ready_fn)() != 0 }
    }
    pub fn pause(&mut self, paused: bool) -> Result<()> {
        if self.paused != Some(paused) {
            if unsafe { (self.pause_fn)(i32::from(paused)) } == 0 {
                return Err(
                    "The PSP engine did not acknowledge pause. Saved progress is kept.".into(),
                );
            }
            self.paused = Some(paused);
        }
        Ok(())
    }
    pub fn run(&mut self) {
        let before = self.inner.state().hardware_frames;
        self.inner.run();
        if self.inner.state().hardware_frames != before {
            self.picture = None;
        }
    }
    pub fn input(&mut self, buttons: u16, axes: [i16; 4], fast: bool) {
        let state = self.inner.state_mut();
        state.buttons = buttons;
        state.axes = [axes[0], axes[1], 0, 0];
        state.fast_forward = fast;
    }
    pub fn flush(&mut self) -> Result<()> {
        self.pause(true)?;
        if unsafe { (self.flush_fn)() } == 0 {
            return Err("PSP saved-data I/O did not settle.".into());
        }
        Ok(())
    }
    pub fn card_identity(&mut self) -> Result<Vec<u8>> {
        self.flush()?;
        psp_saves::identity(&self.memstick).map_err(|e| e.to_string())
    }
    pub(crate) fn capture(&mut self) -> Result<Captured> {
        let before = Instant::now();
        self.flush()?;
        let state = self.inner.checkpoint()?;
        let captured_in = before.elapsed();
        // The composed adapter preserves pause through serialization.
        let before = Instant::now();
        let files = psp_saves::capture(&self.memstick).map_err(|e| e.to_string())?;
        Ok(Captured {
            state,
            files,
            captured_in,
            cards_in: before.elapsed(),
        })
    }
    pub fn restore(&mut self, state: &[u8]) -> Result<()> {
        if !self.ready() {
            return Err("PSP restoration requires completed engine startup.".into());
        }
        self.flush()?;
        self.inner.restore_checkpoint(state)
    }
    pub fn restore_picture(&mut self, picture: Pixels) -> Result<()> {
        if picture.width == 0
            || picture.height == 0
            || picture.width > 1024
            || picture.height > 1024
            || picture.rgba.len() != picture.width as usize * picture.height as usize * 4
        {
            return Err("The PSP checkpoint picture is invalid. Saved progress is kept.".into());
        }
        self.picture = Some(picture);
        Ok(())
    }
    pub fn output(&self) -> Output {
        let s = self.inner.state();
        Output {
            pixels: self.picture.clone(),
            av: s.av,
            hw: s.hw,
            hardware_frames: s.hardware_frames,
            software_frames: s.software_frames,
            frame_width: self.picture.as_ref().map_or(s.frame_width, |p| p.width),
            frame_height: self.picture.as_ref().map_or(s.frame_height, |p| p.height),
            shutdown: s.shutdown,
            last_message: s.last_message.clone(),
            ..Output::default()
        }
    }
    pub fn destroy_context(&mut self) {
        self.inner.destroy_context();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        files::{Game, System},
        resume,
    };
    #[test]
    fn actual_resume_loader_refuses_a_stale_psp_save_without_replacing_new_progress() {
        let root = std::env::temp_dir().join(format!("psx-psp-resume-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let game = Game {
            system: System::Psp,
            id: "abcde".into(),
            title: "PSP fixture".into(),
            path: root.join("game.iso"),
            discs: vec![],
            last_disc: 0,
        };
        let psp = prepare(&root, &GameId::parse(&game.id).unwrap()).unwrap();
        let stick = psp.join("memstick");
        let save = stick.join("saved.dat");
        fs::write(&save, b"old progress").unwrap();
        let identity = psp_saves::identity(&stick).unwrap();
        resume::save(&root, &game, true, &identity, b"machine-state").unwrap();
        assert!(resume::load(&root, &game, &identity).unwrap().is_some());
        let state_path = root.join("resume").join(format!("{}.state", game.id));
        let checkpoint = fs::read(&state_path).unwrap();
        fs::write(&save, b"new progress").unwrap();
        let newer = psp_saves::identity(&stick).unwrap();
        assert!(resume::load(&root, &game, &newer)
            .err()
            .expect("stale state must fail")
            .to_string()
            .contains("changed"));
        assert_eq!(fs::read(&save).unwrap(), b"new progress");
        assert_eq!(fs::read(state_path).unwrap(), checkpoint);
        fs::remove_dir_all(root).unwrap();
    }
}
