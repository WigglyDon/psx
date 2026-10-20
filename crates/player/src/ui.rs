//! Small native menu model. No platform handles or emulation ownership.
use crate::artwork::Artwork;
use crate::input::{DpadFeedback, Layout, Preset, Rect};
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
    group_actions: bool,
    original_actions: Vec<(usize, Rect)>,
    cutouts: Vec<Rect>,
    pointer: Option<EditorPointer>,
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
}

#[derive(Clone, Copy, PartialEq)]
pub enum EditorButton {
    Next,
    Group,
    Individual,
    Reset,
    Cancel,
    Done,
}
impl EditorButton {
    pub fn label(self) -> &'static str {
        match self {
            Self::Next => "Next",
            Self::Group => "Group",
            Self::Individual => "Individual",
            Self::Reset => "Reset",
            Self::Cancel => "Cancel",
            Self::Done => "Done",
        }
    }
}
impl EditorPointer {
    fn id(self) -> i32 {
        match self {
            Self::Move { id, .. } | Self::Resize { id, .. } | Self::Button { id, .. } => id,
        }
    }
}

pub enum EditResult {
    Done,
    Cancel,
}

impl Editor {
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
            group_actions: true,
            original_actions: Vec::new(),
            cutouts,
            pointer: None,
        }
    }
    pub fn preview(&self) -> Rect {
        Rect::new(24., 124., self.width - 48., (self.height - 276.).max(24.))
            .fit(self.width / self.height)
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
        let bottom = (self.width - 40.) / 2.;
        let mut buttons = vec![
            (
                Rect::new(self.width - 96., 16., 80., 48.),
                EditorButton::Next,
            ),
            (
                Rect::new(16., self.height - 64., bottom, 52.),
                EditorButton::Cancel,
            ),
            (
                Rect::new(24. + bottom, self.height - 64., bottom, 52.),
                EditorButton::Done,
            ),
        ];
        if self.action_group_selected() || self.action_selected(self.selected) {
            buttons.push((
                Rect::new(self.width - 228., 16., 124., 48.),
                if self.action_group_selected() {
                    EditorButton::Individual
                } else {
                    EditorButton::Group
                },
            ));
        }
        if !self.selected_is_default() {
            buttons.push((
                Rect::new(16., self.height - 128., self.width - 32., 48.),
                EditorButton::Reset,
            ));
        }
        buttons
    }
    pub fn selected_bounds(&self) -> Rect {
        if self.action_group_selected() {
            self.layout.action_bounds()
        } else if self.selected < self.layout.element_count() {
            self.layout.element(self.selected)
        } else {
            self.layout.game
        }
    }
    pub fn selected_is_default(&self) -> bool {
        if self.action_group_selected() {
            let defaults = self.defaults();
            return self
                .layout
                .action_elements()
                .all(|i| self.layout.element(i).same_bounds(defaults.element(i)));
        }
        let expected = if self.selected < self.layout.element_count() {
            self.defaults().element(self.selected)
        } else {
            let mut reset = self.preset.clone();
            reset.game_scale = 1.;
            reset.game_offset = [0.5, 0.5];
            reset
                .layout_with_cutouts(self.width, self.height, &self.cutouts)
                .game
        };
        self.selected_bounds().same_bounds(expected)
    }
    fn defaults(&self) -> Layout {
        Preset {
            analog: self.preset.analog,
            ..Preset::default()
        }
        .layout_with_cutouts(self.width, self.height, &self.cutouts)
    }
    fn action_selected(&self, index: usize) -> bool {
        self.layout.action_elements().any(|i| i == index)
    }
    fn action_group_selected(&self) -> bool {
        self.selected == self.layout.element_count() + 1
    }
    fn picture_selected(&self) -> bool {
        self.selected == self.layout.element_count()
    }
    fn set_selected_bounds(&mut self, original: Rect, rect: Rect) {
        if self.action_group_selected() {
            for &(i, member) in &self.original_actions {
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
    pub fn cutouts(&self) -> &[Rect] {
        &self.cutouts
    }
    /// Visible corner grips share the same 48-point touch areas as rendering.
    pub fn handles(&self) -> [(f32, f32); 4] {
        let r = self.map(self.selected_bounds());
        // Small preview controls need room between corner touch areas and the
        // movable center. Outset their grips rather than stealing center drags.
        let outset = ((56. - r.w.min(r.h)) / 2.).max(0.);
        [
            (r.x - outset, r.y - outset),
            (r.x + r.w + outset, r.y - outset),
            (r.x + r.w + outset, r.y + r.h + outset),
            (r.x - outset, r.y + r.h + outset),
        ]
    }
    pub fn selected_name(&self) -> &str {
        if self.action_group_selected() {
            "Action buttons"
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
        self.original_actions = self
            .layout
            .action_elements()
            .map(|i| (i, self.layout.element(i)))
            .collect();
        if let Some(corner) = self
            .handles()
            .iter()
            .position(|(hx, hy)| (x - hx).abs() <= 24. && (y - hy).abs() <= 24.)
        {
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
            self.selected = if self.group_actions && self.action_selected(index) {
                self.layout.element_count() + 1
            } else {
                index
            };
        } else if self.group_actions && self.map(self.layout.action_bounds()).contains(x, y) {
            self.selected = self.layout.element_count() + 1;
        } else if self.map(self.layout.game).contains(x, y) {
            self.selected = self.layout.element_count();
        } else {
            return;
        }
        self.guides = [None; 2];
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
            let area = full
                .layout_with_cutouts(self.width, self.height, &self.cutouts)
                .game;
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
        self.layout = self
            .preset
            .layout_with_cutouts(self.width, self.height, &self.cutouts);
        // Keep the drag's unsnapped origin. Incrementing the snapped position
        // would trap slow finger movement at a guide.
    }
    fn snap_move(&mut self, mut r: Rect, area: Rect) -> Rect {
        let threshold = 6. / (self.preview().w / self.width);
        let defaults = self.defaults();
        let mut references = vec![if self.action_group_selected() {
            defaults.action_bounds()
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
                            && !(self.action_group_selected() && self.action_selected(*i))
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
            let mut position = start;
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
            full.layout(self.width, self.height).game
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
        } else if self.action_group_selected() {
            self.original_actions
                .iter()
                .map(|(_, r)| (24. / r.w).max(24. / r.h))
                .fold(0_f32, f32::max)
        } else {
            (24. / original.w).max(24. / original.h)
        };
        let maximum = (available_w / original.w)
            .min(available_h / original.h)
            .max(minimum);
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
        self.layout = self
            .preset
            .layout_with_cutouts(self.width, self.height, &self.cutouts);
    }
    pub fn up(&mut self, id: i32, x: f32, y: f32) -> Option<EditResult> {
        let pointer = self.pointer?;
        if pointer.id() != id {
            return None;
        }
        self.pointer = None;
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
                    .filter(|i| !self.group_actions || !self.action_selected(*i))
                    .collect();
                if self.group_actions {
                    selections.insert(1, self.layout.element_count() + 1);
                }
                let current = selections
                    .iter()
                    .position(|i| *i == self.selected)
                    .unwrap_or(0);
                self.selected = selections[(current + 1) % selections.len()];
            }
            EditorButton::Group => {
                self.group_actions = true;
                self.selected = self.layout.element_count() + 1;
            }
            EditorButton::Individual => {
                self.group_actions = false;
                self.selected = self.layout.action_elements().next().unwrap_or(0);
            }
            EditorButton::Reset => {
                if self.action_group_selected() {
                    let defaults = self.defaults();
                    let members: Vec<_> = self.layout.action_elements().collect();
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
                self.layout =
                    self.preset
                        .layout_with_cutouts(self.width, self.height, &self.cutouts);
            }
            EditorButton::Cancel => return Some(EditResult::Cancel),
            EditorButton::Done => return Some(EditResult::Done),
        }
        None
    }
    pub fn cancel_touch(&mut self) {
        self.pointer = None;
        self.guides = [None; 2];
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;

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
        editor.group_actions = false;
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
    fn small_preview_controls_can_move_from_the_center() {
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
        let w = (width - 24.).clamp(0., 600.);
        let header = 72.
            + if self.description_lines.is_empty() {
                0.
            } else {
                12. + self.description_lines.len().min(4) as f32 * 20.
            };
        let footer = if self.has_footer() { 76. } else { 24. };
        let h = (header + footer + self.content_height(w))
            .min((height - 24.).max(0.))
            .min(height * 0.72);
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
