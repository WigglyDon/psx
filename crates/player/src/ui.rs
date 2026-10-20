//! Small native menu model. No platform handles or emulation ownership.
use crate::artwork::Artwork;
use crate::input::{DpadFeedback, Layout, PlayerViewport, Preset, Rect};
use crate::settings::Speed;
use std::time::{Duration, Instant};

pub const ROW_HEIGHT: f32 = 64.;

/// A captured toolbar press: a hold never also becomes a tap on release.
#[derive(Default)]
pub struct HoldPress(Option<Press>);
pub enum PressAction {
    Tap,
    Hold,
}
struct Press {
    id: i32,
    origin: [f32; 2],
    started: Instant,
    eligible: bool,
}
impl HoldPress {
    pub fn begin(&mut self, id: i32, x: f32, y: f32, now: Instant) {
        if self.0.is_none() {
            self.0 = Some(Press {
                id,
                origin: [x, y],
                started: now,
                eligible: true,
            });
        }
    }
    pub fn owns(&self, id: i32) -> bool {
        self.0.as_ref().is_some_and(|p| p.id == id)
    }
    pub fn motion(&mut self, id: i32, x: f32, y: f32) {
        if let Some(p) = &mut self.0 {
            if p.id == id && (x - p.origin[0]).hypot(y - p.origin[1]) > 12. {
                p.eligible = false;
            }
        }
    }
    pub fn held(&mut self, now: Instant) -> bool {
        if let Some(p) = &mut self.0 {
            if p.eligible && now.duration_since(p.started) >= Duration::from_millis(500) {
                p.eligible = false;
                return true;
            }
        }
        false
    }
    pub fn release(&mut self, id: i32, x: f32, y: f32, now: Instant) -> Option<PressAction> {
        if !self.owns(id) {
            return None;
        }
        self.motion(id, x, y);
        self.0.take().filter(|p| p.eligible).map(|p| {
            if now.duration_since(p.started) >= Duration::from_millis(500) {
                PressAction::Hold
            } else {
                PressAction::Tap
            }
        })
    }
    pub fn cancel(&mut self) {
        self.0 = None;
    }
}

#[cfg(test)]
mod press_tests {
    use super::*;
    #[test]
    fn tap_and_hold_are_exclusive_and_another_finger_cannot_release() {
        let now = Instant::now();
        let mut press = HoldPress::default();
        press.begin(1, 20., 20., now);
        assert!(press.release(2, 20., 20., now).is_none());
        assert!(!press.held(now + Duration::from_millis(400)));
        assert!(matches!(
            press.release(1, 22., 21., now),
            Some(PressAction::Tap)
        ));
        press.begin(1, 20., 20., now);
        assert!(press.held(now + Duration::from_millis(500)));
        assert!(!press.held(now + Duration::from_secs(1)));
        assert!(press
            .release(1, 20., 20., now + Duration::from_secs(1))
            .is_none());
        press.begin(1, 20., 20., now);
        assert!(matches!(
            press.release(1, 20., 20., now + Duration::from_millis(510)),
            Some(PressAction::Hold)
        ));
    }
    #[test]
    fn drifting_or_interruption_cancels_both_actions() {
        let now = Instant::now();
        let mut press = HoldPress::default();
        press.begin(1, 20., 20., now);
        press.motion(1, 60., 20.);
        press.motion(1, 20., 20.);
        assert!(!press.held(now + Duration::from_secs(1)));
        assert!(press.release(1, 20., 20., now).is_none());
        press.begin(1, 20., 20., now);
        press.cancel();
        assert!(!press.held(now + Duration::from_secs(1)));
        assert!(press.release(1, 20., 20., now).is_none());
    }
}

#[derive(Default)]
pub struct Row {
    pub title: String,
    pub detail: String,
    pub checked: Option<bool>,
    pub enabled: bool,
    pub height: f32,
    pub artwork: Option<Artwork>,
    pub toggle: Option<bool>,
    pub slider: Option<u8>,
    pub slider_label: Option<String>,
    pub secondary: bool,
}

/// Visual response never delays logical input. Bounded interpolation keeps
/// frame-rate-independent press/release and menu fades, including reversals.
#[derive(Default)]
pub struct Visuals {
    pub presses: [f32; 16],
    pub overlay: f32,
    pub time: f64,
    pub dpad: DpadVisual,
    pub has_game: bool,
    pub audio_blocked: bool,
    pub playback_paused: bool,
    pub playback_speed: Speed,
    pub timescale_enabled: bool,
    pub muted: bool,
    pub axes: [i16; 4],
    pub pause_overlay: f32,
    pulses: [f32; 16],
}

#[derive(Default)]
pub struct DpadVisual {
    pub active: f32,
    pub center: [f32; 2],
    pub offset: [f32; 2],
}
impl Visuals {
    pub fn trigger(&mut self, pressed: u16) {
        for (i, pulse) in self.pulses.iter_mut().enumerate() {
            if pressed & (1 << i) != 0 {
                *pulse = 0.04;
            }
        }
    }
    pub fn advance(
        &mut self,
        dt: f32,
        buttons: u16,
        menu: bool,
        animated_icons: bool,
        dpad: Option<DpadFeedback>,
    ) -> bool {
        let dt = dt.clamp(0., 0.1);
        let previous_icon_frames = ((self.time / 0.32) as u64, (self.time / 0.22) as u64);
        self.time += dt as f64;
        let mut active = false;
        // Pause is a machine state, so its indicator changes on the same
        // presentation as the toolbar. Only decorative transitions ease.
        let pause = if self.has_game && self.playback_paused && !menu {
            1.
        } else {
            0.
        };
        active |= self.pause_overlay != pause;
        self.pause_overlay = pause;
        active |= approach(
            &mut self.dpad.active,
            if dpad.is_some() { 1. } else { 0. },
            dt,
            0.055,
        );
        let dpad = dpad.unwrap_or_default();
        for i in 0..2 {
            active |= approach(&mut self.dpad.center[i], dpad.center[i], dt, 0.035);
            active |= approach(&mut self.dpad.offset[i], dpad.offset[i], dt, 0.025);
        }
        for (i, value) in self.presses.iter_mut().enumerate() {
            let target = if buttons & (1 << i) != 0 || self.pulses[i] > 0. {
                1.
            } else {
                0.
            };
            self.pulses[i] = (self.pulses[i] - dt).max(0.);
            active |= approach(value, target, dt, if target > 0. { 0.025 } else { 0.065 });
        }
        active |= approach(&mut self.overlay, if menu { 1. } else { 0. }, dt, 0.055);
        active
            || (animated_icons
                && previous_icon_frames != ((self.time / 0.32) as u64, (self.time / 0.22) as u64))
    }
}
fn approach(value: &mut f32, target: f32, dt: f32, response: f32) -> bool {
    if *value == target {
        return false;
    }
    *value += (target - *value) * (1. - (-dt / response).exp());
    if (*value - target).abs() < 0.005 {
        *value = target;
    }
    true
}

#[cfg(test)]
mod animation_tests {
    use super::*;
    #[test]
    fn pause_indicator_matches_state_without_elapsed_animation_time() {
        let mut visual = Visuals {
            has_game: true,
            playback_paused: true,
            ..Visuals::default()
        };
        assert!(visual.advance(0., 0, false, false, None));
        assert_eq!(visual.pause_overlay, 1.);
        visual.playback_paused = false;
        assert!(visual.advance(0., 0, false, false, None));
        assert_eq!(visual.pause_overlay, 0.);
        visual.playback_paused = true;
        visual.advance(0., 0, true, false, None);
        assert_eq!(visual.pause_overlay, 0.);
    }

    #[test]
    fn press_and_menu_fades_reverse_without_jumping() {
        let mut visual = Visuals::default();
        assert!(visual.advance(0.016, 1, true, false, None));
        assert!(visual.presses[0] > 0. && visual.presses[0] < 1.);
        assert!(visual.overlay > 0. && visual.overlay < 1.);
        let pressed = visual.presses[0];
        visual.advance(0.016, 0, false, false, None);
        assert!(visual.presses[0] < pressed && visual.presses[0] > 0.);
        for _ in 0..60 {
            visual.advance(0.016, 0, false, false, None);
        }
        assert_eq!(visual.presses[0], 0.);
        assert_eq!(visual.overlay, 0.);
    }

    #[test]
    fn captured_thumb_visuals_settle_smoothly_after_release() {
        let mut visual = Visuals::default();
        let feedback = DpadFeedback {
            center: [1., -1.],
            offset: [0.7, -0.7],
        };
        visual.advance(0.016, 144, false, false, Some(feedback));
        assert!(visual.dpad.active > 0. && visual.dpad.active < 1.);
        assert!(visual.dpad.center[0] > 0. && visual.dpad.center[0] < 1.);
        let offset = visual.dpad.offset[0];
        visual.advance(0.016, 0, false, false, None);
        assert!(visual.dpad.offset[0] > 0. && visual.dpad.offset[0] < offset);
        for _ in 0..60 {
            visual.advance(0.016, 0, false, false, None);
        }
        assert_eq!(visual.dpad.active, 0.);
        assert_eq!(visual.dpad.center, [0., 0.]);
        assert_eq!(visual.dpad.offset, [0., 0.]);
    }
}

pub struct Editor {
    pub preset: Preset,
    pub layout: Layout,
    pub selected: usize,
    pub width: f32,
    pub height: f32,
    pub guides: [Option<f32>; 2],
    group_inputs: bool,
    original_members: Vec<(usize, Rect)>,
    cutouts: Vec<Rect>,
    pointer: Option<EditorPointer>,
    undo: Vec<Preset>,
    gesture_start: Option<Preset>,
    pub resizing: bool,
    pub grid: bool,
    tools_bottom: bool,
    reusable_top: f32,
    covered_top: f32,
    picture_bounds: Rect,
}

#[derive(Clone, Copy)]
enum EditorPointer {
    Move {
        id: i32,
        x: f32,
        y: f32,
        original: Rect,
    },
    Resize {
        id: i32,
        x: f32,
        y: f32,
        corner: usize,
        original: Rect,
    },
    Button {
        id: i32,
        button: EditorButton,
    },
    Spacing {
        id: i32,
        center: [f32; 2],
    },
}

#[derive(Clone, Copy, PartialEq)]
pub enum EditorButton {
    Next,
    Group,
    Individual,
    Reset,
    Cancel,
    Done,
    Resize,
    Undo,
    Grid,
    Mirror,
    Dock,
}
impl EditorButton {
    pub fn label(self) -> &'static str {
        match self {
            Self::Next => "Next",
            Self::Group => "Group",
            Self::Individual => "Single",
            Self::Reset => "Reset",
            Self::Cancel => "Cancel",
            Self::Done => "Done",
            Self::Resize => "Resize",
            Self::Undo => "Undo",
            Self::Grid => "Grid",
            Self::Mirror => "Mirror",
            Self::Dock => "Dock",
        }
    }
}
impl EditorPointer {
    fn id(self) -> i32 {
        match self {
            Self::Move { id, .. }
            | Self::Resize { id, .. }
            | Self::Button { id, .. }
            | Self::Spacing { id, .. } => id,
        }
    }
}

pub enum EditResult {
    Done,
    Cancel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum InputGroup {
    Actions,
    LeftShoulders,
    RightShoulders,
}
impl InputGroup {
    const ALL: [Self; 3] = [Self::Actions, Self::LeftShoulders, Self::RightShoulders];
    fn offset(self) -> usize {
        match self {
            Self::Actions => 1,
            Self::LeftShoulders => 2,
            Self::RightShoulders => 3,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Actions => "Action buttons",
            Self::LeftShoulders => "Left shoulders",
            Self::RightShoulders => "Right shoulders",
        }
    }
}

impl Editor {
    fn group(&self, index: usize) -> Option<InputGroup> {
        InputGroup::ALL
            .into_iter()
            .find(|g| index == self.layout.element_count() + g.offset())
    }
    fn group_elements(&self, group: InputGroup) -> Vec<usize> {
        self.layout
            .controls
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let matches = match group {
                    InputGroup::Actions => matches!(c.label, "TRI" | "O" | "X" | "SQ"),
                    InputGroup::LeftShoulders => matches!(c.label, "L1" | "L2"),
                    InputGroup::RightShoulders => matches!(c.label, "R1" | "R2"),
                };
                matches.then_some(i + 1)
            })
            .collect()
    }
    fn group_bounds(&self, group: InputGroup, layout: &Layout) -> Rect {
        self.group_elements(group)
            .into_iter()
            .map(|i| layout.element(i))
            .reduce(|a, b| {
                let x = a.x.min(b.x);
                let y = a.y.min(b.y);
                Rect::new(
                    x,
                    y,
                    (a.x + a.w).max(b.x + b.w) - x,
                    (a.y + a.h).max(b.y + b.h) - y,
                )
            })
            .unwrap_or_default()
    }
    fn element_group(&self, index: usize) -> Option<InputGroup> {
        InputGroup::ALL
            .into_iter()
            .find(|g| self.group_elements(*g).contains(&index))
    }
    fn select_element(&self, index: usize) -> usize {
        if self.group_inputs {
            if let Some(group) = self.element_group(index) {
                return self.layout.element_count() + group.offset();
            }
        }
        index
    }
    pub fn new(preset: Preset, width: f32, height: f32) -> Self {
        Self::with_cutouts(preset, width, height, Vec::new())
    }
    pub fn with_cutouts(preset: Preset, width: f32, height: f32, cutouts: Vec<Rect>) -> Self {
        let layout = preset.layout_with_cutouts(width, height, &cutouts);
        Self {
            preset,
            layout,
            selected: 0,
            width,
            height,
            guides: [None; 2],
            group_inputs: true,
            original_members: Vec::new(),
            cutouts,
            pointer: None,
            undo: Vec::new(),
            gesture_start: None,
            resizing: false,
            grid: true,
            tools_bottom: false,
            reusable_top: 0.,
            covered_top: 0.,
            picture_bounds: Rect::new(0., 0., width, height),
        }
    }
    /// Editing uses the actual play surface, including the same bar/camera
    /// clearance. No scaled preview or extra controller margin is introduced.
    pub fn set_viewport(&mut self, reusable_top: f32, covered_top: f32) {
        self.set_picture_viewport(
            reusable_top,
            covered_top,
            Rect::new(0., -reusable_top, self.width, self.height + reusable_top),
        );
    }
    pub fn set_picture_viewport(&mut self, reusable_top: f32, covered_top: f32, picture: Rect) {
        if self.reusable_top != reusable_top
            || self.covered_top != covered_top
            || !self.picture_bounds.same_bounds(picture)
        {
            self.cancel_touch();
            self.reusable_top = reusable_top;
            self.covered_top = covered_top;
            self.picture_bounds = picture;
            self.layout = self.layout_for(&self.preset);
        }
    }
    fn layout_for(&self, preset: &Preset) -> Layout {
        PlayerViewport {
            controls: [self.width, self.height],
            picture: self.picture_bounds,
            reusable_top: self.reusable_top,
            covered_top: self.covered_top,
        }
        .layout(preset, &self.cutouts)
    }
    pub fn preview(&self) -> Rect {
        Rect::new(0., 0., self.width, self.height)
    }
    pub fn map(&self, rect: Rect) -> Rect {
        let preview = self.preview();
        let scale = preview.w / self.width;
        Rect::new(
            preview.x + rect.x * scale,
            preview.y + rect.y * scale,
            rect.w * scale,
            rect.h * scale,
        )
    }
    pub fn buttons(&self) -> Vec<(Rect, EditorButton)> {
        if self.dragging() {
            return Vec::new();
        }
        let tray = self.tools();
        let cell = (tray.w - 12.) / 5.;
        let spacing_row = if self.action_group_selected() {
            50.
        } else {
            0.
        };
        let actions = [
            Some(EditorButton::Next),
            Some(EditorButton::Resize),
            (self.group(self.selected).is_some() || self.element_group(self.selected).is_some())
                .then_some(if self.group(self.selected).is_some() {
                    EditorButton::Individual
                } else {
                    EditorButton::Group
                }),
            self.mirror_target().map(|_| EditorButton::Mirror),
            Some(EditorButton::Dock),
            (!self.undo.is_empty()).then_some(EditorButton::Undo),
            (!self.selected_is_default()).then_some(EditorButton::Reset),
            Some(EditorButton::Grid),
            Some(EditorButton::Cancel),
            Some(EditorButton::Done),
        ];
        actions
            .into_iter()
            .enumerate()
            .filter_map(|(i, b)| {
                b.map(|b| {
                    (
                        Rect::new(
                            tray.x + 6. + (i % 5) as f32 * cell,
                            tray.y + 44. + spacing_row + (i / 5) as f32 * 50.,
                            cell - 2.,
                            48.,
                        ),
                        b,
                    )
                })
            })
            .collect()
    }
    pub fn tools(&self) -> Rect {
        let width = (self.width - 16.).min(520.);
        let height = if self.action_group_selected() {
            198.
        } else {
            148.
        };
        Rect::new(
            (self.width - width) / 2.,
            if self.tools_bottom {
                (self.height - height - 6.).max(0.)
            } else {
                self.covered_top.max(8.)
            },
            width,
            height,
        )
    }
    pub fn dragging(&self) -> bool {
        matches!(
            self.pointer,
            Some(EditorPointer::Move { .. } | EditorPointer::Resize { .. })
        )
    }
    pub fn spacing_slider(&self) -> Option<Rect> {
        self.action_group_selected().then(|| {
            let tray = self.tools();
            Rect::new(tray.x + 16., tray.y + 44., tray.w - 32., 48.)
        })
    }
    pub fn spacing_value(&self) -> f32 {
        let r = self.layout.action_bounds();
        let diameter = self
            .layout
            .action_elements()
            .map(|i| {
                let b = self.layout.element(i);
                b.w.max(b.h)
            })
            .fold(1_f32, f32::max);
        let average = self
            .layout
            .action_elements()
            .map(|i| {
                let b = self.layout.element(i);
                (b.x + b.w / 2. - r.x - r.w / 2.).hypot(b.y + b.h / 2. - r.y - r.h / 2.)
            })
            .sum::<f32>()
            / 4.;
        ((average / diameter - std::f32::consts::FRAC_1_SQRT_2) / 0.8).clamp(0., 1.)
    }
    fn spacing_motion(&mut self, x: f32) {
        let Some(slider) = self.spacing_slider() else {
            return;
        };
        let value = ((x - slider.x) / slider.w).clamp(0., 1.);
        let Some(EditorPointer::Spacing { center, .. }) = self.pointer else {
            return;
        };
        let diameter = self
            .layout
            .action_elements()
            .map(|i| {
                let b = self.layout.element(i);
                b.w.max(b.h)
            })
            .fold(1_f32, f32::max);
        let step = (diameter * (std::f32::consts::FRAC_1_SQRT_2 + 0.8 * value))
            .min(((self.width.min(self.height) - diameter) / 2.).max(0.));
        let extent = step + diameter / 2.;
        let cx = center[0].clamp(extent, (self.width - extent).max(extent));
        let cy = center[1].clamp(extent, (self.height - extent).max(extent));
        if self
            .cutouts
            .iter()
            .any(|c| Rect::new(cx - extent, cy - extent, extent * 2., extent * 2.).overlaps(*c))
        {
            return;
        }
        let members: Vec<_> = self.layout.action_elements().collect();
        for i in members {
            let c = self.layout.controls[i - 1];
            let (dx, dy) = match c.label {
                "TRI" => (0., -step),
                "O" => (step, 0.),
                "X" => (0., step),
                "SQ" => (-step, 0.),
                _ => continue,
            };
            self.layout.set_element(
                i,
                Rect::new(
                    cx + dx - c.bounds.w / 2.,
                    cy + dy - c.bounds.h / 2.,
                    c.bounds.w,
                    c.bounds.h,
                ),
            );
        }
        self.preset.capture(&self.layout, self.width, self.height);
        self.layout = self.layout_for(&self.preset);
    }
    pub fn active_button(&self, button: EditorButton) -> bool {
        (button == EditorButton::Resize && self.resizing)
            || (button == EditorButton::Grid && self.grid)
    }
    fn finish_gesture(&mut self) {
        if let Some(before) = self.gesture_start.take() {
            self.remember(before);
        }
    }
    fn remember(&mut self, before: Preset) {
        if before != self.preset {
            if self.undo.len() == 32 {
                self.undo.remove(0);
            }
            self.undo.push(before);
        }
    }
    fn mirror_target(&self) -> Option<usize> {
        if let Some(group) = self.group(self.selected) {
            match group {
                InputGroup::LeftShoulders => {
                    return Some(self.layout.element_count() + InputGroup::RightShoulders.offset())
                }
                InputGroup::RightShoulders => {
                    return Some(self.layout.element_count() + InputGroup::LeftShoulders.offset())
                }
                InputGroup::Actions => {}
            }
        }
        if self.selected == 0 {
            return Some(self.layout.element_count() + 1);
        }
        if self.action_group_selected() {
            return Some(0);
        }
        let opposite = match self.selected_name() {
            "Left stick" => "Right stick",
            "Right stick" => "Left stick",
            "L1" => "R1",
            "R1" => "L1",
            "L2" => "R2",
            "R2" => "L2",
            "L3" => "R3",
            "R3" => "L3",
            "Select" => "Start",
            "Start" => "Select",
            _ => return None,
        };
        self.layout
            .controller_elements()
            .find(|i| self.layout.element_name(*i) == opposite)
    }
    fn mirror(&mut self) {
        let Some(target) = self.mirror_target() else {
            return;
        };
        let selected = self.selected_bounds();
        let reflected = Rect::new(
            self.width - selected.x - selected.w,
            selected.y,
            selected.w,
            selected.h,
        );
        if self.cutouts.iter().any(|c| reflected.overlaps(*c)) {
            return;
        }
        if let Some(group @ (InputGroup::LeftShoulders | InputGroup::RightShoulders)) =
            self.group(self.selected)
        {
            let source_members = self.group_elements(group);
            for i in source_members {
                let source = self.layout.controls[i - 1];
                let second = source.label.ends_with('2');
                let opposite = match (group, second) {
                    (InputGroup::LeftShoulders, false) => "R1",
                    (InputGroup::LeftShoulders, true) => "R2",
                    (InputGroup::RightShoulders, false) => "L1",
                    _ => "L2",
                };
                if let Some(j) = self
                    .layout
                    .controls
                    .iter()
                    .position(|c| c.label == opposite)
                {
                    let r = source.bounds;
                    self.layout
                        .set_element(j + 1, Rect::new(self.width - r.x - r.w, r.y, r.w, r.h));
                }
            }
        } else if target == self.layout.element_count() + 1 {
            let original = self.layout.action_bounds();
            let members: Vec<_> = self
                .layout
                .action_elements()
                .map(|i| (i, self.layout.element(i)))
                .collect();
            for (i, member) in members {
                self.layout.set_element(
                    i,
                    Rect::new(
                        reflected.x + (member.x - original.x) * reflected.w / original.w,
                        reflected.y + (member.y - original.y) * reflected.h / original.h,
                        member.w * reflected.w / original.w,
                        member.h * reflected.h / original.h,
                    ),
                );
            }
        } else {
            self.layout.set_element(target, reflected);
        }
        self.preset.capture(&self.layout, self.width, self.height);
        self.layout = self.layout_for(&self.preset);
    }
    pub fn selected_bounds(&self) -> Rect {
        if let Some(group) = self.group(self.selected) {
            self.group_bounds(group, &self.layout)
        } else if self.selected < self.layout.element_count() {
            self.layout.element(self.selected)
        } else {
            self.layout.game
        }
    }
    pub fn selected_is_default(&self) -> bool {
        if let Some(group) = self.group(self.selected) {
            let defaults = self.defaults();
            return self
                .group_elements(group)
                .into_iter()
                .all(|i| self.layout.element(i).same_bounds(defaults.element(i)));
        }
        let expected = if self.selected < self.layout.element_count() {
            self.defaults().element(self.selected)
        } else {
            let mut reset = self.preset.clone();
            reset.game_scale = 1.;
            reset.game_offset = [0.5, 0.5];
            self.layout_for(&reset).game
        };
        self.selected_bounds().same_bounds(expected)
    }
    fn defaults(&self) -> Layout {
        self.layout_for(&Preset {
            analog: self.preset.analog,
            ..Preset::default()
        })
    }
    fn action_group_selected(&self) -> bool {
        self.selected == self.layout.element_count() + 1
    }
    fn picture_selected(&self) -> bool {
        self.selected == self.layout.element_count()
    }
    fn set_selected_bounds(&mut self, original: Rect, rect: Rect) {
        // Stop the selected object at a camera edge. Passing an overlapping
        // individual face button into the play-layout normalizer would move
        // the entire diamond, including controls the user did not select.
        let rect = self.camera_limited(original, rect);
        if let Some(group) = self.group(self.selected) {
            let members = self.group_elements(group);
            for &(i, member) in &self.original_members {
                if !members.contains(&i) {
                    continue;
                }
                self.layout.set_element(
                    i,
                    Rect::new(
                        rect.x + (member.x - original.x) * rect.w / original.w,
                        rect.y + (member.y - original.y) * rect.h / original.h,
                        member.w * rect.w / original.w,
                        member.h * rect.h / original.h,
                    ),
                );
            }
        } else {
            self.layout.set_element(self.selected, rect);
        }
        self.preset.capture(&self.layout, self.width, self.height);
    }
    fn camera_limited(&self, original: Rect, requested: Rect) -> Rect {
        if !self.cutouts.iter().any(|c| requested.overlaps(*c)) {
            return requested;
        }
        if self.cutouts.iter().any(|c| original.overlaps(*c)) {
            return original;
        }
        let interpolate = |t| {
            Rect::new(
                original.x + (requested.x - original.x) * t,
                original.y + (requested.y - original.y) * t,
                original.w + (requested.w - original.w) * t,
                original.h + (requested.h - original.h) * t,
            )
        };
        let (mut low, mut high) = (0., 1.);
        for _ in 0..20 {
            let t = (low + high) / 2.;
            if self.cutouts.iter().any(|c| interpolate(t).overlaps(*c)) {
                high = t;
            } else {
                low = t;
            }
        }
        interpolate(low)
    }
    pub fn cutouts(&self) -> &[Rect] {
        &self.cutouts
    }
    /// Visible corner grips share the same 48-point touch areas as rendering.
    pub fn handles(&self) -> [(f32, f32); 4] {
        let r = self.map(self.selected_bounds());
        // Small custom controls need room between corner touch areas and the
        // movable center. Outset their grips rather than stealing center drags.
        let outset = ((88. - r.w.min(r.h)) / 2.).max(0.);
        [
            (r.x - outset, r.y - outset),
            (r.x + r.w + outset, r.y - outset),
            (r.x + r.w + outset, r.y + r.h + outset),
            (r.x - outset, r.y + r.h + outset),
        ]
        .map(|(x, y)| {
            (
                x.clamp(24., (self.width - 24.).max(24.)),
                y.clamp(24., (self.height - 24.).max(24.)),
            )
        })
    }
    pub fn selected_name(&self) -> &str {
        if let Some(group) = self.group(self.selected) {
            group.name()
        } else if self.picture_selected() {
            "Game picture"
        } else {
            self.layout.element_name(self.selected)
        }
    }
    pub fn down(&mut self, id: i32, x: f32, y: f32) {
        if self.pointer.is_some() {
            return;
        }
        if let Some((_, button)) = self.buttons().into_iter().find(|(r, _)| r.contains(x, y)) {
            self.pointer = Some(EditorPointer::Button { id, button });
            return;
        }
        if self.spacing_slider().is_some_and(|r| r.contains(x, y)) {
            self.gesture_start = Some(self.preset.clone());
            let r = self.layout.action_bounds();
            self.pointer = Some(EditorPointer::Spacing {
                id,
                center: [r.x + r.w / 2., r.y + r.h / 2.],
            });
            self.spacing_motion(x);
            return;
        }
        if self.tools().contains(x, y) {
            return;
        }
        self.original_members = self
            .layout
            .controller_elements()
            .map(|i| (i, self.layout.element(i)))
            .collect();
        if let Some(corner) = self
            .resizing
            .then(|| {
                self.handles()
                    .iter()
                    .enumerate()
                    .filter(|(_, (hx, hy))| (x - hx).abs() <= 24. && (y - hy).abs() <= 24.)
                    .min_by(|(_, (ax, ay)), (_, (bx, by))| {
                        (x - ax).hypot(y - ay).total_cmp(&(x - bx).hypot(y - by))
                    })
                    .map(|(i, _)| i)
            })
            .flatten()
        {
            self.gesture_start = Some(self.preset.clone());
            self.pointer = Some(EditorPointer::Resize {
                id,
                x,
                y,
                corner,
                original: self.selected_bounds(),
            });
            return;
        }
        if let Some(index) = self
            .layout
            .controller_elements()
            .rev()
            .find(|i| self.map(self.layout.element(*i)).contains(x, y))
        {
            self.selected = self.select_element(index);
        } else if let Some(index) = self
            .layout
            .controller_elements()
            .filter(|i| {
                let r = self.map(self.layout.element(*i));
                let cx = r.x + r.w / 2.;
                let cy = r.y + r.h / 2.;
                (x - cx).abs() <= r.w.max(48.) / 2. && (y - cy).abs() <= r.h.max(48.) / 2.
            })
            .min_by(|a, b| {
                let distance = |i| {
                    let r = self.map(self.layout.element(i));
                    (x - r.x - r.w / 2.).hypot(y - r.y - r.h / 2.)
                };
                distance(*a).total_cmp(&distance(*b))
            })
        {
            self.selected = self.select_element(index);
        } else if let Some(group) = InputGroup::ALL.into_iter().find(|g| {
            self.group_inputs && self.map(self.group_bounds(*g, &self.layout)).contains(x, y)
        }) {
            self.selected = self.layout.element_count() + group.offset();
        } else if self.map(self.layout.game).contains(x, y) {
            self.selected = self.layout.element_count();
        } else {
            return;
        }
        self.guides = [None; 2];
        self.gesture_start = Some(self.preset.clone());
        self.pointer = Some(EditorPointer::Move {
            id,
            x,
            y,
            original: self.selected_bounds(),
        });
    }
    pub fn motion(&mut self, id: i32, x: f32, y: f32) {
        let Some(pointer) = self.pointer else {
            return;
        };
        if pointer.id() != id {
            return;
        }
        let scale = self.preview().w / self.width;
        let (previous_x, previous_y, original) = match pointer {
            EditorPointer::Move { x, y, original, .. } => (x, y, original),
            EditorPointer::Resize {
                x: start_x,
                y: start_y,
                corner,
                original,
                ..
            } => {
                self.resize(
                    corner,
                    original,
                    (x - start_x) / scale,
                    (y - start_y) / scale,
                );
                return;
            }
            EditorPointer::Button { .. } => return,
            EditorPointer::Spacing { .. } => {
                self.spacing_motion(x);
                return;
            }
        };
        let dx = (x - previous_x) / scale;
        let dy = (y - previous_y) / scale;
        self.guides = [None; 2];
        if !self.picture_selected() {
            let mut r = original;
            r.x = (original.x + dx).clamp(0., self.width - r.w);
            r.y = (original.y + dy).clamp(0., self.height - r.h);
            r = self.snap_move(r, Rect::new(0., 0., self.width, self.height));
            self.set_selected_bounds(original, r);
        } else {
            let mut full = self.preset.clone();
            full.game_scale = 1.;
            let area = self.layout_for(&full).game;
            let slack_x = area.w - self.layout.game.w;
            let slack_y = area.h - self.layout.game.h;
            let mut r = original;
            r.x = (original.x + dx).clamp(area.x, area.x + slack_x.max(0.));
            r.y = (original.y + dy).clamp(area.y, area.y + slack_y.max(0.));
            r = self.snap_move(r, area);
            if slack_x > 0.1 {
                self.preset.game_offset[0] = ((r.x - area.x) / slack_x).clamp(0., 1.);
            }
            if slack_y > 0.1 {
                self.preset.game_offset[1] = ((r.y - area.y) / slack_y).clamp(0., 1.);
            }
        }
        self.layout = self.layout_for(&self.preset);
        // Keep the drag's unsnapped origin. Incrementing the snapped position
        // would trap slow finger movement at a guide.
    }
    fn snap_move(&mut self, mut r: Rect, area: Rect) -> Rect {
        if !self.grid {
            return r;
        }
        let threshold = 6. / (self.preview().w / self.width);
        let defaults = self.defaults();
        let mut references = vec![if let Some(group) = self.group(self.selected) {
            self.group_bounds(group, &defaults)
        } else if self.selected < defaults.element_count() {
            defaults.element(self.selected)
        } else {
            area
        }];
        if !self.picture_selected() {
            references.extend(
                (0..self.layout.element_count())
                    .filter(|i| {
                        *i != self.selected
                            && !self
                                .group(self.selected)
                                .is_some_and(|g| self.group_elements(g).contains(i))
                    })
                    .map(|i| self.layout.element(i)),
            );
        }
        for axis in 0..2 {
            let (start, extent, low, span) = if axis == 0 {
                (r.x, r.w, area.x, area.w)
            } else {
                (r.y, r.h, area.y, area.h)
            };
            let mut best = threshold;
            let origin = if axis == 0 {
                self.width / 2. - extent / 2.
            } else {
                0.
            };
            let mut position = (origin + ((start - origin) / 8.).round() * 8.)
                .clamp(low, (low + span - extent).max(low));
            let mut guide = None;
            let mut consider = |offset: f32, target: f32| {
                let candidate = target - offset;
                let distance = (candidate - start).abs();
                if distance <= best && candidate >= low && candidate + extent <= low + span + 0.01 {
                    best = distance;
                    position = candidate;
                    guide = Some(target);
                }
            };
            let margin = if !self.picture_selected() { 16. } else { 0. };
            consider(0., low + margin);
            consider(extent / 2., low + span / 2.);
            consider(extent, low + span - margin);
            for reference in &references {
                let (p, length) = if axis == 0 {
                    (reference.x, reference.w)
                } else {
                    (reference.y, reference.h)
                };
                for fraction in [0., 0.5, 1.] {
                    consider(extent * fraction, p + length * fraction);
                }
                if axis == 0 {
                    consider(extent / 2., self.width - p - length / 2.);
                }
            }
            if axis == 0 {
                r.x = position;
            } else {
                r.y = position;
            }
            self.guides[axis] = guide;
        }
        r
    }
    fn resize(&mut self, corner: usize, original: Rect, dx: f32, dy: f32) {
        if original.w <= 0. || original.h <= 0. {
            return;
        }
        let left = matches!(corner, 0 | 3);
        let top = matches!(corner, 0 | 1);
        let sx = if left { -1. } else { 1. };
        let sy = if top { -1. } else { 1. };
        let change_x = sx * dx / original.w;
        let change_y = sy * dy / original.h;
        let requested = 1.
            + if change_x.abs() > change_y.abs() {
                change_x
            } else {
                change_y
            };
        let game = self.picture_selected();
        let area = if game {
            let mut full = self.preset.clone();
            full.game_scale = 1.;
            self.layout_for(&full).game
        } else {
            Rect::new(0., 0., self.width, self.height)
        };
        let anchor_x = original.x + if left { original.w } else { 0. };
        let anchor_y = original.y + if top { original.h } else { 0. };
        let available_w = if left {
            anchor_x - area.x
        } else {
            area.x + area.w - anchor_x
        };
        let available_h = if top {
            anchor_y - area.y
        } else {
            area.y + area.h - anchor_y
        };
        let minimum = if game {
            area.w * 0.25 / original.w
        } else if let Some(group) = self.group(self.selected) {
            let members = self.group_elements(group);
            self.original_members
                .iter()
                .filter(|(i, _)| members.contains(i))
                .map(|(_, r)| (24. / r.w).max(24. / r.h))
                .fold(0_f32, f32::max)
        } else {
            (24. / original.w).max(24. / original.h)
        };
        let maximum = (available_w / original.w)
            .min(available_h / original.h)
            .max(minimum);
        let requested = if self.grid {
            (original.w * requested / 8.).round() * 8. / original.w
        } else {
            requested
        };
        let ratio = requested.clamp(minimum, maximum);
        let resized = Rect::new(
            anchor_x - if left { original.w * ratio } else { 0. },
            anchor_y - if top { original.h * ratio } else { 0. },
            original.w * ratio,
            original.h * ratio,
        );
        if game {
            self.preset.game_scale = (resized.w / area.w).clamp(0.25, 1.);
            for (offset, position, start, slack) in [
                (0, resized.x, area.x, area.w - resized.w),
                (1, resized.y, area.y, area.h - resized.h),
            ] {
                self.preset.game_offset[offset] = if slack > 0.1 {
                    ((position - start) / slack).clamp(0., 1.)
                } else {
                    0.5
                };
            }
        } else {
            self.set_selected_bounds(original, resized);
        }
        self.layout = self.layout_for(&self.preset);
    }
    pub fn up(&mut self, id: i32, x: f32, y: f32) -> Option<EditResult> {
        let pointer = self.pointer?;
        if pointer.id() != id {
            return None;
        }
        self.pointer = None;
        self.finish_gesture();
        self.guides = [None; 2];
        let EditorPointer::Button { button, .. } = pointer else {
            return None;
        };
        if !self
            .buttons()
            .iter()
            .any(|(r, b)| *b == button && r.contains(x, y))
        {
            return None;
        }
        match button {
            EditorButton::Next => {
                let mut selections: Vec<_> = self
                    .layout
                    .controller_elements()
                    .chain(std::iter::once(self.layout.element_count()))
                    .filter(|i| !self.group_inputs || self.element_group(*i).is_none())
                    .collect();
                if self.group_inputs {
                    for group in InputGroup::ALL.into_iter().rev() {
                        selections.insert(1, self.layout.element_count() + group.offset());
                    }
                }
                let current = selections
                    .iter()
                    .position(|i| *i == self.selected)
                    .unwrap_or(0);
                self.selected = selections[(current + 1) % selections.len()];
            }
            EditorButton::Group => {
                self.group_inputs = true;
                self.selected = self.select_element(self.selected);
            }
            EditorButton::Individual => {
                let group = self.group(self.selected);
                self.group_inputs = false;
                self.selected = group
                    .and_then(|g| self.group_elements(g).first().copied())
                    .unwrap_or(0);
            }
            EditorButton::Reset => {
                let before = self.preset.clone();
                if let Some(group) = self.group(self.selected) {
                    let defaults = self.defaults();
                    let members = self.group_elements(group);
                    for i in members {
                        self.layout.set_element(i, defaults.element(i));
                    }
                    self.preset.capture(&self.layout, self.width, self.height);
                } else if self.selected < self.layout.element_count() {
                    self.layout
                        .set_element(self.selected, self.defaults().element(self.selected));
                    self.preset.capture(&self.layout, self.width, self.height);
                } else {
                    self.preset.game_scale = 1.;
                    self.preset.game_offset = [0.5, 0.5];
                }
                self.layout = self.layout_for(&self.preset);
                self.remember(before);
            }
            EditorButton::Resize => self.resizing = !self.resizing,
            EditorButton::Grid => self.grid = !self.grid,
            EditorButton::Dock => self.tools_bottom = !self.tools_bottom,
            EditorButton::Undo => {
                if let Some(before) = self.undo.pop() {
                    self.preset = before;
                    self.layout = self.layout_for(&self.preset);
                }
            }
            EditorButton::Mirror => {
                let before = self.preset.clone();
                self.mirror();
                self.remember(before);
            }
            EditorButton::Cancel => return Some(EditResult::Cancel),
            EditorButton::Done => return Some(EditResult::Done),
        }
        None
    }
    pub fn cancel_touch(&mut self) {
        self.pointer = None;
        self.finish_gesture();
        self.guides = [None; 2];
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;

    #[test]
    fn shoulder_pairs_move_resize_mirror_reset_and_undo_independently() {
        for analog in [false, true] {
            for group in [InputGroup::LeftShoulders, InputGroup::RightShoulders] {
                let mut editor = Editor::new(
                    Preset {
                        analog,
                        ..Preset::default()
                    },
                    852.,
                    817.,
                );
                editor.grid = false;
                let members = editor.group_elements(group);
                let initial: Vec<_> = (0..editor.layout.element_count())
                    .map(|i| editor.layout.element(i))
                    .collect();
                let r = editor.layout.element(members[0]);
                let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
                editor.down(1, x, y);
                assert_eq!(editor.group(editor.selected), Some(group));
                editor.motion(2, x + 100., y - 80.);
                assert!(editor.layout.element(members[0]).same_bounds(r));
                editor.motion(1, x + 16., y - 48.);
                editor.up(1, x + 16., y - 48.);
                for &i in &members {
                    let moved = editor.layout.element(i);
                    assert!((moved.x - initial[i].x - 16.).abs() < 0.01);
                    assert!((moved.y - initial[i].y + 48.).abs() < 0.01);
                }
                assert!((0..initial.len())
                    .filter(|i| !members.contains(i))
                    .all(|i| editor.layout.element(i).same_bounds(initial[i])));
                assert!(editor.spacing_slider().is_none());
                let saved: Preset =
                    serde_json::from_slice(&serde_json::to_vec(&editor.preset).unwrap()).unwrap();
                assert!(members.iter().all(|i| saved
                    .layout(852., 817.)
                    .element(*i)
                    .same_bounds(editor.layout.element(*i))));
                press(&mut editor, EditorButton::Undo);
                assert!(members
                    .iter()
                    .all(|i| editor.layout.element(*i).same_bounds(initial[*i])));
                press(&mut editor, EditorButton::Resize);
                let old = editor.selected_bounds();
                let (hx, hy) = editor.handles()[2];
                editor.down(1, hx, hy);
                editor.motion(1, hx + 24., hy + 24.);
                editor.up(1, hx + 24., hy + 24.);
                let resized = editor.selected_bounds();
                assert!(resized.w > old.w && resized.h > old.h);
                let ratios: Vec<_> = members
                    .iter()
                    .map(|i| editor.layout.element(*i).w / initial[*i].w)
                    .collect();
                assert!((ratios[0] - ratios[1]).abs() < 0.001);
                let other = if group == InputGroup::LeftShoulders {
                    InputGroup::RightShoulders
                } else {
                    InputGroup::LeftShoulders
                };
                press(&mut editor, EditorButton::Mirror);
                for &i in &members {
                    // Pair by label suffix rather than vector ordering.
                    let label = editor.layout.controls[i - 1].label;
                    let j = editor
                        .group_elements(other)
                        .into_iter()
                        .find(|j| {
                            editor.layout.controls[*j - 1]
                                .label
                                .ends_with(label.chars().last().unwrap())
                        })
                        .unwrap();
                    let (a, b) = (editor.layout.element(i), editor.layout.element(j));
                    assert!((a.x + a.w + b.x - 852.).abs() < 0.01 && (a.y - b.y).abs() < 0.01);
                }
                press(&mut editor, EditorButton::Reset);
                assert!(editor.selected_is_default());
                press(&mut editor, EditorButton::Individual);
                assert!(editor.group(editor.selected).is_none());
                assert!(members.contains(&editor.selected));
            }
        }
    }

    fn press(editor: &mut Editor, button: EditorButton) {
        let (r, _) = editor
            .buttons()
            .into_iter()
            .find(|(_, b)| *b == button)
            .unwrap();
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        editor.down(9, x, y);
        editor.up(9, x, y);
    }

    #[test]
    fn action_group_drags_keep_custom_spacing_and_survive_preset_round_trip() {
        let (w, h) = (852., 817.);
        let mut preset = Preset::default();
        let mut layout = preset.layout(w, h);
        let mut square = layout.element(4);
        square.x -= 13.;
        layout.set_element(4, square);
        preset.capture(&layout, w, h);
        let mut editor = Editor::new(preset, w, h);
        let before: Vec<_> = editor
            .layout
            .action_elements()
            .map(|i| (i, editor.layout.element(i)))
            .collect();
        let dpad = editor.layout.dpad;
        let shoulder = editor.layout.element(5);
        let r = editor.map(editor.layout.element(1));
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        editor.down(1, x, y);
        assert_eq!(editor.selected_name(), "Action buttons");
        editor.motion(2, x - 40., y - 30.);
        assert!(before
            .iter()
            .all(|(i, r)| editor.layout.element(*i).same_bounds(*r)));
        editor.motion(1, x - 40., y - 30.);
        let first = editor.layout.element(before[0].0);
        let dx = first.x - before[0].1.x;
        let dy = first.y - before[0].1.y;
        assert!(dx < -20. && dy < -20.);
        for &(i, r) in &before {
            let moved = editor.layout.element(i);
            assert!((moved.x - r.x - dx).abs() < 0.01);
            assert!((moved.y - r.y - dy).abs() < 0.01);
            assert_eq!((moved.w, moved.h), (r.w, r.h));
        }
        assert!(editor.layout.dpad.same_bounds(dpad));
        assert!(editor.layout.element(5).same_bounds(shoulder));
        editor.cancel_touch();
        editor.motion(1, x - 100., y - 100.);
        assert!(editor.layout.element(before[0].0).same_bounds(first));
        let restored: Preset =
            serde_json::from_slice(&serde_json::to_vec(&editor.preset).unwrap()).unwrap();
        let saved = restored.layout(w, h);
        assert!((0..saved.element_count())
            .all(|i| saved.element(i).same_bounds(editor.layout.element(i))));
        assert!((0..saved.element_count()).all(|i| !saved.game.overlaps(saved.element(i))));
    }

    #[test]
    fn action_group_corner_resize_anchors_members_and_reset_preserves_other_controls() {
        let mut editor = Editor::new(Preset::default(), 852., 817.);
        editor.layout.dpad.x += 17.;
        editor
            .preset
            .capture(&editor.layout, editor.width, editor.height);
        let dpad = editor.layout.dpad;
        press(&mut editor, EditorButton::Next);
        assert_eq!(editor.selected_name(), "Action buttons");
        assert!(editor.selected_is_default());
        let bounds = editor.selected_bounds();
        let before: Vec<_> = editor
            .layout
            .action_elements()
            .map(|i| (i, editor.layout.element(i)))
            .collect();
        press(&mut editor, EditorButton::Resize);
        let (x, y) = editor.handles()[0];
        editor.down(1, x, y);
        editor.motion(1, x - 15., y - 10.);
        editor.motion(1, x - 30., y - 20.);
        editor.up(1, x - 30., y - 20.);
        let resized = editor.selected_bounds();
        assert!(resized.w > bounds.w);
        assert!((resized.x + resized.w - bounds.x - bounds.w).abs() < 0.01);
        assert!((resized.y + resized.h - bounds.y - bounds.h).abs() < 0.01);
        let ratio = resized.w / bounds.w;
        for &(i, r) in &before {
            let changed = editor.layout.element(i);
            assert!((changed.w / r.w - ratio).abs() < 0.001);
            assert!((changed.h / r.h - ratio).abs() < 0.001);
            assert!((changed.x - resized.x - (r.x - bounds.x) * ratio).abs() < 0.01);
            assert!((changed.y - resized.y - (r.y - bounds.y) * ratio).abs() < 0.01);
        }
        assert!(!editor.selected_is_default());
        press(&mut editor, EditorButton::Reset);
        assert!(editor.selected_is_default());
        assert!(editor.layout.dpad.same_bounds(dpad));
        assert!(!editor
            .buttons()
            .iter()
            .any(|(_, b)| *b == EditorButton::Reset));
    }

    #[test]
    fn individual_button_editing_and_group_reset_are_separate() {
        let mut editor = Editor::new(Preset::default(), 852., 817.);
        press(&mut editor, EditorButton::Next);
        press(&mut editor, EditorButton::Individual);
        assert_eq!(editor.selected_name(), "Triangle");
        let before: Vec<_> = (0..editor.layout.element_count())
            .map(|i| editor.layout.element(i))
            .collect();
        let r = editor.map(editor.selected_bounds());
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        editor.down(1, x, y);
        editor.motion(1, x - 30., y - 30.);
        editor.up(1, x - 30., y - 30.);
        assert_eq!(editor.selected_name(), "Triangle");
        assert!(!editor.layout.element(1).same_bounds(before[1]));
        assert!((0..before.len())
            .filter(|i| *i != 1)
            .all(|i| editor.layout.element(i).same_bounds(before[i])));
        press(&mut editor, EditorButton::Group);
        assert_eq!(editor.selected_name(), "Action buttons");
        assert!(!editor.selected_is_default());
        press(&mut editor, EditorButton::Reset);
        assert!(editor.selected_is_default());
        assert!((0..before.len()).all(|i| editor.layout.element(i).same_bounds(before[i])));
    }

    #[test]
    fn camera_layout_reset_matches_play_and_does_not_change_the_saved_source() {
        let (w, h) = (2342. / 2.4375, 1021. / 2.4375);
        let camera = Rect::new(2160. / 2.4375, 484. / 2.4375, 182. / 2.4375, 112. / 2.4375);
        let original = Preset::default();
        let mut editor = Editor::with_cutouts(original.clone(), w, h, vec![camera]);
        let play = original.layout_with_cutouts(w, h, &[camera]);
        assert!(editor.layout.game.same_bounds(play.game));
        editor.selected = 1;
        let r = editor.map(editor.layout.element(1));
        editor.down(1, r.x + r.w / 2., r.y + r.h / 2.);
        editor.motion(1, r.x + r.w / 2. - 25., r.y + r.h / 2.);
        editor.up(1, r.x + r.w / 2. - 25., r.y + r.h / 2.);
        assert!(!editor.selected_is_default());
        let (r, _) = editor
            .buttons()
            .into_iter()
            .find(|(_, b)| *b == EditorButton::Reset)
            .unwrap();
        editor.down(1, r.x + r.w / 2., r.y + r.h / 2.);
        editor.up(1, r.x + r.w / 2., r.y + r.h / 2.);
        assert!(editor.selected_is_default());
        assert!(
            (0..editor.layout.element_count()).all(|i| !editor.layout.element(i).overlaps(camera))
        );
        assert!(original.elements.is_empty());
    }

    #[test]
    fn alignment_guides_snap_to_a_peer_and_slow_drag_can_escape() {
        let mut preset = Preset::default();
        let mut layout = preset.layout(852., 817.);
        let mut triangle = layout.element(1);
        triangle.x -= 25.;
        layout.set_element(1, triangle);
        preset.capture(&layout, 852., 817.);
        let mut editor = Editor::new(preset, 852., 817.);
        editor.group_inputs = false;
        editor.selected = 1;
        let r = editor.map(editor.layout.element(1));
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        let scale = editor.preview().w / editor.width;
        editor.down(1, x, y);
        editor.motion(1, x + 25. * scale + 0.25, y);
        let aligned = editor.layout.element(1);
        let cross = editor
            .layout
            .controls
            .iter()
            .find(|c| c.label == "X")
            .unwrap()
            .bounds;
        assert!(
            (aligned.x + aligned.w / 2. - cross.x - cross.w / 2.).abs() < 0.01,
            "aligned={aligned:?} cross={cross:?} guides={:?}",
            editor.guides
        );
        assert!(editor.guides[0].is_some());
        // Each small move still uses the unsnapped start, rather than losing
        // its delta at the magnet and trapping the finger indefinitely.
        for delta in 3..=25 {
            editor.motion(1, x + 25. * scale + delta as f32, y);
        }
        assert!(editor.layout.element(1).x > aligned.x + 20.);
        let moved = editor.layout.element(1);
        editor.motion(2, x - 100., y);
        assert!(editor.layout.element(1).same_bounds(moved));
        editor.cancel_touch();
        assert!(editor.guides.iter().all(Option::is_none));
    }

    #[test]
    fn reset_is_contextual_and_preserves_other_custom_elements() {
        let mut editor = Editor::new(Preset::default(), 852., 817.);
        let has_reset = |editor: &Editor| {
            editor
                .buttons()
                .iter()
                .any(|(_, b)| *b == EditorButton::Reset)
        };
        assert!(!has_reset(&editor));
        let original_face = editor.layout.element(1);
        let custom_face = Rect::new(
            original_face.x - 30.,
            original_face.y,
            original_face.w,
            original_face.h,
        );
        editor.layout.set_element(1, custom_face);
        let original_pad = editor.layout.dpad;
        editor.layout.dpad.x += 40.;
        editor
            .preset
            .capture(&editor.layout, editor.width, editor.height);
        assert!(has_reset(&editor));
        let (reset, _) = editor
            .buttons()
            .into_iter()
            .find(|(_, b)| *b == EditorButton::Reset)
            .unwrap();
        let (x, y) = (reset.x + reset.w / 2., reset.y + reset.h / 2.);
        editor.down(1, x, y);
        editor.up(1, x, y);
        assert!(editor.layout.dpad.same_bounds(original_pad));
        assert!(editor.layout.element(1).same_bounds(custom_face));
        assert!(!has_reset(&editor));
        assert!(!editor.preset.is_default(editor.width, editor.height));
        editor.selected = 1;
        assert!(has_reset(&editor));
        editor.down(1, x, y);
        editor.up(1, x, y);
        assert!(editor.preset.is_default(editor.width, editor.height));
        assert!(!has_reset(&editor));
    }

    #[test]
    fn small_controls_move_from_the_center_without_stealing_a_resize() {
        let mut editor = Editor::new(Preset::default(), 360., 740.);
        editor.selected = 1;
        let original = editor.layout.element(1);
        let preview = editor.map(original);
        let (x, y) = (preview.x + preview.w / 2., preview.y + preview.h / 2.);
        editor.down(1, x, y);
        editor.motion(1, x - 20., y);
        editor.up(1, x - 20., y);
        assert!((editor.layout.element(1).w - original.w).abs() < 0.01);
        assert!(editor.layout.element(1).x < original.x - 10.);
    }

    #[test]
    fn corner_drags_anchor_the_opposite_corner_and_ignore_other_fingers() {
        for corner in 0..4 {
            let mut editor = Editor::new(Preset::default(), 852., 817.);
            press(&mut editor, EditorButton::Resize);
            let original = editor.layout.dpad;
            let (x, y) = editor.handles()[corner];
            let dx = if matches!(corner, 0 | 3) { 20. } else { -20. };
            let dy = if matches!(corner, 0 | 1) { 20. } else { -20. };
            editor.down(1, x, y);
            editor.motion(2, x + dx, y + dy);
            assert_eq!(editor.layout.dpad.w, original.w);
            editor.motion(1, x + dx, y + dy);
            editor.up(1, x + dx, y + dy);
            let resized = editor.layout.dpad;
            assert!(resized.w < original.w && resized.w >= 24.);
            assert!((resized.w / resized.h - original.w / original.h).abs() < 0.001);
            let anchor = |r: Rect| {
                (
                    r.x + if matches!(corner, 0 | 3) { r.w } else { 0. },
                    r.y + if matches!(corner, 0 | 1) { r.h } else { 0. },
                )
            };
            let (old_x, old_y) = anchor(original);
            let (new_x, new_y) = anchor(resized);
            assert!((old_x - new_x).abs() < 0.01 && (old_y - new_y).abs() < 0.01);
            assert!(!editor.layout.game.overlaps(resized));
        }
    }

    #[test]
    fn resizing_picture_keeps_aspect_and_cancel_releases_the_gesture() {
        let mut editor = Editor::new(Preset::default(), 852., 817.);
        editor.selected = editor.layout.element_count();
        press(&mut editor, EditorButton::Resize);
        let original = editor.layout.game;
        let (x, y) = editor.handles()[2];
        editor.down(1, x, y);
        editor.motion(1, x - 60., y - 40.);
        let resized = editor.layout.game;
        assert!(resized.w < original.w);
        assert!((resized.w / resized.h - 4. / 3.).abs() < 0.001);
        assert!((resized.x - original.x).abs() < 0.01);
        assert!((resized.y - original.y).abs() < 0.01);
        editor.cancel_touch();
        editor.motion(1, x + 200., y + 200.);
        assert_eq!(editor.layout.game.w, resized.w);
        assert!((0..editor.layout.element_count())
            .all(|i| !editor.layout.game.overlaps(editor.layout.element(i))));
    }

    #[test]
    fn live_canvas_matches_play_and_bar_reveal_preserves_the_draft() {
        for analog in [false, true] {
            let preset = Preset {
                analog,
                ..Preset::default()
            };
            let (w, h) = (852., 785.);
            let camera = Rect::new(795., -66., 57., 66.);
            let mut play = preset.layout_with_cutouts(w, h, &[camera]);
            play.anchor_toolbar(w, h, 66., 0., &[camera]);
            play.place_game_in(Rect::new(0., -66., w, h + 66.), false, 1., [0.5; 2]);
            let mut editor = Editor::with_cutouts(preset, w, h, vec![camera]);
            editor.set_viewport(66., 0.);
            assert!(editor.preview().same_bounds(Rect::new(0., 0., w, h)));
            assert!(editor.map(editor.layout.game).same_bounds(play.game));
            assert!((0..play.element_count()).all(|i| editor
                .map(editor.layout.element(i))
                .same_bounds(play.element(i))));
            let draft = editor.preset.clone();
            editor.set_viewport(0., 32.);
            assert!(editor.layout.playback.y >= 32.);
            assert!(editor.preset == draft);
            editor.set_viewport(66., 0.);
            assert!(editor.layout.game.same_bounds(play.game));
        }
    }

    #[test]
    fn editor_uses_the_same_picture_beyond_the_gesture_safe_control_surface() {
        let density = 2.4375;
        let (w, h) = (2342. / density, 1021. / density);
        let picture = Rect::new(0., 0., w, 1080. / density);
        let camera = Rect::new(0., 484. / density, 182. / density, 112. / density);
        for analog in [false, true] {
            let preset = Preset {
                analog,
                ..Preset::default()
            };
            let play = preset.layout_in(w, h, &[camera], picture);
            let mut editor = Editor::with_cutouts(preset, w, h, vec![camera]);
            editor.set_picture_viewport(0., 0., picture);
            assert!(editor.layout.game.same_bounds(play.game));
            assert!(editor.layout.game.h > editor.height);
            assert!((0..play.element_count())
                .all(|i| editor.layout.element(i).same_bounds(play.element(i))));
            let draft = editor.preset.clone();
            editor.set_picture_viewport(0., 32., picture);
            assert!(editor.layout.game.same_bounds(play.game));
            assert!(editor.preset == draft);
            editor.set_picture_viewport(0., 0., picture);
            assert!(editor.layout.game.same_bounds(play.game));
        }
    }

    #[test]
    fn grid_can_be_disabled_and_tools_dock_away_from_a_control() {
        let mut editor = Editor::new(Preset::default(), 852., 817.);
        let original = Rect::new(283.3, 201.7, 64., 64.);
        editor.selected = editor
            .layout
            .controller_elements()
            .find(|i| editor.layout.element_name(*i) == "L1")
            .unwrap();
        let snapped = editor.snap_move(original, Rect::new(0., 0., 852., 817.));
        assert!((snapped.x - original.x).abs() <= 6.);
        assert!((snapped.y - original.y).abs() <= 6.);
        assert!(!snapped.same_bounds(original));
        press(&mut editor, EditorButton::Grid);
        assert!(editor
            .snap_move(original, Rect::new(0., 0., 852., 817.))
            .same_bounds(original));
        let upper = editor.tools();
        press(&mut editor, EditorButton::Dock);
        assert!(editor.tools().y > upper.y + 300.);
        press(&mut editor, EditorButton::Dock);
        assert!(editor.tools().same_bounds(upper));
    }

    #[test]
    fn mirror_is_explicit_sizes_the_partner_and_undo_restores_everything() {
        for name in [
            "L1",
            "L2",
            "L3",
            "Select",
            "Left stick",
            "Direction pad",
            "Action buttons",
        ] {
            let mut editor = Editor::new(
                Preset {
                    analog: true,
                    ..Preset::default()
                },
                852.,
                817.,
            );
            if name == "Action buttons" {
                editor.selected = editor.layout.element_count() + 1;
            } else {
                editor.selected = editor
                    .layout
                    .controller_elements()
                    .find(|i| editor.layout.element_name(*i) == name)
                    .unwrap();
            }
            let before = editor.preset.clone();
            let bounds: Vec<_> = (0..editor.layout.element_count())
                .map(|i| editor.layout.element(i))
                .collect();
            let source = editor.selected_bounds();
            let target = editor.mirror_target().unwrap();
            press(&mut editor, EditorButton::Mirror);
            let reflected = if target == editor.layout.element_count() + 1 {
                editor.layout.action_bounds()
            } else {
                editor.layout.element(target)
            };
            assert!(
                reflected.same_bounds(Rect::new(
                    852. - source.x - source.w,
                    source.y,
                    source.w,
                    source.h
                )),
                "{name}"
            );
            for (i, r) in bounds.iter().enumerate() {
                if i != target
                    && !(target == editor.layout.element_count() + 1
                        && (editor.element_group(i) == Some(InputGroup::Actions)))
                {
                    assert!(editor.layout.element(i).same_bounds(*r), "{name}: {i}");
                }
            }
            press(&mut editor, EditorButton::Undo);
            assert!(editor.preset == before);
        }
    }

    #[test]
    fn spacing_updates_live_without_resizing_buttons_and_one_undo_reverts_the_drag() {
        let mut editor = Editor::new(
            Preset {
                analog: true,
                ..Preset::default()
            },
            852.,
            817.,
        );
        press(&mut editor, EditorButton::Next);
        let before = editor.preset.clone();
        let bounds: Vec<_> = (0..editor.layout.element_count())
            .map(|i| editor.layout.element(i))
            .collect();
        let slider = editor.spacing_slider().unwrap();
        let y = slider.y + 24.;
        editor.down(1, slider.x, y);
        let tight = editor.layout.action_bounds();
        editor.motion(2, slider.x + slider.w, y);
        assert!(editor.layout.action_bounds().same_bounds(tight));
        editor.motion(1, slider.x + slider.w, y);
        let wide = editor.layout.action_bounds();
        assert!(wide.w > tight.w + 40.);
        assert!(editor.spacing_slider().is_some());
        let centers: Vec<_> = editor
            .layout
            .action_elements()
            .map(|i| {
                let r = editor.layout.element(i);
                (
                    editor.layout.element_name(i),
                    [r.x + r.w / 2., r.y + r.h / 2.],
                )
            })
            .collect();
        let center = |name| centers.iter().find(|(n, _)| *n == name).unwrap().1;
        let t = center("Triangle");
        let x = center("Cross");
        let s = center("Square");
        let o = center("Circle");
        assert!((t[0] - x[0]).abs() < 0.01 && (s[1] - o[1]).abs() < 0.01);
        assert!((t[0] - (s[0] + o[0]) / 2.).abs() < 0.01);
        assert!((s[1] - (t[1] + x[1]) / 2.).abs() < 0.01);
        for (i, b) in bounds.iter().enumerate() {
            let r = editor.layout.element(i);
            assert!((r.w - b.w).abs() < 0.01 && (r.h - b.h).abs() < 0.01);
            if !(editor.element_group(i) == Some(InputGroup::Actions)) {
                assert!(r.same_bounds(*b));
            }
        }
        editor.up(1, slider.x + slider.w, y);
        let restored: Preset =
            serde_json::from_slice(&serde_json::to_vec(&editor.preset).unwrap()).unwrap();
        assert!(restored
            .layout(852., 817.)
            .action_bounds()
            .same_bounds(wide));
        assert_eq!(editor.undo.len(), 1);
        press(&mut editor, EditorButton::Undo);
        assert!(editor.preset == before);
    }

    #[test]
    fn tiny_input_selection_has_a_full_touch_target_and_drag_only_changes_it() {
        let mut preset = Preset::default();
        let mut layout = preset.layout(852., 817.);
        let selected = layout
            .controller_elements()
            .find(|i| layout.element_name(*i) == "L1")
            .unwrap();
        layout.set_element(selected, Rect::new(290., 260., 24., 24.));
        preset.capture(&layout, 852., 817.);
        let mut editor = Editor::new(preset, 852., 817.);
        editor.group_inputs = false;
        let before: Vec<_> = (0..editor.layout.element_count())
            .map(|i| editor.layout.element(i))
            .collect();
        editor.down(1, 282., 272.); // Outside the 24-point cap, inside its 48-point target.
        assert_eq!(editor.selected, selected);
        assert!(editor.buttons().is_empty());
        editor.motion(1, 330., 310.);
        editor.cancel_touch();
        assert!(!editor
            .layout
            .element(selected)
            .same_bounds(before[selected]));
        assert!((0..before.len())
            .filter(|i| *i != selected)
            .all(|i| editor.layout.element(i).same_bounds(before[i])));
        press(&mut editor, EditorButton::Undo);
        assert!((0..before.len()).all(|i| editor.layout.element(i).same_bounds(before[i])));
    }

    #[test]
    fn dragging_one_face_toward_a_camera_does_not_push_its_siblings() {
        let camera = Rect::new(802., 200., 50., 100.);
        let mut editor = Editor::with_cutouts(Preset::default(), 852., 817., vec![camera]);
        editor.group_inputs = false;
        editor.selected = 1;
        let before: Vec<_> = (0..editor.layout.element_count())
            .map(|i| editor.layout.element(i))
            .collect();
        let target = Rect::new(790., 225., before[1].w, before[1].h);
        editor.set_selected_bounds(before[1], target);
        editor.layout = editor.layout_for(&editor.preset);
        assert!(!editor.layout.element(1).overlaps(camera));
        assert!((0..before.len())
            .filter(|i| *i != 1)
            .all(|i| editor.layout.element(i).same_bounds(before[i])));
    }
}

impl Row {
    pub fn action(title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            detail: detail.into(),
            enabled: true,
            height: ROW_HEIGHT,
            ..Self::default()
        }
    }
    pub fn text(line: String) -> Self {
        Self {
            title: line,
            height: 24.,
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub struct Menu {
    pub title: String,
    pub description: String,
    pub description_lines: Vec<String>,
    pub rows: Vec<Row>,
    pub back: String,
    pub scroll: f32,
    pub home: bool,
    pub settings: Option<usize>,
    pub over_game: bool,
    pub anchor: Option<Rect>,
    pub cutouts: Vec<Rect>,
    pointer: Option<(i32, f32, f32, bool)>,
    slider_pointer: Option<(i32, usize)>,
    slider_change: Option<(usize, u8)>,
}

impl Menu {
    pub const SECONDARY: usize = 1 << 20;
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        rows: Vec<Row>,
        back: &str,
    ) -> Self {
        let description = description.into();
        Self {
            title: title.into(),
            description_lines: if description.is_empty() {
                Vec::new()
            } else {
                vec![description.clone()]
            },
            description,
            rows,
            back: back.into(),
            ..Self::default()
        }
    }

    pub fn column(width: f32) -> Rect {
        let w = (width - 32.).clamp(0., 720.);
        Rect::new((width - w) / 2., 0., w, 0.)
    }

    pub fn surface(&self, width: f32, height: f32) -> Rect {
        if let Some(anchor) = self.anchor {
            let w = (width - 24.).clamp(0., 320.);
            let h = (48. + 16. + self.content_height(w)).min((height - 24.).max(0.));
            let panel = Rect::new(
                (anchor.x + anchor.w / 2. - w / 2.).clamp(12., (width - w - 12.).max(12.)),
                (anchor.y + anchor.h + 8.).clamp(8., (height - h - 8.).max(8.)),
                w,
                h,
            );
            if !self.cutouts.iter().any(|r| panel.overlaps(*r)) {
                return panel;
            }
            // An anchored volume panel can reach a side camera even though
            // the centered settings sheet does not. Slide it clear locally.
            let candidates = self.cutouts.iter().flat_map(|c| {
                [
                    Rect::new(c.x - w - 8., panel.y, w, h),
                    Rect::new(c.x + c.w + 8., panel.y, w, h),
                    Rect::new(panel.x, c.y - h - 8., w, h),
                    Rect::new(panel.x, c.y + c.h + 8., w, h),
                ]
            });
            return candidates
                .filter(|r| {
                    r.x >= 0.
                        && r.y >= 0.
                        && r.x + r.w <= width
                        && r.y + r.h <= height
                        && !self.cutouts.iter().any(|c| r.overlaps(*c))
                })
                .min_by(|a, b| {
                    ((a.x - panel.x).hypot(a.y - panel.y))
                        .total_cmp(&(b.x - panel.x).hypot(b.y - panel.y))
                })
                .unwrap_or(panel);
        }
        if !self.over_game {
            return Rect::new(0., 0., width, height);
        }
        let w = (width - 24.).clamp(0., 720.);
        let header = 72.
            + if self.description_lines.is_empty() {
                0.
            } else {
                12. + self.description_lines.len().min(4) as f32 * 20.
            };
        let footer = if self.has_footer() { 76. } else { 24. };
        let h = (header + footer + self.content_height(w))
            .min((height - 24.).max(0.))
            .min(height * 0.86);
        Rect::new((width - w) / 2., (height - h) / 2., w, h)
    }

    fn card_columns(width: f32) -> usize {
        ((Self::column(width).w + 12.) / 168.).floor().clamp(1., 4.) as usize
    }
    pub fn content_height(&self, width: f32) -> f32 {
        if self.home {
            let lines = self.rows.len().div_ceil(Self::card_columns(width));
            (lines as f32 * 176. - 12.).max(0.)
        } else {
            self.rows.iter().map(|r| r.height).sum()
        }
    }
    pub fn row_rect(&self, index: usize, width: f32, height: f32) -> Rect {
        let viewport = self.viewport(width, height);
        if self.home {
            let columns = Self::card_columns(width);
            let card = (viewport.w - (columns - 1) as f32 * 12.) / columns as f32;
            Rect::new(
                viewport.x + (index % columns) as f32 * (card + 12.),
                viewport.y - self.scroll + (index / columns) as f32 * 176.,
                card,
                164.,
            )
        } else {
            Rect::new(
                viewport.x,
                viewport.y - self.scroll
                    + self.rows.iter().take(index).map(|r| r.height).sum::<f32>(),
                viewport.w,
                self.rows.get(index).map_or(0., |r| r.height),
            )
        }
    }
    pub fn secondary_rect(&self, index: usize, width: f32, height: f32) -> Rect {
        let r = self.row_rect(index, width, height);
        Rect::new(r.x + r.w - 48., r.y, 48., if self.home { 48. } else { r.h })
    }
    fn row_at(&self, x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
        let viewport = self.viewport(width, height);
        if !viewport.contains(x, y) {
            return None;
        }
        if self.home {
            let columns = Self::card_columns(width);
            let span = (viewport.w + 12.) / columns as f32;
            let col = ((x - viewport.x) / span) as usize;
            let line = ((y - viewport.y + self.scroll).max(0.) / 176.) as usize;
            let index = line * columns + col;
            (index < self.rows.len() && self.row_rect(index, width, height).contains(x, y))
                .then_some(index)
        } else {
            let target = y - viewport.y + self.scroll;
            let mut offset = 0.;
            self.rows.iter().position(|r| {
                offset += r.height;
                target < offset
            })
        }
    }

    pub fn viewport(&self, width: f32, height: f32) -> Rect {
        let column = Self::column(width);
        let top = if self.anchor.is_some() { 48. } else { 72. }
            + if self.description_lines.is_empty() {
                0.
            } else {
                12. + self.description_lines.len().min(4) as f32 * 20.
            };
        let bottom = if self.has_footer() { 76. } else { 12. };
        Rect::new(column.x, top, column.w, (height - top - bottom).max(0.))
    }

    pub fn has_footer(&self) -> bool {
        self.anchor.is_none()
            && !self.home
            && !matches!(
                self.back.as_str(),
                "Back" | "Back to games" | "Back to game" | "Done"
            )
    }
    pub fn navigation_rect(width: f32) -> Rect {
        Rect::new(Self::column(width).x - 8., 8., 48., 48.)
    }
    pub fn add_rect(width: f32) -> Rect {
        let column = Self::column(width);
        Rect::new(column.x + column.w - 104., 8., 48., 48.)
    }
    pub fn settings_rect(width: f32) -> Rect {
        let column = Self::column(width);
        Rect::new(column.x + column.w - 48., 8., 48., 48.)
    }

    pub fn back_rect(width: f32, height: f32) -> Rect {
        let column = Self::column(width);
        let w = column.w.min(240.);
        Rect::new((width - w) / 2., height - 60., w, 48.)
    }

    pub fn clamp_scroll(&mut self, width: f32, height: f32) {
        self.scroll = self.scroll.clamp(
            0.,
            (self.content_height(width) - self.viewport(width, height).h).max(0.),
        );
    }

    pub fn pressed_row(&self, width: f32, height: f32) -> Option<usize> {
        let (_, x, y, moved) = self.pointer?;
        if moved {
            return None;
        }
        self.row_at(x, y, width, height)
    }

    pub fn slider_rect(&self, index: usize, width: f32, height: f32) -> Rect {
        let viewport = self.viewport(width, height);
        let top =
            viewport.y - self.scroll + self.rows.iter().take(index).map(|r| r.height).sum::<f32>();
        Rect::new(viewport.x + 16., top + 48., viewport.w - 32., 48.)
    }

    fn slide(&mut self, id: i32, x: f32, width: f32, height: f32) {
        if let Some((active, index)) = self.slider_pointer {
            if id == active {
                let r = self.slider_rect(index, width, height);
                self.rows[index].slider = Some(
                    (((x - r.x - 8.) / (r.w - 16.).max(1.)).clamp(0., 1.) * 100.).round() as u8,
                );
                self.slider_change = self.rows[index].slider.map(|value| (index, value));
            }
        }
    }

    pub fn take_slider_change(&mut self) -> Option<(usize, u8)> {
        self.slider_change.take()
    }

    pub fn down(&mut self, id: i32, x: f32, y: f32, width: f32, height: f32) {
        if self.pointer.is_none() && self.slider_pointer.is_none() {
            if self.viewport(width, height).contains(x, y) {
                for (index, row) in self.rows.iter().enumerate() {
                    if row.slider.is_some()
                        && row.enabled
                        && self.slider_rect(index, width, height).contains(x, y)
                    {
                        self.slider_pointer = Some((id, index));
                        self.slide(id, x, width, height);
                        return;
                    }
                }
            }
            self.pointer = Some((id, x, y, false));
        }
    }

    pub fn motion(&mut self, id: i32, x: f32, y: f32, width: f32, height: f32) {
        if self.slider_pointer.is_some() {
            self.slide(id, x, width, height);
            return;
        }
        if let Some((active, start_x, last_y, moved)) = self.pointer.as_mut() {
            if *active == id {
                if (*start_x - x).abs() > 10. || (*last_y - y).abs() > 10. {
                    *moved = true;
                }
                if *moved {
                    self.scroll += *last_y - y;
                    *last_y = y;
                }
            }
        }
        self.clamp_scroll(width, height);
    }

    /// MAX is the primary action; MAX-1 is back; MAX-2 dismisses a game sheet.
    pub fn up(&mut self, id: i32, x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
        if let Some((active, index)) = self.slider_pointer {
            if id == active {
                self.slide(id, x, width, height);
                self.slider_change = self.rows[index].slider.map(|value| (index, value));
                self.slider_pointer = None;
            }
            return None;
        }
        let (active, start_x, start_y, moved) = self.pointer?;
        if active != id {
            return None;
        }
        self.pointer = None;
        if moved || (start_x - x).abs() > 10. || (start_y - y).abs() > 10. {
            return None;
        }
        if self.over_game
            && !Rect::new(0., 0., width, height).contains(start_x, start_y)
            && !Rect::new(0., 0., width, height).contains(x, y)
        {
            return Some(usize::MAX - 2);
        }
        if self.anchor.is_some() && Self::settings_rect(width).contains(x, y) {
            return Some(usize::MAX - 2);
        }
        if self.home {
            if Self::settings_rect(width).contains(x, y) {
                return self.settings;
            }
            if Self::add_rect(width).contains(x, y) {
                return Some(usize::MAX);
            }
        } else if self.anchor.is_none() && Self::navigation_rect(width).contains(x, y) {
            return Some(usize::MAX - 1);
        }
        if self.has_footer() && Self::back_rect(width, height).contains(x, y) {
            return Some(usize::MAX);
        }
        if let Some(index) = self.row_at(x, y, width, height) {
            let row = &self.rows[index];
            if row.enabled
                && row.secondary
                && self.secondary_rect(index, width, height).contains(x, y)
            {
                return Some(Self::SECONDARY + index);
            }
            return (row.enabled && row.slider.is_none()).then_some(index);
        }
        None
    }

    pub fn cancel(&mut self) {
        self.pointer = None;
        self.slider_pointer = None;
        self.slider_change = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_cards_share_render_hit_scroll_and_options_geometry() {
        for width in [360., 443., 600., 852.] {
            let height = 380.;
            let rows = (0..11)
                .map(|i| {
                    let mut r = Row::action(format!("Game {i}"), "PlayStation");
                    r.secondary = true;
                    r
                })
                .collect();
            let mut menu = Menu::new("Games", "", rows, "Back");
            menu.home = true;
            let first = menu.row_rect(0, width, height);
            let second = menu.row_rect(1, width, height);
            assert_eq!(first.y, second.y);
            assert!(first.x + first.w < second.x);
            let (x, y) = (second.x + 12., second.y + 90.);
            menu.down(1, x, y, width, height);
            assert_eq!(menu.pressed_row(width, height), Some(1));
            assert_eq!(menu.up(2, x, y, width, height), None);
            assert_eq!(menu.up(1, x, y, width, height), Some(1));
            let options = menu.secondary_rect(1, width, height);
            menu.down(1, options.x + 24., options.y + 24., width, height);
            assert_eq!(
                menu.up(1, options.x + 24., options.y + 24., width, height),
                Some(Menu::SECONDARY + 1)
            );
            assert!(menu
                .row_at(first.x + first.w + 6., first.y + 80., width, height)
                .is_none());
            menu.scroll = 10000.;
            menu.clamp_scroll(width, height);
            let last = menu.row_rect(10, width, height);
            assert!(
                (last.y + last.h - menu.viewport(width, height).y - menu.viewport(width, height).h)
                    .abs()
                    < 0.01
            );
            assert_eq!(
                menu.row_at(last.x + 12., last.y + 80., width, height),
                Some(10)
            );
            menu.clamp_scroll(852., height);
            assert!(
                menu.scroll <= (menu.content_height(852.) - menu.viewport(852., height).h).max(0.)
            );
        }
    }
    #[test]
    fn side_camera_moves_the_volume_panel_without_losing_live_slider_input() {
        let mut row = Row::action("Game volume", "");
        row.slider = Some(75);
        row.height = 104.;
        let mut menu = Menu::new("Volume", "", vec![row], "Back");
        menu.anchor = Some(Rect::new(840., 4., 48., 48.));
        let camera = Rect::new(886., 198., 75., 46.);
        menu.cutouts = vec![camera];
        let panel = menu.surface(961., 419.);
        assert!(!panel.overlaps(camera));
        assert!(
            panel.x >= 0.
                && panel.y >= 0.
                && panel.x + panel.w <= 961.
                && panel.y + panel.h <= 419.
        );
        let slider = menu.slider_rect(0, panel.w, panel.h);
        menu.down(
            4,
            slider.x + slider.w - 8.,
            slider.y + 24.,
            panel.w,
            panel.h,
        );
        assert_eq!(menu.take_slider_change(), Some((0, 100)));
    }
    #[test]
    fn slider_updates_during_drag_and_cancel_keeps_the_last_listening_level() {
        let mut row = Row::action("Game volume", "");
        row.slider = Some(70);
        row.height = 104.;
        let mut menu = Menu::new("Sound", "", vec![row], "Back");
        let r = menu.slider_rect(0, 420., 800.);
        menu.down(4, r.x + 8., r.y + 24., 420., 800.);
        menu.motion(4, r.x + r.w - 8., r.y + 24., 420., 800.);
        assert_eq!(menu.rows[0].slider, Some(100));
        assert_eq!(menu.take_slider_change(), Some((0, 100)));
        assert_eq!(menu.up(5, r.x, r.y, 420., 800.), None);
        menu.cancel();
        assert_eq!(menu.rows[0].slider, Some(100));
        menu.down(4, r.x + 8., r.y + 24., 420., 800.);
        assert_eq!(menu.up(4, r.x + 8., r.y + 24., 420., 800.), None);
        assert_eq!(menu.take_slider_change(), Some((0, 0)));
        assert_eq!(menu.take_slider_change(), None);
        assert_eq!(menu.scroll, 0.);
    }

    #[test]
    fn live_game_sheets_leave_picture_visible_and_dismiss_only_after_lift() {
        let mut menu = Menu::new("Sound", "", vec![Row::action("Mute", "")], "Back");
        menu.over_game = true;
        let panel = menu.surface(852., 817.);
        assert!((panel.y + panel.h / 2. - 817. / 2.).abs() < 0.01);
        assert!(panel.w < 852.);
        menu.down(3, 50., -50., panel.w, panel.h);
        assert_eq!(menu.up(9, 50., -50., panel.w, panel.h), None);
        assert_eq!(
            menu.up(3, 50., -50., panel.w, panel.h),
            Some(usize::MAX - 2)
        );
    }

    #[test]
    fn scrolling_and_second_fingers_do_not_activate_rows() {
        let mut menu = Menu::new(
            "Games",
            "",
            (0..20).map(|i| Row::action(i.to_string(), "")).collect(),
            "Add selected",
        );
        menu.down(4, 100., 250., 420., 800.);
        assert_eq!(menu.up(9, 100., 250., 420., 800.), None);
        menu.motion(4, 100., 180., 420., 800.);
        assert!(menu.scroll > 0.);
        assert_eq!(menu.up(4, 100., 180., 420., 800.), None);
        menu.down(4, 100., 760., 420., 800.);
        assert_eq!(menu.up(4, 100., 760., 420., 800.), Some(usize::MAX));
    }
    #[test]
    fn app_bar_navigation_is_distinct_from_the_primary_action() {
        let mut menu = Menu::new("Choose games", "", vec![], "Add selected");
        let back = Menu::navigation_rect(420.);
        menu.down(1, back.x + 24., back.y + 24., 420., 800.);
        assert_eq!(
            menu.up(1, back.x + 24., back.y + 24., 420., 800.),
            Some(usize::MAX - 1)
        );
        menu.home = true;
        menu.settings = Some(7);
        let settings = Menu::settings_rect(420.);
        menu.down(1, settings.x + 24., settings.y + 24., 420., 800.);
        assert_eq!(
            menu.up(1, settings.x + 24., settings.y + 24., 420., 800.),
            Some(7)
        );
        let add = Menu::add_rect(420.);
        menu.down(1, add.x + 24., add.y + 24., 420., 800.);
        assert_eq!(
            menu.up(1, add.x + 24., add.y + 24., 420., 800.),
            Some(usize::MAX)
        );
    }
}
