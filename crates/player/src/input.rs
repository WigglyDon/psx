use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Default)]
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
}

#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub bounds: Rect,
    pub label: &'static str,
    pub mask: u16,
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
        let pause = Rect::new(w - 94., 8., 86., 48.);
        let portrait = h > w;
        let area = if portrait {
            Rect::new(8., 64., w - 16., (h - 360.).max(80.))
        } else {
            Rect::new(180., 64., (w - 360.).max(80.), (h - 76.).max(80.))
        };
        let d = if portrait {
            144_f32.min((w - 36.) / 2.)
        } else {
            144.
        };
        let y = if portrait {
            h - 226.
        } else {
            h / 2. - d / 2. + 12.
        };
        let dpad = Rect::new(12., y, d, d);
        let fx = w - d / 2. - 12.;
        let fy = y + d / 2.;
        let size = 48.;
        let mut controls = Vec::with_capacity(10);
        let mut add = |x, y, label, id| {
            controls.push(Control {
                bounds: Rect::new(x, y, size, size),
                label,
                mask: 1u16 << id,
            })
        };
        add(fx - size / 2., fy - d / 2., "TRI", 9);
        add(fx + d / 2. - size, fy - size / 2., "O", 8);
        add(fx - size / 2., fy + d / 2. - size, "X", 0);
        add(fx - d / 2., fy - size / 2., "SQ", 1);
        for (x, label, id) in [
            (12., "L2", 12),
            (72., "L1", 10),
            (w - 132., "R1", 11),
            (w - 72., "R2", 13),
        ] {
            controls.push(Control {
                bounds: Rect::new(x, y - 64., 60., 48.),
                label,
                mask: 1 << id,
            });
        }
        for (x, label, id) in [(16., "SELECT", 2), (w - 96., "START", 3)] {
            controls.push(Control {
                bounds: Rect::new(x, h - 66., 80., 50.),
                label,
                mask: 1 << id,
            });
        }
        Self {
            game: area,
            dpad,
            controls,
            pause,
        }
    }

    pub fn mask_at(&self, x: f32, y: f32) -> u16 {
        let mut result = 0;
        if self.dpad.contains(x, y) {
            let nx = (x - self.dpad.x) / self.dpad.w;
            let ny = (y - self.dpad.y) / self.dpad.h;
            if nx < 0.38 {
                result |= 1 << 6;
            }
            if nx > 0.62 {
                result |= 1 << 7;
            }
            if ny < 0.38 {
                result |= 1 << 4;
            }
            if ny > 0.62 {
                result |= 1 << 5;
            }
        }
        for control in &self.controls {
            if control.bounds.contains(x, y) {
                result |= control.mask;
            }
        }
        result
    }
}

#[derive(Default)]
pub struct Touches {
    held: BTreeMap<i32, u16>,
}
impl Touches {
    /// Newly pressed controls across all fingers, suitable for a brief haptic.
    /// Motion within a held control and a second finger on it do not repeat.
    pub fn update(&mut self, id: i32, mask: u16) -> u16 {
        let before = self.mask();
        self.held.insert(id, mask);
        self.mask() & !before
    }
    pub fn release(&mut self, id: i32) {
        self.held.remove(&id);
    }
    pub fn cancel(&mut self) {
        self.held.clear();
    }
    pub fn mask(&self) -> u16 {
        self.held.values().fold(0, |a, b| a | b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
