// GUI subsystem: no console window when launched from Explorer. `win32::attach_parent_console`
// restores CLI output (`--help`, errors) when launched from a terminal.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod autostart;
mod cat;
mod prefs;
mod render;
mod tray;
#[cfg(any(target_os = "windows", windows_check))]
mod win32;
#[cfg(not(any(target_os = "windows", windows_check)))]
mod x11;

use crate::cat::{Activity, CatSim, CoatStyle, FeedPreset, Temperament};
use crate::prefs::UserPrefs;
use crate::render::render_cat;
use clap::Parser;
use glam::Vec2;
use tiny_skia::{Mask, Pixmap};

#[derive(Parser, Debug)]
#[command(
    name = "catsor",
    about = "Top-down procedural desktop cat companion for Linux (X11) and Windows that leisurely follows the mouse cursor"
)]
pub(crate) struct Args {
    /// Coat pattern (calico, brown, charcoal, tuxedo, silver); overrides saved preference
    #[arg(short, long, value_enum)]
    coat: Option<CoatStyle>,

    /// Visual size multiplier (default 1.0 = compact ~38px body so screen stays clear); overrides saved preference
    #[arg(short, long)]
    scale: Option<f32>,

    /// Initial cat temperament (balanced, playful, lazy, aloof, realistic); overrides saved preference
    #[arg(short, long, value_enum)]
    temperament: Option<Temperament>,

    /// Disable the system tray icon
    #[arg(long, default_value_t = false)]
    no_tray: bool,
    /// Target frame rate
    #[arg(long, default_value_t = 60)]
    fps: u32,

    /// Optional: render a multi-pose preview PNG to the given path and exit
    #[arg(long)]
    preview_sheet: Option<String>,
}

fn generate_preview_sheet(path: &str, scale: f32) -> Result<(), Box<dyn std::error::Error>> {
    let width = 760;
    let height = 260;
    let mut sheet = Pixmap::new(width, height).ok_or("Failed to allocate preview pixmap")?;
    sheet.fill(tiny_skia::Color::from_rgba8(245, 243, 238, 255));

    let coats = [
        CoatStyle::Calico,
        CoatStyle::Brown,
        CoatStyle::Tuxedo,
        CoatStyle::Charcoal,
        CoatStyle::Silver,
    ];

    for (idx, &coat) in coats.iter().enumerate() {
        let col_x = 80.0 + (idx as f32) * 145.0;

        // Top row: walking / curving pose
        let mut walk_cat = CatSim::new(Vec2::new(col_x, 70.0), scale, coat);
        walk_cat.activity = Activity::Walking;
        walk_cat.posture_extension = 1.60;
        walk_cat.gait_weight = 1.0;
        walk_cat.gait_cycle = ((idx as f32) * 0.23).fract();
        walk_cat.body_angle = -std::f32::consts::FRAC_PI_2 + (idx as f32 - 2.0) * 0.12;
        walk_cat.head_yaw = (idx as f32 - 2.0) * 0.18;
        walk_cat.nodes[0] = Vec2::new(col_x, 42.0);
        for _ in 0..12 {
            walk_cat.update(Vec2::new(col_x, -120.0), 0.016);
        }
        render_into_subregion(&walk_cat, &mut sheet);

        // Bottom row: funny & relaxing poses (PawBatting, StalkWiggle, Pouncing, Sleeping curled up, BellyUp)
        let mut rest_cat = CatSim::new(Vec2::new(col_x, 195.0), scale, coat);
        rest_cat.body_angle = -std::f32::consts::FRAC_PI_2;
        for n in 0..cat::TOTAL_NODES {
            rest_cat.nodes[n] = Vec2::new(col_x, 180.0 + (n as f32) * 4.5 * scale);
        }
        match idx {
            0 => {
                rest_cat.start_feeding(FeedPreset::Mins30);
                rest_cat.posture_extension = 1.20;
            }
            1 => {
                rest_cat.activity = Activity::StalkWiggle {
                    timer: 0.25,
                    wiggle_duration: 0.65,
                };
                rest_cat.posture_extension = 1.14;
                rest_cat.hip_wiggle_offset = 2.4 * scale;
                rest_cat.ear_left_twitch = -0.36;
                rest_cat.ear_right_twitch = -0.36;
            }
            2 => {
                rest_cat.activity = Activity::Pouncing {
                    start_pos: Vec2::new(col_x, 210.0),
                    target_pos: Vec2::new(col_x, 150.0),
                    progress: 0.5,
                };
                rest_cat.posture_extension = 1.88;
            }
            3 => {
                rest_cat.curl_side = 1.0;
                rest_cat.activity = Activity::Sleeping;
                rest_cat.posture_extension = 0.98;
                rest_cat.curl_amount = 0.56;
                rest_cat.head_yaw = 2.38;
            }
            _ => {
                rest_cat.curl_side = 1.0;
                rest_cat.activity = Activity::BellyUp {
                    side: 1.0,
                    timer: 0.8,
                };
                rest_cat.belly_roll = 1.0;
                rest_cat.posture_extension = 1.30;
                rest_cat.curl_amount = 0.16;
            }
        }
        let (steps, target_y) = if idx >= 3 { (60, 195.0) } else { (8, 120.0) };
        for _ in 0..steps {
            rest_cat.update(Vec2::new(col_x, target_y), 0.016);
        }
        render_into_subregion(&rest_cat, &mut sheet);
    }

    sheet.save_png(path)?;
    Ok(())
}

fn render_into_subregion(cat: &CatSim, target: &mut Pixmap) {
    let win_size = (112.0 * cat.scale).ceil() as u32;
    let Some(mut sub) = Pixmap::new(win_size, win_size) else {
        return;
    };
    let Some(mut mask) = Mask::new(win_size, win_size) else {
        return;
    };
    let center = cat.window_center();
    let origin = center - Vec2::splat(win_size as f32 * 0.5);
    render_cat(cat, &mut sub.as_mut(), &mut mask, origin);

    let paint = tiny_skia::PixmapPaint::default();
    target.draw_pixmap(
        origin.x.round() as i32,
        origin.y.round() as i32,
        sub.as_ref(),
        &paint,
        tiny_skia::Transform::identity(),
        None,
    );
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(any(target_os = "windows", windows_check))]
    win32::attach_parent_console();

    let args = Args::parse();
    if let Some(preview_path) = &args.preview_sheet {
        let preview_scale = args.scale.unwrap_or(1.0).clamp(0.5, 2.5);
        generate_preview_sheet(preview_path, preview_scale)?;
        return Ok(());
    }

    let mut prefs = UserPrefs::load();
    let mut cli_overrode = false;
    if let Some(coat) = args.coat {
        prefs.coat = coat;
        cli_overrode = true;
    }
    if let Some(scale) = args.scale {
        prefs.scale = scale.clamp(0.5, 2.5);
        cli_overrode = true;
    }
    if let Some(temperament) = args.temperament {
        prefs.behaviors.temperament = temperament;
        cli_overrode = true;
    }
    if cli_overrode {
        prefs.save();
    }

    #[cfg(any(target_os = "windows", windows_check))]
    return win32::run(&args, prefs);

    #[cfg(not(any(target_os = "windows", windows_check)))]
    x11::run(&args, prefs)
}
