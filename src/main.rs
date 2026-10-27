// GUI subsystem: no console window when launched from Explorer. `win32::attach_parent_console`
// restores CLI output (`--help`, errors) when launched from a terminal.
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod autostart;
mod cat;
mod coat;
#[cfg(test)]
mod demo;
mod designer;
mod prefs;
mod preview;
mod render;
mod tray;
#[cfg(any(target_os = "windows", windows_check))]
mod win32;
#[cfg(not(any(target_os = "windows", windows_check)))]
mod x11;

use crate::cat::Temperament;
use crate::coat::{CoatId, CoatLibrary, CoatStyle};
use crate::prefs::UserPrefs;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "catsor",
    about = "Top-down procedural desktop cat companion for Linux (X11) and Windows that leisurely follows the mouse cursor"
)]
pub(crate) struct Args {
    /// The first cat's coat: calico, brown, charcoal, tuxedo, silver, a custom coat's name from coats.toml, or
    /// random; overrides saved preference
    #[arg(short, long)]
    coat: Option<String>,

    /// Visual size multiplier (default 1.0 = compact ~38px body so screen stays clear); overrides saved preference
    #[arg(short, long)]
    scale: Option<f32>,

    /// The first cat's temperament (balanced, playful, lazy, aloof, realistic); overrides saved preference
    #[arg(short, long, value_enum)]
    temperament: Option<Temperament>,

    /// Disable the system tray icon
    #[arg(long, default_value_t = false)]
    no_tray: bool,
    /// Target frame rate
    #[arg(long, default_value_t = 60)]
    fps: u32,

    /// Optional: render a multi-pose preview PNG to the given path and exit (every built-in coat,
    /// or only the one given with --coat)
    #[arg(long)]
    preview_sheet: Option<String>,

    /// Open the coat designer at startup (the tray's "Design Coat..." does the same)
    #[arg(long, default_value_t = false)]
    design_coat: bool,

    /// Print the current coat (or the one given with --coat) as a [[coat]] entry for coats.toml
    /// and exit
    #[arg(long, default_value_t = false)]
    print_coat: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(any(target_os = "windows", windows_check))]
    win32::attach_parent_console();

    let args = Args::parse();
    let coats = CoatLibrary::load();
    let cli_coat = match args.coat.as_deref().map(|name| find_coat(&coats, name)) {
        Some(Ok(id)) => Some(id),
        Some(Err(e)) => {
            eprintln!("catsor: {e}");
            std::process::exit(2);
        }
        None => None,
    };

    if let Some(preview_path) = &args.preview_sheet {
        let preview_scale = args.scale.unwrap_or(1.0).clamp(0.5, 2.5);
        let only = cli_coat.as_ref().map(|id| coats.resolve(id));
        preview::generate_preview_sheet(preview_path, preview_scale, only)?;
        return Ok(());
    }

    let mut prefs = UserPrefs::load();
    if args.print_coat {
        let coat = coats.resolve(cli_coat.as_ref().unwrap_or(&prefs.cats[0].coat));
        print!("{}", coat.to_toml_entry(&format!("My {}", coat.name)));
        return Ok(());
    }

    // --coat and --temperament apply to the first cat
    let mut cli_overrode = false;
    if let Some(coat) = cli_coat {
        prefs.cats[0].coat = coat;
        cli_overrode = true;
    }
    if let Some(scale) = args.scale {
        prefs.scale = scale.clamp(0.5, 2.5);
        cli_overrode = true;
    }
    if let Some(temperament) = args.temperament {
        prefs.cats[0].temperament = temperament;
        cli_overrode = true;
    }
    if cli_overrode {
        prefs.save();
    }

    #[cfg(any(target_os = "windows", windows_check))]
    return win32::run(&args, prefs, coats);

    #[cfg(not(any(target_os = "windows", windows_check)))]
    x11::run(&args, prefs, coats)
}

/// Looks up a `--coat` value, listing the available coats when there is no such coat
fn find_coat(coats: &CoatLibrary, name: &str) -> Result<CoatId, String> {
    if let Some(id) = CoatId::parse(name)
        && coats.find(&id).is_some()
    {
        return Ok(id);
    }
    let mut available: Vec<&str> = CoatStyle::ALL.iter().map(|c| c.as_config_str()).collect();
    available.extend(coats.custom().iter().map(|c| &*c.name));
    available.push("random");
    let file = coats
        .path()
        .map(|p| format!(" (custom coats are read from {})", p.display()))
        .unwrap_or_default();
    Err(format!(
        "unknown coat \"{name}\"; available: {}{file}",
        available.join(", ")
    ))
}
