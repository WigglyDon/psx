//! The two real engine bindings. Runtime count is host policy, not game identity.
use crate::{
    files::System,
    ps2,
    retro::{self, HostConfig},
    saves::MemoryCard,
    settings::Speed,
};
use std::path::{Path, PathBuf};
use std::task::Waker;
type Result<T> = std::result::Result<T, String>;
pub enum Core {
    Ps1(retro::Core),
    Ps2(Box<ps2::Core>, PathBuf),
}
/// Frozen state awaiting packaging, independent of a live engine assignment.
pub enum Checkpoint {
    Ps1(Vec<u8>),
    Ps2(ps2::Checkpoint),
}
impl Checkpoint {
    pub fn into_bytes(self) -> Result<Vec<u8>> {
        match self {
            Self::Ps1(bytes) => Ok(bytes),
            Self::Ps2(captured) => captured.into_bytes(),
        }
    }
}
impl Core {
    /// Threaded engines own their emulation clock; the frontend services a
    /// published frame immediately instead of imposing a second frame clock.
    pub fn owns_clock(&self) -> bool {
        matches!(self, Self::Ps2(..))
    }
    pub fn set_frame_waker(&self, waker: Waker) {
        if let Self::Ps2(core, _) = self {
            core.set_frame_waker(waker);
        }
    }
    pub fn frame_pending(&self) -> bool {
        matches!(self, Self::Ps2(core, _) if core.frame_pending())
    }
    pub fn set_speed(&mut self, speed: Speed) -> Result<Speed> {
        if speed.is_paused() {
            return Err("Zero time must pause the runtime.".into());
        }
        match self {
            Self::Ps1(_) => Ok(speed),
            Self::Ps2(core, _) => core.set_speed(speed),
        }
    }
    /// # Safety
    /// The current graphics context and pinned library must satisfy the selected
    /// engine's load contract, and remain valid through its final destruction.
    pub unsafe fn load(
        system: System,
        library: &Path,
        game: &Path,
        config: HostConfig<'_>,
        gles: [u32; 2],
    ) -> Result<Self> {
        match system {
            System::Ps1 => retro::Core::load(library, game, config).map(Self::Ps1),
            System::Ps2 => {
                let root = config.system.join("pcsx2/memcards");
                ps2::Core::load(library, game, config, gles)
                    .map(|core| Self::Ps2(Box::new(core), root))
            }
            _ => Err("Choose a supported console for this game.".into()),
        }
    }
    pub fn state(&self) -> ps2::Output {
        match self {
            Self::Ps2(core, _) => core.output(),
            Self::Ps1(core) => {
                let s = core.state();
                ps2::Output {
                    metrics: None,
                    render_costs: None,
                    av: s.av,
                    hw: s.hw,
                    hardware_frames: s.hardware_frames,
                    software_frames: s.software_frames,
                    frame_width: s.frame_width,
                    frame_height: s.frame_height,
                    shutdown: s.shutdown,
                    last_message: s.last_message.clone(),
                }
            }
        }
    }
    pub fn input(&mut self, buttons: u16, axes: [i16; 4], fast: bool) {
        match self {
            Self::Ps1(core) => {
                core.state_mut().buttons = buttons;
                core.state_mut().fast_forward = fast;
            }
            Self::Ps2(core, _) => core.input(buttons, axes, fast),
        }
    }
    pub fn pause(&mut self, paused: bool) -> Result<()> {
        match self {
            Self::Ps1(_) => Ok(()),
            Self::Ps2(core, _) => core.pause(paused),
        }
    }
    pub fn ready(&self) -> bool {
        match self {
            Self::Ps1(_) => true,
            Self::Ps2(core, _) => core.ready(),
        }
    }
    pub fn run(&mut self) {
        match self {
            Self::Ps1(core) => core.run(),
            Self::Ps2(core, _) => core.run(),
        }
    }
    pub fn checkpoint(&mut self) -> Result<Checkpoint> {
        match self {
            Self::Ps1(core) => core.checkpoint().map(Checkpoint::Ps1),
            Self::Ps2(core, _) => core.checkpoint().map(Checkpoint::Ps2),
        }
    }
    pub fn restore_checkpoint(&mut self, bytes: &[u8]) -> Result<()> {
        match self {
            Self::Ps1(core) => core.restore_checkpoint(bytes),
            Self::Ps2(core, _) => core.restore(bytes),
        }
    }
    pub fn restore_card(&mut self, card: &MemoryCard) -> Result<()> {
        match self {
            Self::Ps1(core) => core.restore_card(card),
            Self::Ps2(_, _) => {
                Err("PS2 cards are owned files and cannot be replaced during emulation.".into())
            }
        }
    }
    pub fn flush_card(&mut self, card: Option<&mut MemoryCard>) -> Result<()> {
        match self {
            Self::Ps1(core) => {
                core.flush_card(card.ok_or("PS1 card is unavailable.")?)?;
                Ok(())
            }
            Self::Ps2(core, _) => core.flush(),
        }
    }
    pub fn card_identity(&mut self) -> Result<Vec<u8>> {
        match self {
            Self::Ps1(core) => Ok(core.card_ram()?.to_vec()),
            Self::Ps2(core, root) => {
                core.flush()?;
                ps2::card_identity(root).map_err(|e| e.to_string())
            }
        }
    }
    pub fn disc_status(&self) -> Option<(u32, u32)> {
        match self {
            Self::Ps1(core) => core.disc_status(),
            Self::Ps2(_, _) => None,
        }
    }
    pub fn change_disc(&mut self, index: u32) -> Result<()> {
        match self {
            Self::Ps1(core) => core.change_disc(index),
            Self::Ps2(_, _) => {
                Err("PS2 disc replacement is not available for this session.".into())
            }
        }
    }
    pub fn destroy_context(&mut self) {
        if let Self::Ps1(core) = self {
            core.destroy_context();
        }
    }
}
