//! `--preview-sheet`: a static PNG of every coat in a grid of poses (`docs/preview.png`).
//! Each pose is reached by running the real simulation, not by posing the skeleton by hand.

use crate::cat::{Activity, CatSim, CoatStyle, FeedPreset};
use crate::render::render_cat;
use glam::Vec2;
use std::f32::consts::FRAC_PI_2;
use tiny_skia::{Color, Mask, Pixmap, PixmapPaint, Transform};

const DT: f32 = 1.0 / 60.0;
const COATS: [CoatStyle; 5] = [
    CoatStyle::Calico,
    CoatStyle::Brown,
    CoatStyle::Tuxedo,
    CoatStyle::Charcoal,
    CoatStyle::Silver,
];

/// One cell of the grid; rows are moving, resting and playing
#[derive(Clone, Copy)]
enum Pose {
    Walk,
    Trot,
    Gallop,
    CurvingWalk,
    TailChase,
    Sit,
    Groom,
    Loaf,
    SideFlop,
    Sleep,
    Stalk,
    Pounce,
    PawBat,
    BellyUp,
    Eat,
}

const ROWS: [[Pose; 5]; 3] = [
    [
        Pose::Walk,
        Pose::Trot,
        Pose::Gallop,
        Pose::CurvingWalk,
        Pose::TailChase,
    ],
    [
        Pose::Sit,
        Pose::Groom,
        Pose::Loaf,
        Pose::SideFlop,
        Pose::Sleep,
    ],
    [
        Pose::Stalk,
        Pose::Pounce,
        Pose::PawBat,
        Pose::BellyUp,
        Pose::Eat,
    ],
];

fn run(cat: &mut CatSim, cursor: Vec2, secs: f32) {
    for _ in 0..(secs / DT).round() as usize {
        cat.update(cursor, DT);
    }
}

/// Runs a cat for `secs` with the cursor kept `lead` px (unscaled) ahead of its head, heading
/// `angle` and turning at `turn` rad/s, so it keeps moving at a steady gait
fn chase(cat: &mut CatSim, mut angle: f32, turn: f32, lead: f32, secs: f32) {
    for _ in 0..(secs / DT).round() as usize {
        angle += turn * DT;
        let cursor = cat.nodes[0] + Vec2::from_angle(angle) * lead * cat.scale;
        cat.update(cursor, DT);
    }
}

/// A cat that walked up toward a resting cursor and sat down facing it (upward)
fn settled(coat: CoatStyle, scale: f32) -> (CatSim, Vec2) {
    let mut cat = CatSim::new(Vec2::ZERO, scale, coat);
    let cursor = cat.nodes[0] + Vec2::new(10.0, -150.0) * scale;
    run(&mut cat, cursor, 8.0);
    (cat, cursor)
}

fn posed(pose: Pose, coat: CoatStyle, scale: f32) -> CatSim {
    let up = -FRAC_PI_2;
    let moving = |angle: f32, turn: f32, lead: f32| {
        let mut cat = CatSim::new(Vec2::ZERO, scale, coat);
        chase(&mut cat, angle, turn, lead, 3.0);
        cat
    };
    match pose {
        Pose::Walk => moving(up + 0.55, 0.0, 130.0),
        Pose::Trot => moving(up + 0.45, 0.0, 400.0),
        Pose::Gallop => {
            let mut cat = CatSim::new(Vec2::ZERO, scale, coat);
            for _ in 0..(2.0 / DT) as usize {
                cat.sprinting = true;
                cat.shake_energy = 2.0;
                let cursor = cat.nodes[0] + Vec2::from_angle(up + 0.5) * 400.0 * scale;
                cat.update(cursor, DT);
            }
            cat
        }
        Pose::CurvingWalk => moving(up + 1.4, -1.1, 130.0),
        Pose::TailChase => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::TailChasing {
                center: cat.nodes[2],
                dir: 1.0,
                timer: 0.0,
                duration: 60.0,
            };
            run(&mut cat, cursor, 0.75);
            cat
        }
        Pose::Sit => settled(coat, scale).0,
        Pose::Groom => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::Grooming {
                side: 1.0,
                timer: 0.0,
                duration: 60.0,
            };
            run(&mut cat, cursor, 1.3);
            cat
        }
        Pose::Loaf => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::Lying;
            run(&mut cat, cursor, 2.5);
            cat
        }
        Pose::SideFlop => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::DramaticFlop {
                side: -1.0,
                timer: 0.0,
            };
            run(&mut cat, cursor, 2.5);
            cat
        }
        Pose::Sleep => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::Sleeping;
            run(&mut cat, cursor, 4.0);
            cat
        }
        Pose::Stalk => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::StalkWiggle {
                timer: 0.0,
                wiggle_duration: 1.6,
            };
            run(&mut cat, cursor, 1.0);
            cat
        }
        Pose::Pounce => {
            let (mut cat, cursor) = settled(coat, scale);
            let start_pos = cat.nodes[0];
            cat.activity = Activity::Pouncing {
                start_pos,
                target_pos: start_pos + Vec2::new(0.0, -90.0) * scale,
                progress: 0.0,
            };
            while matches!(cat.activity, Activity::Pouncing { progress, .. } if progress < 0.45) {
                cat.update(cursor, DT);
            }
            cat
        }
        Pose::PawBat => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::PawBatting {
                side: 1.0,
                timer: 0.0,
                duration: 60.0,
            };
            run(&mut cat, cursor, 0.32);
            cat
        }
        Pose::BellyUp => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.activity = Activity::BellyUp {
                side: 1.0,
                timer: 0.0,
            };
            run(&mut cat, cursor, 2.5);
            cat
        }
        Pose::Eat => {
            let (mut cat, cursor) = settled(coat, scale);
            cat.start_feeding(FeedPreset::Mins30);
            run(&mut cat, cursor, 3.0);
            cat
        }
    }
}

/// Renders `cat` into its own pixmap, cropped to the pixels it covers
fn render_cropped(cat: &CatSim) -> Option<Pixmap> {
    let dim = (150.0 * cat.scale).ceil() as u32;
    let mut full = Pixmap::new(dim, dim)?;
    let mut mask = Mask::new(dim, dim)?;
    let origin = cat.window_center() - Vec2::splat(dim as f32 * 0.5);
    render_cat(cat, &mut full.as_mut(), &mut mask, origin);

    let (mut x0, mut y0, mut x1, mut y1) = (dim, dim, 0, 0);
    for (i, px) in full.pixels().iter().enumerate() {
        if px.alpha() > 8 {
            let (x, y) = (i as u32 % dim, i as u32 / dim);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x + 1);
            y1 = y1.max(y + 1);
        }
    }
    if x0 >= x1 {
        return None;
    }
    full.clone_rect(tiny_skia::IntRect::from_ltrb(
        x0 as i32, y0 as i32, x1 as i32, y1 as i32,
    )?)
}

pub fn generate_preview_sheet(path: &str, scale: f32) -> Result<(), Box<dyn std::error::Error>> {
    let cell = (118.0 * scale).round() as u32;
    let margin = (14.0 * scale).round() as u32;
    let width = margin * 2 + cell * COATS.len() as u32;
    let height = margin * 2 + cell * ROWS.len() as u32;
    let mut sheet = Pixmap::new(width, height).ok_or("Failed to allocate preview pixmap")?;
    sheet.fill(Color::WHITE);

    for (row, poses) in ROWS.iter().enumerate() {
        for (col, (&pose, &coat)) in poses.iter().zip(COATS.iter()).enumerate() {
            let Some(sprite) = render_cropped(&posed(pose, coat, scale)) else {
                continue;
            };
            // Center each sprite in its cell
            let cx = margin + cell * col as u32 + cell / 2;
            let cy = margin + cell * row as u32 + cell / 2;
            sheet.draw_pixmap(
                cx as i32 - sprite.width() as i32 / 2,
                cy as i32 - sprite.height() as i32 / 2,
                sprite.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
    }

    sheet.save_png(path)?;
    Ok(())
}
