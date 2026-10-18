//! Small native menu model. No platform handles or emulation ownership.
use crate::artwork::Artwork;
use crate::input::{Layout, Preset, Rect};

pub const ROW_HEIGHT: f32 = 88.;

#[derive(Default)]
pub struct Row {
    pub title: String,
    pub detail: String,
    pub checked: Option<bool>,
    pub enabled: bool,
    pub height: f32,
    pub artwork: Option<Artwork>,
}

/// Visual response never delays logical input. Bounded interpolation keeps
/// frame-rate-independent press/release and menu fades, including reversals.
#[derive(Default)]
pub struct Visuals {
    pub presses: [f32; 16],
    pub overlay: f32,
    pub time: f64,
    pulses: [f32; 16],
}
impl Visuals {
    pub fn trigger(&mut self, pressed: u16) {
        for (i, pulse) in self.pulses.iter_mut().enumerate() {
            if pressed & (1 << i) != 0 {
                *pulse = 0.04;
            }
        }
    }
    pub fn advance(&mut self, dt: f32, buttons: u16, menu: bool, animated_icons: bool) -> bool {
        let dt = dt.clamp(0., 0.1);
        let previous_icon_frames = ((self.time / 0.32) as u64, (self.time / 0.22) as u64);
        self.time += dt as f64;
        let mut active = false;
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
    fn press_and_menu_fades_reverse_without_jumping() {
        let mut visual = Visuals::default();
        assert!(visual.advance(0.016, 1, true, false));
        assert!(visual.presses[0] > 0. && visual.presses[0] < 1.);
        assert!(visual.overlay > 0. && visual.overlay < 1.);
        let pressed = visual.presses[0];
        visual.advance(0.016, 0, false, false);
        assert!(visual.presses[0] < pressed && visual.presses[0] > 0.);
        for _ in 0..60 {
            visual.advance(0.016, 0, false, false);
        }
        assert_eq!(visual.presses[0], 0.);
        assert_eq!(visual.overlay, 0.);
    }
}

pub struct Editor {
    pub preset: Preset,
    pub layout: Layout,
    pub selected: usize,
    pub width: f32,
    pub height: f32,
    pointer: Option<EditorPointer>,
}

#[derive(Clone, Copy)]
enum EditorPointer {
    Move {
        id: i32,
        x: f32,
        y: f32,
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
        index: usize,
    },
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
        let layout = preset.layout(width, height);
        Self {
            preset,
            layout,
            selected: 0,
            width,
            height,
            pointer: None,
        }
    }
    pub fn preview(&self) -> Rect {
        Rect::new(24., 124., self.width - 48., (self.height - 212.).max(24.))
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
    pub fn buttons(&self) -> [Rect; 3] {
        let bottom = (self.width - 40.) / 2.;
        [
            Rect::new(self.width - 96., 16., 80., 48.),
            Rect::new(16., self.height - 64., bottom, 52.),
            Rect::new(24. + bottom, self.height - 64., bottom, 52.),
        ]
    }
    fn selected_bounds(&self) -> Rect {
        if self.selected < self.layout.element_count() {
            self.layout.element(self.selected)
        } else {
            self.layout.game
        }
    }
    /// Visible corner grips share the same 48-point touch areas as rendering.
    pub fn handles(&self) -> [(f32, f32); 4] {
        let r = self.map(self.selected_bounds());
        [
            (r.x, r.y),
            (r.x + r.w, r.y),
            (r.x + r.w, r.y + r.h),
            (r.x, r.y + r.h),
        ]
    }
    pub fn selected_name(&self) -> &str {
        if self.selected == self.layout.element_count() {
            "Game picture"
        } else {
            self.layout.element_name(self.selected)
        }
    }
    pub fn down(&mut self, id: i32, x: f32, y: f32) {
        if self.pointer.is_some() {
            return;
        }
        if let Some(index) = self.buttons().iter().position(|r| r.contains(x, y)) {
            self.pointer = Some(EditorPointer::Button { id, index });
            return;
        }
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
        if let Some(index) = (0..self.layout.element_count())
            .rev()
            .find(|i| self.map(self.layout.element(*i)).contains(x, y))
        {
            self.selected = index;
        } else if self.map(self.layout.game).contains(x, y) {
            self.selected = self.layout.element_count();
        } else {
            return;
        }
        self.pointer = Some(EditorPointer::Move { id, x, y });
    }
    pub fn motion(&mut self, id: i32, x: f32, y: f32) {
        let Some(pointer) = self.pointer else {
            return;
        };
        if pointer.id() != id {
            return;
        }
        let scale = self.preview().w / self.width;
        let (previous_x, previous_y) = match pointer {
            EditorPointer::Move { x, y, .. } => (x, y),
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
        if self.selected < self.layout.element_count() {
            let mut r = self.layout.element(self.selected);
            r.x = (r.x + dx).clamp(0., self.width - r.w);
            r.y = (r.y + dy).clamp(0., self.height - r.h);
            self.layout.set_element(self.selected, r);
            self.preset.capture(&self.layout, self.width, self.height);
        } else {
            let mut full = self.preset.clone();
            full.game_scale = 1.;
            let area = full.layout(self.width, self.height).game;
            let slack_x = area.w - self.layout.game.w;
            let slack_y = area.h - self.layout.game.h;
            if slack_x > 0.1 {
                self.preset.game_offset[0] =
                    (self.preset.game_offset[0] + dx / slack_x).clamp(0., 1.);
            }
            if slack_y > 0.1 {
                self.preset.game_offset[1] =
                    (self.preset.game_offset[1] + dy / slack_y).clamp(0., 1.);
            }
        }
        self.layout = self.preset.layout(self.width, self.height);
        self.pointer = Some(EditorPointer::Move { id, x, y });
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
        let game = self.selected == self.layout.element_count();
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
            self.layout.set_element(self.selected, resized);
            self.preset.capture(&self.layout, self.width, self.height);
        }
        self.layout = self.preset.layout(self.width, self.height);
    }
    pub fn up(&mut self, id: i32, x: f32, y: f32) -> Option<EditResult> {
        let pointer = self.pointer?;
        if pointer.id() != id {
            return None;
        }
        self.pointer = None;
        let EditorPointer::Button { index, .. } = pointer else {
            return None;
        };
        if !self.buttons()[index].contains(x, y) {
            return None;
        }
        match index {
            0 => self.selected = (self.selected + 1) % (self.layout.element_count() + 1),
            1 => return Some(EditResult::Cancel),
            2 => return Some(EditResult::Done),
            _ => {}
        }
        None
    }
    pub fn cancel_touch(&mut self) {
        self.pointer = None;
    }
}

#[cfg(test)]
mod editor_tests {
    use super::*;

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
            height: 28.,
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub struct Menu {
    pub title: String,
    pub description: String,
    pub rows: Vec<Row>,
    pub back: String,
    pub scroll: f32,
    pointer: Option<(i32, f32, f32, bool)>,
}

impl Menu {
    pub fn new(
        title: impl Into<String>,
        description: impl Into<String>,
        rows: Vec<Row>,
        back: &str,
    ) -> Self {
        Self {
            title: title.into(),
            description: description.into(),
            rows,
            back: back.into(),
            ..Self::default()
        }
    }

    pub fn column(width: f32) -> Rect {
        let w = (width - 32.).clamp(0., 680.);
        Rect::new((width - w) / 2., 0., w, 0.)
    }

    pub fn viewport(width: f32, height: f32) -> Rect {
        let column = Self::column(width);
        Rect::new(column.x, 164., column.w, (height - 244.).max(0.))
    }

    pub fn back_rect(width: f32, height: f32) -> Rect {
        let column = Self::column(width);
        Rect::new(column.x, height - 64., column.w, 52.)
    }

    pub fn clamp_scroll(&mut self, width: f32, height: f32) {
        self.scroll = self.scroll.clamp(
            0.,
            (self.rows.iter().map(|r| r.height).sum::<f32>() - Self::viewport(width, height).h)
                .max(0.),
        );
    }

    pub fn down(&mut self, id: i32, x: f32, y: f32) {
        if self.pointer.is_none() {
            self.pointer = Some((id, x, y, false));
        }
    }

    pub fn motion(&mut self, id: i32, x: f32, y: f32, width: f32, height: f32) {
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

    /// usize::MAX is the persistent bottom navigation button.
    pub fn up(&mut self, id: i32, x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
        let (active, start_x, start_y, moved) = self.pointer?;
        if active != id {
            return None;
        }
        self.pointer = None;
        if moved || (start_x - x).abs() > 10. || (start_y - y).abs() > 10. {
            return None;
        }
        if Self::back_rect(width, height).contains(x, y) {
            return Some(usize::MAX);
        }
        let viewport = Self::viewport(width, height);
        if !viewport.contains(x, y) {
            return None;
        }
        let target = y - viewport.y + self.scroll;
        let mut offset = 0.;
        for (index, row) in self.rows.iter().enumerate() {
            offset += row.height;
            if target < offset {
                return row.enabled.then_some(index);
            }
        }
        None
    }

    pub fn cancel(&mut self) {
        self.pointer = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scrolling_and_second_fingers_do_not_activate_rows() {
        let mut menu = Menu::new(
            "Games",
            "",
            (0..20).map(|i| Row::action(i.to_string(), "")).collect(),
            "Back",
        );
        menu.down(4, 100., 250.);
        assert_eq!(menu.up(9, 100., 250., 420., 800.), None);
        menu.motion(4, 100., 180., 420., 800.);
        assert!(menu.scroll > 0.);
        assert_eq!(menu.up(4, 100., 180., 420., 800.), None);
        menu.down(4, 100., 760.);
        assert_eq!(menu.up(4, 100., 760., 420., 800.), Some(usize::MAX));
    }
}
