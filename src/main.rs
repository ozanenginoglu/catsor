// GUI subsystem: no console window when launched from Explorer. `win32::attach_parent_console`
// restores CLI output (`--help`, errors) when launched from a terminal.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod autostart;
mod cat;
#[cfg(test)]
mod demo;
mod prefs;
mod preview;
mod render;
mod tray;
#[cfg(any(target_os = "windows", windows_check))]
mod win32;
#[cfg(not(any(target_os = "windows", windows_check)))]
mod x11;

use crate::cat::{CoatStyle, Temperament};
use crate::prefs::UserPrefs;
use clap::Parser;

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(any(target_os = "windows", windows_check))]
    win32::attach_parent_console();

    let args = Args::parse();
    if let Some(preview_path) = &args.preview_sheet {
        let preview_scale = args.scale.unwrap_or(1.0).clamp(0.5, 2.5);
        preview::generate_preview_sheet(preview_path, preview_scale)?;
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
