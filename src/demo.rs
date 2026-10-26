//! Renders the README demo GIF (`docs/demo.gif`): a scripted, human-like cursor drives the real
//! `CatSim`, and each frame is drawn on a white background with a mouse pointer on top.
//!
//! `cargo test regenerate_demo_gif -- --ignored --nocapture` (needs `ffmpeg` on `PATH`)

use crate::cat::{Activity, CatSim, CoatStyle, smoothstep};
use crate::render::{cat_window_dim, render_cat};
use glam::Vec2;
use std::f32::consts::TAU;
use std::path::Path;
use std::process::Command;
use tiny_skia::{
    Color, FillRule, Mask, Paint, PathBuilder, Pixmap, PixmapPaint, Stroke, Transform,
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;
const SCALE: f32 = 1.8;
const SIM_HZ: f32 = 60.0;
const GIF_FPS: u32 = 30;
/// Simulated seconds before the first recorded frame, so the cat starts out settled
const PRE_ROLL: f32 = 6.0;

/// One piece of the scripted cursor path. Every move is a deliberate pointing motion.
enum Gesture {
    /// The hand rests on the mouse; the pointer doesn't move at all
    Hold(f32),
    /// A point-to-point move along a gently curved path with a minimum-jerk speed profile
    /// (smooth start, smooth stop), undershooting slightly and finishing with a small correction,
    /// like a real aimed hand movement
    Move { to: Vec2, secs: f32, bend: f32 },
    /// Teasing the cat like a laser dot: an aimed move out to `dist` px (unscaled) in front of
    /// its face, then flicking the pointer side to side. Aimed relative to where the cat is, as
    /// a person would
    Tease {
        secs: f32,
        amp: f32,
        hz: f32,
        dist: f32,
    },
    /// Shaking the mouse left and right to call the cat over
    Shake { secs: f32, amp: f32, hz: f32 },
}

/// Minimum-jerk position profile (0 → 1) for normalized time `t`
fn min_jerk(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * t * (10.0 - 15.0 * t + 6.0 * t * t)
}

/// Rises and falls smoothly over `t` in 0..1, so wiggles and shakes start and stop gently
fn envelope(t: f32, edge: f32) -> f32 {
    smoothstep((t.min(1.0 - t) / edge).clamp(0.0, 1.0))
}

fn bezier(a: Vec2, c: Vec2, b: Vec2, t: f32) -> Vec2 {
    let u = 1.0 - t;
    a * (u * u) + c * (2.0 * u * t) + b * (t * t)
}

/// Expands one gesture, starting at `pos`, into one cursor position per simulation step
fn gesture_track(gesture: &Gesture, pos: Vec2, cat: &CatSim) -> Vec<Vec2> {
    let dt = 1.0 / SIM_HZ;
    let steps = |secs: f32| (secs * SIM_HZ).round().max(1.0) as usize;
    let mut track = Vec::new();
    match *gesture {
        Gesture::Hold(secs) => track.extend(std::iter::repeat_n(pos, steps(secs))),
        Gesture::Move { to, secs, bend } => aimed_move(&mut track, pos, to, secs, bend),
        Gesture::Tease {
            secs,
            amp,
            hz,
            dist,
        } => {
            let head = cat.nodes[0];
            let out = (pos - head).normalize_or(Vec2::X);
            let center = head + out * dist * cat.scale;
            aimed_move(&mut track, pos, center, 0.45, 0.08);
            // Flick across the cat's line of sight, slightly tilted
            let axis = Vec2::new(-out.y, out.x).rotate(Vec2::from_angle(0.15));
            let n = steps(secs);
            for i in 1..=n {
                let t = i as f32 * dt;
                let k = envelope(t / secs, 0.12);
                // The hand's rhythm wanders a little, and the dot drifts slowly
                let phase = TAU * hz * t + 0.6 * (TAU * 0.7 * t).sin();
                let along = phase.sin() * amp * cat.scale * k;
                let across = (TAU * 0.45 * t).sin() * amp * cat.scale * 0.3 * k;
                track.push(center + axis * along + out * across);
            }
        }
        Gesture::Shake { secs, amp, hz } => {
            let n = steps(secs);
            for i in 1..=n {
                let t = i as f32 * dt;
                let k = envelope(t / secs, 0.15);
                let x = (TAU * hz * t).sin() * amp * k;
                // Wrist shakes swing on a slight arc
                let y = -(x / amp).powi(2) * amp * 0.12;
                track.push(pos + Vec2::new(x, y));
            }
        }
    }
    track
}

/// A curved, minimum-jerk primary movement that lands ~4% short and slightly off the line,
/// then a short corrective sub-movement that homes in on `to`
fn aimed_move(track: &mut Vec<Vec2>, from: Vec2, to: Vec2, secs: f32, bend: f32) {
    let delta = to - from;
    let normal = Vec2::new(-delta.y, delta.x).normalize_or_zero();
    let primary_end = from + delta * 0.96 + normal * (delta.length() * 0.012);
    let control = (from + primary_end) * 0.5 + normal * (delta.length() * bend);
    let primary = secs * 0.82;
    let n = (primary * SIM_HZ).round().max(1.0) as usize;
    for i in 1..=n {
        track.push(bezier(
            from,
            control,
            primary_end,
            min_jerk(i as f32 / n as f32),
        ));
    }
    let n = ((secs - primary) * SIM_HZ).round().max(1.0) as usize;
    for i in 1..=n {
        track.push(primary_end.lerp(to, min_jerk(i as f32 / n as f32)));
    }
}

/// A standard arrow pointer with its hotspot at `tip`
fn draw_pointer(frame: &mut Pixmap, tip: Vec2) {
    const ARROW: [(f32, f32); 7] = [
        (0.0, 0.0),
        (0.0, 16.0),
        (4.0, 12.5),
        (6.6, 18.6),
        (9.0, 17.6),
        (6.5, 11.6),
        (11.4, 11.6),
    ];
    let k = 1.35;
    let mut pb = PathBuilder::new();
    for (i, &(x, y)) in ARROW.iter().enumerate() {
        let p = tip + Vec2::new(x, y) * k;
        if i == 0 {
            pb.move_to(p.x, p.y);
        } else {
            pb.line_to(p.x, p.y);
        }
    }
    pb.close();
    let Some(path) = pb.finish() else { return };

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(0, 0, 0, 40));
    frame.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::from_translate(1.2, 1.8),
        None,
    );
    paint.set_color(Color::from_rgba8(20, 20, 20, 255));
    frame.fill_path(
        &path,
        &paint,
        FillRule::Winding,
        Transform::identity(),
        None,
    );
    paint.set_color(Color::WHITE);
    let stroke = Stroke {
        width: 1.3,
        line_join: tiny_skia::LineJoin::Round,
        ..Stroke::default()
    };
    frame.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
}

fn render_frame(cat: &CatSim, cursor: Vec2, frame: &mut Pixmap, sub: &mut Pixmap, mask: &mut Mask) {
    frame.fill(Color::WHITE);
    let origin = (cat.window_center() - Vec2::splat(sub.width() as f32 * 0.5)).round();
    render_cat(cat, &mut sub.as_mut(), mask, origin);
    frame.draw_pixmap(
        origin.x as i32,
        origin.y as i32,
        sub.as_ref(),
        &PixmapPaint::default(),
        Transform::identity(),
        None,
    );
    draw_pointer(frame, cursor);
}

fn activity_name(a: &Activity) -> String {
    let s = format!("{a:?}");
    s.split([' ', '{']).next().unwrap_or_default().to_string()
}

#[test]
#[ignore]
fn regenerate_demo_gif() {
    use Gesture::*;
    let v = Vec2::new;

    let home = v(250.0, 190.0);
    let script = [
        Hold(1.6),
        // Over to the right: the cat gets up and walks across
        Move {
            to: v(640.0, 330.0),
            secs: 0.9,
            bend: 0.10,
        },
        Hold(6.2),
        // Point a little way off and flick the pointer like a laser dot: the cat gets up,
        // crouches, wiggles its rear and pounces
        Tease {
            secs: 1.9,
            amp: 14.0,
            hz: 4.5,
            dist: 135.0,
        },
        Hold(0.4),
        // The "dot" escapes a little, then rests while the cat recovers
        Move {
            to: v(700.0, 460.0),
            secs: 0.5,
            bend: -0.15,
        },
        Hold(4.0),
        // Far away, then a quick left-right shake: the cat gallops over
        Move {
            to: home,
            secs: 0.85,
            bend: 0.08,
        },
        Hold(0.2),
        Shake {
            secs: 0.9,
            amp: 50.0,
            hz: 3.6,
        },
        Hold(5.0),
    ];

    let dt = 1.0 / SIM_HZ;
    let mut cat = CatSim::new(home + v(60.0, 40.0), SCALE, CoatStyle::Calico);
    // Pre-roll with the pointer resting where the GIF starts, so the cat sits beside it
    let mut pre = 0.0;
    while pre < PRE_ROLL {
        cat.update(home, dt);
        pre += dt;
    }

    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/demo-frames");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let dim = cat_window_dim(SCALE) as u32;
    let mut sub = Pixmap::new(dim, dim).unwrap();
    let mut mask = Mask::new(dim, dim).unwrap();
    let mut frame = Pixmap::new(WIDTH, HEIGHT).unwrap();
    let every = (SIM_HZ as u32 / GIF_FPS) as usize;
    let mut last = String::new();
    let mut cursor = home;
    let mut step = 0;
    for gesture in &script {
        for p in gesture_track(gesture, cursor, &cat) {
            cursor = p;
            cat.update(cursor, dt);
            let name = activity_name(&cat.activity) + if cat.sprinting { " (sprint)" } else { "" };
            if name != last {
                let dist = cursor.distance(cat.nodes[0]) / SCALE;
                println!(
                    "{:6.2}s  {name}  cursor {dist:.0} px from head, excitement {:.2}",
                    step as f32 * dt,
                    cat.play_excitement
                );
                last = name;
            }
            if step % every == 0 {
                render_frame(&cat, cursor, &mut frame, &mut sub, &mut mask);
                frame
                    .save_png(dir.join(format!("{:05}.png", step / every)))
                    .unwrap();
            }
            step += 1;
        }
    }

    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/demo.gif");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-y",
            "-framerate",
            &GIF_FPS.to_string(),
            "-i",
        ])
        .arg(dir.join("%05d.png"))
        .args([
            "-filter_complex",
            "[0]split[a][b];[a]palettegen=max_colors=96:stats_mode=full[p];\
             [b][p]paletteuse=dither=none:diff_mode=rectangle",
            "-loop",
            "0",
        ])
        .arg(&out)
        .status()
        .expect("ffmpeg must be on PATH");
    assert!(status.success(), "ffmpeg failed");
}
