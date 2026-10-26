use crate::Args;
use crate::autostart;
use crate::cat::{CatSim, FeedPreset};
use crate::prefs::UserPrefs;
use crate::render::{bowl_window_dim, cat_window_dim, render_bowl_overlay, render_cat};
use crate::tray::{
    ABOUT_URL, TrayAction, TrayController, TrayState, apply_shared_action, rgba_to_bgra,
};
use glam::Vec2;
use std::time::{Duration, Instant};
use tiny_skia::{Mask, Pixmap};
use x11rb::connection::Connection;
use x11rb::protocol::Event;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClipOrdering, Colormap, ColormapAlloc,
    ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask, Gcontext,
    GrabMode, ImageFormat, PropMode, Screen, ScreenSaver, StackMode, Visibility, VisualClass,
    Visualid, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;

type BoxResult<T> = Result<T, Box<dyn std::error::Error>>;

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

fn apply_input_passthrough<C: Connection>(
    conn: &C,
    win: Window,
    empty_region: xfixes::Region,
) -> BoxResult<()> {
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
fn map_passthrough_window<C: Connection>(
    conn: &C,
    win: Window,
    empty_region: xfixes::Region,
) -> BoxResult<()> {
    conn.map_window(win)?;
    apply_input_passthrough(conn, win, empty_region)
}

fn ungrab_pointer_and_keyboard<C: Connection>(conn: &C) {
    let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
    let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
}

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

fn intern<C: Connection>(conn: &C, name: &[u8]) -> BoxResult<Atom> {
    Ok(conn.intern_atom(false, name)?.reply()?.atom)
}

struct X11KeepAwake {
    active: bool,
    inhibit_child: Option<std::process::Child>,
    last_screen_reset: Instant,
}

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

impl Drop for X11KeepAwake {
    fn drop(&mut self) {
        if let Some(mut child) = self.inhibit_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Screen-wide handles every overlay window is created with
struct OverlayEnv {
    root: Window,
    depth: u8,
    visual_id: Visualid,
    colormap: Colormap,
    /// Empty XFixes region used as the input shape, so clicks and hover pass through
    empty_region: xfixes::Region,
    wm_type: Atom,
    wm_type_utility: Atom,
    wm_state: Atom,
    wm_state_above: Atom,
    wm_state_sticky: Atom,
    wm_desktop: Atom,
    catsor_overlay: Atom,
}

impl OverlayEnv {
    fn new(conn: &RustConnection, screen: &Screen) -> BoxResult<Self> {
        let root = screen.root;
        let (depth, visual_id) = find_argb_visual(screen)
            .ok_or("No 32-bit ARGB TrueColor visual found on X11 screen")?;
        let colormap = conn.generate_id()?;
        conn.create_colormap(ColormapAlloc::NONE, colormap, root, visual_id)?;

        // Initialize XShape + XFixes for the empty input region that makes overlays 100%
        // click-through & hover-transparent (hovering over the cat never changes the cursor)
        conn.shape_query_version()?.reply()?;
        conn.xfixes_query_version(5, 0)?.reply()?;
        let empty_region = conn.generate_id()?;
        conn.xfixes_create_region(empty_region, &[])?;

        Ok(Self {
            root,
            depth,
            visual_id,
            colormap,
            empty_region,
            wm_type: intern(conn, b"_NET_WM_WINDOW_TYPE")?,
            wm_type_utility: intern(conn, b"_NET_WM_WINDOW_TYPE_UTILITY")?,
            wm_state: intern(conn, b"_NET_WM_STATE")?,
            wm_state_above: intern(conn, b"_NET_WM_STATE_ABOVE")?,
            wm_state_sticky: intern(conn, b"_NET_WM_STATE_STICKY")?,
            wm_desktop: intern(conn, b"_NET_WM_DESKTOP")?,
            catsor_overlay: intern(conn, b"_CATSOR_OVERLAY")?,
        })
    }
}

/// An unmapped-at-creation, click-through ARGB overlay window plus the CPU buffers its frames
/// are drawn into
struct OverlayWindow {
    win: Window,
    dim: u16,
    x: i16,
    y: i16,
    pixmap: Pixmap,
    mask: Mask,
    bgra: Vec<u8>,
}

impl OverlayWindow {
    fn create(
        conn: &RustConnection,
        env: &OverlayEnv,
        dim: u16,
        x: i16,
        y: i16,
    ) -> BoxResult<Self> {
        let win = conn.generate_id()?;
        let create_aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .override_redirect(1)
            .event_mask(
                EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY | EventMask::VISIBILITY_CHANGE,
            )
            .colormap(env.colormap);
        conn.create_window(
            env.depth,
            win,
            env.root,
            x,
            y,
            dim,
            dim,
            0,
            WindowClass::INPUT_OUTPUT,
            env.visual_id,
            &create_aux,
        )?;

        // Set window properties so picom suppresses window shadows & fade animations
        conn.change_property32(
            PropMode::REPLACE,
            win,
            env.wm_type,
            AtomEnum::ATOM,
            &[env.wm_type_utility],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            env.wm_state,
            AtomEnum::ATOM,
            &[env.wm_state_above, env.wm_state_sticky],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            env.wm_desktop,
            AtomEnum::CARDINAL,
            &[0xFFFF_FFFF],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            env.catsor_overlay,
            AtomEnum::CARDINAL,
            &[1],
        )?;
        apply_input_passthrough(conn, win, env.empty_region)?;

        Ok(Self {
            win,
            dim,
            x,
            y,
            pixmap: Pixmap::new(dim as u32, dim as u32).ok_or("Failed to allocate Pixmap")?,
            mask: Mask::new(dim as u32, dim as u32).ok_or("Failed to allocate Mask")?,
            bgra: vec![0u8; (dim as usize) * (dim as usize) * 4],
        })
    }

    /// Resizes the window and its buffers to `dim`; returns whether the size changed
    fn resize(&mut self, conn: &RustConnection, env: &OverlayEnv, dim: u16) -> BoxResult<bool> {
        if dim == self.dim {
            return Ok(false);
        }
        self.dim = dim;
        self.pixmap = Pixmap::new(dim as u32, dim as u32).ok_or("Failed to reallocate Pixmap")?;
        self.mask = Mask::new(dim as u32, dim as u32).ok_or("Failed to reallocate Mask")?;
        self.bgra.resize((dim as usize) * (dim as usize) * 4, 0);
        conn.configure_window(
            self.win,
            &ConfigureWindowAux::new()
                .width(dim as u32)
                .height(dim as u32),
        )?;
        apply_input_passthrough(conn, self.win, env.empty_region)?;
        Ok(true)
    }

    /// Converts the drawn pixmap to BGRA and uploads it to the window
    fn present(&mut self, conn: &RustConnection, gc: Gcontext, depth: u8) -> BoxResult<()> {
        // Convert premultiplied RGBA8 -> premultiplied BGRA8 for X11 32-bit ZPixmap
        rgba_to_bgra(self.pixmap.data(), &mut self.bgra);
        conn.put_image(
            ImageFormat::Z_PIXMAP,
            self.win,
            gc,
            self.dim,
            self.dim,
            0,
            0,
            0,
            depth,
            &self.bgra,
        )?;
        Ok(())
    }
}

enum Flow {
    Continue { needs_raise: bool },
    Quit,
}

struct X11App {
    conn: RustConnection,
    env: OverlayEnv,
    gc: Gcontext,
    cat: CatSim,
    cat_win: OverlayWindow,
    /// Standalone bowl window shown while carrying or walking to the placed bowl
    bowl_win: OverlayWindow,
    bowl_mapped: bool,
    /// Pointer and keyboard are grabbed while the user picks where to put the bowl
    bowl_grabbed: bool,
    cat_hidden: bool,
    start_on_login: bool,
    keep_awake: X11KeepAwake,
    tray: Option<TrayController>,
    /// Active monitors as (min, max) corners in root coordinates, refreshed every few seconds
    monitors: Vec<(Vec2, Vec2)>,
    monitors_checked: Instant,
}

/// Each active monitor's (min, max) corners from RandR, or the whole root window when RandR
/// 1.5 monitors aren't available
fn query_monitors(conn: &RustConnection, root: Window) -> Vec<(Vec2, Vec2)> {
    let monitors: Vec<(Vec2, Vec2)> = conn
        .randr_get_monitors(root, true)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|reply| {
            reply
                .monitors
                .iter()
                .filter(|m| m.width > 0 && m.height > 0)
                .map(|m| {
                    let min = Vec2::new(m.x as f32, m.y as f32);
                    (min, min + Vec2::new(m.width as f32, m.height as f32))
                })
                .collect()
        })
        .unwrap_or_default();
    if !monitors.is_empty() {
        return monitors;
    }
    conn.get_geometry(root)
        .ok()
        .and_then(|cookie| cookie.reply().ok())
        .map(|geo| vec![(Vec2::ZERO, Vec2::new(geo.width as f32, geo.height as f32))])
        .unwrap_or_default()
}

/// The monitor containing `p`, or else the one nearest to it
fn monitor_at(monitors: &[(Vec2, Vec2)], p: Vec2) -> Option<(Vec2, Vec2)> {
    monitors.iter().copied().min_by(|a, b| {
        let da = p.distance(p.clamp(a.0, a.1));
        let db = p.distance(p.clamp(b.0, b.1));
        da.total_cmp(&db)
    })
}

pub fn run(args: &Args, prefs: UserPrefs) -> BoxResult<()> {
    let mut app = X11App::new(args, prefs)?;
    let frame_duration = Duration::from_secs_f64(1.0 / (args.fps.max(15) as f64));
    let mut last_tick = Instant::now();
    let mut raise_counter = 0u32;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        let needs_raise = match app.process_events()? {
            Flow::Continue { needs_raise } => needs_raise,
            Flow::Quit => return Ok(()),
        };
        app.release_stale_bowl_grab();
        app.step_cat(dt);

        raise_counter = raise_counter.wrapping_add(1);
        app.sync_tray(raise_counter.is_multiple_of(120))?;

        app.draw_bowl(needs_raise)?;
        if !app.cat_hidden {
            app.draw_cat(needs_raise)?;
        }
        app.conn.flush()?;

        let elapsed = now.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
}

impl X11App {
    fn new(args: &Args, prefs: UserPrefs) -> BoxResult<Self> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let env = OverlayEnv::new(&conn, screen)?;

        let _ = conn.change_window_attributes(
            env.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        );

        // Query initial cursor location so the cat spawns nearby
        let pointer = conn.query_pointer(env.root)?.reply()?;
        let start_cursor = Vec2::new(pointer.root_x as f32, pointer.root_y as f32);
        let mut cat = CatSim::new(start_cursor, prefs.scale, prefs.coat);
        cat.behaviors = prefs.behaviors;
        let _ = conn.randr_query_version(1, 5).map(|cookie| cookie.reply());
        let monitors = query_monitors(&conn, env.root);

        // Compact window bounding box around the cat (~108x108 at scale 1.0)
        let win_dim = cat_window_dim(prefs.scale);
        let init_center = cat.window_center();
        let win_x = (init_center.x - (win_dim as f32) * 0.5).round() as i16;
        let win_y = (init_center.y - (win_dim as f32) * 0.5).round() as i16;
        let cat_win = OverlayWindow::create(&conn, &env, win_dim, win_x, win_y)?;
        conn.change_property8(
            PropMode::REPLACE,
            cat_win.win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"catsor",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            cat_win.win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"catsor\0Catsor\0",
        )?;

        let gc = conn.generate_id()?;
        conn.create_gc(gc, cat_win.win, &CreateGCAux::new())?;
        map_passthrough_window(&conn, cat_win.win, env.empty_region)?;
        conn.flush()?;

        let bowl_win =
            OverlayWindow::create(&conn, &env, bowl_window_dim(prefs.scale), win_x, win_y)?;

        let cat_hidden = false;
        let start_on_login = autostart::is_enabled();
        let tray = if args.no_tray {
            None
        } else {
            let initial_state = TrayState::from_cat(&cat, cat_hidden, start_on_login);
            TrayController::new(
                &conn,
                screen_num,
                env.depth,
                env.visual_id,
                env.colormap,
                initial_state,
            )
            .ok()
        };

        Ok(Self {
            conn,
            env,
            gc,
            cat,
            cat_win,
            bowl_win,
            bowl_mapped: false,
            bowl_grabbed: false,
            cat_hidden,
            start_on_login,
            keep_awake: X11KeepAwake::new(),
            tray,
            monitors,
            monitors_checked: Instant::now(),
        })
    }

    /// Processes pending X11 events, routing them to bowl placement and the tray icon / popup
    /// menu, and notes whether another window stacked above the cat.
    fn process_events(&mut self) -> BoxResult<Flow> {
        let mut needs_raise = false;
        let win = self.cat_win.win;
        let overlay_atom = self.env.catsor_overlay;
        let open_menu_win = self.tray.as_ref().and_then(|t| t.open_menu_window());
        while let Some(event) = self.conn.poll_for_event()? {
            let conn = &self.conn;
            match &event {
                Event::VisibilityNotify(ev)
                    if ev.window == win && ev.state != Visibility::UNOBSCURED =>
                {
                    needs_raise = true;
                }
                Event::MapNotify(ev) => {
                    if ev.window == win {
                        apply_input_passthrough(conn, win, self.env.empty_region)?;
                        needs_raise = true;
                    } else if open_menu_win != Some(ev.window)
                        && !is_catsor_overlay(conn, overlay_atom, ev.window)
                    {
                        needs_raise = true;
                    }
                }
                Event::ConfigureNotify(ev)
                    if ev.window != win
                        && open_menu_win != Some(ev.window)
                        && (ev.above_sibling == win
                            || open_menu_win == Some(ev.above_sibling)
                            || is_catsor_overlay(conn, overlay_atom, ev.above_sibling))
                        && !is_catsor_overlay(conn, overlay_atom, ev.window) =>
                {
                    needs_raise = true;
                }
                Event::ButtonPress(ev) if self.bowl_grabbed => {
                    self.release_bowl_grab();
                    if ev.detail == 1 {
                        self.cat
                            .place_bowl(Vec2::new(ev.root_x as f32, ev.root_y as f32));
                    } else {
                        self.cat.stop_feeding();
                    }
                    continue;
                }
                // Escape cancels bowl placement
                Event::KeyPress(ev) if self.bowl_grabbed && ev.detail == 9 => {
                    self.release_bowl_grab();
                    self.cat.stop_feeding();
                    continue;
                }
                _ => {}
            }

            let action = match &mut self.tray {
                Some(tray_ctrl) => tray_ctrl.handle_event(&self.conn, &event)?,
                None => None,
            };
            if let Some(action) = action {
                match self.handle_tray_action(action)? {
                    Flow::Continue { needs_raise: raise } => needs_raise |= raise,
                    Flow::Quit => return Ok(Flow::Quit),
                }
            }
        }
        Ok(Flow::Continue { needs_raise })
    }

    fn handle_tray_action(&mut self, action: TrayAction) -> BoxResult<Flow> {
        let mut needs_raise = false;
        let Some(action) = apply_shared_action(action, &mut self.cat, &mut self.start_on_login)
        else {
            return Ok(Flow::Continue { needs_raise });
        };
        match action {
            TrayAction::StopFeeding => {
                if self.bowl_grabbed {
                    self.release_bowl_grab();
                }
                self.cat.stop_feeding();
                self.keep_awake.set_active(&self.conn, false);
            }
            TrayAction::SelectFeedPreset(preset) => {
                if self.cat_hidden {
                    self.cat_hidden = false;
                    map_passthrough_window(&self.conn, self.cat_win.win, self.env.empty_region)?;
                    needs_raise = true;
                }
                self.begin_bowl_placement(preset);
            }
            TrayAction::SetScale(new_scale) => {
                self.cat.set_scale(new_scale);
                UserPrefs::from_cat(&self.cat).save();
                if self
                    .cat_win
                    .resize(&self.conn, &self.env, cat_window_dim(self.cat.scale))?
                {
                    needs_raise = true;
                }
                self.bowl_win
                    .resize(&self.conn, &self.env, bowl_window_dim(self.cat.scale))?;
            }
            TrayAction::ToggleHidden => {
                self.cat_hidden = !self.cat_hidden;
                if self.cat_hidden {
                    self.conn.unmap_window(self.cat_win.win)?;
                } else {
                    map_passthrough_window(&self.conn, self.cat_win.win, self.env.empty_region)?;
                    needs_raise = true;
                }
            }
            TrayAction::OpenAbout => open_about_page(),
            TrayAction::Quit => {
                if let Some(tray_ctrl) = &mut self.tray {
                    tray_ctrl.close_menu(&self.conn)?;
                }
                return Ok(Flow::Quit);
            }
            // Handled by `apply_shared_action`
            TrayAction::SetCoat(_)
            | TrayAction::SetTemperament(_)
            | TrayAction::ToggleFollowCursor
            | TrayAction::TogglePlayfulQuirks
            | TrayAction::ToggleStartOnLogin => {}
        }
        Ok(Flow::Continue { needs_raise })
    }

    /// Starts carrying the bowl under the cursor and grabs the pointer and keyboard, so the next
    /// click places it (Escape or another button cancels). Without a grab the bowl is placed
    /// right away.
    fn begin_bowl_placement(&mut self, preset: FeedPreset) {
        let root = self.env.root;
        let cur = self
            .conn
            .query_pointer(root)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| Vec2::new(r.root_x as f32, r.root_y as f32))
            .unwrap_or(self.cat.raw_cursor);
        self.cat.begin_bowl_placement(preset, cur);
        if self
            .conn
            .grab_pointer(
                false,
                root,
                EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
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
            let _ = self.conn.grab_keyboard(
                false,
                root,
                x11rb::CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            );
            self.bowl_grabbed = true;
        } else {
            self.cat.place_bowl(cur);
        }
    }

    fn release_bowl_grab(&mut self) {
        ungrab_pointer_and_keyboard(&self.conn);
        self.bowl_grabbed = false;
    }

    /// Drops the bowl grab once feeding ended some other way (e.g. the tray menu)
    fn release_stale_bowl_grab(&mut self) {
        if self.bowl_grabbed && !self.cat.is_carrying_bowl() {
            self.release_bowl_grab();
        }
    }

    /// Advances the simulation from the root-window cursor. A hidden cat stays paused unless
    /// it is feeding.
    fn step_cat(&mut self, dt: f32) {
        self.update_roam_bounds();
        if (!self.cat_hidden || self.cat.feeding_preset().is_some())
            && let Ok(cookie) = self.conn.query_pointer(self.env.root)
            && let Ok(reply) = cookie.reply()
        {
            let cursor_pos = Vec2::new(reply.root_x as f32, reply.root_y as f32);
            self.cat.update(cursor_pos, dt);
            self.keep_awake.tick(&self.conn, self.cat.keeps_awake());
        } else {
            self.keep_awake.set_active(&self.conn, false);
        }
    }

    /// Keeps a roaming cat on the monitor it is on. The monitor layout is re-read every few
    /// seconds so plugging, unplugging or rearranging screens is picked up.
    fn update_roam_bounds(&mut self) {
        if self.monitors_checked.elapsed() >= Duration::from_secs(2) {
            self.monitors_checked = Instant::now();
            self.monitors = query_monitors(&self.conn, self.env.root);
        }
        self.cat.roam_bounds = monitor_at(&self.monitors, self.cat.window_center());
    }

    fn sync_tray(&mut self, retry_dock: bool) -> BoxResult<()> {
        if let Some(tray_ctrl) = &mut self.tray {
            if retry_dock {
                let _ = tray_ctrl.try_dock(&self.conn);
            }
            tray_ctrl.sync_state(
                &self.conn,
                TrayState::from_cat(&self.cat, self.cat_hidden, self.start_on_login),
            )?;
        }
        Ok(())
    }

    /// Shows the bowl window at the detached bowl while there is one, and hides it otherwise
    fn draw_bowl(&mut self, needs_raise: bool) -> BoxResult<()> {
        let Some(detached_pos) = self.cat.detached_bowl_pos() else {
            if self.bowl_mapped {
                self.conn.unmap_window(self.bowl_win.win)?;
                self.bowl_mapped = false;
            }
            return Ok(());
        };
        if !self.bowl_mapped {
            map_passthrough_window(&self.conn, self.bowl_win.win, self.env.empty_region)?;
            self.bowl_mapped = true;
        }
        let bowl = &mut self.bowl_win;
        render_bowl_overlay(self.cat.scale, &mut bowl.pixmap.as_mut());
        let bx = (detached_pos.x - (bowl.dim as f32) * 0.5).round() as i16;
        let by = (detached_pos.y - (bowl.dim as f32) * 0.5).round() as i16;
        if bx != bowl.x || by != bowl.y || needs_raise {
            bowl.x = bx;
            bowl.y = by;
            self.conn.configure_window(
                bowl.win,
                &ConfigureWindowAux::new()
                    .x(bowl.x as i32)
                    .y(bowl.y as i32)
                    .stack_mode(StackMode::ABOVE),
            )?;
        }
        bowl.present(&self.conn, self.gc, self.env.depth)
    }

    /// Renders the cat, moves its window to follow it and re-raises it when needed (kept
    /// just below an open tray menu)
    fn draw_cat(&mut self, needs_raise: bool) -> BoxResult<()> {
        let cat_win = &mut self.cat_win;
        // Render relative to the continuous window anchor so the cat's torso never shifts
        // inside the pixmap when integer window coordinates round or when the tail sways
        let center = self.cat.window_center();
        let origin = center - Vec2::splat((cat_win.dim as f32) * 0.5);
        render_cat(
            &self.cat,
            &mut cat_win.pixmap.as_mut(),
            &mut cat_win.mask,
            origin,
        );

        let new_x = origin.x.round() as i16;
        let new_y = origin.y.round() as i16;
        let open_menu_win = self.tray.as_ref().and_then(|t| t.open_menu_window());
        let stack_cfg = |cfg: ConfigureWindowAux| -> ConfigureWindowAux {
            if let Some(menu_win) = open_menu_win {
                cfg.sibling(menu_win).stack_mode(StackMode::BELOW)
            } else {
                cfg.stack_mode(StackMode::ABOVE)
            }
        };

        if new_x != cat_win.x || new_y != cat_win.y {
            cat_win.x = new_x;
            cat_win.y = new_y;
            let mut cfg = ConfigureWindowAux::new()
                .x(cat_win.x as i32)
                .y(cat_win.y as i32);
            if needs_raise {
                cfg = stack_cfg(cfg);
            }
            self.conn.configure_window(cat_win.win, &cfg)?;
        } else if needs_raise {
            self.conn
                .configure_window(cat_win.win, &stack_cfg(ConfigureWindowAux::new()))?;
        }
        cat_win.present(&self.conn, self.gc, self.env.depth)
    }
}

fn open_about_page() {
    match std::process::Command::new("xdg-open")
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
    }
}
