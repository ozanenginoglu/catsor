# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Catsor (binary `catsor`) — a single-binary Rust (edition 2024) desktop cat companion that is drawn top-down and procedurally animated, and that follows the mouse cursor. It runs on Linux/X11 (developed under xmonad + picom) and Windows (Win32). The user-facing usage, tray menu and config format are in `README.md`.

## Commands

```sh
cargo build --release                 # binary at target/release/catsor
cargo run --release -- --coat tuxedo --scale 1.5 --temperament playful
cargo run -- --no-tray --fps 30
cargo test                            # all unit tests (cat, prefs, tray)
cargo test rapid_left_right           # single test by name substring
cargo test cat::tests::               # one module's tests

# Regenerate the committed app icons (assets/catsor.ico, assets/catsor.png) from render_tray_icon after changing it
cargo test regenerate_app_icons -- --ignored

# Render a static multi-pose PNG of every coat and exit (no X server needed); use it to check rendering changes visually
cargo run -- --preview-sheet /tmp/sheet.png --scale 1.5

# Type-check the Win32 backend from Linux (separate target dir avoids clobbering the normal build cache)
RUSTFLAGS="--cfg windows_check" cargo check --target-dir target/wincheck
```

Passing CLI `--coat/--scale/--temperament` overrides the saved preferences **and saves them**.

## Architecture

Detailed reference docs: `docs/behavior.md` (state machine, thresholds, temperaments), `docs/animation.md` (skeleton, gaits, posture, render order), `docs/platform-notes.md` (X11/Win32 details and the reasons behind them). Update them when you change the behavior they describe.

There are three layers. Simulation and rendering are platform-independent. All OS work lives in the platform backends.

- **`cat.rs` — `CatSim`, the simulation.** It never touches the OS. `update(cursor_pos, dt)` advances everything: cursor analysis (smoothing, wiggle detection for laser-pointer play, left/right shake detection for sprinting), the `Activity` state machine (Walking, Sitting, Sleeping, BellyUp, Pouncing, Zoomies, Eating, …), the spine and tail chain solve, and gait and paw stepping. The body is a chain of `TOTAL_NODES` world-space points (6 body nodes, head = node 0, then 6 tail nodes). The 4 `PawState`s follow 4-beat walk, trot and rotary-gallop phase schedules. Randomness comes from a deterministic internal RNG, so tests can be reproduced. Food-bowl "feeding mode" (`FeedingPhase`: Carrying → WalkingToBowl → Eating) is handled here too. `trigger(TriggerKind)` is test-only (`#[cfg(test)]`) and puts the cat straight into a state for test setup.
- **`render.rs` — tiny-skia rasterization.** `render_cat(cat, pixmap, mask, window_origin)` draws the cat in layers (shadow → bowl → paws → tail → body/patches/belly → head/ears) into a premultiplied RGBA `Pixmap`. Coordinates are world positions minus the overlay window's origin. Coat colours come from `Palette::for_coat`. The same file also renders the tray icon (`render_tray_icon`) and the bowl overlay.
- **`tray.rs` — the menu model plus the X11 tray.** These parts are shared by both platforms: `TrayState` (a snapshot of what the menu shows), `TrayAction` (what a click asks for), `build_rows`/`compute_layout`/`render_menu_pixmap`/`hit_test_entries`, the built-in 5x7 bitmap font, and `rgba_to_bgra`. The menu is drawn by hand with tiny-skia, with no toolkit. `TrayController` (X11 only) docks through the XEmbed `_NET_SYSTEM_TRAY` protocol and runs its own popup window.
- **Platform backends.** Each one owns the window, the event loop and keep-awake:
  - `main.rs::run_x11` creates an override-redirect 32-bit ARGB window with an empty XFixes input region (clicks pass through), sets EWMH hints (ABOVE, STICKY, UTILITY type, all desktops) and a `_CATSOR_OVERLAY` marker atom, then runs a fixed-fps loop. The loop polls events, re-raises the window when other windows stack above it, moves the window to follow `cat.window_center()`, and uploads frames with `put_image`. `X11KeepAwake` keeps the system awake while the cat is feeding: it spawns `systemd-inhibit` and resets the screensaver. It never synthesizes input.
  - `win32.rs::run` (compiled only under `cfg(any(target_os = "windows", windows_check))`) is the Win32 version of the same thing: a layered window via `UpdateLayeredWindow`/DIB, `Shell_NotifyIcon`, a custom popup menu window, and `SetThreadExecutionState` for keep-awake.
- **`autostart.rs` — "Start on Login".** `is_enabled`/`set_enabled` use an XDG autostart `.desktop` file on Linux and the `HKCU\…\Run` registry value on Windows. This is OS state and is not stored in prefs.
- **`prefs.rs` — `UserPrefs`** (coat, scale, `BehaviorConfig`) is persisted as a hand-parsed TOML subset (no serde) at `$XDG_CONFIG_HOME/catsor/config.toml` → `%APPDATA%`… → `~/.config/catsor/config.toml`. Writes are atomic (temp file + rename). Unknown or invalid entries are ignored.

### Conventions that span files

- Platform gating always uses `#[cfg(not(any(target_os = "windows", windows_check)))]` for X11 and `#[cfg(any(target_os = "windows", windows_check))]` for Win32. Keep this exact pattern, so that `--cfg windows_check` keeps type-checking the Win32 path on Linux. Use `tray.rs` for anything both backends need.
- New tray options need changes in several places: a `TrayAction` variant, a `TrayState` field if the menu shows it, `build_rows` in `tray.rs`, the action handlers in **both** `run_x11` and `win32.rs`, and `UserPrefs` and its TOML (de)serialization if the option should persist. The tray tests check that the menu layout covers every action.
- Every distance or threshold in `CatSim` is multiplied by `self.scale`. Keep new tunables scale-relative.
- Tests are behavioral simulations: they drive `CatSim::update` with synthetic cursor paths at a fixed `dt` and assert on activity, positions or smoothness. Follow that style when you change behavior.
- Commit messages use Conventional Commits (`feat:`, `fix:`).
