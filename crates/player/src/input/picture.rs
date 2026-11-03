//! Picture-first composition. Controller feasibility never changes the picture.
use super::{Layout, Rect};

const GAP: f32 = 8.;
const KEY: f32 = 44.;
const MIN_WELL: f32 = 112.; // Four separate 44-point face caps, with real gaps.

fn diamond(layout: &mut Layout, rect: Rect) {
    let cap = (rect.w - 4. * std::f32::consts::SQRT_2) / (1. + std::f32::consts::SQRT_2);
    let step = (rect.w - cap) / 2.;
    let x = rect.x + (rect.w - cap) / 2.;
    let y = rect.y + (rect.h - cap) / 2.;
    for c in &mut layout.controls {
        let (dx, dy) = match c.label {
            "TRI" => (0., -step),
            "O" => (step, 0.),
            "X" => (0., step),
            "SQ" => (-step, 0.),
            _ => continue,
        };
        c.bounds = Rect::new(x + dx, y + dy, cap, cap);
    }
}

fn auxiliary(layout: &mut Layout, left: Rect, right: Rect, shoulders: f32, system: f32) {
    let pair = ((left.w - 12. - GAP) / 2.).clamp(KEY, 64.);
    let edge = left.x + (left.w - 2. * pair - GAP) / 2.;
    let right_edge = right.x + (right.w - 2. * pair - GAP) / 2.;
    for c in &mut layout.controls {
        c.bounds = match c.label {
            "L1" => Rect::new(edge, shoulders, pair, KEY),
            "L2" => Rect::new(edge + pair + GAP, shoulders, pair, KEY),
            "R1" => Rect::new(right_edge + pair + GAP, shoulders, pair, KEY),
            "R2" => Rect::new(right_edge, shoulders, pair, KEY),
            "SELECT" => Rect::new(left.x + 6., system, 64., KEY),
            "START" => Rect::new(right.x + right.w - 70., system, 64., KEY),
            "L3" => Rect::new(left.x + left.w - 50., system, KEY, KEY),
            "R3" => Rect::new(right.x + 6., system, KEY, KEY),
            _ => continue,
        };
    }
}

fn vertical(w: f32, h: f32, left: Rect, right: Rect, analog: bool, psp: bool) -> Option<Layout> {
    let width = left.w.min(right.w);
    let top = left.y.max(right.y).max(24.);
    let bottom = (left.y + left.h).min(right.y + right.h) - 14.;
    let stick = if analog {
        (width - 12.)
            .clamp(96., 128.)
            .min(bottom - top - 2. * KEY - 3. * GAP - MIN_WELL)
    } else {
        0.
    };
    if analog && stick < 96. {
        return None;
    }
    let well = (width - 12.)
        .min(164.)
        .min(bottom - top - 2. * KEY - 2. * GAP - if analog { stick + GAP } else { 0. });
    if well < MIN_WELL || width < 124. {
        return None;
    }
    let mut layout = if psp {
        Layout::psp(w, h)
    } else if analog {
        Layout::analog_default(w, h)
    } else {
        Layout::arranged(w, h, false)
    };
    let system = bottom - KEY;
    let primary = system - GAP - stick;
    let digital = if analog {
        primary - GAP - well
    } else {
        system - GAP - well
    };
    layout.dpad = Rect::new(left.x + (left.w - well) / 2., digital, well, well);
    diamond(
        &mut layout,
        Rect::new(right.x + (right.w - well) / 2., digital, well, well),
    );
    for (i, rect) in layout.sticks.iter_mut().enumerate() {
        let rail = if i == 0 { left } else { right };
        *rect = Rect::new(rail.x + (rail.w - stick) / 2., primary, stick, stick);
    }
    auxiliary(&mut layout, left, right, digital - GAP - KEY, system);
    Some(layout)
}

fn horizontal(w: f32, h: f32, band: Rect, analog: bool, psp: bool) -> Option<Layout> {
    let columns = if psp {
        3.
    } else if analog {
        4.
    } else {
        2.
    };
    let required = if analog && !psp { 240. } else { 136. };
    let between_wells = if psp {
        GAP
    } else if analog {
        2. * GAP
    } else {
        0.
    };
    let well = ((w - 24. - between_wells - 2. * GAP - required) / columns)
        .min(band.h - KEY - 2. * GAP - 12.)
        .min(164.);
    if well < MIN_WELL {
        return None;
    }
    let mut layout = if psp {
        Layout::psp(w, h)
    } else if analog {
        Layout::analog_default(w, h)
    } else {
        Layout::arranged(w, h, false)
    };
    let top = band.y + (band.h - well - KEY - GAP) / 2.;
    let y = top + KEY + GAP;
    let digital = if analog { 12. + well + GAP } else { 12. };
    layout.dpad = Rect::new(digital, y, well, well);
    let right_digital = if psp { 12. } else { digital };
    diamond(
        &mut layout,
        Rect::new(w - right_digital - well, y, well, well),
    );
    for (i, rect) in layout.sticks.iter_mut().enumerate() {
        *rect = Rect::new(if i == 0 { 12. } else { w - 12. - well }, y, well, well);
    }
    let span = if analog { 2. * well + GAP } else { well };
    auxiliary(
        &mut layout,
        Rect::new(12., 0., span, 0.),
        Rect::new(w - 12. - span, 0., span, 0.),
        top,
        y + well - KEY,
    );
    // System/click keys use the clear center between the two controller halves,
    // never the thumb-travel corridor between a stick and its secondary well.
    let middle_left = digital + well + GAP;
    let middle_right = w - right_digital - well - GAP;
    if middle_right - middle_left < required {
        return None;
    }
    let center = (middle_left + middle_right) / 2.;
    for c in &mut layout.controls {
        let x = match c.label {
            "SELECT" => center - 68.,
            "START" => center + 4.,
            "L3" => center - 120.,
            "R3" => center + 76.,
            _ => continue,
        };
        c.bounds.x = x;
    }
    Some(layout)
}

fn fits(layout: &Layout, w: f32, h: f32, game: Rect, holes: &[Rect]) -> bool {
    let inputs = layout.controller_elements().collect::<Vec<_>>();
    let separate = |i: usize, j: usize| {
        let a = layout.element(i);
        let b = layout.element(j);
        if !a.overlaps(b) {
            return true;
        }
        let face = |index: usize| {
            index
                .checked_sub(1)
                .and_then(|n| layout.controls.get(n))
                .is_some_and(|c| c.is_face())
        };
        // Face caps are circles. Their existing forgiving rectangular targets
        // choose the nearest symbol in unused space; never mistake the corners
        // of their bounding boxes for intersecting visible caps.
        face(i)
            && face(j)
            && (a.x + a.w / 2. - b.x - b.w / 2.).hypot(a.y + a.h / 2. - b.y - b.h / 2.)
                >= (a.w + b.w) / 2. + 3.99
    };
    inputs.iter().enumerate().all(|(n, i)| {
        let r = layout.element(*i);
        r.x >= 0.
            && r.y >= 0.
            && r.x + r.w <= w + 0.01
            && r.y + r.h <= h + 0.01
            && !r.overlaps(game)
            && !holes.iter().any(|hole| r.overlaps(*hole))
            && inputs[n + 1..].iter().all(|j| separate(*i, *j))
    })
}

/// No smaller picture, hidden control, tiny cap or implicit overlay is a fallback.
/// A refused packing needs an explicit player choice, independently of emulation.
pub(super) fn controls(
    w: f32,
    h: f32,
    game: Rect,
    analog: bool,
    psp: bool,
    holes: &[Rect],
) -> Option<Layout> {
    let bottom = (game.y + game.h).clamp(0., h);
    let band = Rect::new(0., bottom, w, h - bottom);
    let left = Rect::new(0., 0., game.x.clamp(0., w), h);
    let right_x = (game.x + game.w).clamp(0., w);
    let right = Rect::new(right_x, 0., w - right_x, h);
    let candidates = [
        // A roomy inner display can keep the main thumb areas level, with
        // secondary inputs beside them. Use a stack only on narrow surfaces.
        horizontal(w, h, band, analog, psp),
        vertical(
            w,
            h,
            Rect::new(0., bottom, w / 2., band.h),
            Rect::new(w / 2., bottom, w / 2., band.h),
            analog,
            psp,
        ),
        vertical(w, h, left, right, analog, psp),
    ];
    candidates
        .into_iter()
        .flatten()
        .find(|layout| fits(layout, w, h, game, holes))
}

#[cfg(test)]
mod tests {
    use super::super::{PictureFit, PlayerViewport, Preset};
    use super::*;

    #[test]
    fn entire_picture_below_compact_toolbar_is_independent_of_inputs_and_transient_chrome() {
        for (w, h, top, bottom) in [
            (393., 829., 0., 24.),
            (829., 393., 0., 24.),
            (840., 900., 48., 24.),
            (900., 840., 36., 24.),
        ] {
            let viewport = PlayerViewport {
                controls: [w, h - bottom],
                picture: Rect::new(0., -top, w, h + top),
                reusable_top: top,
                covered_top: 0.,
            };
            let hole = Rect::new(0., 0., 28., 42.);
            for (analog, psp) in [(false, false), (true, false), (true, true)] {
                let preset = Preset {
                    analog,
                    psp,
                    ..Preset::default()
                };
                let layout = viewport.layout(&preset, &[hole]);
                let aspect = if psp { 480. / 272. } else { 4. / 3. };
                let width = w.min((h + top - super::super::TOOLBAR_HEIGHT) * aspect);
                let expected = Rect::new(
                    (w - width) / 2.,
                    -top + super::super::TOOLBAR_HEIGHT,
                    width,
                    width / aspect,
                );
                assert!(layout.game.same_bounds(expected));
                assert!(PlayerViewport {
                    covered_top: 22.,
                    ..viewport
                }
                .layout(&preset, &[hole])
                .game
                .same_bounds(expected));
                // Explicitly moving controls into the image cannot silently shrink it.
                let mut custom = preset.clone();
                custom.capture(&layout, w, h - bottom);
                custom.elements[0] = Rect::new(0.3, 0.2, 0.4, 0.4);
                let moved = viewport.layout(&custom, &[hole]);
                assert!(moved.game.same_bounds(expected));
                assert!(custom.needs_control_space(&moved));
                let original = custom.elements.clone();
                custom.choose_control_space(true);
                assert!(!custom.needs_control_space(&viewport.layout(&custom, &[hole])));
                assert_eq!(custom.elements, original);
                assert!(viewport.layout(&custom, &[hole]).game.same_bounds(expected));
                custom.choose_control_space(false);
                let clear = viewport.layout(&custom, &[hole]);
                assert_eq!(custom.picture_fit(), PictureFit::KeepClear);
                assert!(!custom.needs_control_space(&clear));
                assert!(clear
                    .controller_elements()
                    .all(|i| !clear.element(i).overlaps(clear.game)));
                assert_eq!(custom.elements, original);
            }
        }
    }

    #[test]
    fn legacy_custom_profiles_keep_their_picture_policy_and_new_choices_round_trip() {
        let mut preset = Preset {
            game_scale: 0.8,
            game_offset: [0.1, 0.3],
            ..Preset::default()
        };
        preset.elements.push(Rect::new(0.1, 0.5, 0.2, 0.2));
        let mut json = serde_json::to_value(&preset).unwrap();
        json.as_object_mut().unwrap().remove("picture_fit");
        let legacy: Preset = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(legacy.picture_fit(), PictureFit::KeepClear);
        assert_eq!(legacy.elements, preset.elements);
        assert_eq!(legacy.game_scale, 0.8);
        assert_eq!(legacy.game_offset, [0.1, 0.3]);
        json["elements"] = serde_json::json!([]);
        assert_eq!(
            serde_json::from_value::<Preset>(json.clone())
                .unwrap()
                .picture_fit(),
            PictureFit::KeepClear
        );
        json["game_scale"] = serde_json::json!(1.);
        json["game_offset"] = serde_json::json!([0.5, 0.5]);
        json["elements"] = serde_json::to_value(&preset.elements).unwrap();
        let default_picture: Preset = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(default_picture.picture_fit(), PictureFit::Maximum);
        assert_eq!(default_picture.elements, preset.elements);
        assert_eq!(
            serde_json::from_value::<Preset>(json)
                .unwrap()
                .picture_fit(),
            PictureFit::Maximum
        );
        for overlay in [false, true] {
            preset.choose_control_space(overlay);
            let restored: Preset =
                serde_json::from_slice(&serde_json::to_vec(&preset).unwrap()).unwrap();
            assert_eq!(
                restored.picture_fit(),
                if overlay {
                    PictureFit::Maximum
                } else {
                    PictureFit::KeepClear
                }
            );
            assert_eq!(restored.allow_overlap, overlay);
            assert_eq!(restored.elements, preset.elements);
        }
    }
    #[test]
    fn packs_console_inputs_after_full_picture_without_moving_it() {
        for (w, h) in [(393., 829.), (829., 393.), (840., 900.), (900., 840.)] {
            for (analog, psp) in [(false, false), (true, false), (true, true)] {
                let aspect = if psp { 480. / 272. } else { 4. / 3. };
                let game = super::super::clear_picture_aspect(w, h, &[], aspect);
                if let Some(layout) = controls(w, h, game, analog, psp, &[]) {
                    assert!(fits(&layout, w, h, game, &[]));
                    assert_eq!(
                        layout.sticks.len(),
                        if psp {
                            1
                        } else if analog {
                            2
                        } else {
                            0
                        }
                    );
                    assert!(layout
                        .controls
                        .iter()
                        .filter(|c| c.is_face())
                        .all(|c| c.bounds.w >= KEY));
                } else {
                    // A PSP picture on a wide phone leaves genuinely narrow rails.
                    // This is a choice requirement, never permission to shrink it.
                    assert!(
                        psp && w > h || h - game.h < 240.,
                        "{w}x{h}, analog={analog}, psp={psp}, game={game:?}, candidate={:?}",
                        vertical(
                            w,
                            h,
                            Rect::new(0., game.h, w / 2., h - game.h),
                            Rect::new(w / 2., game.h, w / 2., h - game.h),
                            analog,
                            psp
                        )
                    );
                }
                assert!((game.w - w).abs() < 0.01 || (game.h - h).abs() < 0.01);
            }
        }
    }
    #[test]
    fn refuses_insufficient_space_and_camera_conflicts_without_overlay() {
        let game = Rect::new(68., 0., 693., 393.);
        assert!(controls(829., 393., game, true, true, &[]).is_none());
        let game = Rect::new(152.5, 0., 524., 393.);
        let hole = Rect::new(0., 0., 152.5, 393.);
        assert!(controls(829., 393., game, false, false, &[hole]).is_none());
    }

    #[test]
    fn gesture_safe_height_and_shallow_inner_space_do_not_force_a_smaller_picture() {
        let game = Rect::new((829. - 393. * 4. / 3.) / 2., 0., 393. * 4. / 3., 393.);
        let rails = controls(829., 369., game, true, false, &[]).unwrap();
        assert!(fits(&rails, 829., 369., game, &[]));
        assert!(rails.sticks.iter().all(|r| r.w >= 96.));
        let game = Rect::new(0., -65., 852., 639.);
        let band = controls(852., 785., game, true, false, &[]).unwrap();
        assert!(fits(&band, 852., 785., game, &[]));
        assert!(band
            .controls
            .iter()
            .filter(|c| c.is_face())
            .all(|c| c.bounds.w >= KEY));
    }

    #[test]
    fn roomy_psp_layout_levels_primary_thumbs_and_keeps_system_keys_in_the_center() {
        // Observed unfolded surface, in logical points, with the shared row.
        let w = 2076. / 2.4375;
        let h = 1914. / 2.4375;
        let game = Rect::new(0., -160. / 2.4375 + 18., w, w * 272. / 480.);
        let layout = controls(w, h, game, true, true, &[]).unwrap();
        assert!(fits(&layout, w, h, game, &[]));
        let stick = layout.sticks[0];
        let face = layout.action_bounds();
        assert!((stick.y + stick.h / 2. - face.y - face.h / 2.).abs() < 0.01);
        assert!(stick.w >= 160.);
        assert!(layout.dpad.x >= stick.x + stick.w + GAP - 0.01);
        for c in &layout.controls {
            if matches!(c.label, "SELECT" | "START") {
                assert!(c.bounds.x > layout.dpad.x + layout.dpad.w);
                assert!(c.bounds.x + c.bounds.w < face.x);
            }
        }
    }
}
