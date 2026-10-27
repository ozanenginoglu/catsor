# Platform notes

The platform code covers everything outside `CatSim` and rendering: windows, events, the tray and keep-awake. Each section below explains **why** the code works the way it does. Many of these choices fix a specific bug, so check the reason before simplifying anything here.

Both backends use the same frame loop:

```
loop {
    handle pending events → tray actions
    query cursor position → cat.update(cursor, dt) → keep_awake.tick(...)
    sync tray state
    position + draw the detached bowl window (while carrying / walking to it)
    render_cat → RGBA→BGRA → move window to window_center() − dim/2 → upload
    sleep until next frame (--fps, min 15)
}
```

The cat window is a square of `max(64, 108 × scale)` px, and the bowl window is `max(22, 26 × scale)` px. The window moves around the screen and the cat is drawn *relative to the window*. The window is centred on `cat.window_center()`, a stable torso point, not on the head, so tail sway and posture changes don't make the window shake. Rounding to whole pixels happens only at the window position, never inside the pixmap.

## X11 (`x11.rs`)

### The overlay window

| What | Why |
|---|---|
| 32-bit ARGB TrueColor visual + its own colormap | Needed for per-pixel transparency. If there's no such visual, startup fails with an error. A compositor such as picom is still needed to actually blend it |
| `override_redirect = 1` | Keeps the window manager (xmonad) from managing it: no tiling, focus, borders or decorations |
| `_NET_WM_WINDOW_TYPE_UTILITY` | Makes picom skip shadows and fade animations for this window |
| `_NET_WM_STATE_ABOVE` + `_STICKY`, `_NET_WM_DESKTOP = 0xFFFFFFFF` | Keeps it above other windows and on every workspace, for WMs that read these hints |
| `WM_CLASS = catsor / Catsor` | Lets you match it in WM or compositor rules |
| `_CATSOR_OVERLAY = 1` (custom atom) | Marks Catsor's own windows, so the restacking logic below can ignore them |
| Pixels uploaded as a BGRA `put_image` Z_PIXMAP every frame | tiny-skia produces premultiplied RGBA, and `rgba_to_bgra` reorders it to what a 32-bit X visual expects |

### Click-through (commits `0bdffb8`, `6b19cca`)

The window's **input shape** is set to an *empty region* using both XShape (`shape_rectangles` with no rectangles) and XFixes (`set_window_shape_region` with an empty region). With the input shape empty, clicks **and hovering** go to whatever is underneath, so the cursor's shape doesn't flicker when it passes over the cat.

`apply_input_passthrough` is **re-applied after every map, unmap and resize**. Some servers or compositors reset the input shape when a window is remapped or resized, and the cat would then start catching clicks again.

### Staying on top (commit `6b19cca`)

An override-redirect window doesn't stay on top by itself. When another window is mapped, the loop sets `needs_raise`. It does the same when another window is restacked **directly above** the cat (`ConfigureNotify` with `above_sibling == win`), or when the cat becomes obscured (`VisibilityNotify`). On the next frame it calls `configure_window(stack_mode = ABOVE)`. To avoid loops and fights between windows:

- events from windows with `_CATSOR_OVERLAY` (the cat and bowl windows) are ignored, and so are events from the tray popup,
- while the tray popup is open, the cat is stacked **below the popup** (`sibling = menu, BELOW`), not above everything.

The root window's `SUBSTRUCTURE_NOTIFY` events supply the Map and Configure notifications for other windows.

### System tray (`TrayController` in `tray.rs`)

- Uses the **XEmbed system tray protocol**. It looks for the owner of `_NET_SYSTEM_TRAY_S{screen}`, sets `_XEMBED_INFO` (version 0, `XEMBED_MAPPED`), and sends `SYSTEM_TRAY_REQUEST_DOCK` through `_NET_SYSTEM_TRAY_OPCODE`.
- **Choosing a visual:** if the tray advertises `_NET_SYSTEM_TRAY_VISUAL` matching our ARGB visual, or the tray window is 32-bit, the icon uses ARGB. Otherwise it uses the root visual and an **opaque dark background** (`opaque_bg`), because a non-ARGB tray can't blend alpha.
- **Re-docking:** `try_dock` runs every 120 frames (~2 s at 60 fps), and also when a `MANAGER` client message announces a new tray. Restarting your bar or tray brings the icon back, and starting Catsor before the tray is fine.
- **Popup menu:** an override-redirect ARGB window drawn by hand (`render_menu_pixmap`). It opens below the icon, or above it if there isn't room, and is clamped so it stays fully on screen with an 8 px margin. If the row count changes while it is open (the "Stop Feeding" row appears only while feeding), `redraw_menu` re-runs `place_menu` to resize and re-place it; Win32 does the same in `MenuWndState::redraw_menu`. On open it **grabs the pointer and the keyboard** (commit `0bdffb8`). The pointer grab lets a click outside the menu close it. The keyboard grab lets Esc (keycode 9) close it, since an override-redirect window never gets focus from the WM. Settings items (coat, random coat, temperament, behaviors, size, hide) keep the menu open, so you can change several at once. Feed and quit items close it (`close_on_click`).

### Coat designer window (`DesignerWindow`)

Unlike the overlays, the designer is an **ordinary managed window**. It is not override-redirect, and it uses the root's opaque visual instead of ARGB. That way the window manager gives it a title bar, keyboard focus and a close button (`WM_DELETE_WINDOW`), and it shows up in window lists. It is marked `_NET_WM_WINDOW_TYPE_DIALOG`, has fixed-size `WM_NORMAL_HINTS`, and sets `WM_TRANSIENT_FOR` to the root window. xmonad floats transient windows, so the designer isn't tiled there. If a window manager resizes it anyway, the contents stay centred at their logical size (`Designer::set_window_size`). Key presses are turned into keysyms with the core `GetKeyboardMapping` table (`resolve_keysym`), without depending on xkbcommon: Shift picks column 1, Caps Lock shifts letters only (Shift + Caps Lock gives lower case), and with NumLock on, keypad keys use their digit column unless Shift is held. Only printable ASCII/Latin-1, keypad digits, BackSpace, Return and Escape are used. If the designer window fails to open or draw, the error is logged and the cat keeps running without it. A frame is uploaded in horizontal strips sized to `maximum_request_bytes`, so it never runs into the request size limit. Opening it again while it is open sends `_NET_ACTIVE_WINDOW` to raise and focus the existing one.

### Bowl placement

When a feed preset is picked, the loop **grabs the pointer and keyboard on the root window**, so the placement click doesn't reach the app underneath:

- Button 1 → `cat.place_bowl(click position)`
- Any other button, or Esc → `cat.stop_feeding()`

If the grab fails (another client already holds a grab), the bowl is placed at the cursor straight away. The grab is released as soon as the cat stops carrying the bowl, whatever the reason. The bowl is drawn in its **own small click-through window** while it's being carried or walked to. Once eating starts, `render_cat` draws it as part of the cat's frame.

### Roaming bounds

Each frame, `update_roam_bounds` sets `cat.roam_bounds` to the monitor that contains the cat's `window_center()`, or the nearest one. Monitors come from RandR 1.5 `GetMonitors` (active monitors only) and are re-read every 2 s, so plugging, unplugging or rearranging screens is picked up. Per-monitor bounds matter because with monitors of different sizes, the root window has areas no screen shows. If RandR monitors aren't available, the whole root window is used. Docks and panels (e.g. xmobar) aren't excluded. Roam targets stay 30 px inside the monitor edge, so the cat may partly overlap a bar.

### Keep-awake (feeding mode)

`X11KeepAwake` is active while `cat.keeps_awake()` is true, which covers WalkingToBowl and Eating.

1. **`systemd-inhibit --what=idle:sleep --who=catsor … sleep infinity`** is started as a child process. The child is killed and reaped when feeding ends, and in `Drop`. If `systemd-inhibit` is missing, this step silently does nothing.
2. **`ForceScreenSaver(Reset)`** is called every 25 s.
Catsor **never synthesizes pointer or keyboard input** (no XTest, no cursor moves, no key taps). That keeps it within typical enterprise policies on input automation. The flip side is that presence detection based on real input events (for example Teams or Slack "away" status in a browser) is not affected: only the session idle timer, the screensaver, screen blanking and sleep are held off.

## Windows (`win32.rs`)

Built when `target_os = "windows"`, or on Linux with `--cfg windows_check` for type-checking only. The structure mirrors X11:

| X11 | Win32 |
|---|---|
| ARGB window + `put_image` | `WS_EX_LAYERED` window + 32-bpp top-down DIB section, presented with `UpdateLayeredWindow(ULW_ALPHA)`, premultiplied BGRA |
| Empty XFixes input shape | `WS_EX_TRANSPARENT` (+ `WS_EX_NOACTIVATE`) |
| override-redirect, ABOVE | `WS_EX_TOPMOST` + `WS_EX_TOOLWINDOW` (no taskbar button). Re-asserted with `SetWindowPos(HWND_TOPMOST)` every 120 frames, or placed just below the open menu |
| XEmbed tray | `Shell_NotifyIconW` with `WM_TRAYICON = WM_APP+1`. The icon is a `CreateIconIndirect` HICON rendered by `render_tray_icon`. When Explorer restarts, the `TaskbarCreated` message re-adds it |
| Popup with pointer/keyboard grab | Layered `WS_POPUP` menu window. It closes on `WM_ACTIVATE(WA_INACTIVE)`, a click outside, or Esc. Hover uses `TrackMouseEvent` / `WM_MOUSELEAVE` |
| Root grab for bowl placement | Polls `GetAsyncKeyState` for VK_LBUTTON, VK_RBUTTON and VK_ESCAPE. A left click only counts after the button has been seen *released* (`bowl_click_armed`), so the click that chose the menu item doesn't immediately place the bowl |
| Managed designer window, `put_image` in strips | `WS_OVERLAPPED \| WS_CAPTION \| WS_SYSMENU` window (taskbar button, fixed size). Its window procedure queues input (`WM_MOUSEMOVE`/`WM_LBUTTON*` with `SetCapture` for drags, `WM_CHAR`, `WM_CLOSE`) for the main loop, and `WM_PAINT` stretches the latest BGRA frame with `StretchDIBits`. The client area is the logical size times a whole-number DPI scale (`GetDpiForWindow`), so the bitmap font stays crisp |
| `systemd-inhibit` + ScreenSaver reset | `SetThreadExecutionState(ES_CONTINUOUS \| ES_SYSTEM_REQUIRED \| ES_DISPLAY_REQUIRED)`, set once when feeding starts (`ES_CONTINUOUS` holds it) and cleared to `ES_CONTINUOUS` when feeding stops |

The binary is built for the **GUI subsystem** (`windows_subsystem = "windows"` in `main.rs`), so launching it doesn't open a console window. Because of that, stdout/stderr go nowhere by default. `main` calls `attach_parent_console` (`AttachConsole(ATTACH_PARENT_PROCESS)`) first, so `--help`, `--preview-sheet` and error messages still print when it is started from a terminal. The terminal doesn't wait for a GUI-subsystem process, so the output can appear after the prompt.

**App icon.** `build.rs` embeds `assets/catsor.ico` as the exe's icon resource, so Explorer, Task Manager and shortcuts show the cat. It writes the compiled `.res` itself (each image as an `RT_ICON`, plus `RT_GROUP_ICON` 1) instead of compiling an `.rc` script. MSVC links the `.res` directly. GNU converts it to a COFF object with `windres --input-format=res`, which skips windres' C preprocessor. The preprocessor step breaks when the project path contains spaces, and it is also why the `winresource` crate was dropped. The output is byte-identical to `llvm-rc` for `1 ICON "catsor.ico"`. The `.ico` (16–256 px, PNG-compressed entries) and the Linux launcher icon `assets/catsor.png` are generated from `render_tray_icon` by the ignored test `regenerate_app_icons`, so they match the tray icon. The overlay and menu windows are tool windows with no taskbar button, so they don't need a window-class icon.

The process declares itself **per-monitor DPI aware (v2)**, so window positions match `GetCursorPos` coordinates on scaled displays. Menu placement uses the work area of the monitor under the cursor (`monitor_work_area`). A roaming cat's `roam_bounds` is set every frame to the work area of the monitor the cat is on, so it stays on that monitor, clear of the taskbar, and follows display changes.

### Start on login (`autostart.rs`)

Login registration is OS state, not a preference, so it is **not** stored in `config.toml`. The OS location is the only source of truth: each backend reads `autostart::is_enabled()` once at startup and again after every toggle, and keeps the result in a local `start_on_login` that feeds `TrayState`. It does not query the filesystem or registry every frame.

- **Linux:** an XDG autostart `.desktop` file whose `Exec=` is `current_exe()`, quoted and escaped per the Desktop Entry spec (`exec_arg`). It is enabled when the file exists.
- **Windows:** the `HKCU\…\Run` value `catsor` = `"<exe>"`. Task Manager's Startup tab disables an entry by setting an odd first byte in `Explorer\StartupApproved\Run` without removing the `Run` value, so `is_enabled` checks that too. `set_enabled` deletes the `StartupApproved` value, so enabling from the tray actually takes effect.

## Preferences file

`prefs.rs` writes `config.toml` **atomically**. It writes a uniquely named temp file in the same directory, then renames it over the old file, so a crash in the middle of a write can't leave a half-written config. A relative `$XDG_CONFIG_HOME` is ignored, as the XDG spec requires (there's a test for this). Every tray change that affects a saved setting calls `UserPrefs::from_cat(&cat).save()` straight away.
