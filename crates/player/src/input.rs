use crate::saves::atomic_write;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fs, io, path::Path};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.w && y < self.y + self.h
    }
    pub fn fit(self, aspect: f32) -> Self {
        let w = self.w.min(self.h * aspect);
        let h = w / aspect;
        Self::new(self.x + (self.w - w) / 2., self.y + (self.h - h) / 2., w, h)
    }
    pub fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w - 0.01
            && self.x + self.w > other.x + 0.01
            && self.y < other.y + other.h - 0.01
            && self.y + self.h > other.y + 0.01
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub bounds: Rect,
    pub label: &'static str,
    pub mask: u16,
}
impl Control {
    pub fn contains(self, x: f32, y: f32) -> bool {
        if matches!(self.label, "TRI" | "O" | "X" | "SQ") {
            let radius = self.bounds.w / 2.;
            let dx = x - self.bounds.x - radius;
            let dy = y - self.bounds.y - radius;
            dx * dx + dy * dy <= radius * radius
        } else {
            self.bounds.contains(x, y)
        }
    }
}
#[derive(Debug)]
pub struct Layout {
    pub game: Rect,
    pub dpad: Rect,
    pub controls: Vec<Control>,
    pub pause: Rect,
}

impl Layout {
    /// Coordinates are density-independent points, with platform insets excluded.
    pub fn new(w: f32, h: f32) -> Self {
        // Default controls occupy their own dock. The picture never intersects
        // even a translucent control's rectangular touch target.
        let cluster = (w / 2. - 32.).clamp(112., 200.).min((h - 140.).max(112.));
        let size = (cluster * 0.36).clamp(48., 72.);
        let offset = (cluster - size) / 2.;
        let fy = h - 76. - cluster / 2.;
        let fx = w - 16. - cluster / 2.;
        let dpad = Rect::new(16., fy - cluster / 2., cluster, cluster);
        let pause = Rect::new(w / 2. - 28., h - 56., 56., 48.);
        let mut controls = Vec::with_capacity(10);
        let mut add = |x, y, label, id| {
            controls.push(Control {
                bounds: Rect::new(x, y, size, size),
                label,
                mask: 1u16 << id,
            })
        };
        add(fx - size / 2., fy - offset - size / 2., "TRI", 9);
        add(fx + offset - size / 2., fy - size / 2., "O", 8);
        add(fx - size / 2., fy + offset - size / 2., "X", 0);
        add(fx - offset - size / 2., fy - size / 2., "SQ", 1);
        let shoulder_width = (cluster - 8.) / 2.;
        for (x, label, id) in [
            (16., "L2", 12),
            (24. + shoulder_width, "L1", 10),
            (w - 16. - cluster, "R1", 11),
            (w - 16. - shoulder_width, "R2", 13),
        ] {
            controls.push(Control {
                bounds: Rect::new(x, dpad.y - 56., shoulder_width, 48.),
                label,
                mask: 1 << id,
            });
        }
        for (x, label, id) in [(w / 2. - 136., "SELECT", 2), (w / 2. + 40., "START", 3)] {
            controls.push(Control {
                bounds: Rect::new(x, h - 56., 96., 48.),
                label,
                mask: 1 << id,
            });
        }
        let mut layout = Self {
            game: Rect::default(),
            dpad,
            controls,
            pause,
        };
        layout.place_game(w, h, false, 1., [0.5, 0.5]);
        layout
    }

    pub fn element_count(&self) -> usize {
        self.controls.len() + 2
    }
    pub fn element(&self, index: usize) -> Rect {
        if index == 0 {
            self.dpad
        } else if index <= self.controls.len() {
            self.controls[index - 1].bounds
        } else {
            self.pause
        }
    }
    pub fn element_name(&self, index: usize) -> &str {
        if index == 0 {
            "Direction pad"
        } else if index <= self.controls.len() {
            match self.controls[index - 1].label {
                "TRI" => "Triangle",
                "O" => "Circle",
                "X" => "Cross",
                "SQ" => "Square",
                "SELECT" => "Select",
                "START" => "Start",
                label => label,
            }
        } else {
            "Settings"
        }
    }
    pub fn set_element(&mut self, index: usize, rect: Rect) {
        if index == 0 {
            self.dpad = rect;
        } else if index <= self.controls.len() {
            self.controls[index - 1].bounds = rect;
        } else {
            self.pause = rect;
        }
    }
    pub fn place_game(&mut self, w: f32, h: f32, overlap: bool, scale: f32, offset: [f32; 2]) {
        let obstacles: Vec<_> = (0..self.element_count()).map(|i| self.element(i)).collect();
        let area = if overlap {
            Rect::new(0., 0., w, h).fit(4. / 3.)
        } else {
            clear_picture(w, h, &obstacles)
        };
        let scale = scale.clamp(0.25, 1.);
        self.game = Rect::new(
            area.x + area.w * (1. - scale) * offset[0].clamp(0., 1.),
            area.y + area.h * (1. - scale) * offset[1].clamp(0., 1.),
            area.w * scale,
            area.h * scale,
        );
    }

    pub fn mask_at(&self, x: f32, y: f32) -> u16 {
        let mut result = 0;
        if self.dpad.contains(x, y) {
            let dx = x - self.dpad.x - self.dpad.w / 2.;
            let dy = y - self.dpad.y - self.dpad.h / 2.;
            if dx.hypot(dy) > self.dpad.w * 0.09 {
                // Eight broad sectors, with a small neutral center. Dragging
                // the thumb to a diagonal does not require hitting a corner.
                let diagonal = std::f32::consts::SQRT_2 - 1.; // tan(22.5 degrees)
                if dx.abs() >= dy.abs() * diagonal {
                    result |= 1 << if dx < 0. { 6 } else { 7 };
                }
                if dy.abs() >= dx.abs() * diagonal {
                    result |= 1 << if dy < 0. { 4 } else { 5 };
                }
            }
        }
        for control in &self.controls {
            if control.contains(x, y) {
                result |= control.mask;
            }
        }
        result
    }
}

/// Largest 4:3 rectangle clear of all control bounds. A feasible placement can
/// slide to the top/left boundary of an obstacle or screen; examine those
/// anchors and stop growth at the first obstacle. Only used on layout changes.
fn clear_picture(w: f32, h: f32, obstacles: &[Rect]) -> Rect {
    let mut best = Rect::default();
    for x in std::iter::once(0.).chain(obstacles.iter().map(|r| r.x + r.w)) {
        for y in std::iter::once(0.).chain(obstacles.iter().map(|r| r.y + r.h)) {
            let mut width = (w - x).min((h - y) * 4. / 3.).max(0.);
            for r in obstacles {
                if r.x + r.w <= x + 0.01 || r.y + r.h <= y + 0.01 {
                    continue;
                }
                let limit = (r.x - x).max((r.y - y) * 4. / 3.).max(0.);
                width = width.min(limit);
            }
            if width > best.w {
                best = Rect::new(x, y, width, width * 3. / 4.);
            }
        }
    }
    // Center within the same unobstructed region, without crossing a target.
    let mut centered = best;
    centered.x = (w - best.w) / 2.;
    if !obstacles.iter().any(|r| centered.overlaps(*r)) {
        best = centered;
    }
    best
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    pub allow_overlap: bool,
    pub elements: Vec<Rect>,
    pub game_scale: f32,
    pub game_offset: [f32; 2],
}
impl Default for Preset {
    fn default() -> Self {
        Self {
            allow_overlap: false,
            elements: Vec::new(),
            game_scale: 1.,
            game_offset: [0.5, 0.5],
        }
    }
}
impl Preset {
    pub fn layout(&self, w: f32, h: f32) -> Layout {
        let mut layout = Layout::new(w, h);
        for (i, r) in self
            .elements
            .iter()
            .take(layout.element_count())
            .enumerate()
        {
            if [r.x, r.y, r.w, r.h].iter().all(|n| n.is_finite()) {
                let width = (r.w * w).clamp(24., w);
                let height = (r.h * h).clamp(24., h);
                layout.set_element(
                    i,
                    Rect::new(
                        (r.x * w).clamp(0., w - width),
                        (r.y * h).clamp(0., h - height),
                        width,
                        height,
                    ),
                );
            }
        }
        layout.place_game(w, h, self.allow_overlap, self.game_scale, self.game_offset);
        layout
    }
    pub fn capture(&mut self, layout: &Layout, w: f32, h: f32) {
        self.elements = (0..layout.element_count())
            .map(|i| {
                let r = layout.element(i);
                Rect::new(r.x / w, r.y / h, r.w / w, r.h / h)
            })
            .collect();
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct ControllerSettings {
    presets: BTreeMap<String, Preset>,
}
impl ControllerSettings {
    pub fn profile(w: f32, h: f32) -> &'static str {
        if h > w * 1.35 {
            "Tall display"
        } else if w > h * 1.35 {
            "Wide display"
        } else {
            "Square display"
        }
    }
    pub fn load(root: &Path) -> io::Result<Self> {
        match fs::read(root.join("controls.json")) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }
    pub fn preset(&self, w: f32, h: f32) -> Preset {
        self.presets
            .get(Self::profile(w, h))
            .cloned()
            .unwrap_or_default()
    }
    pub fn set(&mut self, w: f32, h: f32, preset: Preset) {
        self.presets.insert(Self::profile(w, h).into(), preset);
    }
    pub fn save(&self, root: &Path) -> io::Result<()> {
        atomic_write(&root.join("controls.json"), &serde_json::to_vec(self)?)
    }
}

#[derive(Default)]
pub struct Touches {
    held: BTreeMap<i32, u16>,
    pending: u16,
}
impl Touches {
    /// Newly pressed controls across all fingers, suitable for a brief haptic.
    /// Motion within a held control and a second finger on it do not repeat.
    pub fn update(&mut self, id: i32, mask: u16) -> u16 {
        let before = self.mask();
        self.held.insert(id, mask);
        let pressed = self.mask() & !before;
        self.pending |= pressed;
        pressed
    }
    pub fn release(&mut self, id: i32) {
        self.held.remove(&id);
    }
    pub fn cancel(&mut self) {
        self.held.clear();
        self.pending = 0;
    }
    pub fn mask(&self) -> u16 {
        self.held.values().fold(0, |a, b| a | b)
    }
    /// A press/release delivered between emulation ticks still lasts one
    /// guest input sample. Interruptions cancel the pending press as well.
    pub fn frame_mask(&mut self) -> u16 {
        self.mask() | std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn game_is_clear_of_controls_and_targets_stay_reachable() {
        for (w, h) in [
            (443., 861.),
            (852., 785.),
            (785., 852.),
            (861., 443.),
            (320., 300.),
            (800., 280.),
        ] {
            let layout = Layout::new(w, h);
            assert!((layout.game.w / layout.game.h - 4. / 3.).abs() < 0.01);
            for bounds in layout
                .controls
                .iter()
                .map(|c| c.bounds)
                .chain([layout.pause, layout.dpad])
            {
                assert!(
                    !layout.game.overlaps(bounds),
                    "{w}x{h}: {:?} intersects {:?}",
                    layout.game,
                    bounds
                );
                assert!(bounds.w >= 48. && bounds.h >= 48.);
                assert!(bounds.x >= 0. && bounds.y >= 0.);
                assert!(bounds.x + bounds.w <= w && bounds.y + bounds.h <= h);
            }
            for control in &layout.controls {
                let x = control.bounds.x + control.bounds.w / 2.;
                let y = control.bounds.y + control.bounds.h / 2.;
                assert_eq!(layout.mask_at(x, y), control.mask);
            }
        }
    }

    #[test]
    fn custom_layout_reflows_picture_and_overlap_is_explicit() {
        let mut preset = Preset::default();
        let mut layout = preset.layout(852., 883.);
        layout.dpad = Rect::new(200., 120., 240., 240.);
        preset.capture(&layout, 852., 883.);
        let custom = preset.layout(852., 883.);
        for i in 0..custom.element_count() {
            assert!(!custom.game.overlaps(custom.element(i)));
        }
        preset.allow_overlap = true;
        let overlap = preset.layout(852., 883.);
        assert!((overlap.game.w - 852.).abs() < 0.01);
        assert!(overlap.game.overlaps(overlap.dpad));
    }

    #[test]
    fn press_feedback_is_discrete_while_controls_remain_held() {
        let mut touches = Touches::default();
        let up = 1 << 4;
        let jump = 1;
        assert_eq!(touches.update(2, up), up);
        assert_eq!(touches.update(2, up), 0); // held/moved inside the same target
        assert_eq!(touches.update(8, jump), jump); // independent second thumb
        assert_eq!(touches.update(9, jump), 0); // already pressed by another thumb
        touches.release(8);
        assert_eq!(touches.update(9, jump), 0);
        touches.release(9);
        assert_eq!(touches.update(8, jump), jump); // a fresh press
        assert_eq!(touches.update(2, up | (1 << 7)), 1 << 7); // slide to a diagonal
        touches.cancel();
        assert_eq!(touches.update(2, up), up);
    }
    #[test]
    fn fast_taps_are_sampled_once_and_interruptions_cancel_them() {
        let mut touches = Touches::default();
        touches.update(1, 1);
        touches.release(1);
        assert_eq!(touches.frame_mask(), 1);
        assert_eq!(touches.frame_mask(), 0);
        touches.update(1, 1);
        touches.cancel();
        assert_eq!(touches.frame_mask(), 0);
    }

    #[test]
    fn simultaneous_diagonal_and_face_button_release_independently() {
        for (w, h) in [(360., 800.), (673., 840.), (800., 360.)] {
            let layout = Layout::new(w, h);
            let mut touches = Touches::default();
            let diagonal = layout.mask_at(layout.dpad.x + 2., layout.dpad.y + 2.);
            assert_eq!(diagonal, (1 << 4) | (1 << 6));
            touches.update(2, diagonal);
            let cross = layout.controls.iter().find(|c| c.mask == 1).unwrap();
            touches.update(
                8,
                layout.mask_at(cross.bounds.x + 24., cross.bounds.y + 24.),
            );
            assert_eq!(touches.mask(), diagonal | 1);
            touches.release(2);
            assert_eq!(touches.mask(), 1);
            touches.cancel();
            assert_eq!(touches.mask(), 0);
            assert!(layout.game.h > 0. && layout.game.w > 0.);
        }
    }
}
