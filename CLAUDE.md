# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Catsor (binary `catsor`) — a single-binary Rust (edition 2024) desktop cat companion that is drawn top-down and procedurally animated, and that follows the mouse cursor. It runs on Linux/X11 (developed under xmonad + picom) and Windows (Win32). The user-facing usage, tray menu and config format are in `README.md`.

## Commands

```sh
cargo build --release                 # binary at target/release/catsor
cargo run --release -- --coat tuxedo --scale 1.5 --temperament playful
cargo run -- --no-tray --fps 30
cargo test                            # all unit tests (cat, coat, prefs, tray)
cargo test rapid_left_right           # single test by name substring
cargo test cat::tests::               # one module's tests

# Regenerate the committed app icons (assets/catsor.ico, assets/catsor.png) from render_tray_icon after changing it
cargo test regenerate_app_icons -- --ignored

# Render a static multi-pose PNG of every coat and exit (no X server needed); use it to check rendering changes visually
cargo run -- --preview-sheet /tmp/sheet.png --scale 1.5
cargo run -- --preview-sheet /tmp/sheet.png --coat random:00c0ffee   # one coat (built-in, custom or random) in every cell
cargo run -- --coat tuxedo --print-coat                                # a coat as a coats.toml entry

# Regenerate the README images: the coats-and-poses sheet, and the demo GIF (scripted human-like cursor driving CatSim; needs ffmpeg)
cargo run -- --preview-sheet docs/preview.png --scale 1.5
cargo test regenerate_demo_gif -- --ignored --nocapture

# Type-check the Win32 backend from Linux (separate target dir avoids clobbering the normal build cache)
RUSTFLAGS="--cfg windows_check" cargo check --target-dir target/wincheck
```

Passing CLI `--coat/--scale/--temperament` overrides the saved preferences **and saves them**.

## Architecture

Detailed reference docs: `docs/behavior.md` (state machine, thresholds, temperaments), `docs/animation.md` (skeleton, gaits, posture, render order), `docs/platform-notes.md` (X11/Win32 details and the reasons behind them). Update them when you change the behavior they describe.

There are three layers. Simulation and rendering are platform-independent. All OS work lives in the platform backends.

- **`cat/` — `CatSim`, the simulation.** It never touches the OS. `mod.rs` holds the types, `CatSim`'s fields, setup, the feeding API, the RNG and the tuning constants. `behavior.rs` holds the `update` pipeline and the `Activity` state machine. `body.rs` holds the head, gait, spine, tail and paw kinematics. `tests.rs` holds the tests. `update(cursor_pos, dt)` advances everything: cursor analysis (smoothing, wiggle detection for laser-pointer play, left/right shake detection for sprinting), the `Activity` state machine (Walking, Sitting, Sleeping, BellyUp, Pouncing, Zoomies, Eating, …), the spine and tail chain solve, and gait and paw stepping. The body is a chain of `TOTAL_NODES` world-space points (6 body nodes, head = node 0, then 6 tail nodes). The 4 `PawState`s follow 4-beat walk, trot and rotary-gallop phase schedules. Randomness comes from a deterministic internal RNG, so tests can be reproduced. Food-bowl "feeding mode" (`FeedingPhase`: Carrying → WalkingToBowl → Eating) is handled here too. `trigger(TriggerKind)` is test-only (`#[cfg(test)]`) and puts the cat straight into a state for test setup.
- **`coat.rs` — coats as data.** `Coat` holds a `Pattern` (Solid/Calico/Tuxedo) and every colour the renderer and tray icon use; `CatSim.coat` is a `Coat`. Built-in presets (`CoatStyle`, `Coat::builtin`) are hand-tuned. Custom coats come from `coats.toml` next to the config (parsed by `parse_entries`). Random coats are rolled from a `u32` seed (`Coat::random`). Both fill in missing colours with `Coat::derived`. `CoatId` (builtin / custom name / `random:<seed>`) is what prefs store. `CoatLibrary` resolves ids, cycles coats for the tray, and live-reloads `coats.toml` (`refresh`, called from both backends' `step_cat`). `CoatDesign` is the subset the designer edits; `CoatLibrary::save_design` writes it into `coats.toml` with `upsert_coat_entry`, which rewrites only the design's own keys and keeps comments, other entries and hand-added overrides.
- **`designer.rs` — the coat designer window's contents**, platform-independent like the tray menu: layout, drawing (bitmap font, HSV picker, live `posed` previews from `preview.rs`), input handling and saving. A backend owns the window (`DesignerWindow` in `x11.rs`, `DesignerWnd` in `win32.rs`), forwards `DesignerInput`, and uploads `Designer::render` when `take_dirty()` is true. `--design-coat` opens it at startup.
- **`render.rs` — tiny-skia rasterization.** `render_cat(cat, pixmap, mask, window_origin)` draws the cat in layers (shadow → bowl → paws → tail → body/patches/belly → head/ears) into a premultiplied RGBA `Pixmap`. Coordinates are world positions minus the overlay window's origin. Coat colours come from `Palette::for_coat(&cat.coat)`; never match on a specific coat in rendering code, add a `Coat` field instead. The same file also renders the tray icon (`render_tray_icon`) and the bowl overlay.
- **`tray.rs` — the menu model plus the X11 tray.** These parts are shared by both platforms: `TrayState` (a snapshot of what the menu shows), `TrayAction` (what a click asks for), `apply_shared_action` (applies the platform-independent actions: coat, temperament, toggles, start on login), `build_rows`/`compute_layout`/`render_menu_pixmap`/`hit_test_entries`, the built-in 5x7 bitmap font, and `rgba_to_bgra`. The menu is drawn by hand with tiny-skia, with no toolkit. `TrayController` (X11 only) docks through the XEmbed `_NET_SYSTEM_TRAY` protocol and runs its own popup window.
- **Platform backends.** Each one owns the window, the event loop and keep-awake:
  - `x11.rs::run` (compiled only when not Windows) builds an `X11App`. Each overlay (the cat, and the bowl while placing it) is an `OverlayWindow`: an override-redirect 32-bit ARGB window with an empty XFixes input region (clicks pass through), EWMH hints (ABOVE, STICKY, UTILITY type, all desktops) and a `_CATSOR_OVERLAY` marker atom. It then runs a fixed-fps loop. The loop polls events (`process_events` → `handle_tray_action`), re-raises the window when other windows stack above it, moves the window to follow `cat.window_center()`, and uploads frames with `put_image`. `X11KeepAwake` keeps the system awake while the cat is feeding: it spawns `systemd-inhibit` and resets the screensaver. It never synthesizes input.
  - `win32.rs::run` (compiled only under `cfg(any(target_os = "windows", windows_check))`) is the Win32 version of the same thing: a layered window via `UpdateLayeredWindow`/DIB, `Shell_NotifyIcon`, a custom popup menu window, and `SetThreadExecutionState` for keep-awake.
- **`autostart.rs` — "Start on Login".** `is_enabled`/`set_enabled` use an XDG autostart `.desktop` file on Linux and the `HKCU\…\Run` registry value on Windows. This is OS state and is not stored in prefs.
- **`prefs.rs` — `UserPrefs`** (`CoatId`, scale, `BehaviorConfig`) is persisted as a hand-parsed TOML subset (no serde) at `$XDG_CONFIG_HOME/catsor/config.toml` → `%APPDATA%`… → `~/.config/catsor/config.toml`. Writes are atomic (temp file + rename). Unknown or invalid entries are ignored.

### Conventions that span files

- Platform gating always uses `#[cfg(not(any(target_os = "windows", windows_check)))]` for X11 and `#[cfg(any(target_os = "windows", windows_check))]` for Win32. Keep this exact pattern, so that `--cfg windows_check` keeps type-checking the Win32 path on Linux. Use `tray.rs` for anything both backends need.
- New tray options need changes in several places: a `TrayAction` variant, a `TrayState` field if the menu shows it, `build_rows` in `tray.rs`, `apply_shared_action` in `tray.rs` if the action is platform-independent, otherwise the action handlers in **both** `x11.rs` (`X11App::handle_tray_action`) and `win32.rs` (`WinApp::handle_tray_actions`), and `UserPrefs` and its TOML (de)serialization if the option should persist. The tray tests check that the menu layout covers every action.
- Every distance or threshold in `CatSim` is multiplied by `self.scale`. Keep new tunables scale-relative.
- Tests are behavioral simulations: they drive `CatSim::update` with synthetic cursor paths at a fixed `dt` and assert on activity, positions or smoothness. Follow that style when you change behavior.
- Commit messages use Conventional Commits (`feat:`, `fix:`).
