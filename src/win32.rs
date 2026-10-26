use crate::Args;
use crate::autostart;
use crate::cat::CatSim;
use crate::prefs::UserPrefs;
use crate::render::{
    bowl_window_dim, cat_window_dim, render_bowl_overlay, render_cat, render_tray_icon,
};
use crate::tray::{
    ABOUT_URL, HitEntry, MENU_WIDTH, TrayAction, TrayState, build_rows, compute_layout,
    hit_test_entries, normalize_tray_action, render_menu_pixmap, rgba_to_bgra,
};
use core::cell::RefCell;
use core::ffi::c_void;
use core::mem::{size_of, zeroed};
use core::ptr::{null, null_mut};
use glam::Vec2;
use std::collections::VecDeque;
use std::time::{Duration, Instant};
use tiny_skia::{Mask, Pixmap};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateBitmap,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetMonitorInfoW,
    HBITMAP, HDC, HGDIOBJ, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint, SelectObject,
};
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Power::{
    ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED, SetThreadExecutionState,
};
use windows_sys::Win32::UI::Controls::WM_MOUSELEAVE;
use windows_sys::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, TME_LEAVE, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE, VK_LBUTTON,
    VK_RBUTTON,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
    Shell_NotifyIconW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateIconIndirect, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos, GetSystemMetrics,
    GetWindowLongPtrW, HICON, HWND_TOPMOST, ICONINFO, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE,
    PeekMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW, SM_CXSCREEN,
    SM_CXSMICON, SM_CYSCREEN, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, SW_SHOWNORMAL, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, ULW_ALPHA, UpdateLayeredWindow, WA_INACTIVE, WM_ACTIVATE, WM_APP, WM_CLOSE,
    WM_DESTROY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_QUIT, WM_RBUTTONDOWN,
    WM_RBUTTONUP, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

const WM_TRAYICON: u32 = WM_APP + 1;
const TRAY_ICON_ID: u32 = 1;

pub(crate) fn to_wide_null(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(core::iter::once(0)).collect()
}

fn is_key_down(vk: u16) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

struct WinKeepAwake {
    active: bool,
}

impl WinKeepAwake {
    fn new() -> Self {
        Self { active: false }
    }

    fn set_active(&mut self, active: bool) {
        if self.active == active {
            return;
        }
        self.active = active;
        // ES_CONTINUOUS keeps the request in force until it is cleared, so no periodic refresh is needed.
        unsafe {
            if active {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
            } else {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }

    // Only the execution-state request; never synthesizes pointer or key input.
    fn tick(&mut self, feeding: bool) {
        self.set_active(feeding);
    }
}

impl Drop for WinKeepAwake {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
            }
        }
    }
}

/// Reusable 32-bpp top-down premultiplied ARGB DIB section + memory DC for `UpdateLayeredWindow`.
struct DibSurface {
    dc: HDC,
    hbmp: HBITMAP,
    old_obj: HGDIOBJ,
    bits: *mut u8,
    width: i32,
    height: i32,
}

impl DibSurface {
    fn new(width: u32, height: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let w = width.max(1) as i32;
        let h = height.max(1) as i32;
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return Err("CreateCompatibleDC failed".into());
            }

            let mut bmi: BITMAPINFO = zeroed();
            bmi.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = w;
            // Negative height creates a top-down DIB (origin at top-left, matching tiny-skia)
            bmi.bmiHeader.biHeight = -h;
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let mut bits_ptr: *mut c_void = null_mut();
            let hbmp = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits_ptr, null_mut(), 0);
            if hbmp.is_null() || bits_ptr.is_null() {
                DeleteDC(dc);
                return Err("CreateDIBSection failed".into());
            }

            let old_obj = SelectObject(dc, hbmp);
            Ok(Self {
                dc,
                hbmp,
                old_obj,
                bits: bits_ptr as *mut u8,
                width: w,
                height: h,
            })
        }
    }

    fn write_pixmap(&mut self, pixmap: &Pixmap) {
        let byte_len = (self.width as usize) * (self.height as usize) * 4;
        let src = pixmap.data();
        if src.len() == byte_len {
            let dst = unsafe { core::slice::from_raw_parts_mut(self.bits, byte_len) };
            rgba_to_bgra(src, dst);
        }
    }

    fn present_layered(&self, hwnd: HWND, x: i32, y: i32) {
        let pt_dst = POINT { x, y };
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let pt_src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        unsafe {
            UpdateLayeredWindow(
                hwnd,
                null_mut(),
                &pt_dst,
                &size,
                self.dc,
                &pt_src,
                0,
                &blend,
                ULW_ALPHA,
            );
        }
    }

    fn deselect_bitmap(&mut self) {
        unsafe {
            if !self.dc.is_null() && !self.old_obj.is_null() {
                SelectObject(self.dc, self.old_obj);
                self.old_obj = null_mut();
            }
        }
    }
}

impl Drop for DibSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.dc.is_null() {
                if !self.old_obj.is_null() {
                    SelectObject(self.dc, self.old_obj);
                }
                if !self.hbmp.is_null() {
                    DeleteObject(self.hbmp);
                }
                DeleteDC(self.dc);
            }
        }
    }
}

fn create_tray_hicon(state: &TrayState) -> Option<HICON> {
    let icon_px = unsafe { GetSystemMetrics(SM_CXSMICON).clamp(24, 64) as u32 };
    let mut pixmap = Pixmap::new(icon_px, icon_px)?;
    render_tray_icon(
        state.coat,
        state.sleeping,
        state.hidden,
        false,
        &mut pixmap.as_mut(),
    );

    let mut dib = DibSurface::new(icon_px, icon_px).ok()?;
    dib.write_pixmap(&pixmap);
    // CreateIconIndirect requires that hbmColor not be selected into an active DC.
    dib.deselect_bitmap();

    unsafe {
        let hbm_mask = CreateBitmap(icon_px as i32, icon_px as i32, 1, 1, null());
        if hbm_mask.is_null() {
            return None;
        }
        let icon_info = ICONINFO {
            fIcon: 1,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: hbm_mask,
            hbmColor: dib.hbmp,
        };
        let hicon = CreateIconIndirect(&icon_info);
        DeleteObject(hbm_mask);
        if hicon.is_null() { None } else { Some(hicon) }
    }
}

struct MenuWndState {
    menu_hwnd: HWND,
    menu_w: u16,
    menu_h: u16,
    menu_x: i32,
    menu_y: i32,
    // Cursor position the menu was opened at
    menu_anchor: POINT,
    menu_open: bool,
    tracking_mouse: bool,
    hovered_idx: Option<usize>,
    hit_entries: Vec<HitEntry>,
    last_state: TrayState,
    actions: VecDeque<TrayAction>,
    taskbar_created_msg: u32,
    readd_tray_requested: bool,
    dib: Option<DibSurface>,
}

impl MenuWndState {
    fn redraw_menu(&mut self) {
        let rows = build_rows(&self.last_state);
        let (menu_h, hit_entries) = compute_layout(&rows);
        let resized = menu_h != self.menu_h;
        self.menu_h = menu_h;
        self.hit_entries = hit_entries;
        // Rows can appear/disappear while open (e.g. "Stop Feeding" when the timer runs out);
        // UpdateLayeredWindow below applies the new position and size
        if resized && self.menu_open {
            self.hovered_idx = None;
            self.place_menu();
        }

        let w = self.menu_w as u32;
        let h = self.menu_h as u32;
        let Some(mut pixmap) = Pixmap::new(w, h) else {
            return;
        };
        render_menu_pixmap(&rows, self.hovered_idx, &mut pixmap.as_mut());

        let needs_new_dib = self
            .dib
            .as_ref()
            .is_none_or(|d| d.width != w as i32 || d.height != h as i32);
        if needs_new_dib {
            self.dib = DibSurface::new(w, h).ok();
        }
        if let Some(dib) = &mut self.dib {
            dib.write_pixmap(&pixmap);
            dib.present_layered(self.menu_hwnd, self.menu_x, self.menu_y);
        }
    }

    fn prepare_open_at_cursor(&mut self) -> (HWND, i32, i32, i32, i32) {
        let rows = build_rows(&self.last_state);
        let (menu_h, hit_entries) = compute_layout(&rows);
        self.menu_h = menu_h;
        self.hit_entries = hit_entries;
        self.hovered_idx = None;

        let mut pt = POINT { x: 0, y: 0 };
        unsafe {
            GetCursorPos(&mut pt);
        }
        self.menu_anchor = pt;
        self.place_menu();

        self.menu_open = true;
        self.redraw_menu();
        (
            self.menu_hwnd,
            self.menu_x,
            self.menu_y,
            self.menu_w as i32,
            self.menu_h as i32,
        )
    }

    /// Places the menu centred on `menu_anchor`: below it, or above it if it doesn't fit,
    /// clamped to the monitor's work area.
    fn place_menu(&mut self) {
        let pt = self.menu_anchor;
        let work_rect = monitor_work_area(pt);
        let menu_w = self.menu_w as i32;
        let menu_h = self.menu_h as i32;

        let min_x = work_rect.left + 8;
        let max_x = (work_rect.right - menu_w - 8).max(min_x);
        self.menu_x = (pt.x - menu_w / 2).clamp(min_x, max_x);

        let below_y = pt.y + 12;
        let above_y = pt.y - menu_h - 12;
        let min_y = work_rect.top + 8;
        let max_y = (work_rect.bottom - menu_h - 8).max(min_y);
        self.menu_y = if below_y + menu_h <= work_rect.bottom - 8 {
            below_y.clamp(min_y, max_y)
        } else {
            above_y.clamp(min_y, max_y)
        };
    }

    fn mark_closed(&mut self) -> Option<HWND> {
        if !self.menu_open {
            return None;
        }
        self.menu_open = false;
        self.hovered_idx = None;
        self.tracking_mouse = false;
        Some(self.menu_hwnd)
    }
}

fn show_menu_window(hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
        SetForegroundWindow(hwnd);
    }
}

fn hide_menu_window(hwnd: Option<HWND>) {
    if let Some(h) = hwnd {
        unsafe {
            ShowWindow(h, SW_HIDE);
        }
    }
}

fn monitor_work_area(pt: POINT) -> RECT {
    unsafe {
        let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        if !hmon.is_null() {
            let mut mi: MONITORINFO = zeroed();
            mi.cbSize = size_of::<MONITORINFO>() as u32;
            if GetMonitorInfoW(hmon, &mut mi) != 0 {
                return mi.rcWork;
            }
        }
        RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN).max(800),
            bottom: GetSystemMetrics(SM_CYSCREEN).max(600),
        }
    }
}

struct WinTrayController {
    menu_hwnd: HWND,
    state_ptr: *const RefCell<MenuWndState>,
    current_icon: HICON,
    tray_added: bool,
}

impl WinTrayController {
    fn new(
        hinstance: HINSTANCE,
        menu_class_name: &[u16],
        initial_state: TrayState,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let rows = build_rows(&initial_state);
        let (menu_h, hit_entries) = compute_layout(&rows);
        let taskbar_created_msg =
            unsafe { RegisterWindowMessageW(to_wide_null("TaskbarCreated").as_ptr()) };

        let window_title = to_wide_null("catsor-menu");
        let menu_hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                menu_class_name.as_ptr(),
                window_title.as_ptr(),
                WS_POPUP,
                0,
                0,
                MENU_WIDTH as i32,
                menu_h as i32,
                null_mut(),
                null_mut(),
                hinstance,
                null(),
            )
        };
        if menu_hwnd.is_null() {
            return Err("Failed to create Win32 popup menu window".into());
        }

        let state_box = Box::new(RefCell::new(MenuWndState {
            menu_hwnd,
            menu_w: MENU_WIDTH,
            menu_h,
            menu_x: 0,
            menu_y: 0,
            menu_anchor: POINT { x: 0, y: 0 },
            menu_open: false,
            tracking_mouse: false,
            hovered_idx: None,
            hit_entries,
            last_state: initial_state,
            actions: VecDeque::new(),
            taskbar_created_msg,
            readd_tray_requested: false,
            dib: None,
        }));
        let state_ptr = Box::into_raw(state_box) as *const RefCell<MenuWndState>;
        unsafe {
            SetWindowLongPtrW(menu_hwnd, GWLP_USERDATA, state_ptr as isize);
        }

        let current_icon = create_tray_hicon(&initial_state).unwrap_or(null_mut());
        let mut controller = Self {
            menu_hwnd,
            state_ptr,
            current_icon,
            tray_added: false,
        };
        controller.add_or_modify_tray_icon(NIM_ADD);
        Ok(controller)
    }

    fn add_or_modify_tray_icon(&mut self, msg: u32) {
        unsafe {
            let mut nid: NOTIFYICONDATAW = zeroed();
            nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            nid.hWnd = self.menu_hwnd;
            nid.uID = TRAY_ICON_ID;
            nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            nid.uCallbackMessage = WM_TRAYICON;
            nid.hIcon = self.current_icon;

            let tip = to_wide_null("Catsor - Desktop Cat Companion");
            let copy_len = tip.len().min(nid.szTip.len());
            nid.szTip[..copy_len].copy_from_slice(&tip[..copy_len]);

            if Shell_NotifyIconW(msg, &nid) != 0 {
                self.tray_added = true;
            }
        }
    }

    fn state_cell(&self) -> &RefCell<MenuWndState> {
        unsafe { &*self.state_ptr }
    }

    fn open_menu_window(&self) -> Option<HWND> {
        let st = self.state_cell().borrow();
        st.menu_open.then_some(self.menu_hwnd)
    }

    fn pop_action(&mut self) -> Option<TrayAction> {
        self.state_cell().borrow_mut().actions.pop_front()
    }

    fn close_menu(&mut self) {
        let hide_hwnd = self.state_cell().borrow_mut().mark_closed();
        hide_menu_window(hide_hwnd);
    }

    fn sync_state(&mut self, state: TrayState) {
        let (readd, icon_changed, should_redraw_menu) = {
            let mut st = self.state_cell().borrow_mut();
            let readd = st.readd_tray_requested;
            st.readd_tray_requested = false;

            let icon_changed = state.icon_differs(&st.last_state);
            let menu_changed = state != st.last_state;
            st.last_state = state;
            (readd, icon_changed, st.menu_open && menu_changed)
        };

        if readd {
            self.add_or_modify_tray_icon(NIM_ADD);
        }

        if icon_changed && let Some(new_icon) = create_tray_hicon(&state) {
            let old_icon = self.current_icon;
            self.current_icon = new_icon;
            if self.tray_added {
                self.add_or_modify_tray_icon(NIM_MODIFY);
            } else {
                self.add_or_modify_tray_icon(NIM_ADD);
            }
            if !old_icon.is_null() {
                unsafe {
                    DestroyIcon(old_icon);
                }
            }
        }

        if should_redraw_menu {
            self.state_cell().borrow_mut().redraw_menu();
        }
    }
}

impl Drop for WinTrayController {
    fn drop(&mut self) {
        unsafe {
            if self.tray_added {
                let mut nid: NOTIFYICONDATAW = zeroed();
                nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
                nid.hWnd = self.menu_hwnd;
                nid.uID = TRAY_ICON_ID;
                Shell_NotifyIconW(NIM_DELETE, &nid);
            }
            if !self.current_icon.is_null() {
                DestroyIcon(self.current_icon);
            }
            if !self.menu_hwnd.is_null() {
                SetWindowLongPtrW(self.menu_hwnd, GWLP_USERDATA, 0);
                DestroyWindow(self.menu_hwnd);
            }
            if !self.state_ptr.is_null() {
                drop(Box::from_raw(self.state_ptr as *mut RefCell<MenuWndState>));
            }
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

unsafe extern "system" fn menu_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr =
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<MenuWndState> };
    if state_ptr.is_null() {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }
    let state_cell = unsafe { &*state_ptr };

    let taskbar_created_msg = state_cell.borrow().taskbar_created_msg;
    if taskbar_created_msg != 0 && msg == taskbar_created_msg {
        state_cell.borrow_mut().readd_tray_requested = true;
        return 0;
    }

    match msg {
        WM_TRAYICON => {
            let event_code = (lparam as u32) & 0xFFFF;
            if event_code == WM_LBUTTONUP || event_code == WM_RBUTTONUP {
                let is_open = state_cell.borrow().menu_open;
                if is_open {
                    let hide_hwnd = state_cell.borrow_mut().mark_closed();
                    hide_menu_window(hide_hwnd);
                } else {
                    let (m_hwnd, x, y, w, h) = state_cell.borrow_mut().prepare_open_at_cursor();
                    show_menu_window(m_hwnd, x, y, w, h);
                }
            }
            0
        }
        WM_MOUSEMOVE => {
            let need_track = {
                let mut st = state_cell.borrow_mut();
                if st.menu_open {
                    let lx = (lparam & 0xFFFF) as i16;
                    let ly = ((lparam >> 16) & 0xFFFF) as i16;
                    let new_hover = hit_test_entries(&st.hit_entries, st.menu_w, lx, ly);
                    if new_hover != st.hovered_idx {
                        st.hovered_idx = new_hover;
                        st.redraw_menu();
                    }
                    if !st.tracking_mouse {
                        st.tracking_mouse = true;
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            };
            if need_track {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                unsafe {
                    TrackMouseEvent(&mut tme);
                }
            }
            0
        }
        WM_MOUSELEAVE => {
            let mut st = state_cell.borrow_mut();
            st.tracking_mouse = false;
            if st.menu_open && st.hovered_idx.is_some() {
                st.hovered_idx = None;
                st.redraw_menu();
            }
            0
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN => {
            let hide_hwnd = {
                let mut st = state_cell.borrow_mut();
                if st.menu_open {
                    let lx = (lparam & 0xFFFF) as i16;
                    let ly = ((lparam >> 16) & 0xFFFF) as i16;
                    if lx < 0 || ly < 0 || lx >= st.menu_w as i16 || ly >= st.menu_h as i16 {
                        st.mark_closed()
                    } else if let Some(idx) = hit_test_entries(&st.hit_entries, st.menu_w, lx, ly) {
                        let entry = st.hit_entries[idx];
                        st.actions.push_back(entry.action);
                        if entry.close_on_click {
                            st.mark_closed()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            hide_menu_window(hide_hwnd);
            0
        }
        WM_KEYDOWN => {
            if (wparam as u16) == VK_ESCAPE {
                let hide_hwnd = state_cell.borrow_mut().mark_closed();
                hide_menu_window(hide_hwnd);
            }
            0
        }
        WM_ACTIVATE => {
            if (wparam as u32 & 0xFFFF) == WA_INACTIVE {
                let hide_hwnd = state_cell.borrow_mut().mark_closed();
                hide_menu_window(hide_hwnd);
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_CLOSE => {
            let hide_hwnd = state_cell.borrow_mut().mark_closed();
            hide_menu_window(hide_hwnd);
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// The binary uses the GUI subsystem, so it starts without a console. When it was started from
/// a terminal, attach to that terminal's console so `--help` and error messages still show up.
/// Does nothing when launched from Explorer (there is no parent console).
pub fn attach_parent_console() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

pub fn run(args: &Args, prefs: UserPrefs) -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    let hinstance: HINSTANCE = unsafe { GetModuleHandleW(null()) };
    let cursor = unsafe { LoadCursorW(null_mut(), IDC_ARROW) };

    let overlay_class_name = to_wide_null("catsor_overlay_cls");
    let overlay_wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(overlay_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: null_mut(),
        hCursor: cursor,
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: overlay_class_name.as_ptr(),
        hIconSm: null_mut(),
    };
    unsafe {
        RegisterClassExW(&overlay_wc);
    }

    let menu_class_name = to_wide_null("catsor_menu_cls");
    let menu_wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(menu_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinstance,
        hIcon: null_mut(),
        hCursor: cursor,
        hbrBackground: null_mut(),
        lpszMenuName: null(),
        lpszClassName: menu_class_name.as_ptr(),
        hIconSm: null_mut(),
    };
    unsafe {
        RegisterClassExW(&menu_wc);
    }

    let mut win_dim = cat_window_dim(prefs.scale);

    let mut init_pt = POINT { x: 400, y: 400 };
    unsafe {
        GetCursorPos(&mut init_pt);
    }
    let start_cursor = Vec2::new(init_pt.x as f32, init_pt.y as f32);

    let mut cat = CatSim::new(start_cursor, prefs.scale, prefs.coat);
    cat.behaviors = prefs.behaviors;
    let init_center = cat.window_center();
    let mut win_x = (init_center.x - (win_dim as f32) * 0.5).round() as i32;
    let mut win_y = (init_center.y - (win_dim as f32) * 0.5).round() as i32;

    let overlay_title = to_wide_null("Catsor");
    let cat_hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            overlay_class_name.as_ptr(),
            overlay_title.as_ptr(),
            WS_POPUP,
            win_x,
            win_y,
            win_dim as i32,
            win_dim as i32,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        )
    };
    if cat_hwnd.is_null() {
        return Err("Failed to create Win32 layered overlay window".into());
    }

    unsafe {
        ShowWindow(cat_hwnd, SW_SHOWNOACTIVATE);
    }
    let mut bowl_dim = bowl_window_dim(prefs.scale);
    let bowl_hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            overlay_class_name.as_ptr(),
            overlay_title.as_ptr(),
            WS_POPUP,
            win_x,
            win_y,
            bowl_dim as i32,
            bowl_dim as i32,
            null_mut(),
            null_mut(),
            hinstance,
            null(),
        )
    };
    let mut bowl_visible = false;
    let mut bowl_click_armed = false;
    let mut bowl_pixmap =
        Pixmap::new(bowl_dim as u32, bowl_dim as u32).ok_or("Failed to allocate bowl Pixmap")?;
    let mut bowl_dib = DibSurface::new(bowl_dim as u32, bowl_dim as u32)?;

    let mut cat_hidden = false;
    let mut start_on_login = autostart::is_enabled();
    let mut keep_awake = WinKeepAwake::new();
    let mut tray = if args.no_tray {
        None
    } else {
        let initial_state = TrayState::from_cat(&cat, cat_hidden, start_on_login);
        WinTrayController::new(hinstance, &menu_class_name, initial_state).ok()
    };

    let mut pixmap =
        Pixmap::new(win_dim as u32, win_dim as u32).ok_or("Failed to allocate Pixmap")?;
    let mut clip_mask =
        Mask::new(win_dim as u32, win_dim as u32).ok_or("Failed to allocate Mask")?;
    let mut dib = DibSurface::new(win_dim as u32, win_dim as u32)?;

    let frame_duration = Duration::from_secs_f64(1.0 / (args.fps.max(15) as f64));
    let mut last_tick = Instant::now();
    let mut raise_counter = 0u32;

    loop {
        let now = Instant::now();
        let dt = now.duration_since(last_tick).as_secs_f32();
        last_tick = now;

        // Pump pending Win32 messages
        unsafe {
            let mut msg: MSG = zeroed();
            while PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    if let Some(tray_ctrl) = &mut tray {
                        tray_ctrl.close_menu();
                    }
                    DestroyWindow(cat_hwnd);
                    return Ok(());
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        let mut needs_raise = false;
        while let Some(action) = tray.as_mut().and_then(|t| t.pop_action()) {
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
                    cat.stop_feeding();
                    bowl_click_armed = false;
                    keep_awake.set_active(false);
                }
                TrayAction::SelectFeedPreset(preset) => {
                    if cat_hidden {
                        cat_hidden = false;
                        unsafe {
                            ShowWindow(cat_hwnd, SW_SHOWNOACTIVATE);
                        }
                        needs_raise = true;
                    }
                    let mut pt = POINT { x: 0, y: 0 };
                    unsafe {
                        GetCursorPos(&mut pt);
                    }
                    cat.begin_bowl_placement(preset, Vec2::new(pt.x as f32, pt.y as f32));
                    bowl_click_armed = false;
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
                        dib = DibSurface::new(win_dim as u32, win_dim as u32)?;
                        needs_raise = true;
                    }
                    let new_bowl_dim = bowl_window_dim(cat.scale);
                    if new_bowl_dim != bowl_dim {
                        bowl_dim = new_bowl_dim;
                        bowl_pixmap = Pixmap::new(bowl_dim as u32, bowl_dim as u32)
                            .ok_or("Failed to reallocate bowl Pixmap")?;
                        bowl_dib = DibSurface::new(bowl_dim as u32, bowl_dim as u32)?;
                    }
                }
                TrayAction::ToggleHidden => {
                    cat_hidden = !cat_hidden;
                    unsafe {
                        if cat_hidden {
                            ShowWindow(cat_hwnd, SW_HIDE);
                        } else {
                            ShowWindow(cat_hwnd, SW_SHOWNOACTIVATE);
                            needs_raise = true;
                        }
                    }
                }
                TrayAction::ToggleStartOnLogin => {
                    if let Err(e) = autostart::set_enabled(!start_on_login) {
                        eprintln!("catsor: failed to update start on login: {e}");
                    }
                    start_on_login = autostart::is_enabled();
                }
                TrayAction::OpenAbout => {
                    let verb = to_wide_null("open");
                    let url = to_wide_null(ABOUT_URL);
                    unsafe {
                        ShellExecuteW(
                            null_mut(),
                            verb.as_ptr(),
                            url.as_ptr(),
                            null(),
                            null(),
                            SW_SHOWNORMAL,
                        );
                    }
                }
                TrayAction::Quit => {
                    if let Some(tray_ctrl) = &mut tray {
                        tray_ctrl.close_menu();
                    }
                    unsafe {
                        if !bowl_hwnd.is_null() {
                            DestroyWindow(bowl_hwnd);
                        }
                        DestroyWindow(cat_hwnd);
                    }
                    return Ok(());
                }
            }
        }

        if !cat_hidden || cat.feeding_preset().is_some() {
            let mut pt = POINT { x: 0, y: 0 };
            if unsafe { GetCursorPos(&mut pt) } != 0 {
                let cursor_pos = Vec2::new(pt.x as f32, pt.y as f32);
                if cat.is_carrying_bowl() {
                    if !is_key_down(VK_LBUTTON) {
                        bowl_click_armed = true;
                    } else if bowl_click_armed {
                        bowl_click_armed = false;
                        cat.place_bowl(cursor_pos);
                    }
                    if is_key_down(VK_RBUTTON) || is_key_down(VK_ESCAPE) {
                        bowl_click_armed = false;
                        cat.stop_feeding();
                    }
                }
                cat.update(cursor_pos, dt);
                keep_awake.tick(cat.keeps_awake());
            }
        } else {
            keep_awake.set_active(false);
        }

        raise_counter = raise_counter.wrapping_add(1);
        if raise_counter.is_multiple_of(120) {
            needs_raise = true;
        }

        if let Some(tray_ctrl) = &mut tray {
            tray_ctrl.sync_state(TrayState::from_cat(&cat, cat_hidden, start_on_login));
        }

        if !bowl_hwnd.is_null() {
            if let Some(detached_pos) = cat.detached_bowl_pos() {
                render_bowl_overlay(cat.scale, &mut bowl_pixmap.as_mut());
                bowl_dib.write_pixmap(&bowl_pixmap);
                let bx = (detached_pos.x - (bowl_dim as f32) * 0.5).round() as i32;
                let by = (detached_pos.y - (bowl_dim as f32) * 0.5).round() as i32;
                bowl_dib.present_layered(bowl_hwnd, bx, by);
                if !bowl_visible {
                    unsafe {
                        ShowWindow(bowl_hwnd, SW_SHOWNOACTIVATE);
                    }
                    bowl_visible = true;
                }
            } else if bowl_visible {
                unsafe {
                    ShowWindow(bowl_hwnd, SW_HIDE);
                }
                bowl_visible = false;
            }
        }
        if !cat_hidden {
            let center = cat.window_center();
            let origin = center - Vec2::splat((win_dim as f32) * 0.5);
            win_x = origin.x.round() as i32;
            win_y = origin.y.round() as i32;

            if needs_raise {
                let insert_after = tray
                    .as_ref()
                    .and_then(|t| t.open_menu_window())
                    .unwrap_or(HWND_TOPMOST);
                unsafe {
                    SetWindowPos(
                        cat_hwnd,
                        insert_after,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
            }

            render_cat(&cat, &mut pixmap.as_mut(), &mut clip_mask, origin);
            dib.write_pixmap(&pixmap);
            dib.present_layered(cat_hwnd, win_x, win_y);
        }

        let elapsed = now.elapsed();
        if elapsed < frame_duration {
            std::thread::sleep(frame_duration - elapsed);
        }
    }
}
