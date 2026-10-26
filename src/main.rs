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

use crate::cat::{Activity, CatSim, CoatStyle, FeedPreset, Temperament};
use crate::prefs::UserPrefs;
use crate::render::render_cat;
#[cfg(not(any(target_os = "windows", windows_check)))]
use crate::render::{bowl_window_dim, cat_window_dim, render_bowl_overlay};
#[cfg(not(any(target_os = "windows", windows_check)))]
use crate::tray::{
    ABOUT_URL, TrayAction, TrayController, TrayState, normalize_tray_action, rgba_to_bgra,
};
use clap::Parser;
use glam::Vec2;
#[cfg(not(any(target_os = "windows", windows_check)))]
use std::time::{Duration, Instant};
use tiny_skia::{Mask, Pixmap};
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::connection::Connection;
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::protocol::Event;
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::protocol::shape::{self, ConnectionExt as _};
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClipOrdering, ColormapAlloc, ConfigureWindowAux,
    ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, GrabMode, ImageFormat, PropMode,
    Screen, ScreenSaver, StackMode, Visibility, VisualClass, Visualid, Window, WindowClass,
};
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::wrapper::ConnectionExt as _;

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

#[cfg(not(any(target_os = "windows", windows_check)))]
fn find_argb_visual(screen: &Screen) -> Option<(u8, Visualid)> {
    for depth in &screen.allowed_depths {
        if depth.depth == 32 {
            for visual in &depth.visuals {
                if visual.class == VisualClass::TRUE_COLOR && visual.bits_per_rgb_value == 8 {
                    return Some((32, visual.visual_id));
                }
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "windows", windows_check)))]
fn apply_input_passthrough<C: Connection>(
    conn: &C,
    win: Window,
    empty_region: xfixes::Region,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.shape_rectangles(
        shape::SO::SET,
        shape::SK::INPUT,
        ClipOrdering::UNSORTED,
        win,
        0,
        0,
        &[],
    )?;
    conn.xfixes_set_window_shape_region(win, shape::SK::INPUT, 0, 0, empty_region)?;
    Ok(())
}

/// Maps `win` and re-applies the empty input shape, which some servers reset on map
#[cfg(not(any(target_os = "windows", windows_check)))]
fn map_passthrough_window<C: Connection>(
    conn: &C,
    win: Window,
    empty_region: xfixes::Region,
) -> Result<(), Box<dyn std::error::Error>> {
    conn.map_window(win)?;
    apply_input_passthrough(conn, win, empty_region)
}

#[cfg(not(any(target_os = "windows", windows_check)))]
fn ungrab_pointer_and_keyboard<C: Connection>(conn: &C) {
    let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
    let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
}

#[cfg(not(any(target_os = "windows", windows_check)))]
fn is_catsor_overlay<C: Connection>(conn: &C, catsor_overlay_atom: Atom, win: Window) -> bool {
    if win == x11rb::NONE {
        return false;
    }
    conn.get_property(false, win, catsor_overlay_atom, AtomEnum::CARDINAL, 0, 1)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .and_then(|reply| reply.value32()?.next())
        == Some(1)
}

#[cfg(not(any(target_os = "windows", windows_check)))]
struct X11KeepAwake {
    active: bool,
    inhibit_child: Option<std::process::Child>,
    last_screen_reset: Instant,
}

#[cfg(not(any(target_os = "windows", windows_check)))]
impl X11KeepAwake {
    fn new() -> Self {
        Self {
            active: false,
            inhibit_child: None,
            last_screen_reset: Instant::now(),
        }
    }

    fn set_active<C: Connection>(&mut self, conn: &C, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.last_screen_reset = Instant::now();

        if active {
            let _ = conn.force_screen_saver(ScreenSaver::RESET);
            self.inhibit_child = std::process::Command::new("systemd-inhibit")
                .args([
                    "--what=idle:sleep",
                    "--who=catsor",
                    "--why=Feeding cat (keep screen awake)",
                    "sleep",
                    "infinity",
                ])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .ok();
        } else if let Some(mut child) = self.inhibit_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    // Only idle inhibition and screensaver resets; never synthesizes pointer or key input.
    fn tick<C: Connection>(&mut self, conn: &C, feeding: bool) {
        self.set_active(conn, feeding);
        if feeding && self.last_screen_reset.elapsed() >= Duration::from_secs(25) {
            self.last_screen_reset = Instant::now();
            let _ = conn.force_screen_saver(ScreenSaver::RESET);
        }
    }
}

#[cfg(not(any(target_os = "windows", windows_check)))]
impl Drop for X11KeepAwake {
    fn drop(&mut self) {
        if let Some(mut child) = self.inhibit_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
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
    run_x11(&args, prefs)
}

#[cfg(not(any(target_os = "windows", windows_check)))]
fn run_x11(args: &Args, prefs: UserPrefs) -> Result<(), Box<dyn std::error::Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let screen = &conn.setup().roots[screen_num];
    let root = screen.root;

    let (depth, visual_id) =
        find_argb_visual(screen).ok_or("No 32-bit ARGB TrueColor visual found on X11 screen")?;

    let colormap = conn.generate_id()?;
    conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual_id)?;

    let _ = conn.change_window_attributes(
        root,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
    );
    // Compact window bounding box around the cat (~108x108 at scale 1.0)
    let mut win_dim = cat_window_dim(prefs.scale);
    let win = conn.generate_id()?;

    // Query initial cursor location so the cat spawns nearby
    let pointer = conn.query_pointer(root)?.reply()?;
    let start_cursor = Vec2::new(pointer.root_x as f32, pointer.root_y as f32);

    let mut cat = CatSim::new(start_cursor, prefs.scale, prefs.coat);
    cat.behaviors = prefs.behaviors;
    let init_center = cat.window_center();
    let mut win_x = (init_center.x - (win_dim as f32) * 0.5).round() as i16;
    let mut win_y = (init_center.y - (win_dim as f32) * 0.5).round() as i16;
    let create_aux = CreateWindowAux::new()
        .background_pixel(0)
        .border_pixel(0)
        .override_redirect(1)
        .event_mask(
            EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY | EventMask::VISIBILITY_CHANGE,
        )
        .colormap(colormap);

    conn.create_window(
        depth,
        win,
        root,
        win_x,
        win_y,
        win_dim,
        win_dim,
        0,
        WindowClass::INPUT_OUTPUT,
        visual_id,
        &create_aux,
    )?;

    // Set window properties so picom suppresses window shadows & fade animations
    let wm_type = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
        .reply()?
        .atom;
    let wm_type_utility = conn
        .intern_atom(false, b"_NET_WM_WINDOW_TYPE_UTILITY")?
        .reply()?
        .atom;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        wm_type,
        AtomEnum::ATOM,
        &[wm_type_utility],
    )?;

    let wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
    let wm_state_above = conn
        .intern_atom(false, b"_NET_WM_STATE_ABOVE")?
        .reply()?
        .atom;
    let wm_state_sticky = conn
        .intern_atom(false, b"_NET_WM_STATE_STICKY")?
        .reply()?
        .atom;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        wm_state,
        AtomEnum::ATOM,
        &[wm_state_above, wm_state_sticky],
    )?;

    let wm_desktop = conn.intern_atom(false, b"_NET_WM_DESKTOP")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        wm_desktop,
        AtomEnum::CARDINAL,
        &[0xFFFF_FFFF],
    )?;
    let catsor_overlay_atom = conn.intern_atom(false, b"_CATSOR_OVERLAY")?.reply()?.atom;
    conn.change_property32(
        PropMode::REPLACE,
        win,
        catsor_overlay_atom,
        AtomEnum::CARDINAL,
        &[1],
    )?;

    conn.change_property8(
        PropMode::REPLACE,
        win,
        AtomEnum::WM_NAME,
        AtomEnum::STRING,
        b"catsor",
    )?;
    conn.change_property8(
        PropMode::REPLACE,
        win,
        AtomEnum::WM_CLASS,
        AtomEnum::STRING,
        b"catsor\0Catsor\0",
    )?;

    // Initialize XShape + XFixes and make the window 100% click-through & hover-transparent
    // (empty Input shape region so hovering over the cat never changes the cursor status/icon)
    conn.shape_query_version()?.reply()?;
    conn.xfixes_query_version(5, 0)?.reply()?;
    let empty_region = conn.generate_id()?;
    conn.xfixes_create_region(empty_region, &[])?;
    apply_input_passthrough(&conn, win, empty_region)?;

    let gc = conn.generate_id()?;
    conn.create_gc(gc, win, &CreateGCAux::new())?;
    map_passthrough_window(&conn, win, empty_region)?;
    conn.flush()?;
    // Create standalone click-through bowl overlay window used while carrying or walking to the placed bowl
    let mut bowl_dim = bowl_window_dim(prefs.scale);
    let bowl_win = conn.generate_id()?;
    conn.create_window(
        depth,
        bowl_win,
        root,
        win_x,
        win_y,
        bowl_dim,
        bowl_dim,
        0,
        WindowClass::INPUT_OUTPUT,
        visual_id,
        &create_aux,
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        bowl_win,
        wm_type,
        AtomEnum::ATOM,
        &[wm_type_utility],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        bowl_win,
        wm_state,
        AtomEnum::ATOM,
        &[wm_state_above, wm_state_sticky],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        bowl_win,
        wm_desktop,
        AtomEnum::CARDINAL,
        &[0xFFFF_FFFF],
    )?;
    conn.change_property32(
        PropMode::REPLACE,
        bowl_win,
        catsor_overlay_atom,
        AtomEnum::CARDINAL,
        &[1],
    )?;
    apply_input_passthrough(&conn, bowl_win, empty_region)?;
    let mut bowl_mapped = false;
    let mut bowl_grabbed = false;
    let mut bowl_x = win_x;
    let mut bowl_y = win_y;
    let mut bowl_pixmap =
        Pixmap::new(bowl_dim as u32, bowl_dim as u32).ok_or("Failed to allocate bowl Pixmap")?;
    let mut bowl_bgra = vec![0u8; (bowl_dim as usize) * (bowl_dim as usize) * 4];

    let mut cat_hidden = false;
    let mut start_on_login = autostart::is_enabled();
    let mut keep_awake = X11KeepAwake::new();
    let mut tray = if args.no_tray {
        None
    } else {
        let initial_state = TrayState::from_cat(&cat, cat_hidden, start_on_login);
        TrayController::new(&conn, screen_num, depth, visual_id, colormap, initial_state).ok()
    };

    let mut pixmap =
        Pixmap::new(win_dim as u32, win_dim as u32).ok_or("Failed to allocate Pixmap")?;
    let mut clip_mask =
        Mask::new(win_dim as u32, win_dim as u32).ok_or("Failed to allocate Mask")?;
    let mut bgra_buf = vec![0u8; (win_dim as usize) * (win_dim as usize) * 4];

    let frame_duration = Duration::from_secs_f64(1.0 / (args.fps.max(15) as f64));
    let mut last_tick = Instant::now();
    let mut raise_counter = 0u32;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Process pending X11 events and route to tray icon / popup menu
        let mut needs_raise = false;
        let open_menu_win = tray.as_ref().and_then(|t| t.open_menu_window());
        while let Some(event) = conn.poll_for_event()? {
            match &event {
                Event::VisibilityNotify(ev)
                    if ev.window == win && ev.state != Visibility::UNOBSCURED =>
                {
                    needs_raise = true;
                }
                Event::MapNotify(ev) => {
                    if ev.window == win {
                        apply_input_passthrough(&conn, win, empty_region)?;
                        needs_raise = true;
                    } else if open_menu_win != Some(ev.window)
                        && !is_catsor_overlay(&conn, catsor_overlay_atom, ev.window)
                    {
                        needs_raise = true;
                    }
                }
                Event::ConfigureNotify(ev)
                    if ev.window != win
                        && open_menu_win != Some(ev.window)
                        && (ev.above_sibling == win
                            || open_menu_win == Some(ev.above_sibling)
                            || is_catsor_overlay(&conn, catsor_overlay_atom, ev.above_sibling))
                        && !is_catsor_overlay(&conn, catsor_overlay_atom, ev.window) =>
                {
                    needs_raise = true;
                }
                Event::ButtonPress(ev) if bowl_grabbed => {
                    ungrab_pointer_and_keyboard(&conn);
                    bowl_grabbed = false;
                    if ev.detail == 1 {
                        cat.place_bowl(Vec2::new(ev.root_x as f32, ev.root_y as f32));
                    } else {
                        cat.stop_feeding();
                    }
                    continue;
                }
                Event::KeyPress(ev) if bowl_grabbed && ev.detail == 9 => {
                    ungrab_pointer_and_keyboard(&conn);
                    bowl_grabbed = false;
                    cat.stop_feeding();
                    continue;
                }
                _ => {}
            }

            if let Some(tray_ctrl) = &mut tray
                && let Some(action) = tray_ctrl.handle_event(&conn, &event)?
            {
                match normalize_tray_action(action, cat.feeding_preset()) {
                    TrayAction::SetCoat(coat) => {
                        cat.coat = coat;
                        UserPrefs::from_cat(&cat).save();
                    }
                    TrayAction::SetTemperament(temp) => {
                        cat.behaviors.temperament = temp;
                        UserPrefs::from_cat(&cat).save();
                    }
                    TrayAction::ToggleFollowCursor => {
                        cat.behaviors.follow_cursor = !cat.behaviors.follow_cursor;
                        UserPrefs::from_cat(&cat).save();
                    }
                    TrayAction::TogglePlayfulQuirks => {
                        cat.behaviors.playful_quirks = !cat.behaviors.playful_quirks;
                        UserPrefs::from_cat(&cat).save();
                    }
                    TrayAction::StopFeeding => {
                        if bowl_grabbed {
                            ungrab_pointer_and_keyboard(&conn);
                            bowl_grabbed = false;
                        }
                        cat.stop_feeding();
                        keep_awake.set_active(&conn, false);
                    }
                    TrayAction::SelectFeedPreset(preset) => {
                        if cat_hidden {
                            cat_hidden = false;
                            map_passthrough_window(&conn, win, empty_region)?;
                            needs_raise = true;
                        }
                        let cur = conn
                            .query_pointer(root)
                            .ok()
                            .and_then(|c| c.reply().ok())
                            .map(|r| Vec2::new(r.root_x as f32, r.root_y as f32))
                            .unwrap_or(cat.raw_cursor);
                        cat.begin_bowl_placement(preset, cur);
                        if conn
                            .grab_pointer(
                                false,
                                root,
                                EventMask::BUTTON_PRESS
                                    | EventMask::BUTTON_RELEASE
                                    | EventMask::POINTER_MOTION,
                                GrabMode::ASYNC,
                                GrabMode::ASYNC,
                                x11rb::NONE,
                                x11rb::NONE,
                                x11rb::CURRENT_TIME,
                            )
                            .ok()
                            .and_then(|c| c.reply().ok())
                            .is_some()
                        {
                            let _ = conn.grab_keyboard(
                                false,
                                root,
                                x11rb::CURRENT_TIME,
                                GrabMode::ASYNC,
                                GrabMode::ASYNC,
                            );
                            bowl_grabbed = true;
                        } else {
                            cat.place_bowl(cur);
                        }
                    }
                    TrayAction::SetScale(new_scale) => {
                        cat.set_scale(new_scale);
                        UserPrefs::from_cat(&cat).save();
                        let new_dim = cat_window_dim(cat.scale);
                        if new_dim != win_dim {
                            win_dim = new_dim;
                            pixmap = Pixmap::new(win_dim as u32, win_dim as u32)
                                .ok_or("Failed to reallocate Pixmap")?;
                            clip_mask = Mask::new(win_dim as u32, win_dim as u32)
                                .ok_or("Failed to reallocate Mask")?;
                            bgra_buf.resize((win_dim as usize) * (win_dim as usize) * 4, 0);
                            conn.configure_window(
                                win,
                                &ConfigureWindowAux::new()
                                    .width(win_dim as u32)
                                    .height(win_dim as u32),
                            )?;
                            apply_input_passthrough(&conn, win, empty_region)?;
                            needs_raise = true;
                        }
                        let new_bowl_dim = bowl_window_dim(cat.scale);
                        if new_bowl_dim != bowl_dim {
                            bowl_dim = new_bowl_dim;
                            bowl_pixmap = Pixmap::new(bowl_dim as u32, bowl_dim as u32)
                                .ok_or("Failed to reallocate bowl Pixmap")?;
                            bowl_bgra.resize((bowl_dim as usize) * (bowl_dim as usize) * 4, 0);
                            conn.configure_window(
                                bowl_win,
                                &ConfigureWindowAux::new()
                                    .width(bowl_dim as u32)
                                    .height(bowl_dim as u32),
                            )?;
                            apply_input_passthrough(&conn, bowl_win, empty_region)?;
                        }
                    }
                    TrayAction::ToggleHidden => {
                        cat_hidden = !cat_hidden;
                        if cat_hidden {
                            conn.unmap_window(win)?;
                        } else {
                            map_passthrough_window(&conn, win, empty_region)?;
                            needs_raise = true;
                        }
                    }
                    TrayAction::ToggleStartOnLogin => {
                        if let Err(e) = autostart::set_enabled(!start_on_login) {
                            eprintln!("catsor: failed to update start on login: {e}");
                        }
                        start_on_login = autostart::is_enabled();
                    }
                    TrayAction::OpenAbout => match std::process::Command::new("xdg-open")
                        .arg(ABOUT_URL)
                        // Silence the browser's own stderr chatter (e.g. Chrome's zygote warnings)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                    {
                        // Reap the launcher in the background so it doesn't linger as a zombie
                        Ok(mut child) => {
                            std::thread::spawn(move || child.wait());
                        }
                        Err(e) => eprintln!("catsor: failed to open {ABOUT_URL}: {e}"),
                    },
                    TrayAction::Quit => {
                        tray_ctrl.close_menu(&conn)?;
                        return Ok(());
                    }
                }
            }
        }

        if bowl_grabbed && !cat.is_carrying_bowl() {
            ungrab_pointer_and_keyboard(&conn);
            bowl_grabbed = false;
        }

        // Poll mouse cursor position on root window
        if (!cat_hidden || cat.feeding_preset().is_some())
            && let Ok(cookie) = conn.query_pointer(root)
            && let Ok(reply) = cookie.reply()
        {
            let cursor_pos = Vec2::new(reply.root_x as f32, reply.root_y as f32);
            cat.update(cursor_pos, dt);
            keep_awake.tick(&conn, cat.keeps_awake());
        } else {
            keep_awake.set_active(&conn, false);
        }

        raise_counter = raise_counter.wrapping_add(1);

        if let Some(tray_ctrl) = &mut tray {
            if raise_counter.is_multiple_of(120) {
                let _ = tray_ctrl.try_dock(&conn);
            }
            tray_ctrl.sync_state(&conn, TrayState::from_cat(&cat, cat_hidden, start_on_login))?;
        }

        if let Some(detached_pos) = cat.detached_bowl_pos() {
            if !bowl_mapped {
                map_passthrough_window(&conn, bowl_win, empty_region)?;
                bowl_mapped = true;
            }
            render_bowl_overlay(cat.scale, &mut bowl_pixmap.as_mut());
            rgba_to_bgra(bowl_pixmap.data(), &mut bowl_bgra);
            let bx = (detached_pos.x - (bowl_dim as f32) * 0.5).round() as i16;
            let by = (detached_pos.y - (bowl_dim as f32) * 0.5).round() as i16;
            if bx != bowl_x || by != bowl_y || needs_raise {
                bowl_x = bx;
                bowl_y = by;
                conn.configure_window(
                    bowl_win,
                    &ConfigureWindowAux::new()
                        .x(bowl_x as i32)
                        .y(bowl_y as i32)
                        .stack_mode(StackMode::ABOVE),
                )?;
            }
            conn.put_image(
                ImageFormat::Z_PIXMAP,
                bowl_win,
                gc,
                bowl_dim,
                bowl_dim,
                0,
                0,
                0,
                depth,
                &bowl_bgra,
            )?;
        } else if bowl_mapped {
            conn.unmap_window(bowl_win)?;
            bowl_mapped = false;
        }
        if !cat_hidden {
            // Render relative to the continuous window anchor so the cat's torso never shifts
            // inside the pixmap when integer window coordinates round or when the tail sways
            let center = cat.window_center();
            let origin = center - Vec2::splat((win_dim as f32) * 0.5);
            render_cat(&cat, &mut pixmap.as_mut(), &mut clip_mask, origin);

            // Convert premultiplied RGBA8 -> premultiplied BGRA8 for X11 32-bit ZPixmap
            rgba_to_bgra(pixmap.data(), &mut bgra_buf);

            let new_x = origin.x.round() as i16;
            let new_y = origin.y.round() as i16;
            let open_menu_win = tray.as_ref().and_then(|t| t.open_menu_window());
            let stack_cfg = |cfg: ConfigureWindowAux| -> ConfigureWindowAux {
                if let Some(menu_win) = open_menu_win {
                    cfg.sibling(menu_win).stack_mode(StackMode::BELOW)
                } else {
                    cfg.stack_mode(StackMode::ABOVE)
                }
            };

            if new_x != win_x || new_y != win_y {
                win_x = new_x;
                win_y = new_y;
                let mut cfg = ConfigureWindowAux::new().x(win_x as i32).y(win_y as i32);
                if needs_raise {
                    cfg = stack_cfg(cfg);
                }
                conn.configure_window(win, &cfg)?;
            } else if needs_raise {
                conn.configure_window(win, &stack_cfg(ConfigureWindowAux::new()))?;
            }
            conn.put_image(
                ImageFormat::Z_PIXMAP,
                win,
                gc,
                win_dim,
                win_dim,
                0,
                0,
                0,
                depth,
                &bgra_buf,
            )?;
        }
        conn.flush()?;

        let elapsed = now.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
}
