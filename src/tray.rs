use crate::cat::{Activity, BehaviorConfig, CatSim, CoatStyle, FeedPreset, Temperament};
use crate::render::coat_swatch_colors;
#[cfg(not(any(target_os = "windows", windows_check)))]
use crate::render::render_tray_icon;
use std::borrow::Cow;
#[cfg(any(not(any(target_os = "windows", windows_check)), test))]
use tiny_skia::Pixmap;
use tiny_skia::{
    Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, PixmapMut, Rect, Stroke, Transform,
};
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::connection::Connection;
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::protocol::Event;
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ClientMessageEvent, Colormap, ConfigureWindowAux, ConnectionExt as _,
    CreateGCAux, CreateWindowAux, EventMask, Gcontext, GrabMode, ImageFormat, PropMode, Screen,
    StackMode, Visualid, Window, WindowClass,
};
#[cfg(not(any(target_os = "windows", windows_check)))]
use x11rb::wrapper::ConnectionExt as _;

#[cfg(not(any(target_os = "windows", windows_check)))]
const SYSTEM_TRAY_REQUEST_DOCK: u32 = 0;
#[cfg(not(any(target_os = "windows", windows_check)))]
const XEMBED_MAPPED: u32 = 1 << 0;

pub(crate) const MENU_WIDTH: u16 = 228;
/// Opened by the About row
pub(crate) const ABOUT_URL: &str = env!("CARGO_PKG_REPOSITORY");
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrayState {
    pub coat: CoatStyle,
    pub behaviors: BehaviorConfig,
    pub scale: f32,
    pub sleeping: bool,
    pub hidden: bool,
    pub start_on_login: bool,
    pub feed_preset: Option<FeedPreset>,
    pub feed_remaining_secs: Option<u32>,
}

impl TrayState {
    pub fn from_cat(cat: &CatSim, hidden: bool, start_on_login: bool) -> Self {
        Self {
            coat: cat.coat,
            behaviors: cat.behaviors,
            scale: cat.scale,
            sleeping: matches!(cat.activity, Activity::Sleeping),
            hidden,
            start_on_login,
            feed_preset: cat.feeding_preset(),
            feed_remaining_secs: cat.feeding_remaining_secs(),
        }
    }

    /// Whether the tray icon (which only depends on coat, sleep and visibility) must be redrawn
    pub fn icon_differs(&self, other: &TrayState) -> bool {
        self.coat != other.coat || self.sleeping != other.sleeping || self.hidden != other.hidden
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrayAction {
    SetCoat(CoatStyle),
    SetTemperament(Temperament),
    ToggleFollowCursor,
    TogglePlayfulQuirks,
    SelectFeedPreset(FeedPreset),
    StopFeeding,
    SetScale(f32),
    ToggleHidden,
    ToggleStartOnLogin,
    OpenAbout,
    Quit,
}

/// Choosing the feed preset that is already running stops feeding instead of restarting it
pub(crate) fn normalize_tray_action(action: TrayAction, active: Option<FeedPreset>) -> TrayAction {
    match action {
        TrayAction::SelectFeedPreset(p) if active == Some(p) => TrayAction::StopFeeding,
        a => a,
    }
}

#[derive(Debug, Clone)]
pub(crate) enum MenuRow {
    Header(&'static str),
    Separator,
    Item {
        label: Cow<'static, str>,
        action: TrayAction,
        indicator: Indicator,
        close_on_click: bool,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum Indicator {
    /// Click advances to the next value; optionally shows a coat swatch on the right
    Cycle(Option<CoatStyle>),
    FeedRadio {
        selected: bool,
        remaining_secs: Option<u32>,
    },
    Checkbox(bool),
    Stop,
    Info,
    Danger,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HitEntry {
    pub y_top: i16,
    pub y_bottom: i16,
    pub action: TrayAction,
    pub close_on_click: bool,
}

/// Converts premultiplied RGBA8 pixels from `tiny-skia` into premultiplied BGRA8 pixels
/// expected by both X11 32-bit ZPixmap and Win32 32-bpp DIB sections (`AC_SRC_ALPHA`).
pub(crate) fn rgba_to_bgra(src: &[u8], dst: &mut [u8]) {
    for (dst_px, src_px) in dst
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(src.as_chunks::<4>().0)
    {
        dst_px[0] = src_px[2]; // B
        dst_px[1] = src_px[1]; // G
        dst_px[2] = src_px[0]; // R
        dst_px[3] = src_px[3]; // A
    }
}

pub(crate) fn hit_test_entries(
    hit_entries: &[HitEntry],
    menu_w: u16,
    x: i16,
    y: i16,
) -> Option<usize> {
    if x < 6 || x >= (menu_w as i16 - 6) {
        return None;
    }
    hit_entries
        .iter()
        .position(|e| y >= e.y_top && y < e.y_bottom)
}

/// ICCCM `WM_SIZE_HINTS` (PSize | PMinSize | PMaxSize | PBaseSize) for a square `dim`x`dim` tray icon
#[cfg(not(any(target_os = "windows", windows_check)))]
fn tray_size_hints(dim: u16) -> [u32; 18] {
    let d = dim as u32;
    [
        (1 << 3) | (1 << 4) | (1 << 5) | (1 << 8),
        0,
        0,
        d,
        d,
        d,
        d,
        48,
        48,
        0,
        0,
        0,
        0,
        0,
        0,
        d,
        d,
        0,
    ]
}

#[cfg(not(any(target_os = "windows", windows_check)))]
pub struct TrayController {
    root: Window,
    screen_w: u16,
    screen_h: u16,

    // Tray icon window (docked via XEmbed)
    tray_win: Window,
    tray_gc: Gcontext,
    tray_depth: u8,
    tray_opaque_bg: bool,
    tray_w: u16,
    tray_h: u16,
    tray_owner: Window,

    // Popup menu window (32-bit ARGB override-redirect)
    menu_win: Window,
    menu_gc: Gcontext,
    menu_depth: u8,
    menu_w: u16,
    menu_h: u16,
    menu_open: bool,
    hovered_idx: Option<usize>,
    hit_entries: Vec<HitEntry>,
    // Root position of the tray icon the menu was opened from
    menu_anchor: (i16, i16),

    // X11 atoms
    sys_tray_atom: Atom,
    sys_tray_opcode_atom: Atom,
    xembed_info_atom: Atom,
    manager_atom: Atom,

    last_state: TrayState,
}

#[cfg(not(any(target_os = "windows", windows_check)))]
impl TrayController {
    pub fn new<C: Connection>(
        conn: &C,
        screen_num: usize,
        argb_depth: u8,
        argb_visual: Visualid,
        argb_colormap: Colormap,
        initial_state: TrayState,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let screen = &conn.setup().roots[screen_num];
        let root = screen.root;
        let screen_w = screen.width_in_pixels;
        let screen_h = screen.height_in_pixels;

        // Subscribe to SubstructureNotify / StructureNotify on root to catch MANAGER selection announcements
        let tray_sel_name = format!("_NET_SYSTEM_TRAY_S{screen_num}");
        let sys_tray_atom = conn
            .intern_atom(false, tray_sel_name.as_bytes())?
            .reply()?
            .atom;
        let sys_tray_opcode_atom = conn
            .intern_atom(false, b"_NET_SYSTEM_TRAY_OPCODE")?
            .reply()?
            .atom;
        let sys_tray_visual_atom = conn
            .intern_atom(false, b"_NET_SYSTEM_TRAY_VISUAL")?
            .reply()?
            .atom;
        let xembed_info_atom = conn.intern_atom(false, b"_XEMBED_INFO")?.reply()?.atom;
        let manager_atom = conn.intern_atom(false, b"MANAGER")?.reply()?.atom;

        let (tray_depth, tray_visual, tray_colormap, tray_opaque_bg) = Self::choose_tray_visual(
            conn,
            screen,
            sys_tray_atom,
            sys_tray_visual_atom,
            argb_depth,
            argb_visual,
            argb_colormap,
        )?;

        let tray_w = 24u16;
        let tray_h = 24u16;
        let tray_win = conn.generate_id()?;

        let mut tray_aux = CreateWindowAux::new().border_pixel(0).event_mask(
            EventMask::EXPOSURE
                | EventMask::BUTTON_PRESS
                | EventMask::STRUCTURE_NOTIFY
                | EventMask::ENTER_WINDOW
                | EventMask::LEAVE_WINDOW,
        );
        if tray_opaque_bg {
            tray_aux = tray_aux.background_pixel(0x282a36);
        } else {
            tray_aux = tray_aux.background_pixel(0).colormap(tray_colormap);
        }

        conn.create_window(
            tray_depth,
            tray_win,
            root,
            0,
            0,
            tray_w,
            tray_h,
            0,
            WindowClass::INPUT_OUTPUT,
            tray_visual,
            &tray_aux,
        )?;

        // Set _XEMBED_INFO = [version 0, XEMBED_MAPPED]
        conn.change_property32(
            PropMode::REPLACE,
            tray_win,
            xembed_info_atom,
            xembed_info_atom,
            &[0, XEMBED_MAPPED],
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            tray_win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"catsor",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            tray_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"catsor\0Catsor\0",
        )?;
        // Set ICCCM WM_NORMAL_HINTS so trayer allocates a full square slot
        conn.change_property32(
            PropMode::REPLACE,
            tray_win,
            AtomEnum::WM_NORMAL_HINTS,
            AtomEnum::WM_SIZE_HINTS,
            &tray_size_hints(tray_w),
        )?;

        let tray_gc = conn.generate_id()?;
        conn.create_gc(tray_gc, tray_win, &CreateGCAux::new())?;

        // Create popup menu window (initially unmapped)
        let rows = build_rows(&initial_state);
        let (menu_h, hit_entries) = compute_layout(&rows);
        let menu_w = MENU_WIDTH;
        let menu_win = conn.generate_id()?;

        let menu_aux = CreateWindowAux::new()
            .background_pixel(0)
            .border_pixel(0)
            .override_redirect(1)
            .event_mask(
                EventMask::EXPOSURE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | EventMask::POINTER_MOTION
                    | EventMask::LEAVE_WINDOW
                    | EventMask::KEY_PRESS
                    | EventMask::STRUCTURE_NOTIFY,
            )
            .colormap(argb_colormap);

        conn.create_window(
            argb_depth,
            menu_win,
            root,
            0,
            0,
            menu_w,
            menu_h,
            0,
            WindowClass::INPUT_OUTPUT,
            argb_visual,
            &menu_aux,
        )?;

        let wm_type = conn
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE")?
            .reply()?
            .atom;
        let wm_type_popup = conn
            .intern_atom(false, b"_NET_WM_WINDOW_TYPE_POPUP_MENU")?
            .reply()?
            .atom;
        conn.change_property32(
            PropMode::REPLACE,
            menu_win,
            wm_type,
            AtomEnum::ATOM,
            &[wm_type_popup],
        )?;

        let wm_state = conn.intern_atom(false, b"_NET_WM_STATE")?.reply()?.atom;
        let wm_state_above = conn
            .intern_atom(false, b"_NET_WM_STATE_ABOVE")?
            .reply()?
            .atom;
        conn.change_property32(
            PropMode::REPLACE,
            menu_win,
            wm_state,
            AtomEnum::ATOM,
            &[wm_state_above],
        )?;

        conn.change_property8(
            PropMode::REPLACE,
            menu_win,
            AtomEnum::WM_NAME,
            AtomEnum::STRING,
            b"catsor-menu",
        )?;
        conn.change_property8(
            PropMode::REPLACE,
            menu_win,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            b"catsor-menu\0Catsor\0",
        )?;

        let menu_gc = conn.generate_id()?;
        conn.create_gc(menu_gc, menu_win, &CreateGCAux::new())?;

        let mut controller = Self {
            root,
            screen_w,
            screen_h,
            tray_win,
            tray_gc,
            tray_depth,
            tray_opaque_bg,
            tray_w,
            tray_h,
            tray_owner: x11rb::NONE,
            menu_win,
            menu_gc,
            menu_depth: argb_depth,
            menu_w,
            menu_h,
            menu_open: false,
            menu_anchor: (0, 0),
            hovered_idx: None,
            hit_entries,
            sys_tray_atom,
            sys_tray_opcode_atom,
            xembed_info_atom,
            manager_atom,
            last_state: initial_state,
        };

        controller.try_dock(conn)?;
        controller.redraw_tray_icon(conn)?;
        Ok(controller)
    }

    fn choose_tray_visual<C: Connection>(
        conn: &C,
        screen: &Screen,
        sys_tray_atom: Atom,
        sys_tray_visual_atom: Atom,
        argb_depth: u8,
        argb_visual: Visualid,
        argb_colormap: Colormap,
    ) -> Result<(u8, Visualid, Colormap, bool), Box<dyn std::error::Error>> {
        let owner = conn.get_selection_owner(sys_tray_atom)?.reply()?.owner;
        if owner != x11rb::NONE {
            if let Ok(prop) = conn
                .get_property(false, owner, sys_tray_visual_atom, AtomEnum::VISUALID, 0, 1)?
                .reply()
                && let Some(vis_id) = prop.value32().and_then(|mut it| it.next())
                && vis_id == argb_visual
            {
                return Ok((argb_depth, argb_visual, argb_colormap, false));
            }
            if let Ok(geom) = conn.get_geometry(owner)?.reply()
                && geom.depth == 32
            {
                return Ok((argb_depth, argb_visual, argb_colormap, false));
            }
        }
        Ok((
            screen.root_depth,
            screen.root_visual,
            screen.default_colormap,
            true,
        ))
    }

    pub fn open_menu_window(&self) -> Option<Window> {
        self.menu_open.then_some(self.menu_win)
    }

    /// Checks whether a system tray owner exists and requests docking if not yet docked.
    pub fn try_dock<C: Connection>(&mut self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        let owner = conn.get_selection_owner(self.sys_tray_atom)?.reply()?.owner;
        if owner == x11rb::NONE || owner == self.tray_owner {
            return Ok(());
        }
        self.tray_owner = owner;

        // Re-assert _XEMBED_INFO before docking
        conn.change_property32(
            PropMode::REPLACE,
            self.tray_win,
            self.xembed_info_atom,
            self.xembed_info_atom,
            &[0, XEMBED_MAPPED],
        )?;

        let msg = ClientMessageEvent::new(
            32,
            owner,
            self.sys_tray_opcode_atom,
            [
                x11rb::CURRENT_TIME,
                SYSTEM_TRAY_REQUEST_DOCK,
                self.tray_win,
                0,
                0,
            ],
        );
        conn.send_event(false, owner, EventMask::NO_EVENT, msg)?;
        conn.flush()?;
        Ok(())
    }

    /// Updates state and redraws tray icon (and menu if open) whenever relevant state changes.
    pub fn sync_state<C: Connection>(
        &mut self,
        conn: &C,
        state: TrayState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let icon_changed = state.icon_differs(&self.last_state);
        let menu_changed = state != self.last_state;
        self.last_state = state;

        if icon_changed {
            self.redraw_tray_icon(conn)?;
        }
        if self.menu_open && menu_changed {
            self.redraw_menu(conn)?;
        }
        Ok(())
    }

    /// Handles an X11 event and returns an optional `TrayAction` triggered by the user.
    pub fn handle_event<C: Connection>(
        &mut self,
        conn: &C,
        event: &Event,
    ) -> Result<Option<TrayAction>, Box<dyn std::error::Error>> {
        match event {
            Event::Expose(ev) => {
                if ev.window == self.tray_win && ev.count == 0 {
                    self.redraw_tray_icon(conn)?;
                } else if ev.window == self.menu_win && ev.count == 0 && self.menu_open {
                    self.redraw_menu(conn)?;
                }
            }
            Event::ConfigureNotify(ev) => {
                if ev.window == self.tray_win {
                    let target_dim = ev.height.clamp(22, 48);
                    if ev.width < target_dim {
                        let _ = conn.change_property32(
                            PropMode::REPLACE,
                            self.tray_win,
                            AtomEnum::WM_NORMAL_HINTS,
                            AtomEnum::WM_SIZE_HINTS,
                            &tray_size_hints(target_dim),
                        );
                        let _ = conn.configure_window(
                            self.tray_win,
                            &ConfigureWindowAux::new()
                                .width(target_dim as u32)
                                .height(target_dim as u32),
                        );
                    }
                    self.tray_w = ev.width.clamp(16, 64);
                    self.tray_h = ev.height.clamp(16, 64);
                    self.redraw_tray_icon(conn)?;
                }
            }
            Event::ReparentNotify(ev) => {
                if ev.window == self.tray_win {
                    if ev.parent == self.root {
                        conn.unmap_window(self.tray_win)?;
                        self.tray_owner = x11rb::NONE;
                    } else {
                        conn.map_window(self.tray_win)?;
                        self.redraw_tray_icon(conn)?;
                    }
                }
            }
            Event::DestroyNotify(ev) => {
                if ev.window == self.tray_owner {
                    self.tray_owner = x11rb::NONE;
                }
            }
            Event::ClientMessage(ev) => {
                if ev.type_ == self.manager_atom {
                    self.tray_owner = x11rb::NONE;
                    self.try_dock(conn)?;
                }
            }
            Event::ButtonPress(ev) => {
                if ev.event == self.tray_win {
                    if ev.detail == 1 || ev.detail == 2 || ev.detail == 3 {
                        if self.menu_open {
                            self.close_menu(conn)?;
                        } else {
                            let icon_root_x = ev.root_x.saturating_sub(ev.event_x);
                            let icon_root_y = ev.root_y.saturating_sub(ev.event_y);
                            self.open_menu(conn, icon_root_x, icon_root_y)?;
                        }
                    }
                } else if ev.event == self.menu_win && self.menu_open {
                    let lx = ev.event_x;
                    let ly = ev.event_y;
                    if lx < 0 || ly < 0 || lx >= self.menu_w as i16 || ly >= self.menu_h as i16 {
                        self.close_menu(conn)?;
                    } else if (ev.detail == 1 || ev.detail == 3)
                        && let Some(idx) = self.hit_test(lx, ly)
                    {
                        let entry = self.hit_entries[idx];
                        if entry.close_on_click {
                            self.close_menu(conn)?;
                        }
                        return Ok(Some(entry.action));
                    }
                }
            }
            Event::MotionNotify(ev) => {
                if ev.event == self.menu_win && self.menu_open {
                    let new_hover = self.hit_test(ev.event_x, ev.event_y);
                    if new_hover != self.hovered_idx {
                        self.hovered_idx = new_hover;
                        self.redraw_menu(conn)?;
                    }
                }
            }
            Event::LeaveNotify(ev) => {
                if ev.event == self.menu_win && self.menu_open && self.hovered_idx.is_some() {
                    self.hovered_idx = None;
                    self.redraw_menu(conn)?;
                }
            }
            // Escape keycode on X11 is 9
            Event::KeyPress(ev) if self.menu_open && ev.detail == 9 => {
                self.close_menu(conn)?;
            }
            _ => {}
        }
        Ok(None)
    }

    fn hit_test(&self, x: i16, y: i16) -> Option<usize> {
        hit_test_entries(&self.hit_entries, self.menu_w, x, y)
    }

    fn open_menu<C: Connection>(
        &mut self,
        conn: &C,
        icon_root_x: i16,
        icon_root_y: i16,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let rows = build_rows(&self.last_state);
        let (menu_h, hit_entries) = compute_layout(&rows);
        self.menu_h = menu_h;
        self.hit_entries = hit_entries;
        self.hovered_idx = None;
        self.menu_anchor = (icon_root_x, icon_root_y);
        self.place_menu(conn)?;
        conn.map_window(self.menu_win)?;
        self.menu_open = true;
        self.redraw_menu(conn)?;

        let _ = conn.grab_pointer(
            false,
            self.menu_win,
            EventMask::BUTTON_PRESS | EventMask::BUTTON_RELEASE | EventMask::POINTER_MOTION,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
            x11rb::NONE,
            x11rb::NONE,
            x11rb::CURRENT_TIME,
        );
        let _ = conn.grab_keyboard(
            false,
            self.menu_win,
            x11rb::CURRENT_TIME,
            GrabMode::ASYNC,
            GrabMode::ASYNC,
        );
        conn.flush()?;
        Ok(())
    }

    /// Positions and sizes the menu window next to `menu_anchor`: below the icon, or above it if it
    /// doesn't fit, always kept fully on screen (8 px margin) as long as the screen is tall enough.
    fn place_menu<C: Connection>(&self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        let (icon_x, icon_y) = self.menu_anchor;
        let menu_w = self.menu_w as i16;
        let menu_h = self.menu_h as i16;

        let max_x = (self.screen_w as i16 - menu_w - 8).max(8);
        let menu_x = (icon_x + self.tray_w as i16 - menu_w).clamp(8, max_x);

        let max_y = (self.screen_h as i16 - menu_h - 8).max(8);
        let below_y = icon_y + self.tray_h as i16 + 6;
        let menu_y = if below_y <= max_y {
            below_y
        } else {
            (icon_y - menu_h - 6).clamp(8, max_y)
        };

        conn.configure_window(
            self.menu_win,
            &ConfigureWindowAux::new()
                .x(menu_x as i32)
                .y(menu_y as i32)
                .width(self.menu_w as u32)
                .height(self.menu_h as u32)
                .stack_mode(StackMode::ABOVE),
        )?;
        Ok(())
    }

    pub fn close_menu<C: Connection>(
        &mut self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if !self.menu_open {
            return Ok(());
        }
        self.menu_open = false;
        self.hovered_idx = None;
        let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME);
        let _ = conn.ungrab_pointer(x11rb::CURRENT_TIME);
        conn.unmap_window(self.menu_win)?;
        conn.flush()?;
        Ok(())
    }

    pub fn redraw_tray_icon<C: Connection>(
        &self,
        conn: &C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let w = self.tray_w.max(16) as u32;
        let h = self.tray_h.max(16) as u32;
        let Some(mut pixmap) = Pixmap::new(w, h) else {
            return Ok(());
        };
        render_tray_icon(
            self.last_state.coat,
            self.last_state.sleeping,
            self.last_state.hidden,
            self.tray_opaque_bg,
            &mut pixmap.as_mut(),
        );
        Self::put_pixmap(
            conn,
            self.tray_win,
            self.tray_gc,
            self.tray_depth,
            w as u16,
            h as u16,
            &pixmap,
        )?;
        Ok(())
    }

    fn redraw_menu<C: Connection>(&mut self, conn: &C) -> Result<(), Box<dyn std::error::Error>> {
        let rows = build_rows(&self.last_state);
        let (menu_h, hit_entries) = compute_layout(&rows);
        let resized = menu_h != self.menu_h;
        self.menu_h = menu_h;
        self.hit_entries = hit_entries;
        // Rows can appear/disappear while open (e.g. "Stop Feeding" when the timer runs out)
        if resized && self.menu_open {
            self.hovered_idx = None;
            self.place_menu(conn)?;
        }

        let w = self.menu_w as u32;
        let h = self.menu_h as u32;
        let Some(mut pixmap) = Pixmap::new(w, h) else {
            return Ok(());
        };

        render_menu_pixmap(&rows, self.hovered_idx, &mut pixmap.as_mut());
        Self::put_pixmap(
            conn,
            self.menu_win,
            self.menu_gc,
            self.menu_depth,
            self.menu_w,
            self.menu_h,
            &pixmap,
        )?;
        Ok(())
    }

    fn put_pixmap<C: Connection>(
        conn: &C,
        win: Window,
        gc: Gcontext,
        depth: u8,
        w: u16,
        h: u16,
        pixmap: &Pixmap,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let src = pixmap.data();
        let mut bgra = vec![0u8; src.len()];
        rgba_to_bgra(src, &mut bgra);
        conn.put_image(ImageFormat::Z_PIXMAP, win, gc, w, h, 0, 0, 0, depth, &bgra)?;
        conn.flush()?;
        Ok(())
    }
}

/// Size presets the "Size" row cycles through
const SIZE_PRESETS: [(&str, f32); 4] = [
    ("Compact 0.8x", 0.80),
    ("Normal 1.0x", 1.00),
    ("Large 1.35x", 1.35),
    ("Extra Large 1.75x", 1.75),
];

/// Label for the current scale and the preset the next click switches to.
/// A scale that matches no preset (e.g. `--scale 1.5`) shows as "Custom" and advances to the next larger preset.
fn size_row(scale: f32) -> (Cow<'static, str>, f32) {
    match SIZE_PRESETS
        .iter()
        .position(|&(_, sc)| (scale - sc).abs() < 0.08)
    {
        Some(i) => {
            let name = SIZE_PRESETS[i].0;
            let next = SIZE_PRESETS[(i + 1) % SIZE_PRESETS.len()].1;
            (Cow::Owned(format!("Size: {name}")), next)
        }
        None => {
            let next = SIZE_PRESETS
                .iter()
                .map(|&(_, sc)| sc)
                .find(|&sc| sc > scale)
                .unwrap_or(SIZE_PRESETS[0].1);
            (Cow::Owned(format!("Size: Custom {scale:.2}x")), next)
        }
    }
}

/// The value after `current` in `all`, wrapping around (for "click to advance" rows)
fn next_in<T: Copy + PartialEq>(all: &[T], current: T) -> T {
    let idx = all.iter().position(|&v| v == current).unwrap_or(0);
    all[(idx + 1) % all.len()]
}

pub(crate) fn build_rows(state: &TrayState) -> Vec<MenuRow> {
    let mut rows = Vec::with_capacity(24);

    rows.push(MenuRow::Header("APPEARANCE"));
    rows.push(MenuRow::Item {
        label: Cow::Owned(format!("Coat: {}", state.coat.label())),
        action: TrayAction::SetCoat(next_in(&CoatStyle::ALL, state.coat)),
        indicator: Indicator::Cycle(Some(state.coat)),
        close_on_click: false,
    });
    let (size_label, next_scale) = size_row(state.scale);
    rows.push(MenuRow::Item {
        label: size_label,
        action: TrayAction::SetScale(next_scale),
        indicator: Indicator::Cycle(None),
        close_on_click: false,
    });
    let temperament = state.behaviors.temperament;
    rows.push(MenuRow::Item {
        label: Cow::Owned(format!("Temperament: {}", temperament.label())),
        action: TrayAction::SetTemperament(next_in(&Temperament::ALL, temperament)),
        indicator: Indicator::Cycle(None),
        close_on_click: false,
    });

    rows.push(MenuRow::Separator);
    rows.push(MenuRow::Header("FEED (KEEP AWAKE)"));
    for preset in FeedPreset::ALL {
        let selected = state.feed_preset == Some(preset);
        rows.push(MenuRow::Item {
            label: Cow::Borrowed(preset.label()),
            action: TrayAction::SelectFeedPreset(preset),
            indicator: Indicator::FeedRadio {
                selected,
                remaining_secs: if selected {
                    state.feed_remaining_secs
                } else {
                    None
                },
            },
            close_on_click: true,
        });
    }
    if state.feed_preset.is_some() {
        rows.push(MenuRow::Item {
            label: Cow::Borrowed("Stop Feeding"),
            action: TrayAction::StopFeeding,
            indicator: Indicator::Stop,
            close_on_click: true,
        });
    }

    rows.push(MenuRow::Separator);
    rows.push(MenuRow::Header("BEHAVIORS"));
    rows.push(MenuRow::Item {
        label: Cow::Borrowed("Follow Cursor"),
        action: TrayAction::ToggleFollowCursor,
        indicator: Indicator::Checkbox(state.behaviors.follow_cursor),
        close_on_click: false,
    });
    rows.push(MenuRow::Item {
        label: Cow::Borrowed("Playful Hunting"),
        action: TrayAction::TogglePlayfulQuirks,
        indicator: Indicator::Checkbox(state.behaviors.playful_quirks),
        close_on_click: false,
    });

    rows.push(MenuRow::Separator);
    rows.push(MenuRow::Item {
        label: Cow::Borrowed("Show Cat"),
        action: TrayAction::ToggleHidden,
        indicator: Indicator::Checkbox(!state.hidden),
        close_on_click: false,
    });
    rows.push(MenuRow::Item {
        label: Cow::Borrowed("Start on Login"),
        action: TrayAction::ToggleStartOnLogin,
        indicator: Indicator::Checkbox(state.start_on_login),
        close_on_click: false,
    });
    rows.push(MenuRow::Item {
        label: Cow::Borrowed(concat!("About Catsor v", env!("CARGO_PKG_VERSION"))),
        action: TrayAction::OpenAbout,
        indicator: Indicator::Info,
        close_on_click: true,
    });
    rows.push(MenuRow::Item {
        label: Cow::Borrowed("Quit Catsor"),
        action: TrayAction::Quit,
        indicator: Indicator::Danger,
        close_on_click: true,
    });

    rows
}

pub(crate) fn compute_layout(rows: &[MenuRow]) -> (u16, Vec<HitEntry>) {
    let mut y = 6i16;
    let mut hits = Vec::new();
    for row in rows {
        match row {
            MenuRow::Header(_) => {
                y += 16;
            }
            MenuRow::Separator => {
                y += 7;
            }
            MenuRow::Item {
                action,
                close_on_click,
                ..
            } => {
                let row_h = 19i16;
                hits.push(HitEntry {
                    y_top: y,
                    y_bottom: y + row_h,
                    action: *action,
                    close_on_click: *close_on_click,
                });
                y += row_h;
            }
        }
    }
    y += 6;
    (y as u16, hits)
}

pub(crate) fn render_menu_pixmap(
    rows: &[MenuRow],
    hovered_idx: Option<usize>,
    pixmap: &mut PixmapMut,
) {
    pixmap.fill(Color::TRANSPARENT);
    let w = pixmap.width() as f32;
    let h = pixmap.height() as f32;

    let mut paint = Paint::default();

    // Outer rounded card background (#282a36 Dracula dark theme to match trayer)
    if let Some(bg_path) = build_rounded_rect(1.0, 1.0, w - 2.0, h - 2.0, 8.0) {
        paint.set_color(Color::from_rgba8(0x23, 0x25, 0x30, 248));
        pixmap.fill_path(
            &bg_path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let border_stroke = Stroke {
            width: 1.2,
            ..Default::default()
        };
        paint.set_color(Color::from_rgba8(0x62, 0x72, 0xa4, 210));
        pixmap.stroke_path(
            &bg_path,
            &paint,
            &border_stroke,
            Transform::identity(),
            None,
        );
    }

    let mut y = 6.0f32;
    let mut item_idx = 0usize;

    for row in rows {
        match row {
            MenuRow::Header(title) => {
                draw_text(
                    pixmap,
                    title,
                    12.0,
                    y + 4.5,
                    Color::from_rgba8(0x8b, 0xe9, 0xfd, 230),
                );
                y += 16.0;
            }
            MenuRow::Separator => {
                let line_y = y + 3.0;
                if let Some(rect) = Rect::from_xywh(10.0, line_y, w - 20.0, 1.0) {
                    paint.set_color(Color::from_rgba8(0x44, 0x47, 0x5a, 190));
                    pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                }
                y += 7.0;
            }
            MenuRow::Item {
                label, indicator, ..
            } => {
                let row_h = 19.0f32;
                let is_hovered = hovered_idx == Some(item_idx);
                if is_hovered && let Some(hl) = build_rounded_rect(6.0, y, w - 12.0, row_h, 5.0) {
                    paint.set_color(if matches!(indicator, Indicator::Danger) {
                        Color::from_rgba8(0xff, 0x55, 0x55, 55)
                    } else {
                        Color::from_rgba8(0x44, 0x47, 0x5a, 220)
                    });
                    pixmap.fill_path(&hl, &paint, FillRule::Winding, Transform::identity(), None);
                }

                let cy = y + row_h * 0.5;
                let ix = 18.0f32;

                match *indicator {
                    Indicator::Cycle(swatch) => {
                        // Double chevron: "click to advance"
                        let mut pb = PathBuilder::new();
                        for dx in [-2.5f32, 1.5] {
                            pb.move_to(ix + dx - 1.5, cy - 3.0);
                            pb.line_to(ix + dx + 1.5, cy);
                            pb.line_to(ix + dx - 1.5, cy + 3.0);
                        }
                        if let Some(p) = pb.finish() {
                            let st = Stroke {
                                width: 1.4,
                                line_cap: LineCap::Round,
                                line_join: LineJoin::Round,
                                ..Default::default()
                            };
                            paint.set_color(Color::from_rgba8(0xbd, 0x93, 0xf9, 240));
                            pixmap.stroke_path(&p, &paint, &st, Transform::identity(), None);
                        }
                        if let Some(coat) = swatch {
                            let (base, accent, outline) = coat_swatch_colors(coat);
                            let sx = w - 22.0;
                            if let Some(sw) = PathBuilder::from_circle(sx, cy, 5.5) {
                                paint.set_color(base);
                                pixmap.fill_path(
                                    &sw,
                                    &paint,
                                    FillRule::Winding,
                                    Transform::identity(),
                                    None,
                                );
                                let st = Stroke {
                                    width: 1.0,
                                    ..Default::default()
                                };
                                paint.set_color(outline);
                                pixmap.stroke_path(&sw, &paint, &st, Transform::identity(), None);
                            }
                            if let Some(dot) = PathBuilder::from_circle(sx + 2.0, cy - 1.5, 2.6) {
                                paint.set_color(accent);
                                pixmap.fill_path(
                                    &dot,
                                    &paint,
                                    FillRule::Winding,
                                    Transform::identity(),
                                    None,
                                );
                            }
                        }
                    }
                    Indicator::FeedRadio {
                        selected,
                        remaining_secs,
                    } => {
                        draw_radio_indicator(pixmap, ix, cy, selected);
                        if selected {
                            let badge = match remaining_secs {
                                Some(secs) => {
                                    let mins = secs / 60;
                                    let rem_s = secs % 60;
                                    format!("{mins:02}:{rem_s:02}")
                                }
                                None => String::from("ACTIVE"),
                            };
                            let badge_w = (badge.len() as f32) * 6.0;
                            draw_text(
                                pixmap,
                                &badge,
                                w - 12.0 - badge_w,
                                y + 6.0,
                                Color::from_rgba8(0x50, 0xfa, 0x7b, 245),
                            );
                        }
                    }
                    Indicator::Checkbox(checked) => {
                        draw_checkbox_indicator(pixmap, ix, cy, checked);
                    }
                    Indicator::Stop => {
                        if let Some(sq) = Rect::from_xywh(ix - 3.5, cy - 3.5, 7.0, 7.0) {
                            paint.set_color(Color::from_rgba8(0xff, 0xb8, 0x6c, 240));
                            pixmap.fill_rect(sq, &paint, Transform::identity(), None);
                        }
                    }
                    Indicator::Info => {
                        // Circled "i"
                        let st = Stroke {
                            width: 1.3,
                            line_cap: LineCap::Round,
                            ..Default::default()
                        };
                        paint.set_color(Color::from_rgba8(0x8b, 0xe9, 0xfd, 240));
                        if let Some(ring) = PathBuilder::from_circle(ix, cy, 4.8) {
                            pixmap.stroke_path(&ring, &paint, &st, Transform::identity(), None);
                        }
                        let mut pb = PathBuilder::new();
                        pb.move_to(ix, cy - 0.5);
                        pb.line_to(ix, cy + 2.4);
                        if let Some(p) = pb.finish() {
                            pixmap.stroke_path(&p, &paint, &st, Transform::identity(), None);
                        }
                        if let Some(dot) = PathBuilder::from_circle(ix, cy - 2.6, 0.8) {
                            pixmap.fill_path(
                                &dot,
                                &paint,
                                FillRule::Winding,
                                Transform::identity(),
                                None,
                            );
                        }
                    }
                    Indicator::Danger => {
                        let mut pb = PathBuilder::new();
                        pb.move_to(ix - 3.0, cy - 3.0);
                        pb.line_to(ix + 3.0, cy + 3.0);
                        pb.move_to(ix + 3.0, cy - 3.0);
                        pb.line_to(ix - 3.0, cy + 3.0);
                        if let Some(p) = pb.finish() {
                            let st = Stroke {
                                width: 1.6,
                                line_cap: LineCap::Round,
                                ..Default::default()
                            };
                            paint.set_color(Color::from_rgba8(0xff, 0x55, 0x55, 245));
                            pixmap.stroke_path(&p, &paint, &st, Transform::identity(), None);
                        }
                    }
                }

                let text_col = if matches!(indicator, Indicator::Danger) {
                    Color::from_rgba8(0xff, 0x6e, 0x6e, 255)
                } else if is_hovered {
                    Color::from_rgba8(0xff, 0xff, 0xff, 255)
                } else {
                    Color::from_rgba8(0xf8, 0xf8, 0xf2, 238)
                };

                draw_text(pixmap, label, 32.0, y + 6.0, text_col);

                item_idx += 1;
                y += row_h;
            }
        }
    }
}

fn draw_radio_indicator(pixmap: &mut PixmapMut, cx: f32, cy: f32, selected: bool) {
    let mut paint = Paint::default();

    if let Some(outer) = PathBuilder::from_circle(cx, cy, 4.8) {
        let st = Stroke {
            width: 1.3,
            ..Default::default()
        };
        paint.set_color(if selected {
            Color::from_rgba8(0x50, 0xfa, 0x7b, 255)
        } else {
            Color::from_rgba8(0x62, 0x72, 0xa4, 220)
        });
        pixmap.stroke_path(&outer, &paint, &st, Transform::identity(), None);
    }

    if selected && let Some(inner) = PathBuilder::from_circle(cx, cy, 2.5) {
        paint.set_color(Color::from_rgba8(0x50, 0xfa, 0x7b, 255));
        pixmap.fill_path(
            &inner,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }
}

fn draw_checkbox_indicator(pixmap: &mut PixmapMut, cx: f32, cy: f32, checked: bool) {
    let mut paint = Paint::default();

    if let Some(box_path) = build_rounded_rect(cx - 5.0, cy - 5.0, 10.0, 10.0, 2.5) {
        if checked {
            paint.set_color(Color::from_rgba8(0xbd, 0x93, 0xf9, 240));
            pixmap.fill_path(
                &box_path,
                &paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            let st = Stroke {
                width: 1.3,
                ..Default::default()
            };
            paint.set_color(Color::from_rgba8(0x62, 0x72, 0xa4, 220));
            pixmap.stroke_path(&box_path, &paint, &st, Transform::identity(), None);
        }
    }

    if checked {
        let mut pb = PathBuilder::new();
        pb.move_to(cx - 2.6, cy + 0.2);
        pb.line_to(cx - 0.6, cy + 2.3);
        pb.line_to(cx + 3.0, cy - 2.2);
        if let Some(check) = pb.finish() {
            let st = Stroke {
                width: 1.6,
                line_cap: LineCap::Round,
                line_join: LineJoin::Round,
                ..Default::default()
            };
            paint.set_color(Color::from_rgba8(0x1e, 0x1f, 0x29, 255));
            pixmap.stroke_path(&check, &paint, &st, Transform::identity(), None);
        }
    }
}

fn build_rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let r = r.min(w * 0.5).min(h * 0.5);
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Draws crisp 5x7 bitmap text onto `pixmap`.
fn draw_text(pixmap: &mut PixmapMut, text: &str, mut x: f32, y: f32, color: Color) {
    let mut paint = Paint::default();
    paint.set_color(color);

    for ch in text.chars() {
        let glyph = glyph_5x7(ch);
        // Descender glyphs are stored shifted up so their 7 rows hold body + tail; drop them onto the baseline
        let dy = if matches!(ch, 'g' | 'p' | 'q' | 'y') {
            2.0
        } else {
            0.0
        };
        for (row_idx, &bits) in glyph.iter().enumerate() {
            for col_idx in 0..5 {
                if (bits >> (4 - col_idx)) & 1 == 1 {
                    let px = x + col_idx as f32;
                    let py = y + dy + row_idx as f32;
                    if let Some(rect) = Rect::from_xywh(px, py, 1.0, 1.0) {
                        pixmap.fill_rect(rect, &paint, Transform::identity(), None);
                    }
                }
            }
        }
        x += 6.0;
    }
}

const fn glyph_5x7(ch: char) -> [u8; 7] {
    match ch {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01110, 0b10001, 0b10000, 0b10000, 0b10000, 0b10001, 0b01110,
        ],
        'D' => [
            0b11100, 0b10010, 0b10001, 0b10001, 0b10001, 0b10010, 0b11100,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01110, 0b10001, 0b10000, 0b10111, 0b10001, 0b10001, 0b01110,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b01110, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b11011, 0b10001,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        'a' => [
            0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111,
        ],
        'b' => [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'c' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b10000, 0b10001, 0b01110,
        ],
        'd' => [
            0b00001, 0b00001, 0b01111, 0b10001, 0b10001, 0b10001, 0b01111,
        ],
        'e' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110,
        ],
        'f' => [
            0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000,
        ],
        'g' => [
            0b01111, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110,
        ],
        'h' => [
            0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'i' => [
            0b00100, 0b00000, 0b01100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'j' => [
            0b00010, 0b00000, 0b00110, 0b00010, 0b00010, 0b10010, 0b01100,
        ],
        'k' => [
            0b10000, 0b10000, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010,
        ],
        'l' => [
            0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        'm' => [
            0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001,
        ],
        'n' => [
            0b00000, 0b00000, 0b11110, 0b10001, 0b10001, 0b10001, 0b10001,
        ],
        'o' => [
            0b00000, 0b00000, 0b01110, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'p' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000,
        ],
        'q' => [
            0b01111, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001,
        ],
        'r' => [
            0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000,
        ],
        's' => [
            0b00000, 0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110,
        ],
        't' => [
            0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01001, 0b00110,
        ],
        'u' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b10011, 0b01101,
        ],
        'v' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'w' => [
            0b00000, 0b00000, 0b10001, 0b10001, 0b10101, 0b10101, 0b01010,
        ],
        'x' => [
            0b00000, 0b00000, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001,
        ],
        'y' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b01111, 0b00001, 0b01110,
        ],
        'z' => [
            0b00000, 0b00000, 0b11111, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110,
        ],
        '6' => [
            0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100,
        ],
        '.' => [
            0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100,
        ],
        '!' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00000, 0b00100,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '&' => [
            0b01100, 0b10010, 0b10100, 0b01000, 0b10101, 0b10010, 0b01101,
        ],
        '-' => [
            0b00000, 0b00000, 0b00000, 0b11111, 0b00000, 0b00000, 0b00000,
        ],
        '/' => [
            0b00001, 0b00010, 0b00100, 0b00100, 0b01000, 0b10000, 0b00000,
        ],
        ':' => [
            0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000,
        ],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> TrayState {
        TrayState {
            coat: CoatStyle::Calico,
            behaviors: BehaviorConfig::default(),
            scale: 1.0,
            sleeping: false,
            hidden: false,
            start_on_login: false,
            feed_preset: None,
            feed_remaining_secs: None,
        }
    }

    fn actions(state: &TrayState) -> Vec<TrayAction> {
        let (_, hits) = compute_layout(&build_rows(state));
        hits.into_iter().map(|h| h.action).collect()
    }

    fn label_of(state: &TrayState, action: TrayAction) -> String {
        build_rows(state)
            .into_iter()
            .find_map(|row| match row {
                MenuRow::Item {
                    label, action: a, ..
                } if a == action => Some(label.into_owned()),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn menu_layout_covers_all_actions_and_renders_cleanly() {
        let mut state = test_state();
        state.feed_preset = Some(FeedPreset::Mins30);
        state.feed_remaining_secs = Some(1795);
        let rows = build_rows(&state);
        let (menu_h, hits) = compute_layout(&rows);

        // coat + size + temperament + 5 feed presets + stop + 2 toggles + show + login + about + quit
        assert_eq!(hits.len(), 15);
        // Must fit comfortably on a 768 px tall screen with a panel
        assert!(menu_h > 200 && menu_h < 400, "menu_h = {menu_h}");

        // Verify every hit region is strictly ordered and non-overlapping
        for pair in hits.windows(2) {
            assert!(pair[0].y_top < pair[0].y_bottom);
            assert!(pair[0].y_bottom <= pair[1].y_top);
        }

        assert!(matches!(hits[0].action, TrayAction::SetCoat(_)));
        assert!(matches!(hits[1].action, TrayAction::SetScale(_)));
        assert!(matches!(hits[2].action, TrayAction::SetTemperament(_)));
        assert_eq!(
            hits[3].action,
            TrayAction::SelectFeedPreset(FeedPreset::Mins15)
        );
        assert!(hits[3].close_on_click);
        assert_eq!(hits[8].action, TrayAction::StopFeeding);
        assert!(hits[8].close_on_click);
        assert_eq!(hits[12].action, TrayAction::ToggleStartOnLogin);
        assert_eq!(hits[13].action, TrayAction::OpenAbout);
        assert!(hits[13].close_on_click);
        assert_eq!(hits[14].action, TrayAction::Quit);
        assert!(hits[14].close_on_click);
        for h in &hits {
            if matches!(
                h.action,
                TrayAction::SetCoat(_)
                    | TrayAction::SetScale(_)
                    | TrayAction::SetTemperament(_)
                    | TrayAction::ToggleHidden
                    | TrayAction::ToggleStartOnLogin
            ) {
                assert!(
                    !h.close_on_click,
                    "{:?} should keep the menu open",
                    h.action
                );
            }
        }
        let mut pixmap = Pixmap::new(MENU_WIDTH as u32, menu_h as u32).unwrap();
        render_menu_pixmap(&rows, Some(3), &mut pixmap.as_mut());
    }

    #[test]
    fn stop_feeding_row_only_while_feeding() {
        let mut state = test_state();
        assert!(!actions(&state).contains(&TrayAction::StopFeeding));
        state.feed_preset = Some(FeedPreset::Indefinite);
        assert!(actions(&state).contains(&TrayAction::StopFeeding));

        let active = Some(FeedPreset::Hour1);
        assert_eq!(
            normalize_tray_action(TrayAction::SelectFeedPreset(FeedPreset::Hour1), active),
            TrayAction::StopFeeding
        );
        assert_eq!(
            normalize_tray_action(TrayAction::SelectFeedPreset(FeedPreset::Mins15), active),
            TrayAction::SelectFeedPreset(FeedPreset::Mins15)
        );
    }

    #[test]
    fn coat_row_cycles_through_every_coat() {
        let mut state = test_state();
        let mut seen = Vec::new();
        for _ in 0..CoatStyle::ALL.len() {
            seen.push(state.coat);
            let next = actions(&state)
                .into_iter()
                .find_map(|a| match a {
                    TrayAction::SetCoat(c) => Some(c),
                    _ => None,
                })
                .unwrap();
            state.coat = next;
        }
        assert_eq!(state.coat, CoatStyle::Calico);
        assert_eq!(seen, CoatStyle::ALL.to_vec());
    }

    #[test]
    fn temperament_row_cycles_through_every_temperament() {
        let mut state = test_state();
        let mut seen = Vec::new();
        for _ in 0..Temperament::ALL.len() {
            let current = state.behaviors.temperament;
            seen.push(current);
            let next = actions(&state)
                .into_iter()
                .find_map(|a| match a {
                    TrayAction::SetTemperament(t) => Some(t),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                label_of(&state, TrayAction::SetTemperament(next)),
                format!("Temperament: {}", current.label())
            );
            state.behaviors.temperament = next;
        }
        assert_eq!(state.behaviors.temperament, Temperament::Balanced);
        assert_eq!(seen, Temperament::ALL.to_vec());
    }

    #[test]
    fn size_row_cycles_presets_and_shows_custom_scale() {
        let mut state = test_state();
        let next_scale = |state: &TrayState| {
            actions(state)
                .into_iter()
                .find_map(|a| match a {
                    TrayAction::SetScale(s) => Some(s),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(
            label_of(&state, TrayAction::SetScale(1.35)),
            "Size: Normal 1.0x"
        );
        assert_eq!(next_scale(&state), 1.35);
        state.scale = 1.75;
        assert_eq!(next_scale(&state), 0.80);

        state.scale = 1.5;
        assert_eq!(
            label_of(&state, TrayAction::SetScale(1.75)),
            "Size: Custom 1.50x"
        );
        assert_eq!(next_scale(&state), 1.75);
        state.scale = 3.0;
        assert_eq!(next_scale(&state), 0.80);
    }

    #[test]
    fn about_row_shows_version_and_links_to_repository() {
        let label = label_of(&test_state(), TrayAction::OpenAbout);
        assert_eq!(
            label,
            format!("About Catsor v{}", env!("CARGO_PKG_VERSION"))
        );
        // Every character must exist in the bitmap font (unknown glyphs render blank)
        assert!(label.chars().all(|c| c == ' ' || glyph_5x7(c) != [0; 7]));
        assert!(ABOUT_URL.starts_with("https://"));
    }

    #[test]
    fn show_cat_checkbox_reflects_visibility() {
        let mut state = test_state();
        let checked = |state: &TrayState| {
            build_rows(state).into_iter().find_map(|row| match row {
                MenuRow::Item {
                    label,
                    indicator: Indicator::Checkbox(c),
                    action: TrayAction::ToggleHidden,
                    ..
                } => {
                    assert_eq!(label, "Show Cat");
                    Some(c)
                }
                _ => None,
            })
        };
        assert_eq!(checked(&state), Some(true));
        state.hidden = true;
        assert_eq!(checked(&state), Some(false));
    }

    #[test]
    fn tray_icon_never_clips_at_edges_across_coats_and_states() {
        for &(w, h) in &[(24u32, 24u32), (24u32, 28u32), (28u32, 28u32)] {
            let mut pixmap = Pixmap::new(w, h).unwrap();
            for coat in CoatStyle::ALL {
                for &(sleeping, hidden) in &[(false, false), (true, false), (false, true)] {
                    crate::render::render_tray_icon(
                        coat,
                        sleeping,
                        hidden,
                        false,
                        &mut pixmap.as_mut(),
                    );
                    let data = pixmap.data();
                    // Outer 1px border on all 4 sides must remain completely transparent (no clipped ears/chin)
                    for x in 0..w {
                        let top_a = data[(x as usize) * 4 + 3];
                        let bot_a = data[((h - 1) * w + x) as usize * 4 + 3];
                        assert_eq!(top_a, 0, "top edge clipped for {coat:?} at {w}x{h}");
                        assert_eq!(bot_a, 0, "bottom edge clipped for {coat:?} at {w}x{h}");
                    }
                    for y in 0..h {
                        let left_a = data[(y * w) as usize * 4 + 3];
                        let right_a = data[(y * w + (w - 1)) as usize * 4 + 3];
                        assert_eq!(left_a, 0, "left edge clipped for {coat:?} at {w}x{h}");
                        assert_eq!(right_a, 0, "right edge clipped for {coat:?} at {w}x{h}");
                    }
                }
            }
        }
    }
}
