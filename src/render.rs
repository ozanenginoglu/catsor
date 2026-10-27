use crate::cat::{
    Activity, BODY_NODES, CatSim, Gait, TAIL_NODES, TOTAL_NODES, pounce_stretch, smoothstep,
};
use crate::coat::{Coat, Pattern, Rgba8};
use glam::Vec2;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Mask, Paint, Path, PathBuilder, PixmapMut, Rect, Stroke,
    Transform,
};

struct Palette {
    pattern: Pattern,
    base: Color,
    flank_shade: Color,
    outline: Color,
    crease: Color,
    ear_outer: Color,
    ear_inner: Color,
    nose: Color,
    paw: Color,
    belly: Color,
    tummy: Color,
    chin: Color,
    /// Back patch, flank patch and shoulder spot; `None` on solid coats
    patches: Option<[Color; 3]>,
    tail: Color,
    tail_shade: Color,
    tail_rings: Option<Color>,
}

impl Palette {
    fn for_coat(coat: &Coat) -> Self {
        Self {
            pattern: coat.pattern,
            base: color(coat.base),
            flank_shade: color(coat.shade),
            outline: color(coat.outline),
            crease: color(coat.crease),
            ear_outer: color(coat.ears),
            ear_inner: color(coat.ear_inner),
            nose: color(coat.nose),
            paw: color(coat.paws),
            belly: color(coat.belly),
            tummy: color(coat.tummy),
            chin: color(coat.chin),
            patches: (coat.pattern != Pattern::Solid)
                .then(|| [color(coat.patch), color(coat.patch_shade), color(coat.spot)]),
            tail: color(coat.tail),
            tail_shade: color(coat.tail_shade),
            tail_rings: coat.tail_rings.map(color),
        }
    }
}

#[inline]
pub(crate) fn color(c: Rgba8) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
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

/// Fills `path` with a solid anti-aliased colour (non-zero winding)
fn fill_solid(pixmap: &mut PixmapMut, path: &Path, color: Color) {
    let mut paint = Paint::default();
    paint.set_color(color);
    pixmap.fill_path(path, &paint, FillRule::Winding, Transform::identity(), None);
}

/// Like [`fill_solid`], but only where `mask` is set
fn fill_solid_masked(pixmap: &mut PixmapMut, path: &Path, color: Color, mask: &Mask) {
    let mut paint = Paint::default();
    paint.set_color(color);
    pixmap.fill_path(
        path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        Some(mask),
    );
}

fn stroke_solid(pixmap: &mut PixmapMut, path: &Path, color: Color, stroke: &Stroke) {
    let mut paint = Paint::default();
    paint.set_color(color);
    pixmap.stroke_path(path, &paint, stroke, Transform::identity(), None);
}

/// Like [`stroke_solid`], but only where `mask` is set
fn stroke_solid_masked(
    pixmap: &mut PixmapMut,
    path: &Path,
    color: Color,
    stroke: &Stroke,
    mask: &Mask,
) {
    let mut paint = Paint::default();
    paint.set_color(color);
    pixmap.stroke_path(path, &paint, stroke, Transform::identity(), Some(mask));
}

/// A stroke of `width` with round caps and joins
fn round_stroke(width: f32) -> Stroke {
    Stroke {
        width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    }
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
    let pal = Palette::for_coat(&cat.coat);

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
    let head_angle = cat.body_angle + cat.head_yaw;
    let paws = |pass, pixmap: &mut PixmapMut| {
        draw_paws(
            cat,
            &local,
            &tangents,
            &normals,
            window_origin,
            &pal,
            pass,
            pixmap,
        );
    };

    // 2. Outline pass: the outlines of every part joined to the body go down first, so the
    //    fills below cover them where parts meet and only the shared silhouette stays outlined
    if !paws_on_top {
        paws(Pass::Outline, pixmap);
    }
    draw_tail(cat, &local, &pal, Pass::Outline, pixmap);
    draw_body_outline(cat, &local, &tangents, &normals, &pal, pixmap);
    draw_head_outline(cat, local[0], head_angle, &pal, pixmap);

    // 3. Articulated paws (under the torso normally, or on top of chest/hips once rolled onto back)
    if !paws_on_top {
        paws(Pass::Fill, pixmap);
    }

    // 4. Sleek rounded S-curve tail (always drawn underneath the body & head so it never covers the face)
    draw_tail(cat, &local, &pal, Pass::Fill, pixmap);

    // 5. Layered body silhouette, edge-clipped coat patches, exposed belly (with roll animation), and muscle creases
    draw_body(cat, &local, &tangents, &normals, &pal, clip_mask, pixmap);

    // Paws raised over the belly cover the body, so they keep their own full outline
    if paws_on_top {
        paws(Pass::Outline, pixmap);
        paws(Pass::Fill, pixmap);
    }

    // 6. Head, snout/nose, head patches, and independent 3D triangular ears
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
        fill_solid(pixmap, &path, rgba(0, 0, 0, 36));
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
    // 1. Floor drop shadow under the bowl
    let shadow_c = bowl_center + Vec2::new(1.2 * s, 1.8 * s);
    if let Some(shadow_path) = PathBuilder::from_circle(shadow_c.x, shadow_c.y, 9.1 * s) {
        fill_solid(pixmap, &shadow_path, rgba(0, 0, 0, 44));
    }

    // 2. Ceramic outer bowl body & rim highlight
    if let Some(outer_path) = PathBuilder::from_circle(bowl_center.x, bowl_center.y, 8.6 * s) {
        fill_solid(pixmap, &outer_path, rgba(216, 98, 84, 255));

        if let Some(rim_path) =
            PathBuilder::from_circle(bowl_center.x - 0.35 * s, bowl_center.y - 0.35 * s, 7.5 * s)
        {
            fill_solid(pixmap, &rim_path, rgba(238, 132, 116, 255));
        }

        let rim_stroke = Stroke {
            width: (1.05 * s).max(0.85),
            ..Default::default()
        };
        stroke_solid(pixmap, &outer_path, outline_col, &rim_stroke);
    }

    // 3. Recessed inner bowl basin
    if let Some(well_path) = PathBuilder::from_circle(bowl_center.x, bowl_center.y, 6.3 * s) {
        fill_solid(pixmap, &well_path, rgba(68, 46, 44, 255));

        let inner_stroke = Stroke {
            width: (0.8 * s).max(0.65),
            ..Default::default()
        };
        stroke_solid(pixmap, &well_path, rgba(42, 28, 28, 220), &inner_stroke);
    }

    // 4. Depleting kibble mound & individual morsels proportional to remaining time
    let fill = fill_ratio.clamp(0.0, 1.0);
    if fill > 0.01 {
        let mound_r = (1.8 + 3.9 * fill.sqrt()) * s;
        if let Some(mound_path) = PathBuilder::from_circle(bowl_center.x, bowl_center.y, mound_r) {
            fill_solid(pixmap, &mound_path, rgba(142, 86, 48, 255));
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
                fill_solid(
                    pixmap,
                    &kp_path,
                    if warm_tint {
                        rgba(194, 122, 68, 255)
                    } else {
                        rgba(172, 100, 56, 255)
                    },
                );
                stroke_solid(pixmap, &kp_path, rgba(92, 52, 28, 210), &kibble_stroke);
            }
        }
    }
}

/// Body axes the legs are laid out along: at the shoulders (node 1) and at the hips (node 4)
struct LegFrame {
    fwd: Vec2,
    side: Vec2,
    hip_fwd: Vec2,
    hip_side: Vec2,
    s: f32,
}

/// The cat is drawn in two passes so that parts joined on the body share one silhouette: every
/// part's outline goes down first, then the fills cover the outlines wherever parts overlap
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Outline,
    Fill,
}

/// Draws legs in the paw colour with the coat outline
struct LegPen<'p, 'a> {
    fill: Color,
    outline: Color,
    /// Toe notches on the top of the foot (off when the soles face up)
    toe_lines: bool,
    pass: Pass,
    pixmap: &'p mut PixmapMut<'a>,
}

impl LegPen<'_, '_> {
    fn straight(&mut self, shoulder_pos: Vec2, paw_tip: Vec2, radius: f32) {
        self.limb(
            shoulder_pos,
            (shoulder_pos + paw_tip) * 0.5,
            paw_tip,
            radius,
        );
    }

    fn curved(&mut self, anchor_pos: Vec2, joint_pos: Vec2, paw_tip: Vec2, radius: f32) {
        self.limb(anchor_pos, joint_pos, paw_tip, radius);
    }

    /// A leg tapering from `anchor_pos` along a quadratic curve through `joint_pos` down to a
    /// slim wrist, ending in an oval paw centred on `paw_tip` that is wider than the wrist
    fn limb(&mut self, anchor_pos: Vec2, joint_pos: Vec2, paw_tip: Vec2, radius: f32) {
        let Some(dir) = (paw_tip - joint_pos).try_normalize() else {
            return;
        };
        let perp = Vec2::new(-dir.y, dir.x);

        let curve = |t: f32| {
            let u = 1.0 - t;
            anchor_pos * (u * u) + joint_pos * (2.0 * u * t) + paw_tip * (t * t)
        };
        let curve_tangent =
            |t: f32| (joint_pos - anchor_pos) * (1.0 - t) + (paw_tip - joint_pos) * t;
        let half_width =
            |t: f32| radius * (LEG_TOP_WIDTH + (LEG_WRIST_WIDTH - LEG_TOP_WIDTH) * smoothstep(t));

        const SAMPLES: usize = 8;
        let edge = |t: f32, sign: f32| {
            let n = curve_tangent(t)
                .try_normalize()
                .map_or(perp, |d| Vec2::new(-d.y, d.x));
            curve(t) + n * (sign * half_width(t))
        };
        let mut leg = PathBuilder::new();
        let start = edge(0.0, 1.0);
        leg.move_to(start.x, start.y);
        for i in 1..=SAMPLES {
            let p = edge(i as f32 / SAMPLES as f32, 1.0);
            leg.line_to(p.x, p.y);
        }
        for i in (0..=SAMPLES).rev() {
            let p = edge(i as f32 / SAMPLES as f32, -1.0);
            leg.line_to(p.x, p.y);
        }
        leg.close();

        let paw = Rect::from_ltrb(
            -PAW_HALF_LEN * radius,
            -PAW_HALF_WIDTH * radius,
            PAW_HALF_LEN * radius,
            PAW_HALF_WIDTH * radius,
        )
        .and_then(PathBuilder::from_oval)
        .and_then(|oval| {
            oval.transform(Transform::from_row(
                dir.x, dir.y, perp.x, perp.y, paw_tip.x, paw_tip.y,
            ))
        });
        let shapes = [
            leg.finish(),
            PathBuilder::from_circle(anchor_pos.x, anchor_pos.y, LEG_TOP_WIDTH * radius),
            paw,
        ];

        if self.pass == Pass::Outline {
            let outline = round_stroke(LEG_OUTLINE_WIDTH * radius);
            for path in shapes.iter().flatten() {
                stroke_solid(self.pixmap, path, self.outline, &outline);
            }
            return;
        }
        for path in shapes.iter().flatten() {
            fill_solid(self.pixmap, path, self.fill);
        }

        if self.toe_lines {
            let toe = round_stroke((0.2 * radius).max(0.6));
            for sign in [-1.0f32, 1.0] {
                let from = paw_tip + dir * (0.45 * radius) + perp * (sign * 0.28 * radius);
                let to = paw_tip + dir * (1.12 * radius) + perp * (sign * 0.34 * radius);
                let mut pb = PathBuilder::new();
                pb.move_to(from.x, from.y);
                pb.line_to(to.x, to.y);
                if let Some(path) = pb.finish() {
                    stroke_solid(self.pixmap, &path, scale_alpha(self.outline, 0.75), &toe);
                }
            }
        }
    }
}

/// Leg shape relative to the leg radius: half-widths at the shoulder/hip and at the wrist,
/// the paw oval's half-length and half-width, and the outline stroke width
const LEG_TOP_WIDTH: f32 = 0.86;
const LEG_WRIST_WIDTH: f32 = 0.52;
const PAW_HALF_LEN: f32 = 1.2;
const PAW_HALF_WIDTH: f32 = 0.92;
const LEG_OUTLINE_WIDTH: f32 = 0.5;

/// Draws the legs that show in the current pose
#[allow(clippy::too_many_arguments)]
fn draw_paws(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    window_origin: Vec2,
    pal: &Palette,
    pass: Pass,
    pixmap: &mut PixmapMut,
) {
    let f = LegFrame {
        fwd: tangents[1],
        side: normals[1],
        hip_fwd: tangents[4],
        hip_side: normals[4],
        s: cat.scale,
    };
    let (s, fwd, side, hip_fwd, hip_side) = (f.s, f.fwd, f.side, f.hip_fwd, f.hip_side);

    let paw_color = pal.paw;
    let mut pen = LegPen {
        fill: paw_color,
        outline: pal.outline,
        toe_lines: cat.belly_roll <= 0.04,
        pass,
        pixmap,
    };

    if cat.belly_roll > 0.04 {
        draw_belly_up_legs(cat, local, &f, &mut pen);
        return;
    }

    match cat.activity {
        Activity::Stretching { progress, .. } => {
            let ext = (progress * std::f32::consts::PI).sin();
            if ext > 0.08 {
                let reach = (8.5 + 6.8 * ext) * s;
                let left_paw = local[0] + fwd * reach + side * (3.8 * s);
                let right_paw = local[0] + fwd * reach - side * (3.8 * s);
                pen.straight(local[1] + side * (4.2 * s), left_paw, 2.4 * s);
                pen.straight(local[1] - side * (4.2 * s), right_paw, 2.4 * s);
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
            pen.straight(shoulder_anchor, paw_pos, 2.4 * s);
        }
        Activity::Kneading { timer, .. } => {
            // Front paws treading in turn against its mother's belly
            for (sign, phase) in [(1.0, 0.0), (-1.0, std::f32::consts::PI)] {
                let push = (0.5 + 0.5 * (timer * 5.5 + phase).sin()) * 1.8 * s;
                let paw = local[0] + fwd * (3.4 * s + push) + side * (sign * 4.4 * s);
                pen.straight(local[1] + side * (sign * 4.2 * s), paw, 2.3 * s);
            }
        }
        Activity::Eating { .. } => {
            // Planted front paws flanking the chest while crouched in front of the food bowl
            let left_paw = local[0] + fwd * (3.2 * s) + side * (5.2 * s);
            let right_paw = local[0] + fwd * (3.2 * s) - side * (5.2 * s);
            pen.straight(local[1] + side * (4.5 * s), left_paw, 2.3 * s);
            pen.straight(local[1] - side * (4.5 * s), right_paw, 2.3 * s);
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
            pen.straight(shoulder_anchor, paw_pos, 2.5 * s);
        }
        Activity::DramaticFlop {
            side: flop_side, ..
        } => {
            // All 4 legs sticking out to one side in a lazy side-flop!
            let f1 = local[2] + side * (flop_side * 11.2 * s) + fwd * (2.5 * s);
            let f2 = local[2] + side * (flop_side * 10.0 * s) - fwd * (1.8 * s);
            let h1 = local[4] + hip_side * (flop_side * 10.8 * s) + hip_fwd * (1.8 * s);
            let h2 = local[4] + hip_side * (flop_side * 9.6 * s) - hip_fwd * (2.2 * s);
            pen.straight(local[2], f1, 2.2 * s);
            pen.straight(local[2], f2, 2.2 * s);
            pen.straight(local[4], h1, 2.2 * s);
            pen.straight(local[4], h2, 2.2 * s);
        }
        Activity::StalkWiggle { .. } => {
            // Wide planted rear feet gripping the floor during the pre-launch butt wiggle
            let lh = local[4] + hip_side * (9.2 * s) - hip_fwd * (1.5 * s);
            let rh = local[4] - hip_side * (9.2 * s) - hip_fwd * (1.5 * s);
            pen.straight(local[4] + hip_side * (5.0 * s), lh, 2.3 * s);
            pen.straight(local[4] - hip_side * (5.0 * s), rh, 2.3 * s);
        }
        Activity::Pouncing { progress, .. } => {
            // Leap: both front paws reaching out ahead, both hind paws kicking back after push-off
            let arc = pounce_stretch(progress);
            let lf = local[0] + fwd * ((8.5 + 5.5 * arc) * s) + side * (4.5 * s);
            let rf = local[0] + fwd * ((8.5 + 5.5 * arc) * s) - side * (4.5 * s);
            let lh = local[4] - hip_fwd * ((6.5 + 4.5 * arc) * s) + hip_side * (5.2 * s);
            let rh = local[4] - hip_fwd * ((6.5 + 4.5 * arc) * s) - hip_side * (5.2 * s);
            pen.straight(local[1] + side * (4.2 * s), lf, 2.4 * s);
            pen.straight(local[1] - side * (4.2 * s), rf, 2.4 * s);
            pen.straight(local[4] + hip_side * (4.2 * s), lh, 2.3 * s);
            pen.straight(local[4] - hip_side * (4.2 * s), rh, 2.3 * s);
        }
        Activity::Walking
        | Activity::Zoomies { .. }
        | Activity::TailChasing { .. }
        | Activity::Circling { .. }
            if cat.gait_weight > 0.08 =>
        {
            draw_gait_legs(cat, local, &f, window_origin, &mut pen);
        }
        _ => {}
    }
}

/// Smoothly animated transition onto (and off) the back: paws on `belly_roll_side` extend
/// first as the cat tips onto its side, followed by the other side's paws lifting up
fn draw_belly_up_legs(cat: &CatSim, local: &[Vec2; TOTAL_NODES], f: &LegFrame, pen: &mut LegPen) {
    let roll = smoothstep(cat.belly_roll);
    let roll_side = cat.belly_roll_side;
    let paddle = (cat.time * 2.6).sin() * roll;
    let sway = roll_side * (0.55 * roll + 0.85 * (1.0 - roll));

    let leading_reach = (roll * 1.35).clamp(0.0, 1.0);
    let trailing_reach = ((roll - 0.18) / 0.82).clamp(0.0, 1.0);
    let (left_reach, right_reach) = if roll_side >= 0.0 {
        (leading_reach, trailing_reach)
    } else {
        (trailing_reach, leading_reach)
    };
    for (sign, reach) in [(1.0f32, left_reach), (-1.0, right_reach)] {
        if reach > 0.05 {
            draw_belly_up_side(local, f, pen, sign, reach, sign * sway, sign * paddle);
        }
    }
}

/// Hind and fore leg on one side (`sign` +1 left, -1 right) reaching up from a cat on its back.
/// `sway` and `paddle` are already mirrored for that side.
fn draw_belly_up_side(
    local: &[Vec2; TOTAL_NODES],
    f: &LegFrame,
    pen: &mut LegPen,
    sign: f32,
    reach: f32,
    sway: f32,
    paddle: f32,
) {
    let s = f.s;
    let side = f.side * sign;
    let hip_side = f.hip_side * sign;

    let hip = local[4] + hip_side * (4.6 * s) + f.hip_fwd * (0.4 * s);
    let knee = local[4] + hip_side * ((4.8 + 4.0 * reach + sway * 0.5) * s)
        - f.hip_fwd * (1.0 * reach * s);
    let hind_tip = local[4] + hip_side * ((5.2 + 5.6 * reach + sway) * s)
        - f.hip_fwd * ((3.6 * reach - paddle * 0.8) * s);
    pen.curved(hip, knee, hind_tip, 2.4 * s);

    let shoulder = local[2] + side * (4.6 * s) - f.fwd * (0.3 * s);
    let elbow =
        local[2] + side * ((4.8 + 4.4 * reach + sway * 0.5) * s) + f.fwd * (0.9 * reach * s);
    let fore_tip = local[2]
        + side * ((5.0 + 3.5 * reach + sway) * s)
        + f.fwd * ((4.0 * reach + paddle * 0.9) * s);
    pen.curved(shoulder, elbow, fore_tip, 2.25 * s);

    let bean_s = ((reach - 0.35) / 0.65).clamp(0.0, 1.0) * s;
    if bean_s > 0.15 && pen.pass == Pass::Fill {
        draw_paw_beans(
            hind_tip,
            (hind_tip - knee).normalize_or_zero(),
            bean_s,
            pen.pixmap,
        );
        draw_paw_beans(
            fore_tip,
            (fore_tip - elbow).normalize_or_zero(),
            bean_s,
            pen.pixmap,
        );
    }
}

/// Legs follow the simulated paws in every gait. Walking and trotting only show the parts that
/// reach past the chest or haunches. Galloping draws every leg: the body covers what is
/// underneath, and swinging hind legs flare out past the flanks (in a rotary gallop the hind
/// feet pass outside the forefeet).
fn draw_gait_legs(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    f: &LegFrame,
    window_origin: Vec2,
    pen: &mut LegPen,
) {
    let (s, fwd, side, hip_fwd, hip_side) = (f.s, f.fwd, f.side, f.hip_fwd, f.hip_side);
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
            pen.straight(anchor, paw_local, 2.2 * s);
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
            pen.straight(anchor, paw_local, 2.15 * s);
        }
    }
}

fn draw_paw_beans(paw_tip: Vec2, dir: Vec2, s: f32, pixmap: &mut PixmapMut) {
    let perp = Vec2::new(-dir.y, dir.x);
    let bean_col = rgba(234, 146, 162, 242);

    let pad_center = paw_tip - dir * (0.35 * s);
    if let Some(main_pad) =
        PathBuilder::from_circle(pad_center.x, pad_center.y, (0.92 * s).max(0.7))
    {
        fill_solid(pixmap, &main_pad, bean_col);
    }

    for &(fwd_off, side_off) in &[(0.72, -0.78), (0.92, 0.0), (0.72, 0.78)] {
        let bean = paw_tip + dir * (fwd_off * s) + perp * (side_off * s);
        if let Some(toe) = PathBuilder::from_circle(bean.x, bean.y, (0.45 * s).max(0.4)) {
            fill_solid(pixmap, &toe, bean_col);
        }
    }
}

/// Samples along the tail's centre line (the same quadratic chain through the tail nodes), each
/// with its position `u` counted in nodes from the tail root (0) to the tip (`TAIL_NODES`)
fn tail_centerline(local: &[Vec2; TOTAL_NODES]) -> Vec<(Vec2, f32)> {
    const STEPS: usize = 6;
    let tail_start = BODY_NODES - 1;
    let mut pts = vec![(local[tail_start], 0.0)];
    let mut from = local[tail_start];
    for i in tail_start..(TOTAL_NODES - 1) {
        let ctrl = local[i];
        let to = (local[i] + local[i + 1]) * 0.5;
        let k = (i - tail_start) as f32;
        let (u0, u1) = ((k - 0.5).max(0.0), k + 0.5);
        for step in 1..=STEPS {
            let t = step as f32 / STEPS as f32;
            let v = 1.0 - t;
            let p = from * (v * v) + ctrl * (2.0 * v * t) + to * (t * t);
            pts.push((p, u0 + (u1 - u0) * t));
        }
        from = to;
    }
    pts.push((local[TOTAL_NODES - 1], TAIL_NODES as f32));
    pts
}

/// Half-width of the tail's shade layer at `u`: thick where it leaves the rump, thinning
/// towards the tip
fn tail_half_width(u: f32, s: f32) -> f32 {
    let root_flare = 1.0 + 0.45 * (1.0 - smoothstep((u - 0.6) / 2.2));
    let tip_thin = 1.0 - 0.18 * (u / TAIL_NODES as f32);
    1.85 * s * root_flare * tip_thin
}

/// A closed band along `centerline` between `u0` and `u1`, `width_mult` times the tail's
/// half-width, with a round cap when it reaches the tip
fn tail_band(
    centerline: &[(Vec2, f32)],
    u0: f32,
    u1: f32,
    width_mult: f32,
    s: f32,
) -> Option<Path> {
    let at = |u: f32| {
        let i = centerline
            .windows(2)
            .position(|w| u <= w[1].1)
            .unwrap_or(centerline.len() - 2);
        let ((a, ua), (b, ub)) = (centerline[i], centerline[i + 1]);
        a.lerp(b, ((u - ua) / (ub - ua).max(1e-6)).clamp(0.0, 1.0))
    };
    let mut pts = vec![(at(u0), u0)];
    pts.extend(
        centerline
            .iter()
            .copied()
            .filter(|&(_, u)| u > u0 && u < u1),
    );
    pts.push((at(u1), u1));

    let n = pts.len();
    let dir = |i: usize| {
        (pts[(i + 1).min(n - 1)].0 - pts[i.saturating_sub(1)].0)
            .try_normalize()
            .unwrap_or(Vec2::ZERO)
    };
    let edge = |i: usize, sign: f32| {
        let (p, u) = pts[i];
        let d = dir(i);
        p + Vec2::new(-d.y, d.x) * (sign * width_mult * tail_half_width(u, s))
    };
    let mut pb = PathBuilder::new();
    let start = edge(0, 1.0);
    pb.move_to(start.x, start.y);
    for i in 1..n {
        let p = edge(i, 1.0);
        pb.line_to(p.x, p.y);
    }
    // Round the tip off with a half circle from one edge to the other, inside the same contour
    // so it fills with the same winding
    if u1 >= TAIL_NODES as f32 {
        const ARC_STEPS: usize = 8;
        let (tip, d) = (pts[n - 1].0, dir(n - 1));
        let normal = Vec2::new(-d.y, d.x);
        let r = width_mult * tail_half_width(u1, s);
        for k in 1..ARC_STEPS {
            let (sin, cos) = (std::f32::consts::PI * k as f32 / ARC_STEPS as f32).sin_cos();
            let p = tip + (normal * cos + d * sin) * r;
            pb.line_to(p.x, p.y);
        }
    }
    for i in (0..n).rev() {
        let p = edge(i, -1.0);
        pb.line_to(p.x, p.y);
    }
    pb.close();
    pb.finish()
}

fn draw_tail(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    pal: &Palette,
    pass: Pass,
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale * cat.tail_puff;
    let centerline = tail_centerline(local);
    let tip = TAIL_NODES as f32;

    let Some(shade_band) = tail_band(&centerline, 0.0, tip, 1.0, s) else {
        return;
    };
    if pass == Pass::Outline {
        stroke_solid(pixmap, &shade_band, pal.outline, &round_stroke(1.5 * s));
        return;
    }

    fill_solid(pixmap, &shade_band, pal.tail_shade);
    if let Some(base_band) = tail_band(&centerline, 0.0, tip, 0.65, s) {
        fill_solid(pixmap, &base_band, pal.tail);
    }

    if let Some(ring_col) = pal.tail_rings {
        for ring_u in [1.0f32, 3.0, 5.0] {
            if let Some(ring) = tail_band(&centerline, ring_u, ring_u + 0.52, 0.92, s) {
                fill_solid(pixmap, &ring, ring_col);
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

/// Fore/aft shift of the shoulders and hips with the stride, as `(shoulder, hip)`
fn torso_gait_shifts(cat: &CatSim) -> (f32, f32) {
    let phase_rad = cat.gait_cycle * std::f32::consts::TAU;
    let shoulder_shift = -(phase_rad.cos()) * cat.gait_weight * 1.60 * cat.scale;
    let hip_shift = phase_rad.sin() * cat.gait_weight * 1.05 * cat.scale;
    (shoulder_shift, hip_shift)
}

/// Strokes the torso outline for the outline pass. The stroke is centred on the contour and
/// the fill covers its inner half, so it is twice the visible width.
fn draw_body_outline(
    cat: &CatSim,
    local: &[Vec2; TOTAL_NODES],
    tangents: &[Vec2; TOTAL_NODES],
    normals: &[Vec2; TOTAL_NODES],
    pal: &Palette,
    pixmap: &mut PixmapMut,
) {
    let (shoulder_shift, hip_shift) = torso_gait_shifts(cat);
    if let Some(outer_path) = build_torso_contour(
        local,
        tangents,
        normals,
        &body_half_widths(cat),
        1.0,
        shoulder_shift,
        hip_shift,
        cat.curl_amount,
    ) {
        stroke_solid(
            pixmap,
            &outer_path,
            pal.outline,
            &round_stroke(2.3 * cat.scale),
        );
    }
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
    let widths = body_half_widths(cat);
    let (shoulder_shift, hip_shift) = torso_gait_shifts(cat);

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

    fill_solid(pixmap, &outer_path, pal.flank_shade);

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
        fill_solid(pixmap, &inner_path, pal.base);
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
    let Some([primary, flank_col, spot_col]) = pal.patches else {
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

    {
        draw_blob_patch(
            &[
                local[3] + tangents[3] * (2.5 * s) + normals[3] * (1.2 * s),
                local[3] + normals[3] * (widths[3] * 1.35),
                local[4] + normals[4] * (widths[4] * 1.35),
                local[5] + normals[5] * (widths[5] * 1.1),
                local[4] - normals[4] * (0.8 * s),
            ],
            flank_col,
            body_mask,
            pixmap,
        );
    }

    {
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
        fill_solid_masked(pixmap, &path, color, mask);
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

    // Faint and tapered at both ends: a hint of the muscle under the fur rather than a drawn line
    let crease_col = scale_alpha(pal.crease, 0.6 * dorsal_fade);

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
        let bulge = normals[2] * (sign * 1.5 * s);
        if let Some(path) = crescent(p0, p1, p2, bulge) {
            fill_solid_masked(pixmap, &path, crease_col, body_mask);
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
        let bulge = normals[4] * (sign * 1.5 * s);
        if let Some(path) = crescent(p0, p1, p2, bulge) {
            fill_solid_masked(pixmap, &path, crease_col, body_mask);
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
        if let Some(cpath) = quad_curve(c0, c1, c2) {
            let fold_stroke = round_stroke(1.05 * s);
            stroke_solid_masked(pixmap, &cpath, pal.outline, &fold_stroke, body_mask);
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
    let (belly_col, tummy_tint) = (pal.belly, pal.tummy);

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
        let wisp_col = scale_alpha(pal.crease, wisp_t);
        let stroke = round_stroke(0.9 * s);
        for sign in [-1.0f32, 1.0f32] {
            for node_idx in [2usize, 3usize, 4usize] {
                let base = local[node_idx] + normals[node_idx] * (widths[node_idx] * shift_factor);
                let p0 = base
                    + tangents[node_idx] * (2.0 * s)
                    + normals[node_idx] * (sign * widths[node_idx] * 0.62 * width_scale);
                let p1 = base + normals[node_idx] * (sign * widths[node_idx] * 0.46 * width_scale);
                let p2 = base - tangents[node_idx] * (2.0 * s)
                    + normals[node_idx] * (sign * widths[node_idx] * 0.60 * width_scale);
                if let Some(path) = quad_curve(p0, p1, p2) {
                    stroke_solid_masked(pixmap, &path, wisp_col, &stroke, body_mask);
                }
            }
        }
    }
}

/// Head position and axes: `fwd` points out of the snout, `side` to the cat's left. `roll` is
/// the eased belly roll (0 upright, 1 on its back).
struct HeadFrame {
    center: Vec2,
    angle: f32,
    fwd: Vec2,
    side: Vec2,
    s: f32,
    roll: f32,
}

/// Strokes the head outline for the outline pass, at twice the visible width like the torso's
fn draw_head_outline(
    cat: &CatSim,
    head_center: Vec2,
    head_angle: f32,
    pal: &Palette,
    pixmap: &mut PixmapMut,
) {
    let s = cat.scale;
    let fwd = Vec2::from_angle(head_angle);
    let side = Vec2::new(-fwd.y, fwd.x);
    let roll = smoothstep(cat.belly_roll);
    if let Some(outer_head) = build_head_path_rolled(head_center, fwd, side, s, roll) {
        stroke_solid(pixmap, &outer_head, pal.outline, &round_stroke(2.2 * s));
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
    let h = HeadFrame {
        center: head_center,
        angle: head_angle,
        fwd,
        side: Vec2::new(-fwd.y, fwd.x),
        s,
        roll: smoothstep(cat.belly_roll),
    };
    let side = h.side;
    let roll = h.roll;

    // When lying on its back, the ears rest against the floor at the crown (+fwd) underneath the head silhouette
    let belly_up = roll > 0.45;
    if belly_up {
        draw_ears(cat, &h, pal, pixmap);
    }

    let neck_shadow_center = head_center - fwd * (1.8 * s);
    if let Some(shadow_path) = build_head_path_rolled(neck_shadow_center, fwd, side, s * 1.03, roll)
    {
        fill_solid(pixmap, &shadow_path, rgba(0, 0, 0, 22));
    }

    let Some(outer_head) = build_head_path_rolled(head_center, fwd, side, s, roll) else {
        return;
    };
    // `clip_mask` still holds the torso. The head's darker rim only shades the part outside the
    // torso; over the shoulders it takes the coat's base colour so the neck blends into the body.
    fill_solid_masked(pixmap, &outer_head, pal.base, clip_mask);
    clip_mask.invert();
    fill_solid_masked(pixmap, &outer_head, pal.flank_shade, clip_mask);
    clip_mask.invert();

    let inner_offset = fwd * (0.35 * (1.0 - 2.0 * roll) * s);
    if let Some(inner_head) =
        build_head_path_rolled(head_center + inner_offset, fwd, side, s * 0.84, roll)
    {
        fill_solid(pixmap, &inner_head, pal.base);
    }

    // Curled up asleep, the head rests on top of the flank rather than growing out of it, so a
    // faint occlusion line keeps it readable where it lies over the torso
    let rest_on_body = smoothstep((cat.curl_amount.abs() - 0.42) / 0.12);
    if rest_on_body > 0.01 {
        stroke_solid_masked(
            pixmap,
            &outer_head,
            scale_alpha(pal.outline, 0.55 * rest_on_body),
            &round_stroke(1.0 * s),
            clip_mask,
        );
    }

    if let Some([patch_col, ..]) = pal.patches {
        clip_mask.clear();
        clip_mask.fill_path(&outer_head, FillRule::Winding, true, Transform::identity());
        draw_head_patches(pal.pattern, &h, patch_col, clip_mask, pixmap);
    }

    // The snout pitches back as the cat rolls over: the upright face fades out while the
    // upside-down one facing the body fades in
    if roll < 0.50 {
        draw_upright_face(cat, &h, pal, pixmap);
    }
    if roll > 0.20 {
        clip_mask.clear();
        clip_mask.fill_path(&outer_head, FillRule::Winding, true, Transform::identity());
        draw_upturned_face(cat, &h, pal, clip_mask, pixmap);
    }

    if !belly_up {
        draw_ears(cat, &h, pal, pixmap);
    }
}

fn draw_ears(cat: &CatSim, h: &HeadFrame, pal: &Palette, pixmap: &mut PixmapMut) {
    let ear_bias = if cat.activity == Activity::Sleeping {
        -0.14
    } else {
        -0.22 * h.roll
    };
    let ear_head_center = h.center + h.fwd * (1.4 * h.roll * h.s);
    for (side_sign, twitch) in [(1.0, cat.ear_left_twitch), (-1.0, cat.ear_right_twitch)] {
        draw_ear(
            ear_head_center,
            h.angle,
            side_sign,
            twitch + ear_bias,
            h.s,
            pal,
            pixmap,
        );
    }
}

/// Coat patches on top of the head, clipped to the head silhouette in `clip_mask`
fn draw_head_patches(
    pattern: Pattern,
    h: &HeadFrame,
    patch_col: Color,
    clip_mask: &Mask,
    pixmap: &mut PixmapMut,
) {
    let (c, fwd, side, s) = (h.center, h.fwd, h.side, h.s);
    match pattern {
        Pattern::Calico => {
            draw_blob_patch(
                &[
                    c + fwd * (7.5 * s),
                    c + fwd * (2.5 * s) - side * (8.5 * s),
                    c - fwd * (6.5 * s) - side * (6.5 * s),
                    c - fwd * (4.0 * s) + side * (2.2 * s),
                    c + fwd * (2.0 * s) + side * (1.2 * s),
                ],
                patch_col,
                clip_mask,
                pixmap,
            );
        }
        Pattern::Tuxedo => {
            for sign in [-1.0f32, 1.0f32] {
                draw_blob_patch(
                    &[
                        c + fwd * (5.2 * s) + side * (sign * 1.5 * s),
                        c + fwd * (2.0 * s) + side * (sign * 8.8 * s),
                        c - fwd * (6.8 * s) + side * (sign * 6.8 * s),
                        c - fwd * (5.8 * s) + side * (sign * 0.2 * s),
                    ],
                    patch_col,
                    clip_mask,
                    pixmap,
                );
            }
        }
        Pattern::Solid => {}
    }
}

/// Nose at the snout tip (+fwd), plus happy closed eyes while asleep or nibbling. Fades out
/// over roll in [0.0, 0.50].
fn draw_upright_face(cat: &CatSim, h: &HeadFrame, pal: &Palette, pixmap: &mut PixmapMut) {
    let (c, fwd, side, s, roll) = (h.center, h.fwd, h.side, h.s, h.roll);
    let dorsal_t = smoothstep(1.0 - roll / 0.50);
    let tip_fwd = (7.9 - 1.4 * roll) * s;
    let base_fwd = (6.5 - 1.2 * roll) * s;
    let nose_tip = c + fwd * tip_fwd;
    let nose_l = c + fwd * base_fwd + side * (1.5 * dorsal_t * s);
    let nose_r = c + fwd * base_fwd - side * (1.5 * dorsal_t * s);
    if let Some(nose_path) = quad_wedge(nose_l, nose_tip, nose_r) {
        fill_solid(pixmap, &nose_path, scale_alpha(pal.nose, dorsal_t));
    }

    let happy_closed_eyes = cat.activity == Activity::Sleeping
        || cat.blissful()
        || matches!(cat.activity, Activity::Eating { elapsed, .. } if (elapsed % 16.0) < 13.2);
    if happy_closed_eyes {
        let eye_stroke = round_stroke((0.95 * s).max(0.8));
        let eye_col = scale_alpha(pal.outline, dorsal_t);
        for sign in [-1.0f32, 1.0f32] {
            let eye_c = c + fwd * (2.8 * s) + side * (sign * 3.1 * s);
            let e_inner = eye_c - side * (sign * 1.25 * s) + fwd * (0.25 * s);
            let e_outer = eye_c + side * (sign * 1.25 * s) - fwd * (0.15 * s);
            let ctrl = eye_c + fwd * (0.95 * s);
            if let Some(ep) = quad_curve(e_inner, ctrl, e_outer) {
                stroke_solid(pixmap, &ep, eye_col, &eye_stroke);
            }
        }
    }
}

/// Face of a cat lying on its back, looking toward the body (-fwd): chin, nose, happy closed
/// eyes and a 'w' mouth. Fades in over roll in [0.20, 1.0]; the chin is clipped to `clip_mask`.
fn draw_upturned_face(
    cat: &CatSim,
    h: &HeadFrame,
    pal: &Palette,
    clip_mask: &Mask,
    pixmap: &mut PixmapMut,
) {
    let (fwd, side, s) = (h.fwd, h.side, h.s);
    let face_t = smoothstep((h.roll - 0.20) / 0.80);
    let w_t = 0.35 + 0.65 * face_t;
    let lat_shift = side * (cat.belly_roll_side * (1.0 - face_t) * 2.2 * s);
    let fc = h.center + lat_shift;

    let chin_col = pal.chin;
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
    if let Some(nose_path) = quad_wedge(nose_l, nose_tip, nose_r) {
        fill_solid(
            pixmap,
            &nose_path,
            scale_alpha(rgba(226, 134, 150, 255), face_t),
        );
    }

    let eye_stroke = round_stroke((0.95 * s).max(0.8));
    let line_col = scale_alpha(pal.outline, face_t);

    // Happy closed eyes above the nose (between crown +fwd and nose -fwd), facing the body
    for sign in [-1.0f32, 1.0f32] {
        let eye_c = fc - fwd * (0.4 * s) + side * (sign * 3.0 * w_t * s);
        let e_inner = eye_c - side * (sign * 1.2 * w_t * s) - fwd * (0.15 * s);
        let e_outer = eye_c + side * (sign * 1.2 * w_t * s) + fwd * (0.20 * s);
        let ctrl = eye_c + fwd * (0.85 * s);
        if let Some(ep) = quad_curve(e_inner, ctrl, e_outer) {
            stroke_solid(pixmap, &ep, line_col, &eye_stroke);
        }
    }

    // Cute smiling 'w' mouth between the nose and the chin/body (-fwd)
    let m_center = fc - fwd * (4.3 * s);
    for sign in [-1.0f32, 1.0f32] {
        let m_ctrl = fc - fwd * (5.3 * s) + side * (sign * 0.85 * w_t * s);
        let m_end = fc - fwd * (4.2 * s) + side * (sign * 1.75 * w_t * s);
        if let Some(mp) = quad_curve(m_center, m_ctrl, m_end) {
            stroke_solid(pixmap, &mp, line_col, &eye_stroke);
        }
    }
}

/// A crescent between the quadratic curve `from`-`ctrl`-`to` and a second one whose control
/// point is moved by `bulge`, so it is thickest in the middle (about half of `bulge`) and comes
/// to a point at both ends
fn crescent(from: Vec2, ctrl: Vec2, to: Vec2, bulge: Vec2) -> Option<Path> {
    let back = ctrl + bulge;
    let mut pb = PathBuilder::new();
    pb.move_to(from.x, from.y);
    pb.quad_to(ctrl.x, ctrl.y, to.x, to.y);
    pb.quad_to(back.x, back.y, from.x, from.y);
    pb.close();
    pb.finish()
}

/// An open quadratic curve from `from` to `to`
fn quad_curve(from: Vec2, ctrl: Vec2, to: Vec2) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(from.x, from.y);
    pb.quad_to(ctrl.x, ctrl.y, to.x, to.y);
    pb.finish()
}

/// A quadratic curve closed back to its start by a straight edge
fn quad_wedge(from: Vec2, ctrl: Vec2, to: Vec2) -> Option<Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(from.x, from.y);
    pb.quad_to(ctrl.x, ctrl.y, to.x, to.y);
    pb.close();
    pb.finish()
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

    // Front edge bowing in, back edge bowing out; filled closed, outlined open at the base
    let ear_edge = || {
        let mut pb = PathBuilder::new();
        pb.move_to(base_front.x, base_front.y);
        pb.quad_to(
            (base_front.x + tip.x) * 0.5 - ear_perp.x * (0.7 * s),
            (base_front.y + tip.y) * 0.5 - ear_perp.y * (0.7 * s),
            tip.x,
            tip.y,
        );
        pb.quad_to(
            (tip.x + base_back.x) * 0.5 + ear_perp.x * (1.0 * s),
            (tip.y + base_back.y) * 0.5 + ear_perp.y * (1.0 * s),
            base_back.x,
            base_back.y,
        );
        pb
    };

    let mut fill_pb = ear_edge();
    fill_pb.close();
    let Some(outer_ear_fill) = fill_pb.finish() else {
        return;
    };

    let (outer_col, inner_col) = if pal.patches.is_some() && side_sign > 0.0 {
        (pal.base, pal.flank_shade)
    } else {
        (pal.ear_outer, pal.ear_inner)
    };

    fill_solid(pixmap, &outer_ear_fill, outer_col);

    let in_front = base_front.lerp(base_back, 0.22) + ear_dir * (0.8 * s);
    let in_back = base_back.lerp(base_front, 0.18) + ear_dir * (0.6 * s);
    let in_tip = ear_anchor + ear_dir * (5.3 * s);
    let mut in_pb = PathBuilder::new();
    in_pb.move_to(in_front.x, in_front.y);
    in_pb.line_to(in_tip.x, in_tip.y);
    in_pb.line_to(in_back.x, in_back.y);
    in_pb.close();
    if let Some(inner_ear) = in_pb.finish() {
        fill_solid(pixmap, &inner_ear, inner_col);
    }

    if let Some(ear_outline) = ear_edge().finish() {
        stroke_solid(pixmap, &ear_outline, pal.outline, &round_stroke(1.05 * s));
    }
}

/// Returns `(base_color, accent_patch_color, outline_color)` for UI coat swatches.
pub fn coat_swatch_colors(coat: &Coat) -> (Color, Color, Color) {
    let (base, accent, outline) = coat.swatch();
    (color(base), color(accent), color(outline))
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
    coat: &Coat,
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

    let icon = &coat.icon;
    let (base, shade, outline, ear_in, nose_col, eye_col) = (
        color(icon.base),
        color(icon.shade),
        color(icon.outline),
        color(icon.ear_inner),
        color(icon.nose),
        color(icon.eyes),
    );

    let Some(head_path) = build_tray_head_path(cx, cy, r, droop) else {
        return;
    };

    fill_solid(pixmap, &head_path, shade);

    if let Some(inner_path) = build_tray_head_path(cx, cy + r * 0.02, r * 0.88, droop) {
        fill_solid(pixmap, &inner_path, base);
    }

    if coat.pattern != Pattern::Solid
        && let Some(mut mask) = Mask::new(pixmap.width(), pixmap.height())
    {
        mask.fill_path(&head_path, FillRule::Winding, true, Transform::identity());
        match coat.pattern {
            Pattern::Calico => {
                if let Some(orange) =
                    PathBuilder::from_circle(cx - r * 0.48, cy - r * 0.42, r * 0.62)
                {
                    fill_solid_masked(pixmap, &orange, color(icon.patch), &mask);
                }
                if let Some(dark) = PathBuilder::from_circle(cx + r * 0.56, cy - r * 0.52, r * 0.44)
                {
                    fill_solid_masked(pixmap, &dark, color(icon.spot), &mask);
                }
            }
            Pattern::Tuxedo => {
                for sign in [-1.0f32, 1.0f32] {
                    if let Some(hood) =
                        PathBuilder::from_circle(cx + sign * r * 0.56, cy - r * 0.50, r * 0.50)
                    {
                        fill_solid_masked(pixmap, &hood, color(icon.patch), &mask);
                    }
                }
            }
            Pattern::Solid => {}
        }
    }

    let tip_x = r * (0.76 + 0.08 * droop);
    let tip_y = r * (0.98 - 0.16 * droop);
    for sign in [-1.0f32, 1.0f32] {
        let ear_col = if coat.pattern == Pattern::Calico && sign < 0.0 {
            color(icon.patch_ear)
        } else {
            ear_in
        };
        let mut epb = PathBuilder::new();
        epb.move_to(cx + sign * (r * 0.40), cy - r * 0.42);
        epb.line_to(cx + sign * (tip_x * 0.88), cy - tip_y * 0.84);
        epb.line_to(cx + sign * (r * 0.66), cy - r * 0.18);
        epb.close();
        if let Some(ep) = epb.finish() {
            fill_solid(pixmap, &ep, ear_col);
        }
    }

    let eye_y = cy + r * 0.02;
    let eye_dx = r * 0.33;
    if sleeping {
        let eye_stroke = round_stroke((r * 0.13).clamp(1.1, 2.0));
        for sign in [-1.0f32, 1.0f32] {
            let ex = cx + sign * eye_dx;
            let mut pb = PathBuilder::new();
            pb.move_to(ex - r * 0.14, eye_y - r * 0.02);
            pb.quad_to(ex, eye_y + r * 0.14, ex + r * 0.14, eye_y - r * 0.02);
            if let Some(p) = pb.finish() {
                stroke_solid(pixmap, &p, eye_col, &eye_stroke);
            }
        }
    } else {
        let er = (r * 0.135).max(1.25);
        for sign in [-1.0f32, 1.0f32] {
            if let Some(eye) = PathBuilder::from_circle(cx + sign * eye_dx, eye_y, er) {
                fill_solid(pixmap, &eye, eye_col);
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
        fill_solid(pixmap, &np, nose_col);
    }

    let head_stroke = round_stroke((r * 0.13).clamp(1.1, 2.0));
    stroke_solid(pixmap, &head_path, outline, &head_stroke);

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
    use crate::coat::CoatStyle;
    use std::path::Path;
    use tiny_skia::Pixmap;

    /// Sizes packed into `assets/catsor.ico` (Explorer, taskbar, Alt-Tab and high-DPI views)
    const APP_ICON_SIZES: [u32; 7] = [16, 24, 32, 48, 64, 128, 256];

    fn app_icon_png(size: u32) -> Vec<u8> {
        let mut pixmap = Pixmap::new(size, size).unwrap();
        render_tray_icon(
            &Coat::builtin(CoatStyle::Calico),
            false,
            false,
            false,
            &mut pixmap.as_mut(),
        );
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
