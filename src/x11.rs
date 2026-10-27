use crate::Args;
use crate::autostart;
use crate::bonds::Bonds;
use crate::cat::FeedPreset;
use crate::cat::clowder::{Clowder, unix_now};
use crate::coat::{Coat, CoatLibrary};
use crate::designer::{DESIGNER_H, DESIGNER_TITLE, DESIGNER_W, Designer, DesignerInput};
use crate::prefs::UserPrefs;
use crate::render::{bowl_window_dim, cat_window_dim, render_bowl_overlay, render_cats};
use crate::tray::{
    ABOUT_URL, TextKey, TrayAction, TrayController, TrayState, apply_shared_action, rgba_to_bgra,
};
use glam::Vec2;
use std::time::{Duration, Instant};
use tiny_skia::{Mask, Pixmap};
use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::Event;
use x11rb::protocol::randr::ConnectionExt as _;
use x11rb::protocol::shape::{self, ConnectionExt as _};
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ChangeWindowAttributesAux, ClientMessageEvent, ClipOrdering, Colormap,
    ColormapAlloc, ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux, EventMask,
    Gcontext, GrabMode, ImageFormat, KeyButMask, Keysym, PropMode, Screen, ScreenSaver, StackMode,
    Visibility, VisualClass, Visualid, Window, WindowClass,
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
    /// The root window's own (opaque) visual, for the ordinary designer window
    root_depth: u8,
    root_visual: Visualid,
    screen_w: u16,
    screen_h: u16,
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
            root_depth: screen.root_depth,
            root_visual: screen.root_visual,
            screen_w: screen.width_in_pixels,
            screen_h: screen.height_in_pixels,
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

/// The coat designer: an ordinary window the window manager decorates, focuses and lets the
/// user move (unlike the overlays). Contents, input handling and saving live in `designer.rs`
struct DesignerWindow {
    win: Window,
    gc: Gcontext,
    depth: u8,
    w: u16,
    h: u16,
    wm_protocols: Atom,
    wm_delete: Atom,
    net_active_window: Atom,
    keymap: Keymap,
    designer: Designer,
    /// Reused frame buffers, reallocated when the window is resized
    frame: Pixmap,
    bgra: Vec<u8>,
}

impl DesignerWindow {
    fn open(
        conn: &RustConnection,
        env: &OverlayEnv,
        current: &Coat,
        coats: &CoatLibrary,
    ) -> BoxResult<Self> {
        let (w, h) = (DESIGNER_W as u16, DESIGNER_H as u16);
        let x = (env.screen_w.saturating_sub(w) / 2) as i16;
        let y = (env.screen_h.saturating_sub(h) / 2) as i16;
        let win = conn.generate_id()?;
        conn.create_window(
            env.root_depth,
            win,
            env.root,
            x,
            y,
            w,
            h,
            0,
            WindowClass::INPUT_OUTPUT,
            env.root_visual,
            &CreateWindowAux::new()
                .background_pixel(0x0023_2530)
                .event_mask(
                    EventMask::EXPOSURE
                        | EventMask::BUTTON_PRESS
                        | EventMask::BUTTON_RELEASE
                        | EventMask::POINTER_MOTION
                        | EventMask::LEAVE_WINDOW
                        | EventMask::KEY_PRESS
                        | EventMask::STRUCTURE_NOTIFY,
                ),
        )?;

        let utf8 = intern(conn, b"UTF8_STRING")?;
        let wm_protocols = intern(conn, b"WM_PROTOCOLS")?;
        let wm_delete = intern(conn, b"WM_DELETE_WINDOW")?;
        let wm_type_dialog = intern(conn, b"_NET_WM_WINDOW_TYPE_DIALOG")?;
        let net_active_window = intern(conn, b"_NET_ACTIVE_WINDOW")?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            DESIGNER_TITLE.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            intern(conn, b"_NET_WM_NAME")?,
            utf8,
            DESIGNER_TITLE.as_bytes(),
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"catsor-designer\0Catsor\0",
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            wm_protocols,
            AtomEnum::ATOM,
            &[wm_delete],
        )?;
        // A fixed-size dialog. Being transient also makes tiling window managers such as
        // xmonad float it instead of tiling it
        conn.change_property32(
            PropMode::REPLACE,
            win,
            env.wm_type,
            AtomEnum::ATOM,
            &[wm_type_dialog],
        )?;
        conn.change_property32(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_TRANSIENT_FOR,
            AtomEnum::WINDOW,
            &[env.root],
        )?;
        let (w32, h32) = (w as u32, h as u32);
        // ICCCM WM_SIZE_HINTS: PPosition | PMinSize | PMaxSize
        let hints: [u32; 18] = [
            (1 << 2) | (1 << 4) | (1 << 5),
            x as u32,
            y as u32,
            w32,
            h32,
            w32,
            h32,
            w32,
            h32,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        conn.change_property32(
            PropMode::REPLACE,
            win,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &hints,
        )?;

        let gc = conn.generate_id()?;
        conn.create_gc(gc, win, &CreateGCAux::new())?;

        let keymap = Keymap::load(conn)?;

        conn.map_window(win)?;
        conn.flush()?;
        Ok(Self {
            win,
            gc,
            depth: env.root_depth,
            w,
            h,
            wm_protocols,
            wm_delete,
            net_active_window,
            keymap,
            designer: Designer::new(current, coats),
            frame: Pixmap::new(w as u32, h as u32).ok_or("Failed to allocate Pixmap")?,
            bgra: Vec::new(),
        })
    }

    /// Asks the window manager to raise and focus the already open designer
    fn activate(&self, conn: &RustConnection, root: Window) -> BoxResult<()> {
        conn.map_window(self.win)?;
        conn.configure_window(
            self.win,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        let ev = ClientMessageEvent::new(32, self.win, self.net_active_window, [1, 0, 0, 0, 0]);
        conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        )?;
        Ok(())
    }

    /// Turns an event on the designer window into designer input. `None` means the event
    /// isn't for this window; `Some(true)` means the designer asked to close
    fn handle_event(
        &mut self,
        event: &Event,
        coats: &mut CoatLibrary,
        cat: &mut crate::cat::CatSim,
    ) -> Option<bool> {
        let input = match event {
            Event::Expose(ev) if ev.window == self.win => {
                self.designer.invalidate();
                return Some(false);
            }
            Event::ConfigureNotify(ev) if ev.window == self.win => {
                if (ev.width, ev.height) != (self.w, self.h)
                    && let Some(frame) =
                        Pixmap::new(ev.width.max(1) as u32, ev.height.max(1) as u32)
                {
                    self.frame = frame;
                }
                self.w = ev.width;
                self.h = ev.height;
                self.designer
                    .set_window_size(ev.width as u32, ev.height as u32);
                return Some(false);
            }
            Event::ClientMessage(ev) if ev.window == self.win => {
                if ev.type_ == self.wm_protocols && ev.data.as_data32()[0] == self.wm_delete {
                    DesignerInput::Close
                } else {
                    return Some(false);
                }
            }
            Event::ButtonPress(ev) if ev.event == self.win => {
                if ev.detail != 1 {
                    return Some(false);
                }
                DesignerInput::Press(ev.event_x as f32, ev.event_y as f32)
            }
            Event::ButtonRelease(ev) if ev.event == self.win => {
                if ev.detail != 1 {
                    return Some(false);
                }
                DesignerInput::Release
            }
            Event::MotionNotify(ev) if ev.event == self.win => {
                DesignerInput::Move(ev.event_x as f32, ev.event_y as f32)
            }
            Event::LeaveNotify(ev) if ev.event == self.win => DesignerInput::Leave,
            Event::KeyPress(ev) if ev.event == self.win => {
                match self.keymap.text_key(ev.detail, ev.state) {
                    Some(TextKey::Char(c)) => DesignerInput::Char(c),
                    Some(TextKey::Backspace) => DesignerInput::Backspace,
                    Some(TextKey::Enter) => DesignerInput::Enter,
                    Some(TextKey::Escape) => DesignerInput::Escape,
                    None => return Some(false),
                }
            }
            Event::DestroyNotify(ev) if ev.window == self.win => return Some(true),
            _ => return None,
        };
        Some(self.designer.handle(input, coats, cat))
    }

    /// Redraws and uploads the contents if they changed
    fn present(&mut self, conn: &RustConnection) -> BoxResult<()> {
        if !self.designer.take_dirty() {
            return Ok(());
        }
        let w = self.frame.width() as u16;
        self.designer.render(&mut self.frame.as_mut());
        self.bgra.resize(self.frame.data().len(), 0);
        rgba_to_bgra(self.frame.data(), &mut self.bgra);
        // Upload in horizontal strips so each request stays under the server's size limit
        let row_bytes = w as usize * 4;
        let rows_per_req = ((conn.maximum_request_bytes().saturating_sub(64)) / row_bytes).max(1);
        for (i, strip) in self.bgra.chunks(rows_per_req * row_bytes).enumerate() {
            conn.put_image(
                ImageFormat::Z_PIXMAP,
                self.win,
                self.gc,
                w,
                (strip.len() / row_bytes) as u16,
                0,
                (i * rows_per_req) as i16,
                0,
                self.depth,
                strip,
            )?;
        }
        Ok(())
    }

    fn close(self, conn: &RustConnection) {
        let _ = conn.free_gc(self.gc);
        let _ = conn.destroy_window(self.win);
        let _ = conn.flush();
    }
}

enum Flow {
    Continue { needs_raise: bool },
    Quit,
}

/// A cat's overlay window. Windows outlive removed cats (unmapped) and are reused when cats
/// are added again
struct CatWindow {
    overlay: OverlayWindow,
    mapped: bool,
}

struct X11App {
    conn: RustConnection,
    env: OverlayEnv,
    gc: Gcontext,
    clowder: Clowder,
    /// Custom coats from `coats.toml`, reloaded when the file changes
    coats: CoatLibrary,
    designer: Option<DesignerWindow>,
    /// Which cat the open designer puts its coat on
    designer_cat: usize,
    /// One window per cat, in the clowder's order (plus spare unmapped ones)
    cat_wins: Vec<CatWindow>,
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

pub fn run(args: &Args, prefs: UserPrefs, coats: CoatLibrary) -> BoxResult<()> {
    let mut app = X11App::new(args, prefs, coats)?;
    if args.design_coat {
        app.handle_tray_action(TrayAction::OpenDesigner(0))?;
    }
    let frame_duration = Duration::from_secs_f64(1.0 / (args.fps.max(15) as f64));
    let mut last_tick = Instant::now();
    let mut raise_counter = 0u32;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        let needs_raise = match app.process_events()? {
            Flow::Continue { needs_raise } => needs_raise,
            Flow::Quit => {
                app.clowder.save_bonds();
                return Ok(());
            }
        };
        app.release_stale_bowl_grab();
        app.step_cat(dt);

        raise_counter = raise_counter.wrapping_add(1);
        app.sync_tray(raise_counter.is_multiple_of(120))?;

        app.draw_bowl(needs_raise)?;
        app.sync_cat_windows()?;
        if !app.cat_hidden {
            app.draw_cats(needs_raise)?;
        }
        if let Some(dw) = &mut app.designer
            && let Err(e) = dw.present(&app.conn)
        {
            eprintln!("catsor: closing the coat designer, drawing it failed: {e}");
            if let Some(dw) = app.designer.take() {
                dw.close(&app.conn);
            }
        }
        app.conn.flush()?;

        let elapsed = now.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
}

impl X11App {
    fn new(args: &Args, prefs: UserPrefs, coats: CoatLibrary) -> BoxResult<Self> {
        let (conn, screen_num) = x11rb::connect(None)?;
        let screen = &conn.setup().roots[screen_num];
        let env = OverlayEnv::new(&conn, screen)?;

        let _ = conn.change_window_attributes(
            env.root,
            &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
        );

        // Query initial cursor location so the cats spawn nearby
        let pointer = conn.query_pointer(env.root)?.reply()?;
        let start_cursor = Vec2::new(pointer.root_x as f32, pointer.root_y as f32);
        let clowder = Clowder::new(
            &prefs.cats,
            prefs.scale,
            prefs.follow_cursor,
            prefs.playful_quirks,
            &coats,
            start_cursor,
        )
        .with_bonds(Bonds::load());
        let _ = conn.randr_query_version(1, 5).map(|cookie| cookie.reply());
        let monitors = query_monitors(&conn, env.root);

        let bowl_win = OverlayWindow::create(
            &conn,
            &env,
            bowl_window_dim(prefs.scale),
            start_cursor.x as i16,
            start_cursor.y as i16,
        )?;
        // Every overlay shares one graphics context, made on an overlay for their 32-bit depth
        let gc = conn.generate_id()?;
        conn.create_gc(gc, bowl_win.win, &CreateGCAux::new())?;

        let cat_hidden = false;
        let start_on_login = autostart::is_enabled();
        let tray = if args.no_tray {
            None
        } else {
            let initial_state = TrayState::from_clowder(&clowder, cat_hidden, start_on_login);
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

        let mut app = Self {
            conn,
            env,
            gc,
            clowder,
            coats,
            designer: None,
            designer_cat: 0,
            cat_wins: Vec::new(),
            bowl_win,
            bowl_mapped: false,
            bowl_grabbed: false,
            cat_hidden,
            start_on_login,
            keep_awake: X11KeepAwake::new(),
            tray,
            monitors,
            monitors_checked: Instant::now(),
        };
        app.sync_cat_windows()?;
        app.conn.flush()?;
        Ok(app)
    }

    /// Gives every cat a mapped window of the right size (creating windows as cats are added)
    /// and unmaps the windows of removed cats, or all of them while the cats are hidden
    fn sync_cat_windows(&mut self) -> BoxResult<()> {
        while self.cat_wins.len() < self.clowder.len() {
            let cat = self.clowder.cat(self.cat_wins.len());
            let dim = cat_window_dim(cat.scale);
            let center = cat.window_center();
            let x = (center.x - (dim as f32) * 0.5).round() as i16;
            let y = (center.y - (dim as f32) * 0.5).round() as i16;
            let overlay = OverlayWindow::create(&self.conn, &self.env, dim, x, y)?;
            self.conn.change_property8(
                PropMode::REPLACE,
                overlay.win,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"catsor",
            )?;
            self.conn.change_property8(
                PropMode::REPLACE,
                overlay.win,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"catsor\0Catsor\0",
            )?;
            self.cat_wins.push(CatWindow {
                overlay,
                mapped: false,
            });
        }
        for (i, cw) in self.cat_wins.iter_mut().enumerate() {
            let wanted = !self.cat_hidden && i < self.clowder.len();
            if wanted {
                cw.overlay.resize(
                    &self.conn,
                    &self.env,
                    cat_window_dim(self.clowder.cat(i).scale),
                )?;
            }
            if wanted && !cw.mapped {
                map_passthrough_window(&self.conn, cw.overlay.win, self.env.empty_region)?;
            } else if !wanted && cw.mapped {
                self.conn.unmap_window(cw.overlay.win)?;
            }
            cw.mapped = wanted;
        }
        Ok(())
    }

    fn is_cat_window(&self, win: Window) -> bool {
        self.cat_wins.iter().any(|cw| cw.overlay.win == win)
    }

    /// Processes pending X11 events, routing them to bowl placement and the tray icon / popup
    /// menu, and notes whether another window stacked above the cat.
    fn process_events(&mut self) -> BoxResult<Flow> {
        let mut needs_raise = false;
        let overlay_atom = self.env.catsor_overlay;
        let open_menu_win = self.tray.as_ref().and_then(|t| t.open_menu_window());
        while let Some(event) = self.conn.poll_for_event()? {
            if let Some(dw) = &mut self.designer {
                let cat = self
                    .clowder
                    .cat_mut(self.designer_cat.min(self.clowder.len() - 1));
                let coat_before = cat.coat.id.clone();
                let handled = dw.handle_event(&event, &mut self.coats, cat);
                if cat.coat.id != coat_before {
                    UserPrefs::from_clowder(&self.clowder).save();
                }
                if let Some(close) = handled {
                    if close && let Some(dw) = self.designer.take() {
                        dw.close(&self.conn);
                    }
                    continue;
                }
            }
            let conn = &self.conn;
            match &event {
                Event::VisibilityNotify(ev)
                    if self.is_cat_window(ev.window) && ev.state != Visibility::UNOBSCURED =>
                {
                    needs_raise = true;
                }
                Event::MapNotify(ev) => {
                    if self.is_cat_window(ev.window) {
                        apply_input_passthrough(conn, ev.window, self.env.empty_region)?;
                        needs_raise = true;
                    } else if open_menu_win != Some(ev.window)
                        && !is_catsor_overlay(conn, overlay_atom, ev.window)
                    {
                        needs_raise = true;
                    }
                }
                Event::ConfigureNotify(ev)
                    if !self.is_cat_window(ev.window)
                        && open_menu_win != Some(ev.window)
                        && (self.is_cat_window(ev.above_sibling)
                            || open_menu_win == Some(ev.above_sibling)
                            || is_catsor_overlay(conn, overlay_atom, ev.above_sibling))
                        && !is_catsor_overlay(conn, overlay_atom, ev.window) =>
                {
                    needs_raise = true;
                }
                Event::ButtonPress(ev) if self.bowl_grabbed => {
                    self.release_bowl_grab();
                    if ev.detail == 1 {
                        self.clowder
                            .place_bowl(Vec2::new(ev.root_x as f32, ev.root_y as f32));
                    } else {
                        self.clowder.stop_feeding();
                    }
                    continue;
                }
                // Escape cancels bowl placement
                Event::KeyPress(ev) if self.bowl_grabbed && ev.detail == 9 => {
                    self.release_bowl_grab();
                    self.clowder.stop_feeding();
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
        // The designer follows its cat down the list, and closes if its cat goes
        if let TrayAction::RemoveCat(i) = action
            && self.clowder.len() > 1
        {
            if i == self.designer_cat
                && let Some(dw) = self.designer.take()
            {
                dw.close(&self.conn);
            } else if i < self.designer_cat {
                self.designer_cat -= 1;
            }
        }
        let Some(action) = apply_shared_action(
            action,
            &mut self.clowder,
            &self.coats,
            &mut self.start_on_login,
        ) else {
            return Ok(Flow::Continue { needs_raise });
        };
        match action {
            TrayAction::StopFeeding => {
                if self.bowl_grabbed {
                    self.release_bowl_grab();
                }
                self.clowder.stop_feeding();
                self.keep_awake.set_active(&self.conn, false);
            }
            TrayAction::SelectFeedPreset(preset) => {
                if self.cat_hidden {
                    self.cat_hidden = false;
                    needs_raise = true;
                }
                self.begin_bowl_placement(preset);
            }
            TrayAction::ToggleHidden => {
                self.cat_hidden = !self.cat_hidden;
                needs_raise = !self.cat_hidden;
            }
            TrayAction::OpenAbout => open_about_page(),
            // The designer is optional: if its window fails, say so and keep the cats running.
            // An open designer switches to the cat it was asked for
            TrayAction::OpenDesigner(i) if i < self.clowder.len() => {
                if self.designer_cat != i
                    && let Some(dw) = self.designer.take()
                {
                    dw.close(&self.conn);
                }
                self.designer_cat = i;
                match &self.designer {
                    Some(dw) => {
                        if let Err(e) = dw.activate(&self.conn, self.env.root) {
                            eprintln!("catsor: failed to raise the coat designer: {e}");
                        }
                    }
                    None => match DesignerWindow::open(
                        &self.conn,
                        &self.env,
                        &self.clowder.cat(i).coat,
                        &self.coats,
                    ) {
                        Ok(dw) => self.designer = Some(dw),
                        Err(e) => eprintln!("catsor: failed to open the coat designer: {e}"),
                    },
                }
            }
            TrayAction::Quit => {
                if let Some(dw) = self.designer.take() {
                    dw.close(&self.conn);
                }
                if let Some(tray_ctrl) = &mut self.tray {
                    tray_ctrl.close_menu(&self.conn)?;
                }
                return Ok(Flow::Quit);
            }
            // Handled by `apply_shared_action`, or a stale cat index
            TrayAction::OpenCat(_)
            | TrayAction::ShowMain
            | TrayAction::StartRename(_)
            | TrayAction::RenameCat(..)
            | TrayAction::NextCoat(_)
            | TrayAction::RandomCoat(_)
            | TrayAction::OpenDesigner(_)
            | TrayAction::SetTemperament(..)
            | TrayAction::AddCat
            | TrayAction::AddKitten(_)
            | TrayAction::ToggleGrowsUp(_)
            | TrayAction::RemoveCat(_)
            | TrayAction::SetScale(_)
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
            .unwrap_or(self.clowder.cat(0).raw_cursor);
        self.clowder.begin_bowl_placement(preset, cur);
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
            self.clowder.place_bowl(cur);
        }
    }

    fn release_bowl_grab(&mut self) {
        ungrab_pointer_and_keyboard(&self.conn);
        self.bowl_grabbed = false;
    }

    /// Drops the bowl grab once feeding ended some other way (e.g. the tray menu)
    fn release_stale_bowl_grab(&mut self) {
        if self.bowl_grabbed && !self.clowder.is_carrying_bowl() {
            self.release_bowl_grab();
        }
    }

    /// Advances the simulation from the root-window cursor. Hidden cats stay paused unless
    /// they are feeding.
    fn step_cat(&mut self, dt: f32) {
        self.coats.refresh(self.clowder.cats_mut());
        self.update_roam_bounds();
        if (!self.cat_hidden || self.clowder.feeding_preset().is_some())
            && let Ok(cookie) = self.conn.query_pointer(self.env.root)
            && let Ok(reply) = cookie.reply()
        {
            let cursor_pos = Vec2::new(reply.root_x as f32, reply.root_y as f32);
            self.clowder.update(cursor_pos, dt);
            self.clowder.save_bonds_if_due();
            if self.clowder.grow(unix_now()) {
                UserPrefs::from_clowder(&self.clowder).save();
            }
            self.keep_awake.tick(&self.conn, self.clowder.keeps_awake());
        } else {
            self.keep_awake.set_active(&self.conn, false);
        }
    }

    /// Keeps each roaming cat on the monitor it is on. The monitor layout is re-read every few
    /// seconds so plugging, unplugging or rearranging screens is picked up.
    fn update_roam_bounds(&mut self) {
        if self.monitors_checked.elapsed() >= Duration::from_secs(2) {
            self.monitors_checked = Instant::now();
            self.monitors = query_monitors(&self.conn, self.env.root);
        }
        for cat in self.clowder.cats_mut() {
            cat.roam_bounds = monitor_at(&self.monitors, cat.window_center());
        }
    }

    fn sync_tray(&mut self, retry_dock: bool) -> BoxResult<()> {
        if let Some(tray_ctrl) = &mut self.tray {
            if retry_dock {
                let _ = tray_ctrl.try_dock(&self.conn);
            }
            tray_ctrl.sync_state(
                &self.conn,
                TrayState::from_clowder(&self.clowder, self.cat_hidden, self.start_on_login),
            )?;
        }
        Ok(())
    }

    /// Shows the bowl window at the detached bowl while there is one, and hides it otherwise
    fn draw_bowl(&mut self, needs_raise: bool) -> BoxResult<()> {
        let Some(detached_pos) = self.clowder.detached_bowl_pos() else {
            if self.bowl_mapped {
                self.conn.unmap_window(self.bowl_win.win)?;
                self.bowl_mapped = false;
            }
            return Ok(());
        };
        let bowl_scale = self.clowder.bowl_scale();
        self.bowl_win
            .resize(&self.conn, &self.env, bowl_window_dim(bowl_scale))?;
        if !self.bowl_mapped {
            map_passthrough_window(&self.conn, self.bowl_win.win, self.env.empty_region)?;
            self.bowl_mapped = true;
        }
        let bowl = &mut self.bowl_win;
        render_bowl_overlay(bowl_scale, &mut bowl.pixmap.as_mut());
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

    /// Renders each cat, moves its window to follow it and re-raises it when needed (kept
    /// just below an open tray menu)
    fn draw_cats(&mut self, needs_raise: bool) -> BoxResult<()> {
        let open_menu_win = self.tray.as_ref().and_then(|t| t.open_menu_window());
        let stack_cfg = |cfg: ConfigureWindowAux| -> ConfigureWindowAux {
            if let Some(menu_win) = open_menu_win {
                cfg.sibling(menu_win).stack_mode(StackMode::BELOW)
            } else {
                cfg.stack_mode(StackMode::ABOVE)
            }
        };
        for (i, (cat, cw)) in self.clowder.cats().zip(&mut self.cat_wins).enumerate() {
            let cat_win = &mut cw.overlay;
            // Render relative to the continuous window anchor so the cat's torso never shifts
            // inside the pixmap when integer window coordinates round or when the tail sways
            let center = cat.window_center();
            let origin = center - Vec2::splat((cat_win.dim as f32) * 0.5);
            render_cats(
                self.clowder.drawn_in(i),
                &mut cat_win.pixmap.as_mut(),
                &mut cat_win.mask,
                origin,
            );

            let new_x = origin.x.round() as i16;
            let new_y = origin.y.round() as i16;
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
            cat_win.present(&self.conn, self.gc, self.env.depth)?;
        }
        Ok(())
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

/// The keyboard mapping, to turn keycodes into keysyms without xkbcommon. The default (empty)
/// one knows only Escape
#[derive(Default)]
pub(crate) struct Keymap {
    min_keycode: u8,
    keysyms_per_keycode: u8,
    keysyms: Vec<Keysym>,
}

impl Keymap {
    pub(crate) fn load<C: Connection>(conn: &C) -> BoxResult<Self> {
        let setup = conn.setup();
        let (min_keycode, max_keycode) = (setup.min_keycode, setup.max_keycode);
        let mapping = conn
            .get_keyboard_mapping(min_keycode, max_keycode - min_keycode + 1)?
            .reply()?;
        Ok(Self {
            min_keycode,
            keysyms_per_keycode: mapping.keysyms_per_keycode,
            keysyms: mapping.keysyms,
        })
    }

    /// The keysym for a key press, from the keycode's first two columns
    fn keysym(&self, keycode: u8, state: KeyButMask) -> Keysym {
        let per = self.keysyms_per_keycode as usize;
        let base = keycode.saturating_sub(self.min_keycode) as usize * per;
        let col = |i: usize| {
            if i < per {
                self.keysyms.get(base + i).copied().unwrap_or(0)
            } else {
                0
            }
        };
        resolve_keysym(
            col(0),
            col(1),
            state.contains(KeyButMask::SHIFT),
            state.contains(KeyButMask::LOCK),
            state.contains(KeyButMask::MOD2),
        )
    }

    /// A key press as text editing sees it: a character, or one of the editing keys
    pub(crate) fn text_key(&self, keycode: u8, state: KeyButMask) -> Option<TextKey> {
        // Escape's usual keycode
        if self.keysyms.is_empty() && keycode == 9 {
            return Some(TextKey::Escape);
        }
        // Ctrl, Alt or Super with a key is a shortcut, not typing
        let shortcut = state.intersects(KeyButMask::CONTROL | KeyButMask::MOD1 | KeyButMask::MOD4);
        match self.keysym(keycode, state) {
            0xff08 => Some(TextKey::Backspace),
            0xff0d | 0xff8d => Some(TextKey::Enter),
            0xff1b => Some(TextKey::Escape),
            // Keypad digits (with NumLock on)
            sym @ 0xffb0..=0xffb9 if !shortcut => {
                Some(TextKey::Char((b'0' + (sym - 0xffb0) as u8) as char))
            }
            // Printable ASCII and Latin-1 keysyms equal their code points
            sym @ (0x20..=0x7e | 0xa0..=0xff) if !shortcut => {
                char::from_u32(sym).map(TextKey::Char)
            }
            _ => None,
        }
    }
}

/// Picks the keysym for a key press from the keycode's first two keysym columns (unshifted,
/// shifted), following the core protocol rules: Shift selects the second column, Caps Lock
/// only shifts letters (Shift + Caps Lock gives lower case), and with NumLock on keypad keys
/// use their second (digit) column unless Shift is held
fn resolve_keysym(col0: Keysym, col1: Keysym, shift: bool, lock: bool, numlock: bool) -> Keysym {
    let col1 = if col1 == 0 { col0 } else { col1 };
    let is_keypad = |sym: Keysym| (0xff80..=0xffbd).contains(&sym);
    if numlock && is_keypad(col1) {
        return if shift { col0 } else { col1 };
    }
    // Latin-1 letters have distinct lower and upper case keysyms in the two columns
    let is_letter = col0 != col1 && char::from_u32(col0).is_some_and(|c| c.is_alphabetic());
    if lock && is_letter {
        return if shift { col0 } else { col1 };
    }
    if shift { col1 } else { col0 }
}

#[cfg(test)]
mod tests {
    use super::{Keymap, TextKey, resolve_keysym};
    use x11rb::protocol::xproto::KeyButMask;

    #[test]
    fn shortcuts_dont_type_and_an_empty_keymap_still_knows_escape() {
        // Keycode 38: a / A; keycode 22: BackSpace
        let keymap = Keymap {
            min_keycode: 22,
            keysyms_per_keycode: 2,
            keysyms: [[0xff08, 0].as_slice(), &[0; 30], &[0x61, 0x41]].concat(),
        };
        let none = KeyButMask::default();
        assert_eq!(keymap.text_key(38, none), Some(TextKey::Char('a')));
        assert_eq!(
            keymap.text_key(38, KeyButMask::SHIFT),
            Some(TextKey::Char('A'))
        );
        for held in [KeyButMask::CONTROL, KeyButMask::MOD1, KeyButMask::MOD4] {
            assert_eq!(keymap.text_key(38, held), None);
            assert_eq!(keymap.text_key(22, held), Some(TextKey::Backspace));
        }
        let empty = Keymap::default();
        assert_eq!(empty.text_key(9, none), Some(TextKey::Escape));
        assert_eq!(empty.text_key(38, none), None);
    }

    #[test]
    fn caps_lock_shifts_only_letters_and_numlock_enables_keypad_digits() {
        let (a, upper_a) = (0x61, 0x41);
        let (one, bang) = (0x31, 0x21);
        // Shift
        assert_eq!(resolve_keysym(a, upper_a, false, false, false), a);
        assert_eq!(resolve_keysym(a, upper_a, true, false, false), upper_a);
        assert_eq!(resolve_keysym(one, bang, true, false, false), bang);
        // Caps Lock: letters only, and Shift undoes it
        assert_eq!(resolve_keysym(a, upper_a, false, true, false), upper_a);
        assert_eq!(resolve_keysym(a, upper_a, true, true, false), a);
        assert_eq!(resolve_keysym(one, bang, false, true, false), one);
        // Keypad: KP_End / KP_1
        let (kp_end, kp_1) = (0xff9c, 0xffb1);
        assert_eq!(resolve_keysym(kp_end, kp_1, false, false, true), kp_1);
        assert_eq!(resolve_keysym(kp_end, kp_1, true, false, true), kp_end);
        assert_eq!(resolve_keysym(kp_end, kp_1, false, false, false), kp_end);
        // Keys with a single keysym
        assert_eq!(resolve_keysym(0xff08, 0, true, true, true), 0xff08);
    }
}
