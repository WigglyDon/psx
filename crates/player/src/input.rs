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
    pub fn same_bounds(self, other: Self) -> bool {
        (self.x - other.x).abs() < 0.01
            && (self.y - other.y).abs() < 0.01
            && (self.w - other.w).abs() < 0.01
            && (self.h - other.h).abs() < 0.01
    }
}

/// Window-inset callbacks and drawable resizes arrive independently. Never
/// apply another display's edges to the current drawable.
pub fn safe_content(size: [i32; 2], inset_size: [i32; 2], edges: [i32; 4]) -> Rect {
    let [w, h] = size;
    let [left, top, right, bottom] = if size == inset_size { edges } else { [0; 4] };
    let left = left.clamp(0, w.max(0));
    let top = top.clamp(0, h.max(0));
    Rect::new(
        left as f32,
        top as f32,
        (w - left - right.max(0)).max(1) as f32,
        (h - top - bottom.max(0)).max(1) as f32,
    )
}

/// Merge decoration, reserved navigation and camera margins. A side camera
/// may be a local obstacle; its top/bottom margins still apply.
pub fn safe_edges(
    decoration: [i32; 4],
    navigation: [i32; 4],
    camera: [i32; 4],
    local_side_camera: bool,
) -> [i32; 4] {
    std::array::from_fn(|i| {
        decoration[i]
            .max(navigation[i])
            .max(if local_side_camera && (i == 0 || i == 2) {
                0
            } else {
                camera[i]
            })
    })
}

#[derive(Clone, Copy, Debug)]
pub struct Control {
    pub bounds: Rect,
    pub label: &'static str,
    pub mask: u16,
}
impl Control {
    fn is_face(self) -> bool {
        matches!(self.label, "TRI" | "O" | "X" | "SQ")
    }
    pub fn contains(self, x: f32, y: f32) -> bool {
        self.bounds.contains(x, y)
    }
}
#[derive(Debug)]
pub struct Layout {
    pub game: Rect,
    pub dpad: Rect,
    pub controls: Vec<Control>,
    pub pause: Rect,
    pub playback: Rect,
    pub speed: Rect,
    pub mute: Rect,
    pub sticks: Vec<Rect>,
    cutouts: Vec<Rect>,
}

impl Layout {
    /// Coordinates are density-independent points, with platform insets excluded.
    pub fn new(w: f32, h: f32) -> Self {
        Self::default_with_cutouts(w, h, &[])
    }

    fn default_with_cutouts(w: f32, h: f32, cutouts: &[Rect]) -> Self {
        let mut rails = Self::arranged(w, h, false);
        let cluster = rails.dpad.w;
        let shoulder = (cluster - 8.) / 2.;
        let middle = w - 2. * (16. + cluster);
        // A square display can put the shoulders and system buttons between
        // the two thumb clusters. Compare the resulting clear picture rather
        // than imposing the same dock on a wide or narrow display.
        if cluster >= 160. && middle >= 2. * shoulder + 16. {
            let dock = Self::arranged(w, h, true);
            if dock.game.w > rails.game.w + 0.01 {
                rails = dock;
            }
        }
        if w > h * 1.35 && h >= 304. {
            let side = Self::side_rails(w, h, cutouts);
            if side.game.w > rails.game.w + 0.01 {
                rails = side;
            }
        }
        rails
    }

    fn arranged(w: f32, h: f32, compact: bool) -> Self {
        // Default controls occupy their own dock. The picture never intersects
        // even a translucent control's rectangular touch target.
        let cluster = (w / 2. - 20.).clamp(112., 224.).min((h - 140.).max(112.));
        let size = (cluster * 0.4).clamp(48., 84.);
        let well_height = if compact {
            // Leave a real gap between Triangle and Cross, even in the
            // compact dock. The picture is maximized around these targets.
            // Main thumb controls set the minimum dock height. Do not shrink
            // their faces just to win a full-width picture comparison.
            cluster.min(200.).max(size * 2. + 16.)
        } else {
            cluster
        };
        let offset = (cluster - size) / 2.;
        let fy = h - if compact { 32. } else { 76. } - well_height / 2.;
        let fx = w - 16. - cluster / 2.;
        let dpad = Rect::new(16., fy - well_height / 2., cluster, well_height);
        let pause = Rect::new(w - 56., 4., 48., 48.);
        let playback = Rect::new(w - 224., 4., 48., 48.);
        let speed = Rect::new(w - 168., 4., 48., 48.);
        let mute = Rect::new(w - 112., 4., 48., 48.);
        let mut controls = Vec::with_capacity(10);
        let mut add = |x, y, label, id| {
            controls.push(Control {
                bounds: Rect::new(x, y, size, size),
                label,
                mask: 1u16 << id,
            })
        };
        let vertical = (well_height - size) / 2.;
        add(fx - size / 2., fy - vertical - size / 2., "TRI", 9);
        add(fx + offset - size / 2., fy - size / 2., "O", 8);
        add(fx - size / 2., fy + vertical - size / 2., "X", 0);
        add(fx - offset - size / 2., fy - size / 2., "SQ", 1);
        let shoulder_width = (cluster - 8.) / 2.;
        let left_column = dpad.x + cluster + 8.;
        let right_column = w - 16. - cluster - 8. - shoulder_width;
        for (x, label, id) in [
            (16., "L2", 12),
            (24. + shoulder_width, "L1", 10),
            (w - 16. - cluster, "R1", 11),
            (w - 16. - shoulder_width, "R2", 13),
        ] {
            let bounds = if compact {
                Rect::new(
                    if label.starts_with('L') {
                        left_column
                    } else {
                        right_column
                    },
                    dpad.y + if label.ends_with('1') { 56. } else { 0. },
                    shoulder_width,
                    48.,
                )
            } else {
                Rect::new(x, dpad.y - 56., shoulder_width, 48.)
            };
            controls.push(Control {
                bounds,
                label,
                mask: 1 << id,
            });
        }
        for (x, label, id) in [
            ((w / 2. - 196.).max(16.), "SELECT", 2),
            ((w / 2. + 100.).min(w - 112.), "START", 3),
        ] {
            let x = if compact {
                if label == "SELECT" {
                    left_column
                } else {
                    right_column
                }
            } else {
                x
            };
            controls.push(Control {
                bounds: Rect::new(x, h - if compact { 56. } else { 64. }, 96., 48.),
                label,
                mask: 1 << id,
            });
        }
        let mut layout = Self {
            game: Rect::default(),
            dpad,
            controls,
            pause,
            playback,
            speed,
            mute,
            sticks: Vec::new(),
            cutouts: Vec::new(),
        };
        layout.anchor_toolbar(w, h, 0., 0., &[]);
        layout.place_game(w, h, false, 1., [0.5, 0.5]);
        layout
    }

    fn side_rails(w: f32, h: f32, cutouts: &[Rect]) -> Self {
        let mut layout = Self::arranged(w, h, false);
        // Grow the thumb wells into spare side-rail width before taking space
        // from a full-height picture. Narrower windows retain the old ceiling.
        let picture_clearance = ((w - h * 4. / 3.) / 2. - 16.).max(160.);
        let cluster = (h - 184.).clamp(112., 200.).min(picture_clearance);
        // A side camera can meet the upper edge of a larger thumb well. Use
        // the clear height below it, retaining rail width and placing it below
        // the camera. Do this only for generated defaults;
        // saved custom element bounds are applied later without alteration.
        let well_height = |x| {
            let well = Rect::new(x, h - 32. - cluster, cluster, cluster);
            cutouts
                .iter()
                .filter(|c| well.overlaps(**c))
                .map(|c| (h - c.y - c.h - 8.).clamp(112., cluster))
                .fold(cluster, f32::min)
        };
        let face_height = well_height(w - 16. - cluster);
        let face_bottom = if face_height < cluster { 0. } else { 32. };
        let size = (cluster * 0.4).min(face_height * 0.43).clamp(48., 80.);
        let offset = (cluster - size) / 2.;
        let vertical = (face_height - size) / 2.;
        let (fx, fy) = (w - 16. - cluster / 2., h - face_bottom - face_height / 2.);
        let dpad_height = well_height(16.);
        let dpad_bottom = if dpad_height < cluster { 0. } else { 32. };
        layout.dpad = Rect::new(16., h - dpad_bottom - dpad_height, cluster, dpad_height);
        let shoulder = (cluster - 8.) / 2.;
        let system_y = if layout.dpad.y >= 200. { 144. } else { h - 48. };
        for c in &mut layout.controls {
            let (x, y, width, height) = match c.label {
                "TRI" => (fx - size / 2., fy - vertical - size / 2., size, size),
                "O" => (fx + offset - size / 2., fy - size / 2., size, size),
                "X" => (fx - size / 2., fy + vertical - size / 2., size, size),
                "SQ" => (fx - offset - size / 2., fy - size / 2., size, size),
                "L1" => (16., 64., shoulder, 48.),
                "L2" => (24. + shoulder, 64., shoulder, 48.),
                "R2" => (w - 16. - cluster, 64., shoulder, 48.),
                "R1" => (w - 16. - shoulder, 64., shoulder, 48.),
                "SELECT" => (
                    if layout.dpad.y >= 200. {
                        16. + cluster - 88.
                    } else {
                        w / 2. - 96.
                    },
                    system_y,
                    88.,
                    48.,
                ),
                _ => (
                    if layout.dpad.y >= 200. {
                        w - 16. - cluster
                    } else {
                        w / 2. + 8.
                    },
                    system_y,
                    88.,
                    48.,
                ),
            };
            c.bounds = Rect::new(x, y, width, height);
        }
        layout.anchor_toolbar(w, h, 0., 0., &[]);
        layout.place_game(w, h, false, 1., [0.5, 0.5]);
        layout
    }

    pub fn element_count(&self) -> usize {
        self.controls.len() + 5 + self.sticks.len()
    }
    fn ps2(w: f32, h: f32) -> Self {
        // Solve both halves together. The primary thumb band contains a stick
        // and an inner column (system key above stick-click), rather than
        // adding separate full-width rows for every new PS2 input.
        let requested = [w, h];
        let (w, h) = (w.max(360.), h.max(360.));
        let landscape = w > h * 1.35;
        let mut layout = Self::new(w, h);
        let bottom = if landscape {
            (h - 336.).clamp(24., 32.)
        } else {
            32.
        };
        let stick = if landscape {
            104.
        } else {
            (w / 2. - 76.).clamp(104., 128.)
        };
        let face = if landscape {
            // 48-point minimum caps, a real Triangle/Cross gap, and a fixed
            // comfortable stick. Extra height grows both digital clusters.
            (h - 56. - 48. - 16. - stick - bottom).clamp(112., 184.)
        } else {
            (w / 2. - 20.).clamp(112., 184.)
        };
        let column = face.max(stick + 56.);
        let main_y = h - bottom - stick;
        let face_y = main_y - 8. - face;
        let left = Rect::new(16., main_y, stick, stick);
        let right = Rect::new(w - 16. - stick, main_y, stick, stick);
        let digital_x = 16. + (column - face) / 2.;
        layout.dpad = Rect::new(digital_x, face_y, face, face);
        let fx = w - 16. - column / 2.;
        let fy = face_y + face / 2.;
        let cap = (face - 16.) / 2.;
        let step = (face - cap) / 2.;
        let middle_shoulders = !landscape && w - 2. * (16. + column) >= 192.;
        let shoulder = if middle_shoulders {
            88.
        } else {
            (column - 8.) / 2.
        };
        for control in &mut layout.controls {
            let (x, y, width, height) = match control.label {
                "TRI" => (fx - cap / 2., fy - step - cap / 2., cap, cap),
                "O" => (fx + step - cap / 2., fy - cap / 2., cap, cap),
                "X" => (fx - cap / 2., fy + step - cap / 2., cap, cap),
                "SQ" => (fx - step - cap / 2., fy - cap / 2., cap, cap),
                "SELECT" => (left.x + stick + 8., main_y, 48., 48.),
                "START" => (right.x - 56., main_y, 48., 48.),
                label => {
                    let left_side = label.starts_with('L');
                    let first = label.ends_with('1');
                    if middle_shoulders {
                        let x = 16. + column + 8.;
                        (
                            if left_side { x } else { w - x - shoulder },
                            face_y + if first { 56. } else { 0. },
                            shoulder,
                            48.,
                        )
                    } else {
                        let outer = 16.;
                        let x = outer + if first { 0. } else { shoulder + 8. };
                        (
                            if left_side { x } else { w - x - shoulder },
                            face_y - 56.,
                            shoulder,
                            48.,
                        )
                    }
                }
            };
            control.bounds = Rect::new(x, y, width, height);
        }
        if landscape {
            layout.playback = Rect::new(16., 4., 48., 48.);
            layout.speed = Rect::new(72., 4., 48., 48.);
            layout.mute = Rect::new(w - 120., 4., 48., 48.);
            layout.pause = Rect::new(w - 64., 4., 48., 48.);
        }
        layout.controls.push(Control {
            bounds: Rect::new(left.x + left.w + 8., left.y + left.h - 48., 48., 48.),
            label: "L3",
            mask: 1 << 14,
        });
        layout.controls.push(Control {
            bounds: Rect::new(right.x - 56., right.y + right.h - 48., 48., 48.),
            label: "R3",
            mask: 1 << 15,
        });
        layout.sticks = vec![left, right];
        // Transient Android surfaces can be smaller than any usable layout.
        // Scale only that transient presentation, never the saved profile.
        if requested != [w, h] {
            for i in 0..layout.element_count() {
                let r = layout.element(i);
                layout.set_element(
                    i,
                    Rect::new(
                        r.x * requested[0] / w,
                        r.y * requested[1] / h,
                        r.w * requested[0] / w,
                        r.h * requested[1] / h,
                    ),
                );
            }
        }
        layout.anchor_toolbar(requested[0], requested[1], 0., 0., &[]);
        layout.place_game(requested[0], requested[1], false, 1., [0.5, 0.5]);
        if let Some(compact) = Self::ps2_shared_band(requested[0], requested[1]) {
            if compact.game.w > layout.game.w + 0.01 {
                return compact;
            }
        }
        layout
    }
    /// Wide inner displays can share a lower band horizontally instead of
    /// stacking every digital cluster above a stick. Keep primary sticks low
    /// and outside, their system/click column adjacent, and secondary digital
    /// controls slightly higher/inside. Compare the actual clear picture.
    fn ps2_shared_band(w: f32, h: f32) -> Option<Self> {
        if w < 800. || h < 600. || w > h * 1.35 {
            return None;
        }
        let mut layout = Self::new(w, h);
        let stick = (w / 2. - 296.).clamp(104., 128.);
        let digital_x = 16. + stick + 56.;
        let face = (w / 2. - digital_x - 64.).clamp(160., 184.);
        let main_y = h - 32. - stick;
        let face_y = h - 32. - face;
        let cap = (face - 16.) / 2.;
        let step = (face - cap) / 2.;
        let fx = w - digital_x - face / 2.;
        let fy = face_y + face / 2.;
        layout.dpad = Rect::new(digital_x, face_y, face, face);
        for c in &mut layout.controls {
            c.bounds = match c.label {
                "TRI" => Rect::new(fx - cap / 2., fy - step - cap / 2., cap, cap),
                "O" => Rect::new(fx + step - cap / 2., fy - cap / 2., cap, cap),
                "X" => Rect::new(fx - cap / 2., fy + step - cap / 2., cap, cap),
                "SQ" => Rect::new(fx - step - cap / 2., fy - cap / 2., cap, cap),
                "SELECT" => Rect::new(24. + stick, main_y, 48., 48.),
                "START" => Rect::new(w - 72. - stick, main_y, 48., 48.),
                label => {
                    let x = digital_x + face + 8.;
                    Rect::new(
                        if label.starts_with('L') {
                            x
                        } else {
                            w - x - 48.
                        },
                        face_y + if label.ends_with('1') { 56. } else { 0. },
                        48.,
                        48.,
                    )
                }
            };
        }
        for (x, label, id) in [(24. + stick, "L3", 14), (w - 72. - stick, "R3", 15)] {
            layout.controls.push(Control {
                bounds: Rect::new(x, main_y + 56., 48., 48.),
                label,
                mask: 1 << id,
            });
        }
        layout.sticks = vec![
            Rect::new(16., main_y, stick, stick),
            Rect::new(w - 16. - stick, main_y, stick, stick),
        ];
        layout.anchor_toolbar(w, h, 0., 0., &[]);
        layout.place_game(w, h, false, 1., [0.5, 0.5]);
        Some(layout)
    }
    pub fn action_elements(&self) -> impl Iterator<Item = usize> + '_ {
        self.controls
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.is_face().then_some(i + 1))
    }
    pub fn action_bounds(&self) -> Rect {
        self.action_elements()
            .map(|i| self.element(i))
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
    /// Application chrome stays in role-based corner pairs in every profile.
    /// Reuse a clear camera margin only while OS decoration is absent; visible
    /// status decoration pushes the toolbar without changing the game canvas.
    pub fn anchor_toolbar(
        &mut self,
        w: f32,
        h: f32,
        reusable_top: f32,
        covered_top: f32,
        cutouts: &[Rect],
    ) {
        let padding = 16_f32.min(w * 0.025);
        let gap = 8_f32.min(w * 0.01);
        let size = 40_f32
            .min(((w - 2. * padding - 3. * gap) / 4.).max(1.))
            .min(h)
            .min(w);
        let height = size.min(32.);
        let top = if reusable_top >= height && covered_top <= 0. {
            -reusable_top + 4_f32.min((reusable_top - height) / 2.)
        } else {
            (covered_top + 4.).clamp(0., (h - height).max(0.))
        };
        let pair = |mut x: f32, left: bool| {
            let mut y = top;
            for _ in 0..cutouts.len() + 1 {
                let bar = Rect::new(x, y, 2. * size + gap, height);
                if let Some(c) = cutouts.iter().find(|c| bar.overlaps(**c)) {
                    let shifted = if left {
                        c.x + c.w + 8.
                    } else {
                        c.x - bar.w - 8.
                    };
                    let candidate = Rect::new(shifted, y, bar.w, height);
                    if shifted >= 0.
                        && shifted + bar.w <= w
                        && if left {
                            shifted + bar.w <= w / 2.
                        } else {
                            shifted >= w / 2.
                        }
                        && !cutouts.iter().any(|c| candidate.overlaps(*c))
                    {
                        x = shifted;
                    } else {
                        y = (c.y + c.h + 8.).min((h - height).max(0.));
                    }
                } else {
                    break;
                }
            }
            (x, y)
        };
        let right = w - padding - size;
        let (left_x, left_y) = pair(padding, true);
        let (right_x, right_y) = pair(right - size - gap, false);
        self.playback = Rect::new(left_x, left_y, size, height);
        self.speed = Rect::new(left_x + size + gap, left_y, size, height);
        self.mute = Rect::new(right_x, right_y, size, height);
        self.pause = Rect::new(right_x + size + gap, right_y, size, height);
    }
    pub fn controller_elements(&self) -> impl DoubleEndedIterator<Item = usize> {
        (0..self.controls.len() + 1).chain(self.controls.len() + 5..self.element_count())
    }
    pub fn element(&self, index: usize) -> Rect {
        if index == 0 {
            self.dpad
        } else if index <= self.controls.len() {
            self.controls[index - 1].bounds
        } else if index == self.controls.len() + 1 {
            self.pause
        } else if index == self.controls.len() + 2 {
            self.playback
        } else if index == self.controls.len() + 3 {
            self.speed
        } else if index == self.controls.len() + 4 {
            self.mute
        } else {
            self.sticks[index - self.controls.len() - 5]
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
        } else if index == self.controls.len() + 1 {
            "Settings"
        } else if index == self.controls.len() + 2 {
            "Pause / play"
        } else if index == self.controls.len() + 3 {
            "Time scale"
        } else if index == self.controls.len() + 4 {
            "Mute"
        } else if index == self.controls.len() + 5 {
            "Left stick"
        } else {
            "Right stick"
        }
    }
    pub fn set_element(&mut self, index: usize, rect: Rect) {
        if index == 0 {
            self.dpad = rect;
        } else if index <= self.controls.len() {
            self.controls[index - 1].bounds = rect;
        } else if index == self.controls.len() + 1 {
            self.pause = rect;
        } else if index == self.controls.len() + 2 {
            self.playback = rect;
        } else if index == self.controls.len() + 3 {
            self.speed = rect;
        } else if index == self.controls.len() + 4 {
            self.mute = rect;
        } else {
            self.sticks[index - self.controls.len() - 5] = rect;
        }
    }
    pub fn place_game(&mut self, w: f32, h: f32, overlap: bool, scale: f32, offset: [f32; 2]) {
        self.place_game_in(Rect::new(0., 0., w, h), overlap, scale, offset);
    }
    /// Reuse clear pixels below corner chrome in an excluded camera margin
    /// without changing controller coordinates or the saved display profile.
    pub fn place_game_in(&mut self, bounds: Rect, overlap: bool, scale: f32, offset: [f32; 2]) {
        let mut obstacles = self.cutouts.clone();
        if !overlap {
            obstacles.extend((0..self.element_count()).map(|i| self.element(i)));
        }
        let obstacles: Vec<_> = obstacles
            .into_iter()
            .filter(|r| r.overlaps(bounds))
            .map(|r| Rect::new(r.x - bounds.x, r.y - bounds.y, r.w, r.h))
            .collect();
        let area = clear_picture(bounds.w, bounds.h, &obstacles);
        let scale = scale.clamp(0.25, 1.);
        self.game = Rect::new(
            bounds.x + area.x + area.w * (1. - scale) * offset[0].clamp(0., 1.),
            bounds.y + area.y + area.h * (1. - scale) * offset[1].clamp(0., 1.),
            area.w * scale,
            area.h * scale,
        );
    }

    /// A camera is a local obstruction, not an unusable full-height strip.
    /// Keep the face-button diamond intact when shifting it clear of a hole.
    pub fn avoid_cutouts(&mut self, w: f32, h: f32, cutouts: &[Rect]) {
        self.cutouts = cutouts
            .iter()
            .copied()
            .filter(|r| r.w > 0. && r.h > 0.)
            .collect();
        if self.cutouts.is_empty() {
            return;
        }
        let faces: Vec<_> = self
            .controls
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_face())
            .map(|(i, _)| i + 1)
            .collect();
        let mut groups = vec![faces.clone()];
        groups.extend(
            (0..self.element_count())
                .filter(|i| !faces.contains(i))
                .map(|i| vec![i]),
        );
        for group in groups {
            let original: Vec<_> = group.iter().map(|&i| self.element(i)).collect();
            if !original
                .iter()
                .any(|r| self.cutouts.iter().any(|c| r.overlaps(*c)))
            {
                continue;
            }
            let mut candidates = Vec::new();
            for r in &original {
                for c in &self.cutouts {
                    if r.overlaps(*c) {
                        candidates.extend([
                            (c.x - r.w - 8. - r.x, 0.),
                            (c.x + c.w + 8. - r.x, 0.),
                            (0., c.y - r.h - 8. - r.y),
                            (0., c.y + c.h + 8. - r.y),
                        ]);
                    }
                }
            }
            candidates
                .sort_by(|(ax, ay), (bx, by)| (ax * ax + ay * ay).total_cmp(&(bx * bx + by * by)));
            if let Some((dx, dy)) = candidates.into_iter().find(|(dx, dy)| {
                original.iter().all(|r| {
                    let moved = Rect::new(r.x + dx, r.y + dy, r.w, r.h);
                    moved.x >= 0.
                        && moved.y >= 0.
                        && moved.x + moved.w <= w
                        && moved.y + moved.h <= h
                        && !self.cutouts.iter().any(|c| moved.overlaps(*c))
                        && !(0..self.element_count())
                            .filter(|i| !group.contains(i))
                            .any(|i| moved.overlaps(self.element(i)))
                })
            }) {
                for (&i, r) in group.iter().zip(original) {
                    self.set_element(i, Rect::new(r.x + dx, r.y + dy, r.w, r.h));
                }
            }
        }
    }

    pub fn mask_at(&self, x: f32, y: f32) -> u16 {
        self.mask_near(x, y, 0)
    }

    fn mask_near(&self, x: f32, y: f32, previous: u16) -> u16 {
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
        // The round face buttons have square targets and a small forgiveness
        // margin in unused control space. Never take that margin from the
        // picture, another control, or the settings button. One finger picks
        // the nearest symbol; independent fingers still permit combinations.
        let free = !self.game.contains(x, y)
            && !self.dpad.contains(x, y)
            && !self.pause.contains(x, y)
            && !self.playback.contains(x, y)
            && !self.speed.contains(x, y)
            && !self.mute.contains(x, y)
            && !self
                .controls
                .iter()
                .any(|c| !c.is_face() && c.contains(x, y));
        let mut face = None;
        let mut closest = f32::INFINITY;
        for control in &self.controls {
            if control.is_face() {
                let r = control.bounds;
                let padding = (r.w.min(r.h) * 0.15).min(10.);
                let target = Rect::new(
                    r.x - padding,
                    r.y - padding,
                    r.w + 2. * padding,
                    r.h + 2. * padding,
                );
                if control.contains(x, y) || (free && target.contains(x, y)) {
                    let distance = (x - r.x - r.w / 2.).hypot(y - r.y - r.h / 2.)
                        - if previous & control.mask != 0 { 6. } else { 0. };
                    if distance < closest {
                        closest = distance;
                        face = Some(control.mask);
                    }
                }
            } else if control.contains(x, y) {
                result |= control.mask;
            }
        }
        result | face.unwrap_or(0)
    }
}

/// Largest clear 4:3 picture, preferring its highest feasible placement.
/// Examine screen/obstacle anchors and stop growth at the first obstruction.
/// Center horizontally only when doing so retains that maximum size and top.
/// Only used on layout changes, never during emulation frames.
fn clear_picture(w: f32, h: f32, obstacles: &[Rect]) -> Rect {
    let mut best = Rect::default();
    for x in std::iter::once(0.)
        .chain(obstacles.iter().map(|r| r.x + r.w))
        .filter(|x| *x >= 0. && *x <= w)
    {
        for y in std::iter::once(0.)
            .chain(obstacles.iter().map(|r| r.y + r.h))
            .filter(|y| *y >= 0. && *y <= h)
        {
            let mut width = (w - x).min((h - y) * 4. / 3.).max(0.);
            for r in obstacles {
                if r.x + r.w <= x + 0.01 || r.y + r.h <= y + 0.01 {
                    continue;
                }
                let limit = (r.x - x).max((r.y - y) * 4. / 3.).max(0.);
                width = width.min(limit);
            }
            if width > best.w || (width == best.w && y < best.y) {
                best = Rect::new(x, y, width, width * 3. / 4.);
            }
        }
    }
    let mut centered = best;
    centered.x = (w - best.w) / 2.;
    if !obstacles.iter().any(|r| centered.overlaps(*r)) {
        best = centered;
    } else {
        // A local camera can prevent centering. Use an available screen edge
        // rather than leave a gratuitous strip on both sides of the picture.
        for x in [0., w - best.w] {
            let edge = Rect::new(x, best.y, best.w, best.h);
            if !obstacles.iter().any(|r| edge.overlaps(*r)) {
                best = edge;
                break;
            }
        }
    }
    best
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Preset {
    #[serde(default, skip_serializing_if = "is_false")]
    pub analog: bool,
    pub allow_overlap: bool,
    #[serde(default = "enabled")]
    pub follow_thumb: bool,
    pub elements: Vec<Rect>,
    pub game_scale: f32,
    pub game_offset: [f32; 2],
}
impl Default for Preset {
    fn default() -> Self {
        Self {
            analog: false,
            allow_overlap: false,
            follow_thumb: true,
            elements: Vec::new(),
            game_scale: 1.,
            game_offset: [0.5, 0.5],
        }
    }
}
impl Preset {
    pub fn is_default(&self, w: f32, h: f32) -> bool {
        self.is_default_with_cutouts(w, h, &[])
    }
    pub fn is_default_with_cutouts(&self, w: f32, h: f32, cutouts: &[Rect]) -> bool {
        let layout = self.layout_with_cutouts(w, h, cutouts);
        let defaults = Self {
            analog: self.analog,
            ..Self::default()
        }
        .layout_with_cutouts(w, h, cutouts);
        !self.allow_overlap
            && self.follow_thumb
            && layout.game.same_bounds(defaults.game)
            && (0..layout.element_count())
                .all(|i| layout.element(i).same_bounds(defaults.element(i)))
    }
    pub fn layout(&self, w: f32, h: f32) -> Layout {
        self.layout_with_cutouts(w, h, &[])
    }
    pub fn layout_with_cutouts(&self, w: f32, h: f32, cutouts: &[Rect]) -> Layout {
        // Android may briefly expose a 1-pixel pbuffer before the window is
        // adopted. Saved controls must remain safe during that transition.
        let (w, h) = (w.max(1.), h.max(1.));
        let mut layout = if self.analog {
            Layout::ps2(w, h)
        } else {
            Layout::default_with_cutouts(w, h, cutouts)
        };
        for (i, r) in self
            .elements
            .iter()
            .take(layout.element_count())
            .enumerate()
        {
            if [r.x, r.y, r.w, r.h].iter().all(|n| n.is_finite()) {
                let width = (r.w * w).clamp(24_f32.min(w), w);
                let height = (r.h * h).clamp(24_f32.min(h), h);
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
        layout.avoid_cutouts(w, h, cutouts);
        layout.anchor_toolbar(w, h, 0., 0., cutouts);
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

fn enabled() -> bool {
    true
}
fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Default, Serialize, Deserialize)]
pub struct ControllerSettings {
    presets: BTreeMap<String, Preset>,
    #[serde(skip)]
    analog: bool,
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
            .unwrap_or_else(|| Preset {
                analog: self.analog,
                ..Preset::default()
            })
    }
    pub fn set(&mut self, w: f32, h: f32, mut preset: Preset) {
        preset.analog = self.analog;
        self.presets.insert(Self::profile(w, h).into(), preset);
    }
    pub fn load_ps2(root: &Path) -> io::Result<Self> {
        let mut settings = Self::load(root)?;
        settings.analog = true;
        Ok(settings)
    }
    pub fn analog(&self) -> bool {
        self.analog
    }
    pub fn save(&self, root: &Path) -> io::Result<()> {
        atomic_write(&root.join("controls.json"), &serde_json::to_vec(self)?)
    }
}

#[derive(Default)]
pub struct Touches {
    held: BTreeMap<i32, u16>,
    pending: u16,
    dpad: Option<DpadGesture>,
    sticks: [Option<StickGesture>; 2],
}

struct StickGesture {
    pointer: i32,
    origin: [f32; 2],
    radius: f32,
    axes: [i16; 2],
    follow: bool,
}
impl StickGesture {
    fn track(&mut self, x: f32, y: f32) {
        let mut delta = [x - self.origin[0], y - self.origin[1]];
        let distance = delta[0].hypot(delta[1]);
        if distance > self.radius {
            let scale = self.radius / distance;
            delta = delta.map(|d| d * scale);
            if self.follow {
                self.origin = [x - delta[0], y - delta[1]];
            }
        }
        let normalized = delta.map(|d| d / self.radius);
        let magnitude = normalized[0].hypot(normalized[1]);
        self.axes = if magnitude < 0.12 {
            [0; 2]
        } else {
            let scale = ((magnitude - 0.12) / 0.88).min(1.) / magnitude;
            normalized.map(|d| (d * scale * 32767.).round() as i16)
        };
    }
}

const DIRECTIONS: u16 = (1 << 4) | (1 << 5) | (1 << 6) | (1 << 7);

#[derive(Clone, Copy, Default)]
pub struct DpadFeedback {
    pub center: [f32; 2],
    pub offset: [f32; 2],
}

struct DpadGesture {
    pointer: i32,
    bounds: Rect,
    origin: [f32; 2],
    finger: [f32; 2],
    follow: bool,
    mask: u16,
}
impl DpadGesture {
    fn track(&mut self, x: f32, y: f32, moving: bool) -> u16 {
        let travel = self.bounds.w * 0.32;
        let mut dx = x - self.origin[0];
        let mut dy = y - self.origin[1];
        let distance = dx.hypot(dy);
        // Keep ordinary taps on the drawn arrows. Once the thumb moves beyond
        // its travel, the logical center follows it instead of losing input.
        if moving && self.follow && distance > travel {
            let scale = travel / distance;
            dx *= scale;
            dy *= scale;
            self.origin = [x - dx, y - dy];
        }
        self.finger = [x, y];
        let dead_zone = self.bounds.w * 0.09;
        let threshold = if self.mask == 0 {
            dead_zone
        } else {
            dead_zone * 0.75
        };
        self.mask = if dx.hypot(dy) <= threshold {
            0
        } else {
            // Five degrees of angular hysteresis keeps a near-diagonal thumb
            // from chattering between directions (and repeating haptics).
            let angle = dy.atan2(dx);
            let sector = std::f32::consts::FRAC_PI_4;
            let masks = [128, 160, 32, 96, 64, 80, 16, 144];
            let previous = masks.iter().position(|&mask| mask == self.mask);
            let keep = previous.is_some_and(|index| {
                let delta = (angle - index as f32 * sector + std::f32::consts::PI)
                    .rem_euclid(std::f32::consts::TAU)
                    - std::f32::consts::PI;
                delta.abs() <= sector / 2. + 5_f32.to_radians()
            });
            if keep {
                self.mask
            } else {
                masks[(angle / sector).round().rem_euclid(8.) as usize]
            }
        };
        self.mask
    }

    fn feedback(&self) -> DpadFeedback {
        let radius = self.bounds.w * 0.32;
        let dx = self.finger[0] - self.origin[0];
        let dy = self.finger[1] - self.origin[1];
        let scale = radius.max(dx.hypot(dy));
        DpadFeedback {
            center: [
                ((self.origin[0] - self.bounds.x - self.bounds.w / 2.) / radius).clamp(-1., 1.),
                ((self.origin[1] - self.bounds.y - self.bounds.h / 2.) / radius).clamp(-1., 1.),
            ],
            offset: [dx / scale, dy / scale],
        }
    }
}

impl Touches {
    pub fn down(&mut self, id: i32, x: f32, y: f32, layout: &Layout, follow: bool) -> u16 {
        for (i, bounds) in layout.sticks.iter().enumerate() {
            if bounds.contains(x, y) && self.sticks[i].is_none() {
                let mut gesture = StickGesture {
                    pointer: id,
                    origin: [bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.],
                    radius: bounds.w.min(bounds.h) * 0.32,
                    axes: [0; 2],
                    follow,
                };
                gesture.track(x, y);
                self.sticks[i] = Some(gesture);
                return 1 << (14 + i);
            }
        }
        if self.dpad.is_none() && layout.dpad.contains(x, y) {
            let bounds = layout.dpad;
            let mut gesture = DpadGesture {
                pointer: id,
                bounds,
                origin: [bounds.x + bounds.w / 2., bounds.y + bounds.h / 2.],
                finger: [x, y],
                follow,
                mask: 0,
            };
            let mask = gesture.track(x, y, false);
            self.dpad = Some(gesture);
            self.pending &= !DIRECTIONS;
            self.update(id, mask)
        } else {
            self.motion(id, x, y, layout)
        }
    }

    pub fn motion(&mut self, id: i32, x: f32, y: f32, layout: &Layout) -> u16 {
        if let Some(gesture) = self.sticks.iter_mut().flatten().find(|g| g.pointer == id) {
            gesture.track(x, y);
            return 0;
        }
        let mask = if let Some(gesture) = self.dpad.as_mut().filter(|g| g.pointer == id) {
            // Direction is a current vector. Keep fast face-button taps, but
            // never latch opposing directions from two moves in one frame.
            self.pending &= !DIRECTIONS;
            gesture.track(x, y, true)
        } else {
            // A captured direction finger cannot become a face-button press;
            // other fingers remain independent and cannot steal the D-pad.
            layout.mask_near(x, y, self.held.get(&id).copied().unwrap_or(0)) & !DIRECTIONS
        };
        self.update(id, mask)
    }

    pub fn dpad_feedback(&self) -> Option<DpadFeedback> {
        self.dpad.as_ref().map(DpadGesture::feedback)
    }

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
        for stick in &mut self.sticks {
            if stick.as_ref().is_some_and(|g| g.pointer == id) {
                *stick = None;
            }
        }
        self.held.remove(&id);
        if self.dpad.as_ref().is_some_and(|g| g.pointer == id) {
            self.dpad = None;
        }
    }
    pub fn cancel(&mut self) {
        self.held.clear();
        self.pending = 0;
        self.dpad = None;
        self.sticks = [None, None];
    }
    pub fn mask(&self) -> u16 {
        self.held.values().fold(0, |a, b| a | b)
    }
    pub fn axes(&self) -> [i16; 4] {
        let left = self.sticks[0].as_ref().map_or([0; 2], |g| g.axes);
        let right = self.sticks[1].as_ref().map_or([0; 2], |g| g.axes);
        [left[0], left[1], right[0], right[1]]
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
    fn saved_controls_survive_temporary_startup_and_replaced_surfaces() {
        for analog in [false, true] {
            let mut preset = Preset {
                analog,
                ..Preset::default()
            };
            let normal = preset.layout(960., 420.);
            preset.capture(&normal, 960., 420.);
            let saved = serde_json::to_vec(&preset).unwrap();
            for (w, h) in [(1., 1.), (1., 420.), (960., 1.), (23., 23.), (0., 0.)] {
                let transient = preset.layout(w, h);
                for i in 0..transient.element_count() {
                    let rect = transient.element(i);
                    assert!(rect.w.is_finite() && rect.h.is_finite());
                    assert!(rect.w > 0. && rect.w <= w.max(1.));
                    assert!(rect.h > 0. && rect.h <= h.max(1.));
                }
            }
            assert_eq!(serde_json::to_vec(&preset).unwrap(), saved);
            let restored = preset.layout(960., 420.);
            assert!((0..normal.element_count())
                .all(|i| normal.element(i).same_bounds(restored.element(i))));
        }
    }
    #[test]
    fn ps2_defaults_mirror_reachable_controls_and_clear_primary_sticks_across_fold_displays() {
        for (w, h) in [
            (360., 800.),
            (443., 961.),
            (960., 360.),
            (960., 420.),
            (852., 817.),
            (817., 852.),
        ] {
            let layout = Preset {
                analog: true,
                ..Preset::default()
            }
            .layout(w, h);
            for i in 0..layout.element_count() {
                let r = layout.element(i);
                assert!(
                    r.x >= 0. && r.y >= 0. && r.x + r.w <= w + 0.01 && r.y + r.h <= h + 0.01,
                    "{w}x{h} {}: {r:?}",
                    layout.element_name(i)
                );
                assert!(!r.overlaps(layout.game));
            }
            for (s, stick) in layout.sticks.iter().enumerate() {
                for i in 0..layout.controls.len() + 5 {
                    assert!(
                        !stick.overlaps(layout.element(i)),
                        "{w}x{h} stick {s} overlaps {}",
                        layout.element_name(i)
                    );
                }
            }
            for c in &layout.controls {
                let r = c.bounds;
                assert_eq!(
                    layout.mask_at(r.x + r.w / 2., r.y + r.h / 2.),
                    c.mask,
                    "{w}x{h} {}",
                    c.label
                );
            }
            assert!(layout
                .controls
                .iter()
                .all(|c| c.bounds.w >= 48. && c.bounds.h >= 48.));
            assert!(layout.sticks.iter().all(|s| s.w >= 104. && s.w == s.h));
            assert!(
                layout.sticks[0].y + layout.sticks[0].h / 2. > layout.dpad.y + layout.dpad.h / 2.
            );
            assert!(
                layout.sticks[1].y + layout.sticks[1].h / 2.
                    > layout.action_bounds().y + layout.action_bounds().h / 2.
            );
            let mirrored = |a: Rect, b: Rect| {
                assert!((a.x + a.w + b.x - w).abs() < 0.02);
                assert!((a.y - b.y).abs() < 0.02);
                assert!((a.w - b.w).abs() < 0.02 && (a.h - b.h).abs() < 0.02);
            };
            mirrored(layout.dpad, layout.action_bounds());
            mirrored(layout.sticks[0], layout.sticks[1]);
            for (left, right) in [
                ("L1", "R1"),
                ("L2", "R2"),
                ("L3", "R3"),
                ("SELECT", "START"),
            ] {
                mirrored(
                    layout
                        .controls
                        .iter()
                        .find(|c| c.label == left)
                        .unwrap()
                        .bounds,
                    layout
                        .controls
                        .iter()
                        .find(|c| c.label == right)
                        .unwrap()
                        .bounds,
                );
            }
            if w > h * 1.35 {
                assert!(layout.game.h >= h - 0.02, "{w}x{h}: {:?}", layout.game);
            }
        }
    }
    #[test]
    fn ps2_square_dock_does_not_add_a_separate_row_for_every_input() {
        for (w, h) in [(852., 817.), (817., 852.), (852., 883.)] {
            let layout = Layout::ps2(w, h);
            assert!(layout.game.h >= h - 410., "{w}x{h}: {:?}", layout.game);
            assert!(layout
                .controls
                .iter()
                .filter(|c| c.is_face())
                .all(|c| c.bounds.w >= 48. && c.bounds.w <= 84.));
        }
    }
    #[test]
    fn dual_analog_digital_and_actions_remain_independent_and_cancel_on_interruption() {
        let preset = Preset {
            analog: true,
            ..Preset::default()
        };
        let layout = preset.layout(960., 420.);
        assert_eq!(layout.sticks.len(), 2);
        assert!((0..layout.element_count()).all(|i| !layout.game.overlaps(layout.element(i))));
        let mut touches = Touches::default();
        for (i, rect) in layout.sticks.iter().enumerate() {
            touches.down(
                i as i32,
                rect.x + rect.w / 2.,
                rect.y + rect.h / 2.,
                &layout,
                true,
            );
        }
        let left = layout.sticks[0];
        let right = layout.sticks[1];
        touches.motion(0, left.x + left.w * 2., left.y + left.h / 2., &layout);
        touches.motion(1, right.x + right.w / 2., right.y - right.h, &layout);
        assert_eq!(touches.axes(), [32767, 0, 0, -32767]);
        let cross = layout
            .controls
            .iter()
            .find(|c| c.label == "X")
            .unwrap()
            .bounds;
        touches.down(
            2,
            cross.x + cross.w / 2.,
            cross.y + cross.h / 2.,
            &layout,
            true,
        );
        let dpad = layout.dpad;
        touches.down(3, dpad.x + 2., dpad.y + dpad.h / 2., &layout, true);
        assert_eq!(touches.mask() & ((1 << 6) | 1), (1 << 6) | 1);
        touches.release(0);
        assert_eq!(touches.axes(), [0, 0, 0, -32767]);
        assert_eq!(touches.mask() & 1, 1);
        touches.cancel();
        assert_eq!(touches.axes(), [0; 4]);
        assert_eq!(touches.frame_mask(), 0);
        let mut captured = preset.clone();
        captured.capture(&layout, 960., 420.);
        let restored = captured.layout(960., 420.);
        assert!(
            (0..layout.element_count()).all(|i| layout.element(i).same_bounds(restored.element(i)))
        );
        let mut margin = restored;
        margin.anchor_toolbar(960., 420., 64., 0., &[]);
        assert!(margin.pause.y < 0.);
        assert!(margin.sticks[0].same_bounds(layout.sticks[0]));
        margin.pause.x -= 20.;
        margin.anchor_toolbar(960., 420., 64., 0., &[]);
        assert_eq!(margin.pause.x, 904.);
    }
    #[test]
    fn ps2_control_profiles_do_not_modify_legacy_ps1_profiles() {
        let root =
            std::env::temp_dir().join(format!("psx-console-controls-{}", std::process::id()));
        let mut ps1 = ControllerSettings::default();
        let custom = Preset {
            game_scale: 0.9,
            ..Preset::default()
        };
        ps1.set(960., 420., custom);
        ps1.save(&root).unwrap();
        let before = fs::read(root.join("controls.json")).unwrap();
        assert!(!String::from_utf8_lossy(&before).contains("analog"));
        let mut ps2 = ControllerSettings::load_ps2(&root.join("ps2-controls")).unwrap();
        ps2.set(960., 420., Preset::default());
        ps2.save(&root.join("ps2-controls")).unwrap();
        assert!(ps2.preset(960., 420.).analog);
        assert_eq!(fs::read(root.join("controls.json")).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn fullscreen_keeps_recorded_fold_landscape_height_while_bars_are_revealed() {
        let size = [2342, 1080];
        let navigation = [0, 0, 0, 59];
        let camera = [0, 0, 182, 0];
        let visible = safe_edges([0, 59, 0, 59], navigation, camera, true);
        let fullscreen = safe_edges([0, 0, 0, 59], navigation, camera, true);
        assert_eq!(safe_content(size, size, visible).h, 962.);
        let content = safe_content(size, size, fullscreen);
        assert_eq!(
            (content.x, content.y, content.w, content.h),
            (0., 0., 2342., 1021.)
        );
        let density = 2.4375;
        let hole = Rect::new(
            2160. / density,
            484. / density,
            182. / density,
            112. / density,
        );
        let layout = Preset::default().layout_with_cutouts(
            content.w / density,
            content.h / density,
            &[hole],
        );
        assert!((layout.game.h * density - 1021.).abs() < 0.01);
        assert!(!layout.game.overlaps(hole));
        assert_eq!(
            safe_edges([0, 40, 0, 0], [0, 0, 0, 78], [0, 136, 0, 0], false),
            [0, 136, 0, 78]
        );
    }

    #[test]
    fn landscape_camera_keeps_the_picture_centered_and_controls_clear() {
        let density = 2.4375;
        let (w, h) = (2342. / density, 1021. / density);
        let old_width = Layout::new(2160. / density, h).game.w;
        for x in [0., 2160. / density] {
            let camera = Rect::new(x, 484. / density, 182. / density, 112. / density);
            let layout = Preset::default().layout_with_cutouts(w, h, &[camera]);
            assert!((layout.game.x + layout.game.w / 2. - w / 2.).abs() < 0.01);
            assert!(layout.game.w >= old_width);
            assert!(!layout.game.overlaps(camera));
            for i in 0..layout.element_count() {
                let r = layout.element(i);
                assert!(
                    !r.overlaps(camera),
                    "camera touches {}",
                    layout.element_name(i)
                );
                assert!(r.x >= 0. && r.y >= 0. && r.x + r.w <= w && r.y + r.h <= h);
            }
            let plain = Layout::new(w, h);
            // Explicitly saved geometry retains its diamond proportions;
            // generated defaults may compact vertically below the camera.
            let mut saved = Preset::default();
            saved.capture(&plain, w, h);
            let custom = saved.layout_with_cutouts(w, h, &[camera]);
            let shifts: Vec<_> = custom
                .controls
                .iter()
                .zip(&plain.controls)
                .filter(|(c, _)| c.is_face())
                .map(|(c, old)| (c.bounds.x - old.bounds.x, c.bounds.y - old.bounds.y))
                .collect();
            assert!(shifts
                .iter()
                .all(|(x, y)| (x - shifts[0].0).abs() < 0.01 && (y - shifts[0].1).abs() < 0.01));
        }
    }
    #[test]
    fn larger_default_thumb_controls_keep_clear_spacing_and_full_fold_landscape_height() {
        for (w, h, camera, min_pad, min_face) in [
            (443., 861., None, 200., 80.),
            (852., 817., None, 224., 65.),
            (
                2342. / 2.4375,
                1021. / 2.4375,
                Some(Rect::new(
                    2160. / 2.4375,
                    484. / 2.4375,
                    182. / 2.4375,
                    112. / 2.4375,
                )),
                184.,
                71.,
            ),
        ] {
            let cutouts: Vec<_> = camera.into_iter().collect();
            let layout = Preset::default().layout_with_cutouts(w, h, &cutouts);
            assert!(layout.dpad.w >= min_pad);
            let faces: Vec<_> = layout
                .action_elements()
                .map(|i| layout.element(i))
                .collect();
            for (i, a) in faces.iter().enumerate() {
                assert!(a.w >= min_face && a.w == a.h);
                for b in faces.iter().skip(i + 1) {
                    let distance =
                        (a.x + a.w / 2. - b.x - b.w / 2.).hypot(a.y + a.h / 2. - b.y - b.h / 2.);
                    assert!(distance > (a.w + b.w) / 2.);
                }
            }
            assert!(faces[2].y - faces[0].y - faces[0].h >= 16. - 0.01);
            assert!((0..layout.element_count()).all(|i| !layout.game.overlaps(layout.element(i))));
            if camera.is_some() {
                assert!((layout.game.h - h).abs() < 0.01);
                assert!((layout.game.x + layout.game.w / 2. - w / 2.).abs() < 0.01);
            }
        }
    }
    #[test]
    fn short_landscape_system_keys_do_not_take_direction_or_face_targets() {
        for (w, h) in [(800., 304.), (861., 360.), (960., 419.)] {
            let layout = Layout::new(w, h);
            for c in layout.controls.iter().filter(|c| !c.is_face()) {
                assert!(!c.bounds.overlaps(layout.dpad), "{} at {w}x{h}", c.label);
                assert!(!layout
                    .controls
                    .iter()
                    .filter(|other| other.is_face())
                    .any(|other| c.bounds.overlaps(other.bounds)));
            }
        }
    }
    #[test]
    fn folding_ignores_stale_metrics_and_keeps_each_saved_profile() {
        let inner = safe_content([2076, 2152], [1080, 2342], [0, 182, 0, 190]);
        assert_eq!((inner.w, inner.h), (2076., 2152.));
        let settled = safe_content([2076, 2152], [2076, 2152], [0, 160, 0, 0]);
        assert_eq!((settled.w, settled.h, settled.y), (2076., 1992., 160.));
        let root = std::env::temp_dir().join(format!("psx-fold-layouts-{}", std::process::id()));
        let mut settings = ControllerSettings::default();
        for (w, h, x) in [(443., 861., 23.), (852., 817., 31.), (861., 443., 19.)] {
            let mut preset = Preset::default();
            let mut layout = preset.layout(w, h);
            layout.dpad.x = x;
            preset.capture(&layout, w, h);
            settings.set(w, h, preset);
        }
        settings.save(&root).unwrap();
        let settings = ControllerSettings::load(&root).unwrap();
        for (w, h, x) in [
            (861., 443., 19.),
            (852., 817., 31.),
            (443., 861., 23.),
            (852., 817., 31.),
        ] {
            assert!((settings.preset(w, h).layout(w, h).dpad.x - x).abs() < 0.01);
        }
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn top_margin_toolbar_avoids_the_camera_and_preserves_the_larger_picture() {
        for (w, h) in [(852., 817.), (817., 852.)] {
            let mut layout = Layout::new(w, h);
            let camera = Rect::new(w - 54., -62., 40., 40.);
            layout.anchor_toolbar(w, h, 66., 0., &[camera]);
            layout.place_game(w, h, false, 1., [0.5, 0.5]);
            assert!(layout.game.w > w * 0.90);
            for r in [layout.pause, layout.speed, layout.playback, layout.mute] {
                assert!(r.y < 0. && r.y + r.h <= 0.);
                assert!(!r.overlaps(camera));
                assert!(!r.overlaps(layout.game));
            }
            layout.pause.x += 25.;
            layout.anchor_toolbar(w, h, 66., 0., &[]);
            assert_eq!(layout.pause.x, w - 56.);
        }
    }

    #[test]
    fn face_targets_accept_near_misses_but_leave_the_picture_and_other_inputs_alone() {
        for (w, h) in [(443., 861.), (852., 817.), (817., 852.), (861., 443.)] {
            let layout = Layout::new(w, h);
            let cross = layout.controls.iter().find(|c| c.mask == 1).unwrap().bounds;
            // Corners outside the drawn circle, and a small miss below it.
            assert_eq!(layout.mask_at(cross.x + 4., cross.y + cross.h - 4.), 1);
            assert_eq!(
                layout.mask_at(cross.x + cross.w / 2., cross.y + cross.h + 6.),
                1
            );
            let game = layout.game;
            for px in [0.01, 0.5, 0.99] {
                for py in [0.01, 0.5, 0.99] {
                    assert_eq!(
                        layout.mask_at(game.x + game.w * px, game.y + game.h * py),
                        0
                    );
                }
            }
            let mut touches = Touches::default();
            let x = cross.x + cross.w / 2.;
            let y = cross.y + cross.h / 2.;
            assert_eq!(touches.down(1, x, y, &layout, true), 1);
            assert_eq!(touches.motion(1, x, cross.y + cross.h + 6., &layout), 0);
            assert_eq!(touches.mask(), 1);
            touches.release(1);
            assert_eq!(touches.mask(), 0);
        }
    }

    #[test]
    fn square_dock_grows_the_picture_and_retains_usable_face_targets() {
        let (w, h) = (852., 817.);
        let mut previous = Layout::arranged(w, h, false);
        let mut layout = Layout::new(w, h);
        for candidate in [&mut previous, &mut layout] {
            candidate.anchor_toolbar(w, h, 66., 0., &[]);
            candidate.place_game(w, h, false, 1., [0.5, 0.5]);
        }
        assert!(layout.game.w * layout.game.h > previous.game.w * previous.game.h * 1.5);
        assert!(layout.game.w > w * 0.90);
        for i in 0..layout.element_count() {
            let before = previous.element(i);
            let after = layout.element(i);
            if i == 0 {
                assert_eq!(before.w, after.w);
                assert!(after.h >= 136.);
            } else if i <= layout.controls.len() && layout.controls[i - 1].is_face() {
                assert!(after.w >= 48. && after.h >= 48.);
            } else {
                assert_eq!((before.w, before.h), (after.w, after.h));
            }
            assert!(!layout.game.overlaps(after));
        }
    }

    #[test]
    fn measured_inner_camera_keeps_the_largest_clear_picture_high_and_inputs_unchanged() {
        let density = 2.4375;
        let (w, h, top) = (
            2076. / density,
            (2152. - 160. - 78.) / density,
            160. / density,
        );
        for analog in [false, true] {
            for camera_x in [0., 1940. / density] {
                let camera = Rect::new(camera_x, -top, 136. / density, top);
                let preset = Preset {
                    analog,
                    ..Preset::default()
                };
                let saved = serde_json::to_vec(&preset).unwrap();
                let mut layout = preset.layout_with_cutouts(w, h, &[camera]);
                let controls: Vec<_> = layout
                    .controller_elements()
                    .map(|i| layout.element(i))
                    .collect();
                let normal = layout.game;
                layout.anchor_toolbar(w, h, top, 0., &[camera]);
                layout.place_game_in(Rect::new(0., -top, w, h + top), false, 1., [0.5; 2]);
                let picture = layout.game;
                assert!((picture.w / picture.h - 4. / 3.).abs() < 0.001);
                assert!(picture.w >= normal.w, "{analog}: {picture:?}");
                assert!(picture.y >= -top && picture.y + picture.h <= h);
                assert!(
                    picture.y <= 0.,
                    "Picture should use clear top-margin space: {picture:?}"
                );
                assert!(!picture.overlaps(camera));
                assert!((0..layout.element_count()).all(|i| !picture.overlaps(layout.element(i))));
                assert!(
                    picture.x < 0.01 || picture.x + picture.w > w - 0.01,
                    "{analog} {camera_x}: {picture:?}"
                );
                // At the actual camera, maximizing the picture reaches a
                // screen edge; it must not sacrifice size to force centering.
                let wider = picture.w + 1.;
                for x in [0., w - wider] {
                    for y in (-160..2152).map(|pixel| pixel as f32 / density) {
                        let candidate = Rect::new(x, y, wider, wider * 3. / 4.);
                        if candidate.y >= -top && candidate.y + candidate.h <= h {
                            assert!(
                                candidate.overlaps(camera)
                                    || (0..layout.element_count())
                                        .any(|i| candidate.overlaps(layout.element(i)))
                            );
                        }
                    }
                }
                layout.anchor_toolbar(w, h, 0., 32., &[camera]);
                layout.place_game(w, h, false, 1., [0.5; 2]);
                assert!(layout.game.y >= 0.);
                layout.anchor_toolbar(w, h, top, 0., &[camera]);
                layout.place_game_in(Rect::new(0., -top, w, h + top), false, 1., [0.5; 2]);
                assert!(picture.same_bounds(layout.game));
                assert_eq!(saved, serde_json::to_vec(&preset).unwrap());
                for (before, i) in controls.iter().zip(layout.controller_elements()) {
                    assert!(before.same_bounds(layout.element(i)));
                }
            }
        }
    }

    #[test]
    fn primary_controls_keep_their_size_while_picture_reuses_only_clear_top_pixels() {
        for analog in [false, true] {
            for (w, h) in [(852., 817.), (817., 852.)] {
                let camera = Rect::new(w - 54., -62., 40., 40.);
                let preset = Preset {
                    analog,
                    ..Preset::default()
                };
                let saved = serde_json::to_vec(&preset).unwrap();
                let mut layout = preset.layout_with_cutouts(w, h, &[camera]);
                let inputs: Vec<_> = layout
                    .controller_elements()
                    .map(|i| layout.element(i))
                    .collect();
                if analog {
                    assert!(layout.sticks.iter().all(|r| r.w >= 112.));
                    assert!(layout.dpad.h >= 160.);
                } else {
                    assert!(layout.dpad.h >= 200.);
                }
                assert!(layout
                    .controls
                    .iter()
                    .filter(|c| c.is_face())
                    .all(|c| c.bounds.w >= 72.));
                let normal = layout.game;
                layout.anchor_toolbar(w, h, 66., 0., &[camera]);
                layout.place_game_in(Rect::new(0., -66., w, h + 66.), false, 1., [0.5; 2]);
                let expanded = layout.game;
                assert!(
                    expanded.w >= normal.w && expanded.w >= w * 0.94,
                    "{analog} {w}x{h}: {expanded:?}"
                );
                assert!(expanded.y >= -66. && expanded.y + expanded.h <= h);
                assert!(!expanded.overlaps(camera));
                assert!((0..layout.element_count()).all(|i| !expanded.overlaps(layout.element(i))));
                layout.anchor_toolbar(w, h, 0., 32., &[camera]);
                layout.place_game(w, h, false, 1., [0.5; 2]);
                assert!(layout.game.y >= 0.);
                for r in [layout.playback, layout.speed, layout.mute, layout.pause] {
                    assert!(r.y >= 32. && !r.overlaps(layout.game));
                }
                layout.anchor_toolbar(w, h, 66., 0., &[camera]);
                layout.place_game_in(Rect::new(0., -66., w, h + 66.), false, 1., [0.5; 2]);
                assert!(expanded.same_bounds(layout.game));
                assert_eq!(saved, serde_json::to_vec(&preset).unwrap());
                for (before, i) in inputs.iter().zip(layout.controller_elements()) {
                    assert!(before.same_bounds(layout.element(i)));
                }
            }
        }
    }

    #[test]
    fn corner_toolbar_tracks_status_clearance_and_returns_to_maximum_picture() {
        for analog in [false, true] {
            for (w, h) in [(443., 961.), (960., 420.), (852., 817.), (817., 852.)] {
                let mut preset = Preset {
                    analog,
                    ..Preset::default()
                };
                let original = preset.layout(w, h);
                preset.capture(&original, w, h);
                let stored = serde_json::to_vec(&preset).unwrap();
                let mut layout = preset.layout(w, h);
                layout.anchor_toolbar(w, h, 66., 0., &[]);
                layout.place_game(w, h, false, 1., [0.5; 2]);
                let picture = layout.game;
                assert!(layout.playback.x < layout.speed.x && layout.speed.x < w / 2.);
                assert!(layout.mute.x > w / 2. && layout.pause.x > layout.mute.x);
                if w >= 800. && w < h * 1.35 {
                    assert!(picture.w >= w * 0.90, "{analog} {w}x{h}: {picture:?}");
                }
                layout.anchor_toolbar(w, h, 0., 32., &[]);
                layout.place_game(w, h, false, 1., [0.5; 2]);
                for r in [layout.playback, layout.speed, layout.mute, layout.pause] {
                    assert!(r.y >= 32. && !r.overlaps(layout.game));
                }
                layout.anchor_toolbar(w, h, 66., 0., &[]);
                layout.place_game(w, h, false, 1., [0.5; 2]);
                assert!(picture.same_bounds(layout.game));
                assert_eq!(stored, serde_json::to_vec(&preset).unwrap());
                for i in layout.controller_elements() {
                    assert!(original.element(i).same_bounds(layout.element(i)));
                }
            }
        }
    }

    #[test]
    fn default_arrangement_chooses_more_picture_space_across_display_shapes() {
        for (w, h) in [
            (443., 861.),
            (852., 785.),
            (785., 852.),
            (861., 443.),
            (800., 280.),
        ] {
            let rails = Layout::arranged(w, h, false);
            let chosen = Layout::new(w, h);
            assert!(chosen.game.w >= rails.game.w - 0.01);
            assert!(chosen.dpad.w >= 112. && chosen.dpad.h >= 112.);
        }
    }

    #[test]
    fn folded_landscape_uses_side_rails_without_a_bottom_row_blocking_the_picture() {
        let layout = Layout::new(886., 443.);
        assert!(layout.game.h > 443. * 0.9);
        assert!(layout.dpad.w >= 160.);
        let bounds = |label| {
            layout
                .controls
                .iter()
                .find(|c| c.label == label)
                .unwrap()
                .bounds
        };
        assert!(bounds("L1").x < bounds("L2").x);
        assert!(bounds("R1").x > bounds("R2").x);
        assert_eq!(bounds("L1").y, bounds("L2").y);
        assert_eq!(bounds("R1").y, bounds("R2").y);
        assert!(bounds("SELECT").x + bounds("SELECT").w / 2. > layout.dpad.x + layout.dpad.w / 2.);
        assert!(
            bounds("START").x + bounds("START").w / 2. < 886. - layout.dpad.x - layout.dpad.w / 2.
        );
        for control in &layout.controls {
            assert!(!layout.game.overlaps(control.bounds));
            assert!(control.bounds.w >= 48. && control.bounds.h >= 48.);
        }
    }

    #[test]
    fn thumb_clearance_keeps_the_maximum_picture_and_other_button_rows_separate() {
        for (w, h) in [(852., 817.), (817., 852.), (886., 443.)] {
            let mut layout = Layout::new(w, h);
            layout.anchor_toolbar(w, h, 66., 0., &[]);
            layout.place_game(w, h, false, 1., [0.5, 0.5]);
            assert!(layout.dpad.y + layout.dpad.h <= h - 32. + 0.01);
            for face in layout.controls.iter().filter(|c| c.is_face()) {
                assert!(face.bounds.y + face.bounds.h <= h - 32. + 0.01);
            }
            let triangle = layout
                .controls
                .iter()
                .find(|c| c.label == "TRI")
                .unwrap()
                .bounds;
            let cross = layout
                .controls
                .iter()
                .find(|c| c.label == "X")
                .unwrap()
                .bounds;
            assert!(cross.y - triangle.y - triangle.h >= 16. - 0.01);
            for (i, a) in layout.controls.iter().enumerate() {
                for b in layout.controls.iter().skip(i + 1) {
                    if !a.is_face() || !b.is_face() {
                        assert!(!a.bounds.overlaps(b.bounds));
                    }
                }
            }
            if w / h < 1.35 {
                assert!(layout.game.w >= w * 0.90);
            } else {
                assert!(layout.game.h >= h * 0.9);
            }
        }
    }

    #[test]
    fn captured_dpad_survives_leaving_the_well_and_releases_independently() {
        let layout = Layout::new(852., 817.);
        let r = layout.dpad;
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        let mut touches = Touches::default();
        assert_eq!(touches.down(1, x + r.w * 0.3, y, &layout, true), 1 << 7);
        assert_eq!(touches.motion(1, r.x + r.w + 100., y, &layout), 0);
        assert_eq!(touches.mask(), 1 << 7);
        let cross = layout.controls.iter().find(|c| c.mask == 1).unwrap().bounds;
        let (bx, by) = (cross.x + cross.w / 2., cross.y + cross.h / 2.);
        assert_eq!(touches.down(2, bx, by, &layout, true), 1);
        touches.motion(1, bx, by, &layout);
        touches.release(2);
        assert_eq!(touches.mask() & 1, 0); // direction finger cannot become Cross
        assert_ne!(touches.mask() & DIRECTIONS, 0);
        touches.release(1);
        assert_eq!(touches.mask(), 0);
        assert!(touches.dpad_feedback().is_none());
        touches.down(1, x, y, &layout, true);
        touches.motion(1, x + 100., y, &layout);
        touches.cancel();
        assert_eq!(touches.frame_mask(), 0);
        assert!(touches.dpad_feedback().is_none());
    }

    #[test]
    fn following_center_allows_reversing_after_thumb_drift() {
        let layout = Layout::new(852., 817.);
        let r = layout.dpad;
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        for follow in [true, false] {
            let mut touches = Touches::default();
            touches.down(1, x, y, &layout, follow);
            touches.motion(1, x + 300., y, &layout);
            assert_eq!(touches.mask(), 1 << 7);
            touches.motion(1, x + 200., y, &layout);
            assert_eq!(touches.mask(), 1 << if follow { 6 } else { 7 });
            let feedback = touches.dpad_feedback().unwrap();
            assert_eq!(feedback.center[0] > 0., follow);
            assert!(feedback.offset[0].abs() <= 1.);
            // Another finger cannot take over the captured direction gesture.
            touches.down(2, x - 50., y, &layout, follow);
            assert_eq!(touches.mask(), 1 << if follow { 6 } else { 7 });
        }
    }

    #[test]
    fn fast_direction_changes_do_not_latch_opposites_in_one_guest_frame() {
        let layout = Layout::new(852., 817.);
        let r = layout.dpad;
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        let mut touches = Touches::default();
        touches.down(1, x + 50., y, &layout, false);
        touches.update(2, 1);
        touches.release(2);
        touches.motion(1, x - 50., y, &layout);
        assert_eq!(touches.frame_mask(), (1 << 6) | 1);
        assert_eq!(touches.frame_mask(), 1 << 6);
        touches.motion(1, x, y, &layout);
        assert_eq!(touches.frame_mask(), 0);
    }

    #[test]
    fn dpad_dead_zone_and_direction_boundaries_have_hysteresis() {
        let layout = Layout::new(852., 817.);
        let r = layout.dpad;
        let (x, y) = (r.x + r.w / 2., r.y + r.h / 2.);
        let mut touches = Touches::default();
        touches.down(1, x, y, &layout, true);
        for (degrees, expected) in [
            (20_f32, 128),
            (24., 128),
            (31., 160),
            (24., 160),
            (16., 128),
        ] {
            let angle = degrees.to_radians();
            touches.motion(1, x + angle.cos() * 50., y + angle.sin() * 50., &layout);
            assert_eq!(touches.mask(), expected);
        }
        for (fraction, expected) in [
            (0.095, 128),
            (0.08, 128),
            (0.06, 0),
            (0.08, 0),
            (0.095, 128),
        ] {
            touches.motion(1, x + r.w * fraction, y, &layout);
            assert_eq!(touches.mask(), expected);
        }
    }

    #[test]
    fn existing_layouts_gain_following_without_losing_customization() {
        let mut preset = Preset::default();
        let mut layout = preset.layout(852., 817.);
        layout.dpad.x += 25.;
        preset.capture(&layout, 852., 817.);
        let mut old = serde_json::to_value(&preset).unwrap();
        old.as_object_mut().unwrap().remove("follow_thumb");
        let restored: Preset = serde_json::from_value(old).unwrap();
        assert!(restored.follow_thumb);
        assert!(restored.layout(852., 817.).dpad.same_bounds(layout.dpad));
        let mut fixed = Preset::default();
        assert!(fixed.is_default(852., 817.));
        fixed.follow_thumb = false;
        assert!(!fixed.is_default(852., 817.));
    }

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
                .chain([layout.dpad])
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
            assert!(layout.pause.w >= 40. && layout.pause.h >= 32.);
            assert!(!layout.game.overlaps(layout.pause));
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
