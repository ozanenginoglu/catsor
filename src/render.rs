use crate::cat::{
    Activity, BODY_NODES, CatSim, CoatStyle, Gait, TOTAL_NODES, pounce_stretch, smoothstep,
};
use glam::Vec2;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, PathBuilder, PixmapMut, Stroke, Transform,
};

struct Palette {
    base: Color,
    flank_shade: Color,
    outline: Color,
    crease: Color,
    ear_outer: Color,
    ear_inner: Color,
    nose: Color,
    patch_primary: Option<Color>,
    patch_primary_shade: Option<Color>,
    patch_secondary: Option<Color>,
}

impl Palette {
    fn for_coat(coat: CoatStyle) -> Self {
        match coat {
            CoatStyle::Calico => Self {
                base: rgba(238, 235, 230, 255),
                flank_shade: rgba(210, 205, 198, 255),
                outline: rgba(68, 56, 48, 255),
                crease: rgba(168, 160, 150, 210),
                ear_outer: rgba(226, 134, 66, 255),
                ear_inner: rgba(178, 88, 34, 255),
                nose: rgba(68, 52, 46, 255),
                patch_primary: Some(rgba(220, 124, 54, 255)),
                patch_primary_shade: Some(rgba(190, 96, 34, 255)),
                patch_secondary: Some(rgba(58, 60, 66, 255)),
            },
            CoatStyle::Brown => Self {
                base: rgba(154, 118, 88, 255),
                flank_shade: rgba(126, 94, 68, 255),
                outline: rgba(56, 38, 24, 255),
                crease: rgba(94, 66, 44, 225),
                ear_outer: rgba(146, 110, 80, 255),
                ear_inner: rgba(98, 66, 42, 255),
                nose: rgba(44, 28, 18, 255),
                patch_primary: None,
                patch_primary_shade: None,
                patch_secondary: None,
            },
            CoatStyle::Charcoal => Self {
                base: rgba(62, 62, 66, 255),
                flank_shade: rgba(42, 42, 46, 255),
                outline: rgba(18, 18, 20, 255),
                crease: rgba(26, 26, 30, 235),
                ear_outer: rgba(56, 56, 60, 255),
                ear_inner: rgba(32, 32, 36, 255),
                nose: rgba(16, 16, 18, 255),
                patch_primary: None,
                patch_primary_shade: None,
                patch_secondary: None,
            },
            CoatStyle::Tuxedo => Self {
                base: rgba(232, 233, 236, 255),
                flank_shade: rgba(202, 204, 210, 255),
                outline: rgba(36, 38, 42, 255),
                crease: rgba(156, 158, 166, 210),
                ear_outer: rgba(46, 48, 52, 255),
                ear_inner: rgba(26, 28, 32, 255),
                nose: rgba(54, 54, 58, 255),
                patch_primary: Some(rgba(48, 50, 56, 255)),
                patch_primary_shade: Some(rgba(34, 36, 40, 255)),
                patch_secondary: Some(rgba(78, 80, 88, 255)),
            },
            CoatStyle::Silver => Self {
                base: rgba(222, 223, 226, 255),
                flank_shade: rgba(194, 196, 202, 255),
                outline: rgba(110, 112, 118, 255),
                crease: rgba(162, 164, 170, 215),
                ear_outer: rgba(216, 218, 222, 255),
                ear_inner: rgba(172, 174, 182, 255),
                nose: rgba(92, 92, 98, 255),
                patch_primary: None,
                patch_primary_shade: None,
                patch_secondary: None,
            },
        }
    }
}

#[inline]
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

#[inline]
fn scale_alpha(col: Color, factor: f32) -> Color {
    let a = (col.alpha() * factor.clamp(0.0, 1.0) * 255.0).round() as u8;
    Color::from_rgba8(
        (col.red() * 255.0).round() as u8,
        (col.green() * 255.0).round() as u8,
        (col.blue() * 255.0).round() as u8,
        a,
    )
}

/// Side length of the square cat overlay window for a given cat scale
pub fn cat_window_dim(scale: f32) -> u16 {
    ((108.0 * scale).ceil() as u16).max(64)
}

/// Side length of the square detached food-bowl overlay window for a given cat scale
pub fn bowl_window_dim(scale: f32) -> u16 {
    ((26.0 * scale).ceil() as u16).max(22)
}

/// Renders the cat into `pixmap` where `window_origin` is the top-left screen coordinate of the window.
pub fn render_cat(cat: &CatSim, pixmap: &mut PixmapMut, clip_mask: &mut Mask, window_origin: Vec2) {
    pixmap.fill(Color::TRANSPARENT);

    let s = cat.scale;
    let pal = Palette::for_coat(cat.coat);

    let local = cat.nodes.map(|n| n - window_origin);

    let mut tangents = [Vec2::X; TOTAL_NODES];
    let mut normals = [Vec2::Y; TOTAL_NODES];
    for i in 0..TOTAL_NODES {
        let dir = if i == 0 {
            local[0] - local[1]
        } else if i == BODY_NODES - 1 || i + 1 == TOTAL_NODES {
            local[i - 1] - local[i]
        } else {
            local[i - 1] - local[i + 1]
        };
        let t = dir.normalize_or_zero();
        let t = if t.length_squared() > 0.1 {
            t
        } else {
            Vec2::from_angle(cat.body_angle)
        };
        tangents[i] = t;
        normals[i] = Vec2::new(-t.y, t.x);
    }

    // 1. Soft ground shadow under the body & head
    draw_ground_shadow(cat, &local, &tangents, &normals, pixmap);

    // 1b. Food bowl & depleting kibble on the floor at the placed bowl spot while eating
    if let (Some(bowl_pos), Some(fill_ratio)) = (cat.eating_bowl_pos(), cat.feeding_fill_ratio()) {
        draw_food_bowl(bowl_pos - window_origin, s, fill_ratio, pal.outline, pixmap);
    }
    let paws_on_top = cat.belly_roll > 0.35;

    // 2. Articulated paws (drawn under torso normally, or on top of chest/hips once rolled onto back)
    if !paws_on_top {
        draw_paws(
            cat,
            &local,
            &tangents,
            &normals,
            window_origin,
            &pal,
            pixmap,
        );
    }

    // 3. Sleek rounded S-curve tail (always drawn underneath the body & head so it never covers the face)
    draw_tail(cat, &local, &pal, pixmap);

    // 4. Layered body silhouette, edge-clipped coat patches, exposed belly (with roll animation), and muscle creases
    draw_body(cat, &local, &tangents, &normals, &pal, clip_mask, pixmap);

    if paws_on_top {
        draw_paws(
            cat,
            &local,
            &tangents,
            &normals,
            window_origin,
            &pal,
            pixmap,
        );
    }

    // 5. Head, snout/nose, head patches, and independent 3D triangular ears
    let head_angle = cat.body_angle + cat.head_yaw;
    draw_head_and_ears(cat, local[0], head_angle, s, &pal, clip_mask, pixmap);
}

fn body_half_widths(cat: &CatSim) -> [f32; BODY_NODES] {
    let s = cat.scale;
    let breath_amp = if cat.activity == Activity::Sleeping {
        0.36 * s
    } else if cat.belly_roll > 0.1 {
        0.30 * s
    } else {
        0.24 * s
    };
    let breath = cat.breath_phase.sin() * breath_amp;

    let compact_factor = ((1.60 - cat.posture_extension) / 0.65).clamp(0.0, 1.0);
    let stretch_factor = ((cat.posture_extension - 1.60) / 0.34).clamp(0.0, 1.0);
    let crouch_bonus = if matches!(cat.activity, Activity::StalkWiggle { .. }) {
        1.15 * s
    } else {
        0.0
    };
    let waist_plump = (compact_factor * 1.45 - stretch_factor * 1.10) * s + crouch_bonus * 0.6;
    let hip_plump = (compact_factor * 0.85 - stretch_factor * 0.40) * s + crouch_bonus;
    let shoulder_streamline = stretch_factor * 0.35 * s;

    [
        5.8 * s,                                                      // 0: Under-head neck anchor
        6.5 * s,                                                      // 1: Neck collar
        8.9 * s + breath * 0.6 - shoulder_streamline, // 2: Shoulders / Free-floating Scapula
        7.8 * s + breath + waist_plump,               // 3: Waist / Mid-torso
        8.5 * s + hip_plump,                          // 4: Hips / Haunches
        5.0 * s + (compact_factor * 0.4 - stretch_factor * 0.25) * s, // 5: Rump / Tail base
    ]
}

fn draw_ground_shadow(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale;
    let widths = body_half_widths(cat);
    let bound_lift =
        cat.gallop_weight * (0.5 + 0.5 * (cat.gait_cycle * std::f32::consts::TAU).sin());
    let shadow_offset = Vec2::new((1.4 + 1.1 * bound_lift) * s, (2.0 + 1.6 * bound_lift) * s);

    let mut shifted = *local;
    for p in &mut shifted {
        *p += shadow_offset;
    }

    if let Some(path) = build_torso_contour(
        &shifted,
        tangents,
        normals,
        &widths,
        1.06,
        0.0,
        0.0,
        cat.curl_amount,
    ) {
        let mut paint = Paint::default();
        paint.set_color(rgba(0, 0, 0, 36));
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

/// Renders a full food bowl centred in `pixmap` (carried under the cursor or waiting on the floor)
pub fn render_bowl_overlay(scale: f32, pixmap: &mut PixmapMut) {
    pixmap.fill(Color::TRANSPARENT);
    let center = Vec2::new(pixmap.width() as f32 * 0.5, pixmap.height() as f32 * 0.5);
    draw_food_bowl(center, scale, 1.0, rgba(46, 36, 32, 235), pixmap);
}

fn draw_food_bowl(
    bowl_center: Vec2,
    s: f32,
    fill_ratio: f32,
    outline_col: Color,
    pixmap: &mut PixmapMut,
) {
    let mut paint = Paint::default();

    // 1. Floor drop shadow under the bowl
    let shadow_c = bowl_center + Vec2::new(1.2 * s, 1.8 * s);
    if let Some(shadow_path) = PathBuilder::from_circle(shadow_c.x, shadow_c.y, 9.1 * s) {
        paint.set_color(rgba(0, 0, 0, 44));
        pixmap.fill_path(
            &shadow_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    // 2. Ceramic outer bowl body & rim highlight
    if let Some(outer_path) = PathBuilder::from_circle(bowl_center.x, bowl_center.y, 8.6 * s) {
        paint.set_color(rgba(216, 98, 84, 255));
        pixmap.fill_path(
            &outer_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        if let Some(rim_path) =
            PathBuilder::from_circle(bowl_center.x - 0.35 * s, bowl_center.y - 0.35 * s, 7.5 * s)
        {
            paint.set_color(rgba(238, 132, 116, 255));
            pixmap.fill_path(
                &rim_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        let rim_stroke = Stroke {
            width: (1.05 * s).max(0.85),
            ..Default::default()
        };
        paint.set_color(outline_col);
        pixmap.stroke_path(
            &outer_path,
            &paint,
            &rim_stroke,
            Transform::identity(),
            None,
        );
    }

    // 3. Recessed inner bowl basin
    if let Some(well_path) = PathBuilder::from_circle(bowl_center.x, bowl_center.y, 6.3 * s) {
        paint.set_color(rgba(68, 46, 44, 255));
        pixmap.fill_path(
            &well_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let inner_stroke = Stroke {
            width: (0.8 * s).max(0.65),
            ..Default::default()
        };
        paint.set_color(rgba(42, 28, 28, 220));
        pixmap.stroke_path(
            &well_path,
            &paint,
            &inner_stroke,
            Transform::identity(),
            None,
        );
    }

    // 4. Depleting kibble mound & individual morsels proportional to remaining time
    let fill = fill_ratio.clamp(0.0, 1.0);
    if fill > 0.01 {
        let mound_r = (1.8 + 3.9 * fill.sqrt()) * s;
        if let Some(mound_path) = PathBuilder::from_circle(bowl_center.x, bowl_center.y, mound_r) {
            paint.set_color(rgba(142, 86, 48, 255));
            pixmap.fill_path(
                &mound_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        // Ordered from center outward so as `fill` decreases, outer morsels disappear first
        const KIBBLE_OFFSETS: [(f32, f32, bool); 12] = [
            (0.0, 0.0, true),
            (1.5, 0.4, false),
            (-1.2, 1.1, true),
            (0.3, -1.6, false),
            (-1.5, -0.9, false),
            (2.4, -1.3, true),
            (2.6, 1.6, false),
            (-0.4, 2.8, true),
            (-2.8, 0.8, false),
            (-2.3, -2.1, true),
            (0.8, -3.2, false),
            (3.4, 0.1, true),
        ];
        let total_kibble = KIBBLE_OFFSETS.len();
        let visible_count = ((fill * total_kibble as f32).ceil() as usize).clamp(1, total_kibble);
        let kibble_stroke = Stroke {
            width: (0.55 * s).max(0.45),
            ..Default::default()
        };
        for &(ox, oy, warm_tint) in &KIBBLE_OFFSETS[..visible_count] {
            let kp = bowl_center + Vec2::new(ox * s, oy * s);
            if let Some(kp_path) = PathBuilder::from_circle(kp.x, kp.y, 1.25 * s) {
                paint.set_color(if warm_tint {
                    rgba(194, 122, 68, 255)
                } else {
                    rgba(172, 100, 56, 255)
                });
                pixmap.fill_path(
                    &kp_path,
                    &paint,
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
                paint.set_color(rgba(92, 52, 28, 210));
                pixmap.stroke_path(
                    &kp_path,
                    &paint,
                    &kibble_stroke,
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_paws(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    window_origin: Vec2,
    pal: &Palette,
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale;
    let fwd = tangents[1];
    let side = normals[1];
    let hip_fwd = tangents[4];
    let hip_side = normals[4];

    let paw_color = match cat.coat {
        CoatStyle::Tuxedo | CoatStyle::Calico | CoatStyle::Silver => rgba(236, 234, 230, 255),
        CoatStyle::Brown => rgba(138, 102, 74, 255),
        CoatStyle::Charcoal => rgba(48, 48, 52, 255),
    };

    if cat.belly_roll > 0.04 {
        // Smoothly animated transition onto (and off) the back:
        // paws on `roll_side` extend first as the cat tips onto its side, followed by `-roll_side` paws lifting up!
        let roll = smoothstep(cat.belly_roll);
        let roll_side = cat.belly_roll_side;
        let paddle = (cat.time * 2.6).sin() * roll;
        let sway = roll_side * (0.55 * roll + 0.85 * (1.0 - roll));

        let left_reach = if roll_side >= 0.0 {
            (roll * 1.35).clamp(0.0, 1.0)
        } else {
            ((roll - 0.18) / 0.82).clamp(0.0, 1.0)
        };
        let right_reach = if roll_side < 0.0 {
            (roll * 1.35).clamp(0.0, 1.0)
        } else {
            ((roll - 0.18) / 0.82).clamp(0.0, 1.0)
        };

        if left_reach > 0.05 {
            let lh_hip = local[4] + hip_side * (4.6 * s) + hip_fwd * (0.4 * s);
            let lh_knee = local[4] + hip_side * ((4.8 + 4.0 * left_reach + sway * 0.5) * s)
                - hip_fwd * (1.0 * left_reach * s);
            let lh_tip = local[4] + hip_side * ((5.2 + 5.6 * left_reach + sway) * s)
                - hip_fwd * ((3.6 * left_reach - paddle * 0.8) * s);
            draw_curved_paw(
                lh_hip,
                lh_knee,
                lh_tip,
                2.4 * s,
                paw_color,
                pal.outline,
                pixmap,
            );

            let lf_sh = local[2] + side * (4.6 * s) - fwd * (0.3 * s);
            let lf_elbow = local[2]
                + side * ((4.8 + 4.4 * left_reach + sway * 0.5) * s)
                + fwd * (0.9 * left_reach * s);
            let lf_tip = local[2]
                + side * ((5.0 + 3.5 * left_reach + sway) * s)
                + fwd * ((4.0 * left_reach + paddle * 0.9) * s);
            draw_curved_paw(
                lf_sh,
                lf_elbow,
                lf_tip,
                2.25 * s,
                paw_color,
                pal.outline,
                pixmap,
            );

            let bean_s = ((left_reach - 0.35) / 0.65).clamp(0.0, 1.0) * s;
            if bean_s > 0.15 {
                draw_paw_beans(
                    lh_tip,
                    (lh_tip - lh_knee).normalize_or_zero(),
                    bean_s,
                    pixmap,
                );
                draw_paw_beans(
                    lf_tip,
                    (lf_tip - lf_elbow).normalize_or_zero(),
                    bean_s,
                    pixmap,
                );
            }
        }

        if right_reach > 0.05 {
            let rh_hip = local[4] - hip_side * (4.6 * s) + hip_fwd * (0.4 * s);
            let rh_knee = local[4]
                - hip_side * ((4.8 + 4.0 * right_reach - sway * 0.5) * s)
                - hip_fwd * (1.0 * right_reach * s);
            let rh_tip = local[4]
                - hip_side * ((5.2 + 5.6 * right_reach - sway) * s)
                - hip_fwd * ((3.6 * right_reach + paddle * 0.8) * s);
            draw_curved_paw(
                rh_hip,
                rh_knee,
                rh_tip,
                2.4 * s,
                paw_color,
                pal.outline,
                pixmap,
            );

            let rf_sh = local[2] - side * (4.6 * s) - fwd * (0.3 * s);
            let rf_elbow = local[2] - side * ((4.8 + 4.4 * right_reach - sway * 0.5) * s)
                + fwd * (0.9 * right_reach * s);
            let rf_tip = local[2] - side * ((5.0 + 3.5 * right_reach - sway) * s)
                + fwd * ((4.0 * right_reach - paddle * 0.9) * s);
            draw_curved_paw(
                rf_sh,
                rf_elbow,
                rf_tip,
                2.25 * s,
                paw_color,
                pal.outline,
                pixmap,
            );

            let bean_s = ((right_reach - 0.35) / 0.65).clamp(0.0, 1.0) * s;
            if bean_s > 0.15 {
                draw_paw_beans(
                    rh_tip,
                    (rh_tip - rh_knee).normalize_or_zero(),
                    bean_s,
                    pixmap,
                );
                draw_paw_beans(
                    rf_tip,
                    (rf_tip - rf_elbow).normalize_or_zero(),
                    bean_s,
                    pixmap,
                );
            }
        }
        return;
    }

    match cat.activity {
        Activity::Stretching { progress, .. } => {
            let ext = (progress * std::f32::consts::PI).sin();
            if ext > 0.08 {
                let reach = (8.5 + 6.8 * ext) * s;
                let left_paw = local[0] + fwd * reach + side * (3.8 * s);
                let right_paw = local[0] + fwd * reach - side * (3.8 * s);
                draw_single_paw(
                    local[1] + side * (4.2 * s),
                    left_paw,
                    2.4 * s,
                    paw_color,
                    pal.outline,
                    pixmap,
                );
                draw_single_paw(
                    local[1] - side * (4.2 * s),
                    right_paw,
                    2.4 * s,
                    paw_color,
                    pal.outline,
                    pixmap,
                );
            }
        }
        Activity::Grooming {
            side: groom_side,
            timer,
            ..
        } => {
            let lift = (timer * 7.5).sin() * 0.8 * s;
            let paw_pos = local[0] + fwd * (4.6 * s + lift) + side * (groom_side * 5.8 * s);
            let shoulder_anchor = local[1] + side * (groom_side * 4.8 * s);
            draw_single_paw(
                shoulder_anchor,
                paw_pos,
                2.4 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
        }
        Activity::Eating { .. } => {
            // Planted front paws flanking the chest while crouched in front of the food bowl
            let left_paw = local[0] + fwd * (3.2 * s) + side * (5.2 * s);
            let right_paw = local[0] + fwd * (3.2 * s) - side * (5.2 * s);
            draw_single_paw(
                local[1] + side * (4.5 * s),
                left_paw,
                2.3 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
            draw_single_paw(
                local[1] - side * (4.5 * s),
                right_paw,
                2.3 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
        }
        Activity::PawBatting {
            side: bat_side,
            timer,
            ..
        } => {
            // Rapid playful swatting motion ("tap-tap!") reaching out toward the cursor!
            let swat_wave = (timer * 18.5).sin();
            let reach = (10.2 + 4.8 * swat_wave.max(-0.3)) * s;
            let lateral = (bat_side * 4.2 + swat_wave * 1.8) * s;
            let paw_pos = local[0] + fwd * reach + side * lateral;
            let shoulder_anchor = local[1] + side * (bat_side * 5.0 * s);
            draw_single_paw(
                shoulder_anchor,
                paw_pos,
                2.5 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
        }
        Activity::DramaticFlop {
            side: flop_side, ..
        } => {
            // All 4 legs sticking out to one side in a lazy side-flop!
            let f1 = local[2] + side * (flop_side * 11.2 * s) + fwd * (2.5 * s);
            let f2 = local[2] + side * (flop_side * 10.0 * s) - fwd * (1.8 * s);
            let h1 = local[4] + hip_side * (flop_side * 10.8 * s) + hip_fwd * (1.8 * s);
            let h2 = local[4] + hip_side * (flop_side * 9.6 * s) - hip_fwd * (2.2 * s);
            draw_single_paw(local[2], f1, 2.2 * s, paw_color, pal.outline, pixmap);
            draw_single_paw(local[2], f2, 2.2 * s, paw_color, pal.outline, pixmap);
            draw_single_paw(local[4], h1, 2.2 * s, paw_color, pal.outline, pixmap);
            draw_single_paw(local[4], h2, 2.2 * s, paw_color, pal.outline, pixmap);
        }
        Activity::StalkWiggle { .. } => {
            // Wide planted rear feet gripping the floor during the pre-launch butt wiggle
            let lh = local[4] + hip_side * (9.2 * s) - hip_fwd * (1.5 * s);
            let rh = local[4] - hip_side * (9.2 * s) - hip_fwd * (1.5 * s);
            draw_single_paw(
                local[4] + hip_side * (5.0 * s),
                lh,
                2.3 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
            draw_single_paw(
                local[4] - hip_side * (5.0 * s),
                rh,
                2.3 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
        }
        Activity::Pouncing { progress, .. } => {
            // Leap: both front paws reaching out ahead, both hind paws kicking back after push-off
            let arc = pounce_stretch(progress);
            let lf = local[0] + fwd * ((8.5 + 5.5 * arc) * s) + side * (4.5 * s);
            let rf = local[0] + fwd * ((8.5 + 5.5 * arc) * s) - side * (4.5 * s);
            let lh = local[4] - hip_fwd * ((6.5 + 4.5 * arc) * s) + hip_side * (5.2 * s);
            let rh = local[4] - hip_fwd * ((6.5 + 4.5 * arc) * s) - hip_side * (5.2 * s);
            draw_single_paw(
                local[1] + side * (4.2 * s),
                lf,
                2.4 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
            draw_single_paw(
                local[1] - side * (4.2 * s),
                rf,
                2.4 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
            draw_single_paw(
                local[4] + hip_side * (4.2 * s),
                lh,
                2.3 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
            draw_single_paw(
                local[4] - hip_side * (4.2 * s),
                rh,
                2.3 * s,
                paw_color,
                pal.outline,
                pixmap,
            );
        }
        Activity::Walking
        | Activity::Zoomies { .. }
        | Activity::TailChasing { .. }
        | Activity::Circling { .. }
            if cat.gait_weight > 0.08 =>
        {
            // Legs follow the simulated paws in every gait. Walking and trotting only show the parts
            // that reach past the chest or haunches. Galloping draws every leg: the body covers what
            // is underneath, and swinging hind legs flare out past the flanks (in a rotary gallop the
            // hind feet pass outside the forefeet).
            let galloping = matches!(cat.activity, Activity::Walking | Activity::Zoomies { .. })
                && cat.gait == Gait::Gallop;
            // Forepaws (LF=0, RF=1) reach out ahead of the chest/collar during protraction & early stance
            let fore_anchors = [
                local[1] - fwd * (1.2 * s) + side * (4.8 * s),
                local[1] - fwd * (1.2 * s) - side * (4.8 * s),
            ];
            for (i, &anchor) in fore_anchors.iter().enumerate() {
                let paw_local = cat.paws[i].world_pos - window_origin;
                if galloping || (paw_local - local[2]).dot(fwd) > 6.5 * s {
                    draw_single_paw(anchor, paw_local, 2.2 * s, paw_color, pal.outline, pixmap);
                }
            }

            // Hindpaws (LH=2, RH=3) extend backward behind the haunches during late stance push-off.
            // The visible part is the lower leg coming out from under the back of the haunch, so it
            // is anchored there rather than at the hip, where it would run along the narrowing rump.
            let hind_anchors = [
                local[4] - hip_fwd * (4.5 * s) + hip_side * (3.2 * s),
                local[4] - hip_fwd * (4.5 * s) - hip_side * (3.2 * s),
            ];
            for (i, &anchor) in hind_anchors.iter().enumerate() {
                let mut paw_local = cat.paws[i + 2].world_pos - window_origin;
                if galloping {
                    let outward = if i == 0 { hip_side } else { -hip_side };
                    let flare = (cat.paws[i + 2].swing_progress * std::f32::consts::PI).sin();
                    paw_local += outward * (4.0 * flare * cat.gallop_weight * s);
                }
                if galloping || (paw_local - local[4]).dot(hip_fwd) < -6.0 * s {
                    draw_single_paw(anchor, paw_local, 2.15 * s, paw_color, pal.outline, pixmap);
                }
            }
        }
        _ => {}
    }
}

fn draw_single_paw(
    shoulder_pos: Vec2,
    paw_tip: Vec2,
    radius: f32,
    fill_col: Color,
    outline_col: Color,
    pixmap: &mut PixmapMut,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(shoulder_pos.x, shoulder_pos.y);
    pb.line_to(paw_tip.x, paw_tip.y);
    stroke_limb(pb, radius, fill_col, outline_col, pixmap);
}

fn draw_curved_paw(
    anchor_pos: Vec2,
    joint_pos: Vec2,
    paw_tip: Vec2,
    radius: f32,
    fill_col: Color,
    outline_col: Color,
    pixmap: &mut PixmapMut,
) {
    let mut pb = PathBuilder::new();
    pb.move_to(anchor_pos.x, anchor_pos.y);
    pb.quad_to(joint_pos.x, joint_pos.y, paw_tip.x, paw_tip.y);
    stroke_limb(pb, radius, fill_col, outline_col, pixmap);
}

/// Strokes a limb path as a round-capped outline with a narrower fill on top
fn stroke_limb(
    pb: PathBuilder,
    radius: f32,
    fill_col: Color,
    outline_col: Color,
    pixmap: &mut PixmapMut,
) {
    let Some(path) = pb.finish() else {
        return;
    };
    let mut paint = Paint::default();
    let mut stroke = Stroke {
        width: radius * 2.05,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    paint.set_color(outline_col);
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);

    stroke.width = radius * 1.5;
    paint.set_color(fill_col);
    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

fn draw_paw_beans(paw_tip: Vec2, dir: Vec2, s: f32, pixmap: &mut PixmapMut) {
    let perp = Vec2::new(-dir.y, dir.x);
    let mut paint = Paint::default();
    paint.set_color(rgba(234, 146, 162, 242));

    let pad_center = paw_tip - dir * (0.35 * s);
    if let Some(main_pad) =
        PathBuilder::from_circle(pad_center.x, pad_center.y, (0.92 * s).max(0.7))
    {
        pixmap.fill_path(
            &main_pad,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    for &(fwd_off, side_off) in &[(0.72, -0.78), (0.92, 0.0), (0.72, 0.78)] {
        let bean = paw_tip + dir * (fwd_off * s) + perp * (side_off * s);
        if let Some(toe) = PathBuilder::from_circle(bean.x, bean.y, (0.45 * s).max(0.4)) {
            pixmap.fill_path(&toe, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }
}

fn draw_tail(cat: &CatSim, local: &[Vec2; TOTAL_NODES], pal: &Palette, pixmap: &mut PixmapMut) {
    let s = cat.scale * cat.tail_puff;
    let tail_start = BODY_NODES - 1;

    let mut pb = PathBuilder::new();
    pb.move_to(local[tail_start].x, local[tail_start].y);
    for i in tail_start..(TOTAL_NODES - 1) {
        let mid = (local[i] + local[i + 1]) * 0.5;
        pb.quad_to(local[i].x, local[i].y, mid.x, mid.y);
    }
    let last = local[TOTAL_NODES - 1];
    pb.line_to(last.x, last.y);

    let Some(tail_path) = pb.finish() else {
        return;
    };

    let mut paint = Paint::default();

    let (tail_base_col, tail_shade_col) = match cat.coat {
        CoatStyle::Calico => (rgba(220, 124, 54, 255), rgba(182, 92, 32, 255)),
        _ => (pal.base, pal.flank_shade),
    };

    let mut stroke = Stroke {
        width: 5.2 * s,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    paint.set_color(pal.outline);
    pixmap.stroke_path(&tail_path, &paint, &stroke, Transform::identity(), None);

    stroke.width = 3.7 * s;
    paint.set_color(tail_shade_col);
    pixmap.stroke_path(&tail_path, &paint, &stroke, Transform::identity(), None);

    stroke.width = 2.4 * s;
    paint.set_color(tail_base_col);
    pixmap.stroke_path(&tail_path, &paint, &stroke, Transform::identity(), None);

    if cat.coat == CoatStyle::Calico {
        stroke.width = 3.4 * s;
        for ring_idx in [6usize, 8usize, 10usize] {
            let p0 = local[ring_idx];
            let p1 = local[ring_idx].lerp(local[ring_idx + 1], 0.52);
            let mut ring_pb = PathBuilder::new();
            ring_pb.move_to(p0.x, p0.y);
            ring_pb.line_to(p1.x, p1.y);
            if let Some(rp) = ring_pb.finish() {
                paint.set_color(rgba(178, 82, 24, 235));
                pixmap.stroke_path(&rp, &paint, &stroke, Transform::identity(), None);
            }
        }
    }
}

#[inline]
fn torso_curl_multipliers(curl_amount: f32) -> (f32, f32) {
    // Compress inner (concave) normal offsets during tight curls so inner contour never self-intersects
    let curl_excess = (curl_amount.abs() - 0.18).max(0.0);
    let inner_mult = (1.0 - curl_excess * 1.25).clamp(0.42, 1.0);
    let outer_mult = 1.0 + (curl_excess * 0.25).min(0.12);
    if curl_amount >= 0.0 {
        (inner_mult, outer_mult)
    } else {
        (outer_mult, inner_mult)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_torso_contour(
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    widths: &[f32; BODY_NODES],
    width_scale: f32,
    shoulder_gait_shift: f32,
    hip_gait_shift: f32,
    curl_amount: f32,
) -> Option<tiny_skia::Path> {
    let mut left = [Vec2::ZERO; BODY_NODES];
    let mut right = [Vec2::ZERO; BODY_NODES];
    let (left_curl_mult, right_curl_mult) = torso_curl_multipliers(curl_amount);
    for idx in 0..BODY_NODES {
        let w = widths[idx] * width_scale;
        // Free-floating scapula glide at shoulders (2) and subtle pelvic haunch shift at hips (4)
        let stride_offset = if idx == 2 {
            tangents[idx] * shoulder_gait_shift
        } else if idx == 4 {
            tangents[idx] * hip_gait_shift
        } else {
            Vec2::ZERO
        };
        left[idx] = local[idx] + stride_offset + normals[idx] * (w * left_curl_mult);
        right[idx] = local[idx] - stride_offset - normals[idx] * (w * right_curl_mult);
    }
    let front_cap = local[0] + tangents[0] * (1.5 * width_scale);
    let rump_cap = local[5] - tangents[5] * (4.6 * width_scale);

    let mut pb = PathBuilder::new();
    pb.move_to(front_cap.x, front_cap.y);

    for i in 0..(BODY_NODES - 1) {
        let mid = (left[i] + left[i + 1]) * 0.5;
        pb.quad_to(left[i].x, left[i].y, mid.x, mid.y);
    }
    pb.line_to(left[BODY_NODES - 1].x, left[BODY_NODES - 1].y);

    let rump_ctrl_l = rump_cap + normals[5] * (widths[5] * 0.9 * width_scale);
    let rump_ctrl_r = rump_cap - normals[5] * (widths[5] * 0.9 * width_scale);
    pb.cubic_to(
        rump_ctrl_l.x,
        rump_ctrl_l.y,
        rump_ctrl_r.x,
        rump_ctrl_r.y,
        right[BODY_NODES - 1].x,
        right[BODY_NODES - 1].y,
    );

    for i in (1..BODY_NODES).rev() {
        let mid = (right[i] + right[i - 1]) * 0.5;
        pb.quad_to(right[i].x, right[i].y, mid.x, mid.y);
    }
    pb.quad_to(right[0].x, right[0].y, front_cap.x, front_cap.y);
    pb.close();

    pb.finish()
}

fn draw_body(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    pal: &Palette,
    clip_mask: &mut Mask,
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale;
    let widths = body_half_widths(cat);
    let phase_rad = cat.gait_cycle * std::f32::consts::TAU;
    let shoulder_shift = -(phase_rad.cos()) * cat.gait_weight * 1.60 * s;
    let hip_shift = phase_rad.sin() * cat.gait_weight * 1.05 * s;

    let Some(outer_path) = build_torso_contour(
        local,
        tangents,
        normals,
        &widths,
        1.0,
        shoulder_shift,
        hip_shift,
        cat.curl_amount,
    ) else {
        return;
    };

    let mut paint = Paint::default();

    paint.set_color(pal.flank_shade);
    pixmap.fill_path(
        &outer_path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    if let Some(inner_path) = build_torso_contour(
        local,
        tangents,
        normals,
        &widths,
        0.78,
        shoulder_shift * 0.8,
        hip_shift * 0.8,
        cat.curl_amount,
    ) {
        paint.set_color(pal.base);
        pixmap.fill_path(
            &inner_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    clip_mask.clear();
    clip_mask.fill_path(&outer_path, FillRule::Winding, true, Transform::identity());

    draw_coat_patches(
        cat, local, tangents, normals, &widths, pal, clip_mask, pixmap,
    );

    if cat.belly_roll < 0.65 {
        draw_muscle_creases(
            cat,
            local,
            tangents,
            normals,
            &widths,
            shoulder_shift,
            hip_shift,
            pal,
            clip_mask,
            pixmap,
        );
    }
    if cat.belly_roll > 0.01 {
        draw_exposed_belly(
            cat, local, tangents, normals, &widths, pal, clip_mask, pixmap,
        );
    }
    let stroke = Stroke {
        width: 1.15 * s,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    paint.set_color(pal.outline);
    pixmap.stroke_path(&outer_path, &paint, &stroke, Transform::identity(), None);
}

#[allow(clippy::too_many_arguments)]
fn draw_coat_patches(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    widths: &[f32; BODY_NODES],
    pal: &Palette,
    body_mask: &Mask,
    pixmap: &mut PixmapMut,
) {
    let Some(primary) = pal.patch_primary else {
        return;
    };
    let s = cat.scale;

    draw_blob_patch(
        &[
            local[1] + tangents[1] * (1.5 * s) - normals[1] * (0.5 * s),
            local[2] - normals[2] * (widths[2] * 1.35),
            local[3] - normals[3] * (widths[3] * 1.35),
            local[4] - normals[4] * (widths[4] * 0.85),
            local[3] + normals[3] * (1.8 * s),
        ],
        primary,
        body_mask,
        pixmap,
    );

    if let Some(sec_col) = match cat.coat {
        CoatStyle::Calico => pal.patch_primary_shade,
        CoatStyle::Tuxedo => pal.patch_primary,
        _ => pal.patch_secondary,
    } {
        draw_blob_patch(
            &[
                local[3] + tangents[3] * (2.5 * s) + normals[3] * (1.2 * s),
                local[3] + normals[3] * (widths[3] * 1.35),
                local[4] + normals[4] * (widths[4] * 1.35),
                local[5] + normals[5] * (widths[5] * 1.1),
                local[4] - normals[4] * (0.8 * s),
            ],
            sec_col,
            body_mask,
            pixmap,
        );
    }

    if let Some(spot_col) = pal.patch_secondary {
        draw_blob_patch(
            &[
                local[1] + normals[1] * (widths[1] * 0.25),
                local[1] + normals[1] * (widths[1] * 1.3),
                local[2] + normals[2] * (widths[2] * 1.3),
                local[2] - tangents[2] * (2.5 * s) + normals[2] * (2.2 * s),
            ],
            spot_col,
            body_mask,
            pixmap,
        );
    }
}

fn draw_blob_patch(pts: &[Vec2], color: Color, mask: &Mask, pixmap: &mut PixmapMut) {
    if pts.len() < 3 {
        return;
    }
    let mut pb = PathBuilder::new();
    let start_mid = (pts[pts.len() - 1] + pts[0]) * 0.5;
    pb.move_to(start_mid.x, start_mid.y);
    for i in 0..pts.len() {
        let next = pts[(i + 1) % pts.len()];
        let mid = (pts[i] + next) * 0.5;
        pb.quad_to(pts[i].x, pts[i].y, mid.x, mid.y);
    }
    pb.close();

    if let Some(path) = pb.finish() {
        let mut paint = Paint::default();
        paint.set_color(color);
        pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            Some(mask),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_muscle_creases(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    widths: &[f32; BODY_NODES],
    shoulder_shift: f32,
    hip_shift: f32,
    pal: &Palette,
    body_mask: &Mask,
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale;
    let dorsal_fade = (1.0 - cat.belly_roll / 0.65).clamp(0.0, 1.0);
    let (left_curl_mult, right_curl_mult) = torso_curl_multipliers(cat.curl_amount);

    let mut paint = Paint::default();
    paint.set_color(scale_alpha(pal.crease, dorsal_fade));

    let stroke = Stroke {
        width: 0.9 * s,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };

    for sign in [-1.0f32, 1.0f32] {
        let side_mult = if sign >= 0.0 {
            left_curl_mult
        } else {
            right_curl_mult
        };
        let w2 = widths[2] * side_mult;
        // Scapula muscle creases slide forward/backward with each forelimb's protraction/retraction
        let sh_base = local[2] + tangents[2] * (sign * shoulder_shift * 0.95);
        let p0 = sh_base + tangents[2] * (2.4 * s) + normals[2] * (sign * w2 * 0.74);
        let p1 = sh_base + normals[2] * (sign * w2 * 0.56);
        let p2 = sh_base - tangents[2] * (2.8 * s) + normals[2] * (sign * w2 * 0.68);
        let mut pb = PathBuilder::new();
        pb.move_to(p0.x, p0.y);
        pb.quad_to(p1.x, p1.y, p2.x, p2.y);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                Transform::identity(),
                Some(body_mask),
            );
        }
    }

    for sign in [-1.0f32, 1.0f32] {
        let side_mult = if sign >= 0.0 {
            left_curl_mult
        } else {
            right_curl_mult
        };
        let w4 = widths[4] * side_mult;
        // Haunch muscle creases slide with pelvic yaw as each hindlimb protracts/retracts
        let hip_base = local[4] + tangents[4] * (sign * hip_shift * 0.95);
        let p0 = hip_base + tangents[4] * (2.4 * s) + normals[4] * (sign * w4 * 0.72);
        let p1 = hip_base + normals[4] * (sign * w4 * 0.54);
        let p2 = hip_base - tangents[4] * (2.4 * s) + normals[4] * (sign * w4 * 0.62);
        let mut pb = PathBuilder::new();
        pb.move_to(p0.x, p0.y);
        pb.quad_to(p1.x, p1.y, p2.x, p2.y);
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                Transform::identity(),
                Some(body_mask),
            );
        }
    }
    // When curled up asleep, draw a soft inner curl fold line where the hind leg tucks against the waist
    if cat.activity == Activity::Sleeping {
        let cs = cat.curl_side;
        let inner_mult = if cs >= 0.0 {
            left_curl_mult
        } else {
            right_curl_mult
        };
        let c0 = local[2] + normals[2] * (cs * widths[2] * inner_mult * 0.68);
        let c1 = local[3] + normals[3] * (cs * widths[3] * inner_mult * 0.42);
        let c2 = local[4] + normals[4] * (cs * widths[4] * inner_mult * 0.72);
        let mut cpb = PathBuilder::new();
        cpb.move_to(c0.x, c0.y);
        cpb.quad_to(c1.x, c1.y, c2.x, c2.y);
        if let Some(cpath) = cpb.finish() {
            let fold_stroke = Stroke {
                width: 1.05 * s,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            paint.set_color(pal.outline);
            pixmap.stroke_path(
                &cpath,
                &paint,
                &fold_stroke,
                Transform::identity(),
                Some(body_mask),
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_exposed_belly(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    widths: &[f32; BODY_NODES],
    pal: &Palette,
    body_mask: &Mask,
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale;
    let roll = smoothstep(cat.belly_roll);
    if roll <= 1e-3 {
        return;
    }
    let roll_side = cat.belly_roll_side;
    let (belly_col, tummy_tint) = match cat.coat {
        CoatStyle::Calico => (rgba(248, 245, 240, 248), rgba(244, 198, 198, 155)),
        CoatStyle::Tuxedo => (rgba(246, 247, 250, 252), rgba(240, 198, 204, 145)),
        CoatStyle::Brown => (rgba(218, 188, 156, 244), rgba(236, 192, 174, 145)),
        CoatStyle::Charcoal => (rgba(108, 108, 116, 242), rgba(152, 132, 138, 135)),
        CoatStyle::Silver => (rgba(245, 246, 249, 250), rgba(240, 196, 206, 150)),
    };

    // Smoothly widen and fade in from zero at the `roll_side` flank toward the center as `roll` goes 0 -> 1
    let shift_factor = roll_side * (1.0 - roll) * 0.58;
    let width_scale = roll;

    let shift_pt = |idx: usize, w_mult: f32| -> Vec2 {
        local[idx] + normals[idx] * (widths[idx] * (shift_factor + w_mult * width_scale))
    };

    // Soft fluffy underbelly oval running from chest (node 1) to lower abdomen (node 4/5)
    let belly_pts = [
        local[1] + tangents[1] * (2.4 * s) + normals[1] * (widths[1] * shift_factor),
        shift_pt(2, 0.64),
        shift_pt(3, 0.70),
        shift_pt(4, 0.60),
        local[5] - tangents[5] * (1.2 * s) + normals[5] * (widths[5] * shift_factor),
        shift_pt(4, -0.60),
        shift_pt(3, -0.70),
        shift_pt(2, -0.64),
    ];
    draw_blob_patch(&belly_pts, scale_alpha(belly_col, roll), body_mask, pixmap);

    if roll > 0.20 {
        let tummy_t = smoothstep((roll - 0.20) / 0.80);
        // Warm rosy/soft center tummy oval around waist (nodes 2..4)
        let tummy_pts = [
            local[2] - tangents[2] * (0.6 * s) + normals[2] * (widths[2] * shift_factor),
            shift_pt(3, 0.44),
            local[4] + tangents[4] * (0.4 * s) + normals[4] * (widths[4] * shift_factor),
            shift_pt(3, -0.44),
        ];
        draw_blob_patch(
            &tummy_pts,
            scale_alpha(tummy_tint, tummy_t),
            body_mask,
            pixmap,
        );
    }

    // Fluffy chest tuft & belly fur wisps along the left and right edges of the tummy
    if roll > 0.30 {
        let wisp_t = smoothstep((roll - 0.30) / 0.70);
        let mut paint = Paint::default();
        paint.set_color(scale_alpha(pal.crease, wisp_t));
        let stroke = Stroke {
            width: 0.9 * s,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        for sign in [-1.0f32, 1.0f32] {
            for node_idx in [2usize, 3usize, 4usize] {
                let base = local[node_idx] + normals[node_idx] * (widths[node_idx] * shift_factor);
                let p0 = base
                    + tangents[node_idx] * (2.0 * s)
                    + normals[node_idx] * (sign * widths[node_idx] * 0.62 * width_scale);
                let p1 = base + normals[node_idx] * (sign * widths[node_idx] * 0.46 * width_scale);
                let p2 = base - tangents[node_idx] * (2.0 * s)
                    + normals[node_idx] * (sign * widths[node_idx] * 0.60 * width_scale);
                let mut pb = PathBuilder::new();
                pb.move_to(p0.x, p0.y);
                pb.quad_to(p1.x, p1.y, p2.x, p2.y);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(
                        &path,
                        &paint,
                        &stroke,
                        Transform::identity(),
                        Some(body_mask),
                    );
                }
            }
        }
    }
}

fn draw_head_and_ears(
    cat: &CatSim,
    head_center: Vec2,
    head_angle: f32,
    s: f32,
    pal: &Palette,
    clip_mask: &mut Mask,
    pixmap: &mut PixmapMut,
) {
    let fwd = Vec2::from_angle(head_angle);
    let side = Vec2::new(-fwd.y, fwd.x);

    let mut paint = Paint::default();

    let roll = smoothstep(cat.belly_roll);
    let belly_up = roll > 0.45;
    let ear_bias = if cat.activity == Activity::Sleeping {
        -0.14
    } else {
        -0.22 * roll
    };
    let ear_head_center = head_center + fwd * (1.4 * roll * s);

    // When lying on its back, the ears rest against the floor at the crown (+fwd) underneath the head silhouette
    if belly_up {
        draw_ear(
            ear_head_center,
            head_angle,
            1.0,
            cat.ear_left_twitch + ear_bias,
            s,
            pal,
            pixmap,
        );
        draw_ear(
            ear_head_center,
            head_angle,
            -1.0,
            cat.ear_right_twitch + ear_bias,
            s,
            pal,
            pixmap,
        );
    }

    let neck_shadow_center = head_center - fwd * (1.8 * s);
    if let Some(shadow_path) = build_head_path_rolled(neck_shadow_center, fwd, side, s * 1.03, roll)
    {
        paint.set_color(rgba(0, 0, 0, 32));
        pixmap.fill_path(
            &shadow_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let Some(outer_head) = build_head_path_rolled(head_center, fwd, side, s, roll) else {
        return;
    };
    paint.set_color(pal.flank_shade);
    pixmap.fill_path(
        &outer_head,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    let inner_offset = fwd * (0.35 * (1.0 - 2.0 * roll) * s);
    if let Some(inner_head) =
        build_head_path_rolled(head_center + inner_offset, fwd, side, s * 0.84, roll)
    {
        paint.set_color(pal.base);
        pixmap.fill_path(
            &inner_head,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    if let Some(patch_col) = pal.patch_primary {
        clip_mask.clear();
        clip_mask.fill_path(&outer_head, FillRule::Winding, true, Transform::identity());
        match cat.coat {
            CoatStyle::Calico => {
                draw_blob_patch(
                    &[
                        head_center + fwd * (7.5 * s),
                        head_center + fwd * (2.5 * s) - side * (8.5 * s),
                        head_center - fwd * (6.5 * s) - side * (6.5 * s),
                        head_center - fwd * (4.0 * s) + side * (2.2 * s),
                        head_center + fwd * (2.0 * s) + side * (1.2 * s),
                    ],
                    patch_col,
                    clip_mask,
                    pixmap,
                );
            }
            CoatStyle::Tuxedo => {
                for sign in [-1.0f32, 1.0f32] {
                    draw_blob_patch(
                        &[
                            head_center + fwd * (5.2 * s) + side * (sign * 1.5 * s),
                            head_center + fwd * (2.0 * s) + side * (sign * 8.8 * s),
                            head_center - fwd * (6.8 * s) + side * (sign * 6.8 * s),
                            head_center - fwd * (5.8 * s) + side * (sign * 0.2 * s),
                        ],
                        patch_col,
                        clip_mask,
                        pixmap,
                    );
                }
            }
            _ => {}
        }
    }

    // Smoothly fade out the dorsal nose (at +fwd) over roll in [0.0, 0.50] as the snout pitches back
    if roll < 0.50 {
        let dorsal_t = smoothstep(1.0 - roll / 0.50);
        let tip_fwd = (7.9 - 1.4 * roll) * s;
        let base_fwd = (6.5 - 1.2 * roll) * s;
        let nose_tip = head_center + fwd * tip_fwd;
        let nose_l = head_center + fwd * base_fwd + side * (1.5 * dorsal_t * s);
        let nose_r = head_center + fwd * base_fwd - side * (1.5 * dorsal_t * s);
        let mut nose_pb = PathBuilder::new();
        nose_pb.move_to(nose_l.x, nose_l.y);
        nose_pb.quad_to(nose_tip.x, nose_tip.y, nose_r.x, nose_r.y);
        nose_pb.close();
        if let Some(nose_path) = nose_pb.finish() {
            paint.set_color(scale_alpha(pal.nose, dorsal_t));
            pixmap.fill_path(
                &nose_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        let happy_closed_eyes = cat.activity == Activity::Sleeping
            || matches!(cat.activity, Activity::Eating { elapsed, .. } if (elapsed % 16.0) < 13.2);
        if happy_closed_eyes {
            let eye_stroke = Stroke {
                width: (0.95 * s).max(0.8),
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            paint.set_color(scale_alpha(pal.outline, dorsal_t));
            for sign in [-1.0f32, 1.0f32] {
                let eye_c = head_center + fwd * (2.8 * s) + side * (sign * 3.1 * s);
                let e_inner = eye_c - side * (sign * 1.25 * s) + fwd * (0.25 * s);
                let e_outer = eye_c + side * (sign * 1.25 * s) - fwd * (0.15 * s);
                let ctrl = eye_c + fwd * (0.95 * s);
                let mut epb = PathBuilder::new();
                epb.move_to(e_inner.x, e_inner.y);
                epb.quad_to(ctrl.x, ctrl.y, e_outer.x, e_outer.y);
                if let Some(ep) = epb.finish() {
                    pixmap.stroke_path(&ep, &paint, &eye_stroke, Transform::identity(), None);
                }
            }
        }
    }

    // Smoothly fade in and rotate the ventral face (facing the body, -fwd) over roll in [0.20, 1.0]
    if roll > 0.20 {
        let face_t = smoothstep((roll - 0.20) / 0.80);
        let w_t = 0.35 + 0.65 * face_t;
        let lat_shift = side * (cat.belly_roll_side * (1.0 - face_t) * 2.2 * s);
        let fc = head_center + lat_shift;

        let chin_col = match cat.coat {
            CoatStyle::Calico | CoatStyle::Tuxedo | CoatStyle::Silver => rgba(246, 245, 242, 245),
            CoatStyle::Brown => rgba(216, 186, 154, 240),
            CoatStyle::Charcoal => rgba(102, 102, 110, 235),
        };
        clip_mask.clear();
        clip_mask.fill_path(&outer_head, FillRule::Winding, true, Transform::identity());
        draw_blob_patch(
            &[
                fc - fwd * (2.0 * s),
                fc - fwd * (4.2 * s) + side * (3.6 * w_t * s),
                fc - fwd * (6.8 * s),
                fc - fwd * (4.2 * s) - side * (3.6 * w_t * s),
            ],
            scale_alpha(chin_col, face_t),
            clip_mask,
            pixmap,
        );

        // Pink nose pointing toward the body (-fwd)
        let nose_tip = fc - fwd * (4.5 * s);
        let nose_l = fc - fwd * (3.2 * s) + side * (1.45 * w_t * s);
        let nose_r = fc - fwd * (3.2 * s) - side * (1.45 * w_t * s);
        let mut nose_pb = PathBuilder::new();
        nose_pb.move_to(nose_l.x, nose_l.y);
        nose_pb.quad_to(nose_tip.x, nose_tip.y, nose_r.x, nose_r.y);
        nose_pb.close();
        if let Some(nose_path) = nose_pb.finish() {
            paint.set_color(scale_alpha(rgba(226, 134, 150, 255), face_t));
            pixmap.fill_path(
                &nose_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        let eye_stroke = Stroke {
            width: (0.95 * s).max(0.8),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        paint.set_color(scale_alpha(pal.outline, face_t));

        // Happy closed eyes above the nose (between crown +fwd and nose -fwd), facing the body
        for sign in [-1.0f32, 1.0f32] {
            let eye_c = fc - fwd * (0.4 * s) + side * (sign * 3.0 * w_t * s);
            let e_inner = eye_c - side * (sign * 1.2 * w_t * s) - fwd * (0.15 * s);
            let e_outer = eye_c + side * (sign * 1.2 * w_t * s) + fwd * (0.20 * s);
            let ctrl = eye_c + fwd * (0.85 * s);
            let mut epb = PathBuilder::new();
            epb.move_to(e_inner.x, e_inner.y);
            epb.quad_to(ctrl.x, ctrl.y, e_outer.x, e_outer.y);
            if let Some(ep) = epb.finish() {
                pixmap.stroke_path(&ep, &paint, &eye_stroke, Transform::identity(), None);
            }
        }

        // Cute smiling 'w' mouth between the nose and the chin/body (-fwd)
        let m_center = fc - fwd * (4.3 * s);
        for sign in [-1.0f32, 1.0f32] {
            let m_ctrl = fc - fwd * (5.3 * s) + side * (sign * 0.85 * w_t * s);
            let m_end = fc - fwd * (4.2 * s) + side * (sign * 1.75 * w_t * s);
            let mut mpb = PathBuilder::new();
            mpb.move_to(m_center.x, m_center.y);
            mpb.quad_to(m_ctrl.x, m_ctrl.y, m_end.x, m_end.y);
            if let Some(mp) = mpb.finish() {
                pixmap.stroke_path(&mp, &paint, &eye_stroke, Transform::identity(), None);
            }
        }
    }

    let head_stroke = Stroke {
        width: 1.1 * s,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    paint.set_color(pal.outline);
    pixmap.stroke_path(
        &outer_head,
        &paint,
        &head_stroke,
        Transform::identity(),
        None,
    );

    if !belly_up {
        draw_ear(
            ear_head_center,
            head_angle,
            1.0,
            cat.ear_left_twitch + ear_bias,
            s,
            pal,
            pixmap,
        );
        draw_ear(
            ear_head_center,
            head_angle,
            -1.0,
            cat.ear_right_twitch + ear_bias,
            s,
            pal,
            pixmap,
        );
    }
}

fn build_head_path_rolled(
    center: Vec2,
    fwd: Vec2,
    side: Vec2,
    s: f32,
    roll: f32,
) -> Option<tiny_skia::Path> {
    // At roll = 0.0: snout is at +fwd (away from body), nape is at -fwd (near neck).
    // At roll = 1.0: rounded skull crown is at +fwd (away from body), muzzle/face points toward -fwd (toward body).
    let top_dist = 7.6 * (1.0 - roll) + 6.0 * roll;
    let bot_dist = 5.8 * (1.0 - roll) + 6.6 * roll;
    let up_fwd = 1.6 * (1.0 - roll) + 2.4 * roll;
    let up_side = 7.3 * (1.0 - roll) + 6.2 * roll;
    let lo_fwd = -3.8 * (1.0 - roll) - 1.4 * roll;
    let lo_side = 6.0 * (1.0 - roll) + 7.3 * roll;
    let top_ctrl_w = 4.4 * (1.0 - roll) + 4.0 * roll;
    let bot_ctrl_w = 3.8 * (1.0 - roll) + 4.4 * roll;

    let top_tip = center + fwd * (top_dist * s);
    let upper_l = center + fwd * (up_fwd * s) + side * (up_side * s);
    let upper_r = center + fwd * (up_fwd * s) - side * (up_side * s);
    let lower_l = center + fwd * (lo_fwd * s) + side * (lo_side * s);
    let lower_r = center + fwd * (lo_fwd * s) - side * (lo_side * s);
    let bot_tip = center - fwd * (bot_dist * s);

    let mut pb = PathBuilder::new();
    pb.move_to(top_tip.x, top_tip.y);
    pb.cubic_to(
        top_tip.x + side.x * (top_ctrl_w * s),
        top_tip.y + side.y * (top_ctrl_w * s),
        upper_l.x + fwd.x * (2.6 * s),
        upper_l.y + fwd.y * (2.6 * s),
        upper_l.x,
        upper_l.y,
    );
    pb.cubic_to(
        lower_l.x + side.x * (1.2 * s),
        lower_l.y + side.y * (1.2 * s),
        bot_tip.x + side.x * (bot_ctrl_w * s),
        bot_tip.y + side.y * (bot_ctrl_w * s),
        bot_tip.x,
        bot_tip.y,
    );
    pb.cubic_to(
        bot_tip.x - side.x * (bot_ctrl_w * s),
        bot_tip.y - side.y * (bot_ctrl_w * s),
        lower_r.x - side.x * (1.2 * s),
        lower_r.y - side.y * (1.2 * s),
        upper_r.x,
        upper_r.y,
    );
    pb.cubic_to(
        upper_r.x + fwd.x * (2.6 * s),
        upper_r.y + fwd.y * (2.6 * s),
        top_tip.x - side.x * (top_ctrl_w * s),
        top_tip.y - side.y * (top_ctrl_w * s),
        top_tip.x,
        top_tip.y,
    );
    pb.close();
    pb.finish()
}

fn draw_ear(
    head_center: Vec2,
    head_angle: f32,
    side_sign: f32,
    twitch: f32,
    s: f32,
    pal: &Palette,
    pixmap: &mut PixmapMut,
) {
    let fwd = Vec2::from_angle(head_angle);
    let side = Vec2::new(-fwd.y, fwd.x) * side_sign;

    let ear_anchor = head_center + fwd * (1.3 * s) + side * (4.3 * s);
    let ear_angle = head_angle + side_sign * (0.54 - twitch);
    let ear_dir = Vec2::from_angle(ear_angle);
    let ear_perp = Vec2::new(-ear_dir.y, ear_dir.x) * side_sign;

    let base_front = ear_anchor + fwd * (3.0 * s) - side * (1.1 * s);
    let base_back = ear_anchor - fwd * (2.8 * s) + side * (1.9 * s);
    let tip = ear_anchor + ear_dir * (6.8 * s) + ear_perp * (0.4 * s);

    let mut fill_pb = PathBuilder::new();
    fill_pb.move_to(base_front.x, base_front.y);
    fill_pb.quad_to(
        (base_front.x + tip.x) * 0.5 - ear_perp.x * (0.7 * s),
        (base_front.y + tip.y) * 0.5 - ear_perp.y * (0.7 * s),
        tip.x,
        tip.y,
    );
    fill_pb.quad_to(
        (tip.x + base_back.x) * 0.5 + ear_perp.x * (1.0 * s),
        (tip.y + base_back.y) * 0.5 + ear_perp.y * (1.0 * s),
        base_back.x,
        base_back.y,
    );
    fill_pb.close();

    let Some(outer_ear_fill) = fill_pb.finish() else {
        return;
    };

    let mut paint = Paint::default();

    let (outer_col, inner_col) = if pal.patch_primary.is_some() && side_sign > 0.0 {
        (pal.base, pal.flank_shade)
    } else {
        (pal.ear_outer, pal.ear_inner)
    };

    paint.set_color(outer_col);
    pixmap.fill_path(
        &outer_ear_fill,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    let in_front = base_front.lerp(base_back, 0.22) + ear_dir * (0.8 * s);
    let in_back = base_back.lerp(base_front, 0.18) + ear_dir * (0.6 * s);
    let in_tip = ear_anchor + ear_dir * (5.3 * s);
    let mut in_pb = PathBuilder::new();
    in_pb.move_to(in_front.x, in_front.y);
    in_pb.line_to(in_tip.x, in_tip.y);
    in_pb.line_to(in_back.x, in_back.y);
    in_pb.close();
    if let Some(inner_ear) = in_pb.finish() {
        paint.set_color(inner_col);
        pixmap.fill_path(
            &inner_ear,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    let mut stroke_pb = PathBuilder::new();
    stroke_pb.move_to(base_front.x, base_front.y);
    stroke_pb.quad_to(
        (base_front.x + tip.x) * 0.5 - ear_perp.x * (0.7 * s),
        (base_front.y + tip.y) * 0.5 - ear_perp.y * (0.7 * s),
        tip.x,
        tip.y,
    );
    stroke_pb.quad_to(
        (tip.x + base_back.x) * 0.5 + ear_perp.x * (1.0 * s),
        (tip.y + base_back.y) * 0.5 + ear_perp.y * (1.0 * s),
        base_back.x,
        base_back.y,
    );
    if let Some(ear_outline) = stroke_pb.finish() {
        let stroke = Stroke {
            width: 1.05 * s,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        paint.set_color(pal.outline);
        pixmap.stroke_path(&ear_outline, &paint, &stroke, Transform::identity(), None);
    }
}

/// Returns `(base_color, accent_patch_color, outline_color)` for UI coat swatches.
pub fn coat_swatch_colors(coat: CoatStyle) -> (Color, Color, Color) {
    let pal = Palette::for_coat(coat);
    let accent = pal.patch_primary.unwrap_or(pal.flank_shade);
    (pal.base, accent, pal.outline)
}

fn build_tray_head_path(cx: f32, cy: f32, r: f32, droop: f32) -> Option<tiny_skia::Path> {
    let l_in = Vec2::new(cx - r * 0.35, cy - r * 0.48);
    let r_in = Vec2::new(cx + r * 0.35, cy - r * 0.48);

    let tip_x = r * (0.76 + 0.08 * droop);
    let tip_y = r * (0.98 - 0.16 * droop);
    let l_tip = Vec2::new(cx - tip_x, cy - tip_y);
    let r_tip = Vec2::new(cx + tip_x, cy - tip_y);

    let l_out = Vec2::new(cx - r * 0.80, cy - r * 0.12);
    let r_out = Vec2::new(cx + r * 0.80, cy - r * 0.12);
    let chin = Vec2::new(cx, cy + r * 0.78);

    let mut pb = PathBuilder::new();
    pb.move_to(l_in.x, l_in.y);
    pb.quad_to(cx, cy - r * 0.56, r_in.x, r_in.y);
    pb.quad_to(cx + r * 0.56, cy - r * 0.78, r_tip.x, r_tip.y);
    pb.quad_to(cx + r * 0.86, cy - r * 0.54, r_out.x, r_out.y);
    pb.cubic_to(
        cx + r * 0.94,
        cy + r * 0.36,
        cx + r * 0.50,
        cy + r * 0.78,
        chin.x,
        chin.y,
    );
    pb.cubic_to(
        cx - r * 0.50,
        cy + r * 0.78,
        cx - r * 0.94,
        cy + r * 0.36,
        l_out.x,
        l_out.y,
    );
    pb.quad_to(cx - r * 0.86, cy - r * 0.54, l_tip.x, l_tip.y);
    pb.quad_to(cx - r * 0.56, cy - r * 0.78, l_in.x, l_in.y);
    pb.close();
    pb.finish()
}

/// Renders a front-facing cat-head silhouette icon for the system tray.
pub fn render_tray_icon(
    coat: CoatStyle,
    sleeping: bool,
    hidden: bool,
    opaque_bg: bool,
    pixmap: &mut PixmapMut,
) {
    if opaque_bg {
        pixmap.fill(rgba(0x28, 0x2a, 0x36, 255));
    } else {
        pixmap.fill(Color::TRANSPARENT);
    }

    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;
    let dim = w.min(h);
    let cx = w * 0.5;
    let r = dim * 0.48;
    let cy = h * 0.5 + r * 0.10;
    let droop = if sleeping || hidden { 1.0 } else { 0.0 };

    let (base, shade, outline, ear_in, nose_col, eye_col) = match coat {
        CoatStyle::Calico => (
            rgba(242, 239, 234, 255),
            rgba(218, 212, 204, 255),
            rgba(68, 56, 48, 255),
            rgba(218, 138, 132, 255),
            rgba(216, 126, 136, 255),
            rgba(58, 48, 42, 255),
        ),
        CoatStyle::Brown => (
            rgba(162, 124, 92, 255),
            rgba(134, 100, 72, 255),
            rgba(56, 38, 24, 255),
            rgba(196, 132, 118, 255),
            rgba(74, 46, 34, 255),
            rgba(46, 30, 18, 255),
        ),
        CoatStyle::Charcoal => (
            rgba(112, 116, 128, 255),
            rgba(88, 92, 104, 255),
            rgba(28, 30, 36, 255),
            rgba(168, 122, 132, 255),
            rgba(46, 48, 56, 255),
            rgba(26, 28, 34, 255),
        ),
        CoatStyle::Tuxedo => (
            rgba(238, 240, 244, 255),
            rgba(212, 215, 222, 255),
            rgba(34, 36, 42, 255),
            rgba(164, 118, 128, 255),
            rgba(216, 132, 146, 255),
            rgba(34, 36, 42, 255),
        ),
        CoatStyle::Silver => (
            rgba(224, 226, 232, 255),
            rgba(196, 198, 206, 255),
            rgba(96, 100, 110, 255),
            rgba(206, 152, 162, 255),
            rgba(198, 126, 138, 255),
            rgba(58, 60, 68, 255),
        ),
    };

    let Some(head_path) = build_tray_head_path(cx, cy, r, droop) else {
        return;
    };

    let mut paint = Paint::default();

    paint.set_color(shade);
    pixmap.fill_path(
        &head_path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    if let Some(inner_path) = build_tray_head_path(cx, cy + r * 0.02, r * 0.88, droop) {
        paint.set_color(base);
        pixmap.fill_path(
            &inner_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    if matches!(coat, CoatStyle::Calico | CoatStyle::Tuxedo)
        && let Some(mut mask) = Mask::new(pixmap.width(), pixmap.height())
    {
        mask.fill_path(&head_path, FillRule::Winding, true, Transform::identity());
        match coat {
            CoatStyle::Calico => {
                if let Some(orange) =
                    PathBuilder::from_circle(cx - r * 0.48, cy - r * 0.42, r * 0.62)
                {
                    paint.set_color(rgba(226, 132, 60, 255));
                    pixmap.fill_path(
                        &orange,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        Some(&mask),
                    );
                }
                if let Some(dark) = PathBuilder::from_circle(cx + r * 0.56, cy - r * 0.52, r * 0.44)
                {
                    paint.set_color(rgba(62, 64, 72, 255));
                    pixmap.fill_path(
                        &dark,
                        &paint,
                        FillRule::Winding,
                        Transform::identity(),
                        Some(&mask),
                    );
                }
            }
            CoatStyle::Tuxedo => {
                for sign in [-1.0f32, 1.0f32] {
                    if let Some(hood) =
                        PathBuilder::from_circle(cx + sign * r * 0.56, cy - r * 0.50, r * 0.50)
                    {
                        paint.set_color(rgba(46, 48, 56, 255));
                        pixmap.fill_path(
                            &hood,
                            &paint,
                            FillRule::Winding,
                            Transform::identity(),
                            Some(&mask),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    let tip_x = r * (0.76 + 0.08 * droop);
    let tip_y = r * (0.98 - 0.16 * droop);
    for sign in [-1.0f32, 1.0f32] {
        if coat == CoatStyle::Calico && sign < 0.0 {
            paint.set_color(rgba(186, 96, 40, 255));
        } else {
            paint.set_color(ear_in);
        }
        let mut epb = PathBuilder::new();
        epb.move_to(cx + sign * (r * 0.40), cy - r * 0.42);
        epb.line_to(cx + sign * (tip_x * 0.88), cy - tip_y * 0.84);
        epb.line_to(cx + sign * (r * 0.66), cy - r * 0.18);
        epb.close();
        if let Some(ep) = epb.finish() {
            pixmap.fill_path(&ep, &paint, FillRule::Winding, Transform::identity(), None);
        }
    }

    let eye_y = cy + r * 0.02;
    let eye_dx = r * 0.33;
    if sleeping {
        let eye_stroke = Stroke {
            width: (r * 0.13).clamp(1.1, 2.0),
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Default::default()
        };
        paint.set_color(eye_col);
        for sign in [-1.0f32, 1.0f32] {
            let ex = cx + sign * eye_dx;
            let mut pb = PathBuilder::new();
            pb.move_to(ex - r * 0.14, eye_y - r * 0.02);
            pb.quad_to(ex, eye_y + r * 0.14, ex + r * 0.14, eye_y - r * 0.02);
            if let Some(p) = pb.finish() {
                pixmap.stroke_path(&p, &paint, &eye_stroke, Transform::identity(), None);
            }
        }
    } else {
        paint.set_color(eye_col);
        let er = (r * 0.135).max(1.25);
        for sign in [-1.0f32, 1.0f32] {
            if let Some(eye) = PathBuilder::from_circle(cx + sign * eye_dx, eye_y, er) {
                pixmap.fill_path(&eye, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
    }

    let ny = cy + r * 0.26;
    let mut npb = PathBuilder::new();
    npb.move_to(cx - r * 0.13, ny);
    npb.line_to(cx + r * 0.13, ny);
    npb.line_to(cx, ny + r * 0.16);
    npb.close();
    if let Some(np) = npb.finish() {
        paint.set_color(nose_col);
        pixmap.fill_path(&np, &paint, FillRule::Winding, Transform::identity(), None);
    }

    let head_stroke = Stroke {
        width: (r * 0.13).clamp(1.1, 2.0),
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };
    paint.set_color(outline);
    pixmap.stroke_path(
        &head_path,
        &paint,
        &head_stroke,
        Transform::identity(),
        None,
    );

    if hidden {
        if opaque_bg {
            for px in pixmap.data_mut().as_chunks_mut::<4>().0 {
                px[0] = ((px[0] as u16 * 115 + 0x28 * 140) / 255) as u8;
                px[1] = ((px[1] as u16 * 115 + 0x2a * 140) / 255) as u8;
                px[2] = ((px[2] as u16 * 115 + 0x36 * 140) / 255) as u8;
            }
        } else {
            for b in pixmap.data_mut().iter_mut() {
                *b = ((*b as u16 * 115) / 255) as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tiny_skia::Pixmap;

    /// Sizes packed into `assets/catsor.ico` (Explorer, taskbar, Alt-Tab and high-DPI views)
    const APP_ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

    fn app_icon_png(size: u32) -> Vec<u8> {
        let mut pixmap = Pixmap::new(size, size).unwrap();
        render_tray_icon(CoatStyle::Calico, false, false, false, &mut pixmap.as_mut());
        pixmap.encode_png().unwrap()
    }

    /// ICO container with PNG-compressed entries (supported since Windows Vista)
    fn encode_ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&(images.len() as u16).to_le_bytes());
        let mut offset = 6 + 16 * images.len() as u32;
        for (size, png) in images {
            // A 256 px dimension is stored as 0
            let dim = if *size >= 256 { 0 } else { *size as u8 };
            out.extend_from_slice(&[dim, dim, 0, 0]);
            out.extend_from_slice(&1u16.to_le_bytes());
            out.extend_from_slice(&32u16.to_le_bytes());
            out.extend_from_slice(&(png.len() as u32).to_le_bytes());
            out.extend_from_slice(&offset.to_le_bytes());
            offset += png.len() as u32;
        }
        for (_, png) in images {
            out.extend_from_slice(png);
        }
        out
    }

    /// Regenerates the committed app icons from `render_tray_icon`:
    /// `cargo test regenerate_app_icons -- --ignored`
    #[test]
    #[ignore]
    fn regenerate_app_icons() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
        std::fs::create_dir_all(&dir).unwrap();
        let images: Vec<_> = APP_ICON_SIZES
            .iter()
            .map(|&s| (s, app_icon_png(s)))
            .collect();
        std::fs::write(dir.join("catsor.ico"), encode_ico(&images)).unwrap();
        std::fs::write(dir.join("catsor.png"), app_icon_png(256)).unwrap();
    }
}
