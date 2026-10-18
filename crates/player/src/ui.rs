//! Small native menu model. No platform handles or emulation ownership.
use crate::artwork::Artwork;
use crate::input::{Layout, Preset, Rect};

pub const ROW_HEIGHT: f32 = 72.;

#[derive(Default)]
pub struct Row {
    pub title: String,
    pub detail: String,
    pub checked: Option<bool>,
    pub enabled: bool,
    pub height: f32,
    pub artwork: Option<Artwork>,
    pub toggle: Option<bool>,
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
        button: EditorButton,
    },
}

#[derive(Clone, Copy, PartialEq)]
pub enum EditorButton {
    Next,
    Reset,
    Cancel,
    Done,
}
impl EditorButton {
    pub fn label(self) -> &'static str {
        match self {
            Self::Next => "Next",
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
        if !self.selected_is_default() {
            buttons.push((
                Rect::new(16., self.height - 128., self.width - 32., 48.),
                EditorButton::Reset,
            ));
        }
        buttons
    }
    fn selected_bounds(&self) -> Rect {
        if self.selected < self.layout.element_count() {
            self.layout.element(self.selected)
        } else {
            self.layout.game
        }
    }
    pub fn selected_is_default(&self) -> bool {
        let expected = if self.selected < self.layout.element_count() {
            Layout::new(self.width, self.height).element(self.selected)
        } else {
            let mut reset = self.preset.clone();
            reset.game_scale = 1.;
            reset.game_offset = [0.5, 0.5];
            reset.layout(self.width, self.height).game
        };
        self.selected_bounds().same_bounds(expected)
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
        if let Some((_, button)) = self.buttons().into_iter().find(|(r, _)| r.contains(x, y)) {
            self.pointer = Some(EditorPointer::Button { id, button });
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
                self.selected = (self.selected + 1) % (self.layout.element_count() + 1)
            }
            EditorButton::Reset => {
                if self.selected < self.layout.element_count() {
                    self.layout.set_element(
                        self.selected,
                        Layout::new(self.width, self.height).element(self.selected),
                    );
                    self.preset.capture(&self.layout, self.width, self.height);
                } else {
                    self.preset.game_scale = 1.;
                    self.preset.game_offset = [0.5, 0.5];
                }
                self.layout = self.preset.layout(self.width, self.height);
            }
            EditorButton::Cancel => return Some(EditResult::Cancel),
            EditorButton::Done => return Some(EditResult::Done),
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
    pointer: Option<(i32, f32, f32, bool)>,
}

impl Menu {
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

    pub fn viewport(&self, width: f32, height: f32) -> Rect {
        let column = Self::column(width);
        let top = 72.
            + if self.description_lines.is_empty() {
                0.
            } else {
                12. + self.description_lines.len().min(4) as f32 * 20.
            };
        let bottom = if self.has_footer() { 76. } else { 12. };
        Rect::new(column.x, top, column.w, (height - top - bottom).max(0.))
    }

    pub fn has_footer(&self) -> bool {
        !self.home && !matches!(self.back.as_str(), "Back" | "Back to games")
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
        let w = column.w.min(320.);
        Rect::new((width - w) / 2., height - 60., w, 48.)
    }

    pub fn clamp_scroll(&mut self, width: f32, height: f32) {
        self.scroll = self.scroll.clamp(
            0.,
            (self.rows.iter().map(|r| r.height).sum::<f32>() - self.viewport(width, height).h)
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

    /// MAX is the primary action; MAX-1 is app-bar back. Other values are rows.
    pub fn up(&mut self, id: i32, x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
        let (active, start_x, start_y, moved) = self.pointer?;
        if active != id {
            return None;
        }
        self.pointer = None;
        if moved || (start_x - x).abs() > 10. || (start_y - y).abs() > 10. {
            return None;
        }
        if self.home {
            if Self::settings_rect(width).contains(x, y) {
                return self.settings;
            }
            if Self::add_rect(width).contains(x, y) {
                return Some(usize::MAX);
            }
        } else if Self::navigation_rect(width).contains(x, y) {
            return Some(usize::MAX - 1);
        }
        if self.has_footer() && Self::back_rect(width, height).contains(x, y) {
            return Some(usize::MAX);
        }
        let viewport = self.viewport(width, height);
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
            "Done",
        );
        menu.down(4, 100., 250.);
        assert_eq!(menu.up(9, 100., 250., 420., 800.), None);
        menu.motion(4, 100., 180., 420., 800.);
        assert!(menu.scroll > 0.);
        assert_eq!(menu.up(4, 100., 180., 420., 800.), None);
        menu.down(4, 100., 760.);
        assert_eq!(menu.up(4, 100., 760., 420., 800.), Some(usize::MAX));
    }
    #[test]
    fn app_bar_navigation_is_distinct_from_the_primary_action() {
        let mut menu = Menu::new("Choose games", "", vec![], "Add selected");
        let back = Menu::navigation_rect(420.);
        menu.down(1, back.x + 24., back.y + 24.);
        assert_eq!(
            menu.up(1, back.x + 24., back.y + 24., 420., 800.),
            Some(usize::MAX - 1)
        );
        menu.home = true;
        menu.settings = Some(7);
        let settings = Menu::settings_rect(420.);
        menu.down(1, settings.x + 24., settings.y + 24.);
        assert_eq!(
            menu.up(1, settings.x + 24., settings.y + 24., 420., 800.),
            Some(7)
        );
        let add = Menu::add_rect(420.);
        menu.down(1, add.x + 24., add.y + 24.);
        assert_eq!(
            menu.up(1, add.x + 24., add.y + 24., 420., 800.),
            Some(usize::MAX)
        );
    }
}
