//! The coat designer window's contents: layout, drawing, input handling and saving.
//!
//! It is platform-independent, like the tray menu. A backend owns the actual window: it
//! reports the window size, forwards mouse and keyboard input as [`DesignerInput`], and
//! uploads the pixmap from [`Designer::render`] whenever [`Designer::take_dirty`] says the
//! picture changed. Saving writes the design into `coats.toml` (keeping everything else in the
//! file) and puts the coat on the cat.

use crate::cat::CatSim;
use crate::coat::{
    Coat, CoatDesign, CoatId, CoatLibrary, MAX_NAME_CHARS, NameProblem, Pattern, Rgba8, SaveError,
    fresh_seed, rgb,
};
use crate::prefs::UserPrefs;
use crate::preview::{Pose, posed, render_cropped};
use crate::render::{color as to_color, render_tray_icon};
use crate::tray::{
    build_rounded_rect, draw_checkbox_indicator, draw_text, draw_text_sized, glyph_5x7,
};
use tiny_skia::{
    Color, FillRule, Paint, PathBuilder, Pixmap, PixmapMut, PixmapPaint, PremultipliedColorU8,
    Rect, Stroke, Transform,
};

/// Logical size of the designer's contents; a larger window centres them
pub const DESIGNER_W: u32 = 600;
pub const DESIGNER_H: u32 = 400;
pub const DESIGNER_TITLE: &str = "Catsor Coat Designer";

/// Input from the window, in window pixels
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DesignerInput {
    Move(f32, f32),
    /// Left button
    Press(f32, f32),
    Release,
    /// The pointer left the window
    Leave,
    Char(char),
    Backspace,
    Enter,
    Escape,
    /// The window's close button
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Base,
    Patch,
    Spot,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Base, Slot::Patch, Slot::Spot];

    fn label(self, pattern: Pattern) -> &'static str {
        match (self, pattern) {
            (Slot::Base, _) => "Fur",
            (Slot::Patch, Pattern::Tuxedo) => "Mask and back",
            (Slot::Spot, Pattern::Tuxedo) => "Shoulder spot",
            (Slot::Patch, _) => "Patches",
            (Slot::Spot, _) => "Second patches",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Focus {
    Name,
    Hex,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Drag {
    SatVal,
    Hue,
}

/// Everything clickable
#[derive(Debug, Clone, Copy, PartialEq)]
enum Hit {
    Name,
    Pattern(Pattern),
    Slot(Slot),
    Hex(Slot),
    Rings,
    /// Drops the entry's hand-tuned colours
    ResetTuning,
    SatVal,
    Hue,
    Preset(usize),
    Randomize,
    Cancel,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Hsv {
    /// Degrees in [0, 360)
    h: f32,
    s: f32,
    v: f32,
}

impl Hsv {
    fn from_rgb(c: Rgba8) -> Self {
        let (r, g, b) = (c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0);
        let max = r.max(g).max(b);
        let delta = max - r.min(g).min(b);
        let h = if delta <= f32::EPSILON {
            0.0
        } else if max == r {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        let s = if max <= f32::EPSILON {
            0.0
        } else {
            delta / max
        };
        Self { h, s, v: max }
    }

    fn to_rgb(self) -> Rgba8 {
        let c = self.v * self.s;
        let hp = (self.h.rem_euclid(360.0)) / 60.0;
        let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
        let (r, g, b) = match hp as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = self.v - c;
        let ch = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
        Rgba8 {
            r: ch(r),
            g: ch(g),
            b: ch(b),
            a: 255,
        }
    }
}

/// Quick picks: real cat colours on the first row, playful ones on the second
const PRESETS: [Rgba8; 24] = [
    rgb(242, 240, 236), // white
    rgb(228, 190, 150), // cream
    rgb(214, 128, 62),  // ginger
    rgb(186, 92, 38),   // red
    rgb(196, 168, 148), // fawn
    rgb(142, 94, 62),   // cinnamon
    rgb(96, 64, 46),    // chocolate
    rgb(154, 118, 88),  // brown
    rgb(46, 45, 49),    // black
    rgb(118, 124, 136), // blue
    rgb(170, 158, 160), // lilac
    rgb(210, 212, 216), // silver
    rgb(62, 62, 66),    // charcoal
    rgb(140, 140, 150), // smoke
    rgb(232, 170, 120), // apricot
    rgb(176, 124, 80),  // caramel
    rgb(240, 180, 138), // peach
    rgb(236, 168, 190), // pink
    rgb(178, 150, 214), // lavender
    rgb(150, 214, 186), // mint
    rgb(140, 190, 230), // sky
    rgb(238, 214, 110), // lemon
    rgb(232, 120, 110), // coral
    rgb(90, 130, 110),  // moss
];

// Layout, in logical pixels from the top-left of the contents
const LEFT: f32 = 16.0;
const LEFT_W: f32 = 300.0;
const RIGHT: f32 = 332.0;
const RIGHT_W: f32 = 252.0;
const NAME_Y: f32 = 46.0;
const PATTERN_Y: f32 = 88.0;
const SLOTS_Y: f32 = 130.0;
const ROW_H: f32 = 20.0;
const RINGS_Y: f32 = 198.0;
const RINGS_W: f32 = 160.0;
const PICKER_Y: f32 = 234.0;
const PICKER_H: f32 = 88.0;
const SV_W: f32 = 220.0;
const HUE_X: f32 = LEFT + SV_W + 8.0;
const HUE_W: f32 = 18.0;
const PRESETS_Y: f32 = 330.0;
const PRESET_W: f32 = LEFT_W / 12.0;
const PREVIEW_Y: f32 = 46.0;
const PREVIEW_H: f32 = 244.0;
const ICON_Y: f32 = 298.0;
const STATUS_Y: f32 = 344.0;
const BUTTONS_Y: f32 = 364.0;
const BUTTON_H: f32 = 24.0;

const PREVIEW_POSES: [Pose; 4] = [Pose::Walk, Pose::Sit, Pose::BellyUp, Pose::Sleep];

const BG: Rgba8 = Rgba8::new(0x23, 0x25, 0x30, 255);
const FIELD_BG: Rgba8 = Rgba8::new(0x1b, 0x1c, 0x25, 255);
const BORDER: Rgba8 = Rgba8::new(0x62, 0x72, 0xa4, 210);
const HOVER: Rgba8 = Rgba8::new(0x44, 0x47, 0x5a, 220);
const HEADER: Rgba8 = Rgba8::new(0x8b, 0xe9, 0xfd, 230);
const TEXT: Rgba8 = Rgba8::new(0xf8, 0xf8, 0xf2, 238);
const DIM_TEXT: Rgba8 = Rgba8::new(0xf8, 0xf8, 0xf2, 90);
const ACCENT: Rgba8 = Rgba8::new(0xbd, 0x93, 0xf9, 255);
const OK: Rgba8 = Rgba8::new(0x50, 0xfa, 0x7b, 255);
const ERROR: Rgba8 = Rgba8::new(0xff, 0x6e, 0x6e, 255);
const PREVIEW_BG: Rgba8 = Rgba8::new(0xf4, 0xf3, 0xf0, 255);

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_xywh(x, y, w, h).unwrap_or_else(|| Rect::from_xywh(0.0, 0.0, 1.0, 1.0).unwrap())
}

fn contains(r: Rect, x: f32, y: f32) -> bool {
    x >= r.left() && x < r.right() && y >= r.top() && y < r.bottom()
}

fn pattern_rect(i: usize) -> Rect {
    let w = (LEFT_W - 12.0) / 3.0;
    rect(LEFT + i as f32 * (w + 6.0), PATTERN_Y, w, ROW_H)
}

fn slot_rect(i: usize) -> Rect {
    rect(LEFT, SLOTS_Y + i as f32 * 22.0, LEFT_W, ROW_H)
}

fn hex_rect(i: usize) -> Rect {
    rect(LEFT + LEFT_W - 84.0, SLOTS_Y + i as f32 * 22.0, 84.0, ROW_H)
}

fn preset_rect(i: usize) -> Rect {
    let (col, row) = (i % 12, i / 12);
    rect(
        LEFT + col as f32 * PRESET_W,
        PRESETS_Y + row as f32 * 22.0,
        PRESET_W,
        22.0,
    )
}

/// "Reset" for hand-tuned colours, at the right of the rings row
fn reset_rect() -> Rect {
    rect(LEFT + LEFT_W - 48.0, RINGS_Y - 2.0, 48.0, 20.0)
}

fn sv_rect() -> Rect {
    rect(LEFT, PICKER_Y, SV_W, PICKER_H)
}

fn hue_rect() -> Rect {
    rect(HUE_X, PICKER_Y, HUE_W, PICKER_H)
}

fn button_rects() -> [(Hit, Rect, &'static str); 3] {
    [
        (
            Hit::Randomize,
            rect(RIGHT, BUTTONS_Y, 80.0, BUTTON_H),
            "Randomize",
        ),
        (
            Hit::Cancel,
            rect(RIGHT + 88.0, BUTTONS_Y, 72.0, BUTTON_H),
            "Close",
        ),
        (
            Hit::Save,
            rect(RIGHT + 168.0, BUTTONS_Y, 84.0, BUTTON_H),
            "Save",
        ),
    ]
}

struct Preview {
    sprites: Vec<Option<Pixmap>>,
    icon: Option<Pixmap>,
}

pub struct Designer {
    design: CoatDesign,
    /// Name of the custom coat being edited, so saving updates (or renames) it
    original: Option<String>,
    /// HSV of each slot, kept so the hue survives dragging to grey or black
    hsv: [Hsv; 3],
    active: Slot,
    focus: Option<Focus>,
    hex_input: String,
    drag: Option<Drag>,
    hovered: Option<Hit>,
    status: Option<(String, bool)>,
    /// Cats already in the preview poses; only their coat changes
    preview_cats: Vec<CatSim>,
    /// Rendered previews of the current design; cleared when the design changes
    preview: Option<Preview>,
    /// The hue strip never changes; the saturation/value square only with the hue
    hue_strip: Option<Pixmap>,
    sv_square: Option<(f32, Pixmap)>,
    win_w: u32,
    win_h: u32,
    dirty: bool,
}

impl Designer {
    /// Opens on the cat's current coat, including a custom coat's hand-tuned colours
    pub fn new(current: &Coat, coats: &CoatLibrary) -> Self {
        let design = coats.design_for(current);
        let original = match &current.id {
            CoatId::Custom(name) => Some(name.to_string()),
            _ => None,
        };
        let preview_cats = PREVIEW_POSES
            .iter()
            .map(|&pose| posed(pose, current, 1.0))
            .collect();
        Self {
            hsv: [
                Hsv::from_rgb(design.base),
                Hsv::from_rgb(design.patch),
                Hsv::from_rgb(design.spot),
            ],
            design,
            original,
            active: Slot::Base,
            focus: None,
            hex_input: String::new(),
            drag: None,
            hovered: None,
            status: None,
            preview_cats,
            preview: None,
            hue_strip: None,
            sv_square: None,
            win_w: DESIGNER_W,
            win_h: DESIGNER_H,
            dirty: true,
        }
    }

    /// Forces a redraw (the window was exposed). Win32 repaints from its own copy of the frame
    #[cfg_attr(any(target_os = "windows", windows_check), allow(dead_code))]
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    /// Whether the picture changed since the last call
    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    /// The window's current size (X11 window managers may give it a different one; the Win32
    /// window has a fixed size)
    #[cfg_attr(any(target_os = "windows", windows_check), allow(dead_code))]
    pub fn set_window_size(&mut self, w: u32, h: u32) {
        if (w, h) != (self.win_w, self.win_h) {
            self.win_w = w.max(1);
            self.win_h = h.max(1);
            self.dirty = true;
        }
    }

    /// Top-left of the centred contents in window pixels
    fn origin(&self) -> (f32, f32) {
        (
            ((self.win_w as f32 - DESIGNER_W as f32) * 0.5)
                .max(0.0)
                .floor(),
            ((self.win_h as f32 - DESIGNER_H as f32) * 0.5)
                .max(0.0)
                .floor(),
        )
    }

    fn slot_enabled(&self, slot: Slot) -> bool {
        slot == Slot::Base || self.design.pattern != Pattern::Solid
    }

    fn color_of(&self, slot: Slot) -> Rgba8 {
        match slot {
            Slot::Base => self.design.base,
            Slot::Patch => self.design.patch,
            Slot::Spot => self.design.spot,
        }
    }

    fn set_color(&mut self, slot: Slot, c: Rgba8, hsv: Option<Hsv>) {
        let c = Rgba8 { a: 255, ..c };
        match slot {
            Slot::Base => self.design.base = c,
            Slot::Patch => self.design.patch = c,
            Slot::Spot => self.design.spot = c,
        }
        self.hsv[slot as usize] = hsv.unwrap_or_else(|| Hsv::from_rgb(c));
        self.design_changed();
    }

    fn design_changed(&mut self) {
        self.preview = None;
        self.status = None;
        self.dirty = true;
    }

    fn hit_test(&self, x: f32, y: f32) -> Option<Hit> {
        let (ox, oy) = self.origin();
        let (x, y) = (x - ox, y - oy);
        if contains(rect(LEFT, NAME_Y, LEFT_W, ROW_H), x, y) {
            return Some(Hit::Name);
        }
        for (i, p) in Pattern::ALL.into_iter().enumerate() {
            if contains(pattern_rect(i), x, y) {
                return Some(Hit::Pattern(p));
            }
        }
        for (i, slot) in Slot::ALL.into_iter().enumerate() {
            if self.slot_enabled(slot) && contains(slot_rect(i), x, y) {
                return Some(if contains(hex_rect(i), x, y) {
                    Hit::Hex(slot)
                } else {
                    Hit::Slot(slot)
                });
            }
        }
        if contains(rect(LEFT, RINGS_Y, RINGS_W, 18.0), x, y) {
            return Some(Hit::Rings);
        }
        if !self.design.overrides.is_empty() && contains(reset_rect(), x, y) {
            return Some(Hit::ResetTuning);
        }
        if contains(sv_rect(), x, y) {
            return Some(Hit::SatVal);
        }
        if contains(hue_rect(), x, y) {
            return Some(Hit::Hue);
        }
        if let Some(i) = (0..PRESETS.len()).find(|&i| contains(preset_rect(i), x, y)) {
            return Some(Hit::Preset(i));
        }
        button_rects()
            .into_iter()
            .find(|(_, r, _)| contains(*r, x, y))
            .map(|(hit, _, _)| hit)
    }

    /// Sets the active colour from a pointer position over the picker
    fn drag_to(&mut self, drag: Drag, x: f32, y: f32) {
        let (ox, oy) = self.origin();
        let (x, y) = (x - ox, y - oy);
        let mut hsv = self.hsv[self.active as usize];
        match drag {
            Drag::SatVal => {
                hsv.s = ((x - sv_rect().left()) / sv_rect().width()).clamp(0.0, 1.0);
                hsv.v = 1.0 - ((y - sv_rect().top()) / sv_rect().height()).clamp(0.0, 1.0);
            }
            Drag::Hue => {
                hsv.h = ((y - hue_rect().top()) / hue_rect().height()).clamp(0.0, 0.999) * 360.0;
            }
        }
        self.set_color(self.active, hsv.to_rgb(), Some(hsv));
    }

    fn set_focus(&mut self, focus: Option<Focus>) {
        if focus != self.focus {
            self.focus = focus;
            if focus == Some(Focus::Hex) {
                self.hex_input = self.color_of(self.active).to_hex()[1..].to_string();
            }
            self.dirty = true;
        }
    }

    /// Handles one input event. Returns `true` when the designer should close
    pub fn handle(
        &mut self,
        input: DesignerInput,
        coats: &mut CoatLibrary,
        cat: &mut CatSim,
    ) -> bool {
        match input {
            DesignerInput::Move(x, y) => {
                if let Some(drag) = self.drag {
                    self.drag_to(drag, x, y);
                }
                let hovered = self.hit_test(x, y);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    self.dirty = true;
                }
            }
            DesignerInput::Leave => {
                if self.hovered.take().is_some() {
                    self.dirty = true;
                }
            }
            DesignerInput::Release => self.drag = None,
            DesignerInput::Press(x, y) => {
                let hit = self.hit_test(x, y);
                if let Some(Hit::Hex(slot)) = hit
                    && self.active != slot
                {
                    // Focus moves to another row: refill the box from that row's colour
                    self.focus = None;
                    self.active = slot;
                }
                let focus = match hit {
                    Some(Hit::Name) => Some(Focus::Name),
                    Some(Hit::Hex(_)) => Some(Focus::Hex),
                    _ => None,
                };
                self.set_focus(focus);
                match hit {
                    Some(Hit::Pattern(p)) => {
                        self.design.pattern = p;
                        if !self.slot_enabled(self.active) {
                            self.active = Slot::Base;
                        }
                        self.design_changed();
                    }
                    Some(Hit::Slot(slot)) => {
                        self.active = slot;
                        self.dirty = true;
                    }
                    Some(Hit::Rings) => {
                        self.design.tail_rings = !self.design.tail_rings;
                        if !self.design.tail_rings {
                            // A hand-tuned ring colour would keep the rings on
                            self.design.overrides.retain(|(k, _)| k != "tail_rings");
                        }
                        self.design_changed();
                    }
                    Some(Hit::ResetTuning) => {
                        self.design.overrides.clear();
                        self.design_changed();
                    }
                    Some(Hit::SatVal) => {
                        self.drag = Some(Drag::SatVal);
                        self.drag_to(Drag::SatVal, x, y);
                    }
                    Some(Hit::Hue) => {
                        self.drag = Some(Drag::Hue);
                        self.drag_to(Drag::Hue, x, y);
                    }
                    Some(Hit::Preset(i)) => self.set_color(self.active, PRESETS[i], None),
                    Some(Hit::Randomize) => self.randomize(),
                    Some(Hit::Cancel) => return true,
                    Some(Hit::Save) => self.save(coats, cat),
                    Some(Hit::Name | Hit::Hex(_)) | None => {}
                }
            }
            DesignerInput::Char(c) => match self.focus {
                Some(Focus::Name) => {
                    let allowed = c == ' ' || (c != ':' && glyph_5x7(c) != [0; 7]);
                    if allowed && self.design.name.chars().count() < MAX_NAME_CHARS {
                        self.design.name.push(c);
                        self.design_changed();
                    }
                }
                Some(Focus::Hex) if c.is_ascii_hexdigit() && self.hex_input.len() < 6 => {
                    self.hex_input.push(c.to_ascii_lowercase());
                    self.dirty = true;
                    if let Some(parsed) = Rgba8::parse_hex(&self.hex_input) {
                        self.set_color(self.active, parsed, None);
                    }
                }
                Some(Focus::Hex) | None => {}
            },
            DesignerInput::Backspace => {
                match self.focus {
                    Some(Focus::Name) => {
                        self.design.name.pop();
                        self.design_changed();
                    }
                    Some(Focus::Hex) => {
                        self.hex_input.pop();
                    }
                    None => {}
                }
                self.dirty = true;
            }
            DesignerInput::Enter => {
                if self.focus.is_some() {
                    self.set_focus(None);
                } else {
                    self.save(coats, cat);
                }
            }
            DesignerInput::Escape => {
                if self.focus.is_none() {
                    return true;
                }
                self.set_focus(None);
            }
            DesignerInput::Close => return true,
        }
        false
    }

    /// Rolls a random coat, keeping the name
    fn randomize(&mut self) {
        let mut rolled = CoatDesign::from_coat(&Coat::random(fresh_seed()));
        rolled.name = std::mem::take(&mut self.design.name);
        self.hsv = [
            Hsv::from_rgb(rolled.base),
            Hsv::from_rgb(rolled.patch),
            Hsv::from_rgb(rolled.spot),
        ];
        self.design = rolled;
        if !self.slot_enabled(self.active) {
            self.active = Slot::Base;
        }
        self.design_changed();
    }

    fn save(&mut self, coats: &mut CoatLibrary, cat: &mut CatSim) {
        self.set_focus(None);
        self.status = Some(
            match coats.save_design(&self.design, self.original.as_deref()) {
                Ok(coat) => {
                    let name = coat.name.to_string();
                    cat.coat = coat;
                    UserPrefs::from_cat(cat).save();
                    let msg = format!("Saved {name}. Your cat is wearing it.");
                    self.design.name = name.clone();
                    self.original = Some(name);
                    (msg, true)
                }
                Err(e) => {
                    let msg = match e {
                        SaveError::Name(NameProblem::Empty) => "Give the coat a name first.",
                        SaveError::Name(NameProblem::Reserved) => {
                            "That name belongs to a built-in coat."
                        }
                        SaveError::Name(NameProblem::Quote) => "Names cannot contain quote marks.",
                        SaveError::Taken => "Another coat already has that name.",
                        SaveError::Failed(e) => {
                            eprintln!("catsor: saving coat failed: {e}");
                            "Could not save coats.toml (see terminal)."
                        }
                    };
                    (msg.to_string(), false)
                }
            },
        );
        self.dirty = true;
    }

    /// Draws the designer into `pixmap`, which should be the window's size
    pub fn render(&mut self, pixmap: &mut PixmapMut) {
        pixmap.fill(to_color(BG));
        let (ox, oy) = self.origin();
        let t = Transform::from_translate(ox, oy);
        let fill = |pixmap: &mut PixmapMut, r: Rect, radius: f32, c: Color| {
            if let Some(p) = build_rounded_rect(r.x(), r.y(), r.width(), r.height(), radius) {
                let mut paint = Paint::default();
                paint.set_color(c);
                pixmap.fill_path(&p, &paint, FillRule::Winding, t, None);
            }
        };
        let outline = |pixmap: &mut PixmapMut, r: Rect, radius: f32, c: Color, w: f32| {
            if let Some(p) = build_rounded_rect(r.x(), r.y(), r.width(), r.height(), radius) {
                let mut paint = Paint::default();
                paint.set_color(c);
                let stroke = Stroke {
                    width: w,
                    ..Default::default()
                };
                pixmap.stroke_path(&p, &paint, &stroke, t, None);
            }
        };
        let text = |pixmap: &mut PixmapMut, s: &str, x: f32, y: f32, c: Color| {
            draw_text(pixmap, s, ox + x, oy + y, c);
        };
        let circle = |pixmap: &mut PixmapMut, cx: f32, cy: f32, r: f32, c: Color, ring: Color| {
            if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
                let mut paint = Paint::default();
                paint.set_color(c);
                pixmap.fill_path(&p, &paint, FillRule::Winding, t, None);
                paint.set_color(ring);
                let stroke = Stroke {
                    width: 1.0,
                    ..Default::default()
                };
                pixmap.stroke_path(&p, &paint, &stroke, t, None);
            }
        };
        let hovered = self.hovered;

        draw_text_sized(
            pixmap,
            "COAT DESIGNER",
            ox + LEFT,
            oy + 12.0,
            2.0,
            to_color(TEXT),
        );
        text(pixmap, "NAME", LEFT, 34.0, to_color(HEADER));
        text(pixmap, "PATTERN", LEFT, 76.0, to_color(HEADER));
        text(pixmap, "COLOURS", LEFT, 118.0, to_color(HEADER));
        text(pixmap, "PICK A COLOUR", LEFT, 222.0, to_color(HEADER));
        text(pixmap, "PREVIEW", RIGHT, 34.0, to_color(HEADER));

        // Name field
        let name_rect = rect(LEFT, NAME_Y, LEFT_W, ROW_H);
        fill(pixmap, name_rect, 4.0, to_color(FIELD_BG));
        let name_focused = self.focus == Some(Focus::Name);
        outline(
            pixmap,
            name_rect,
            4.0,
            if name_focused {
                to_color(ACCENT)
            } else {
                to_color(BORDER)
            },
            1.0,
        );
        if self.design.name.is_empty() && !name_focused {
            text(
                pixmap,
                "Type a name",
                LEFT + 7.0,
                NAME_Y + 7.0,
                to_color(DIM_TEXT),
            );
        } else {
            text(
                pixmap,
                &self.design.name,
                LEFT + 7.0,
                NAME_Y + 7.0,
                to_color(TEXT),
            );
        }
        if name_focused {
            let cx = LEFT + 7.0 + self.design.name.chars().count() as f32 * 6.0;
            fill(
                pixmap,
                rect(cx, NAME_Y + 5.0, 1.0, 10.0),
                0.0,
                to_color(ACCENT),
            );
        }

        // Pattern buttons
        for (i, p) in Pattern::ALL.into_iter().enumerate() {
            let r = pattern_rect(i);
            let selected = self.design.pattern == p;
            let bg = if selected {
                to_color(ACCENT)
            } else if hovered == Some(Hit::Pattern(p)) {
                to_color(HOVER)
            } else {
                to_color(FIELD_BG)
            };
            fill(pixmap, r, 4.0, bg);
            outline(
                pixmap,
                r,
                4.0,
                if selected {
                    to_color(ACCENT)
                } else {
                    to_color(BORDER)
                },
                1.0,
            );
            let label = match p {
                Pattern::Solid => "Solid",
                Pattern::Calico => "Calico",
                Pattern::Tuxedo => "Tuxedo",
            };
            let tx = r.x() + (r.width() - label.len() as f32 * 6.0) * 0.5;
            let tc = if selected {
                to_color(BG)
            } else {
                to_color(TEXT)
            };
            text(pixmap, label, tx, r.y() + 7.0, tc);
        }

        // Colour rows
        for (i, slot) in Slot::ALL.into_iter().enumerate() {
            let r = slot_rect(i);
            let enabled = self.slot_enabled(slot);
            let active = slot == self.active;
            if active {
                fill(pixmap, r, 4.0, to_color(HOVER));
                outline(pixmap, r, 4.0, to_color(ACCENT), 1.0);
            } else if enabled && matches!(hovered, Some(Hit::Slot(s) | Hit::Hex(s)) if s == slot) {
                fill(pixmap, r, 4.0, to_color(HOVER));
            }
            let tc = if enabled {
                to_color(TEXT)
            } else {
                to_color(DIM_TEXT)
            };
            let c = self.color_of(slot);
            let swatch = if enabled {
                to_color(c)
            } else {
                to_color(Rgba8 { a: 60, ..c })
            };
            circle(
                pixmap,
                LEFT + 13.0,
                r.y() + 10.0,
                6.0,
                swatch,
                to_color(BORDER),
            );
            text(
                pixmap,
                slot.label(self.design.pattern),
                LEFT + 28.0,
                r.y() + 7.0,
                tc,
            );
            if !enabled {
                text(
                    pixmap,
                    "(not used)",
                    LEFT + 140.0,
                    r.y() + 7.0,
                    to_color(DIM_TEXT),
                );
                continue;
            }
            let hr = hex_rect(i);
            let hex_focused = active && self.focus == Some(Focus::Hex);
            let hex_box = rect(
                hr.x() + 2.0,
                hr.y() + 2.0,
                hr.width() - 4.0,
                hr.height() - 4.0,
            );
            fill(pixmap, hex_box, 3.0, to_color(FIELD_BG));
            if hex_focused {
                outline(pixmap, hex_box, 3.0, to_color(ACCENT), 1.0);
                text(
                    pixmap,
                    &self.hex_input,
                    hr.x() + 10.0,
                    r.y() + 7.0,
                    to_color(TEXT),
                );
                let cx = hr.x() + 10.0 + self.hex_input.len() as f32 * 6.0;
                fill(
                    pixmap,
                    rect(cx, r.y() + 5.0, 1.0, 10.0),
                    0.0,
                    to_color(ACCENT),
                );
            } else {
                // The bitmap font has no '#', so hex colours show as bare digits
                text(pixmap, &c.to_hex()[1..], hr.x() + 10.0, r.y() + 7.0, tc);
            }
        }

        // Tabby rings toggle
        draw_checkbox_indicator(
            pixmap,
            ox + LEFT + 6.0,
            oy + RINGS_Y + 9.0,
            self.design.tail_rings,
        );
        let rings_col = if hovered == Some(Hit::Rings) {
            Color::WHITE
        } else {
            to_color(TEXT)
        };
        text(
            pixmap,
            "Tabby rings on the tail",
            LEFT + 18.0,
            RINGS_Y + 6.0,
            rings_col,
        );
        let tuned = self.design.overrides.len();
        if tuned > 0 {
            let note = format!("{tuned} hand-tuned");
            let r = reset_rect();
            let nx = r.x() - 8.0 - note.len() as f32 * 6.0;
            text(pixmap, &note, nx, RINGS_Y + 6.0, to_color(DIM_TEXT));
            let hover = hovered == Some(Hit::ResetTuning);
            fill(
                pixmap,
                r,
                4.0,
                to_color(if hover { HOVER } else { FIELD_BG }),
            );
            outline(pixmap, r, 4.0, to_color(BORDER), 1.0);
            let fg = if hover { Color::WHITE } else { to_color(TEXT) };
            text(pixmap, "Reset", r.x() + 9.0, r.y() + 7.0, fg);
        }

        // Saturation/value square for the active hue, and the hue strip
        let hsv = self.hsv[self.active as usize];
        if self.sv_square.as_ref().is_none_or(|(h, _)| *h != hsv.h) {
            self.sv_square = gradient_pixmap(SV_W as u32, PICKER_H as u32, |fx, fy| {
                Hsv {
                    h: hsv.h,
                    s: fx,
                    v: 1.0 - fy,
                }
                .to_rgb()
            })
            .map(|pm| (hsv.h, pm));
        }
        if self.hue_strip.is_none() {
            self.hue_strip = gradient_pixmap(HUE_W as u32, PICKER_H as u32, |_, fy| {
                Hsv {
                    h: fy * 360.0,
                    s: 1.0,
                    v: 1.0,
                }
                .to_rgb()
            });
        }
        for (pm, x) in [
            (self.sv_square.as_ref().map(|(_, pm)| pm), LEFT),
            (self.hue_strip.as_ref(), HUE_X),
        ] {
            if let Some(pm) = pm {
                pixmap.draw_pixmap(
                    (ox + x) as i32,
                    (oy + PICKER_Y) as i32,
                    pm.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
        outline(pixmap, sv_rect(), 0.0, to_color(BORDER), 1.0);
        outline(pixmap, hue_rect(), 0.0, to_color(BORDER), 1.0);
        let mx = LEFT + hsv.s * SV_W;
        let my = PICKER_Y + (1.0 - hsv.v) * PICKER_H;
        let marker = if hsv.v > 0.6 && hsv.s < 0.5 {
            Color::BLACK
        } else {
            Color::WHITE
        };
        circle(
            pixmap,
            mx,
            my,
            4.0,
            to_color(self.color_of(self.active)),
            marker,
        );
        let hy = PICKER_Y + hsv.h / 360.0 * PICKER_H;
        outline(
            pixmap,
            rect(HUE_X - 2.0, hy - 2.0, HUE_W + 4.0, 4.0),
            1.5,
            Color::WHITE,
            1.5,
        );
        // The colour being edited, large, next to the picker
        let big = rect(
            HUE_X + HUE_W + 8.0,
            PICKER_Y,
            LEFT + LEFT_W - (HUE_X + HUE_W + 8.0),
            PICKER_H,
        );
        fill(pixmap, big, 4.0, to_color(self.color_of(self.active)));
        outline(pixmap, big, 4.0, to_color(BORDER), 1.0);

        // Presets
        for (i, &c) in PRESETS.iter().enumerate() {
            let r = preset_rect(i);
            let ring = if hovered == Some(Hit::Preset(i)) {
                Color::WHITE
            } else {
                to_color(BORDER)
            };
            circle(
                pixmap,
                r.x() + r.width() * 0.5,
                r.y() + 10.0,
                if hovered == Some(Hit::Preset(i)) {
                    9.0
                } else {
                    8.0
                },
                to_color(c),
                ring,
            );
        }

        // Preview: the cat in four poses, and the tray icon
        let panel = rect(RIGHT, PREVIEW_Y, RIGHT_W, PREVIEW_H);
        fill(pixmap, panel, 6.0, to_color(PREVIEW_BG));
        if self.preview.is_none() {
            let coat = self.design.to_coat();
            let sprites = self
                .preview_cats
                .iter_mut()
                .map(|cat| {
                    cat.coat = coat.clone();
                    render_cropped(cat)
                })
                .collect();
            let icon = Pixmap::new(32, 32).map(|mut icon| {
                render_tray_icon(&coat, false, false, false, &mut icon.as_mut());
                icon
            });
            self.preview = Some(Preview { sprites, icon });
        }
        let icon_tile = rect(RIGHT, ICON_Y, RIGHT_W, 40.0);
        fill(pixmap, icon_tile, 6.0, to_color(FIELD_BG));
        if let Some(preview) = &self.preview {
            for (i, sprite) in preview.sprites.iter().enumerate() {
                let Some(sprite) = sprite else { continue };
                let cx = ox + RIGHT + RIGHT_W * (0.25 + 0.5 * (i % 2) as f32);
                let cy = oy + PREVIEW_Y + PREVIEW_H * (0.25 + 0.5 * (i / 2) as f32);
                pixmap.draw_pixmap(
                    (cx - sprite.width() as f32 * 0.5) as i32,
                    (cy - sprite.height() as f32 * 0.5) as i32,
                    sprite.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
            if let Some(icon) = &preview.icon {
                pixmap.draw_pixmap(
                    (ox + RIGHT + 8.0) as i32,
                    (oy + ICON_Y + 4.0) as i32,
                    icon.as_ref(),
                    &PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
        text(
            pixmap,
            "Tray icon",
            RIGHT + 50.0,
            ICON_Y + 10.0,
            to_color(DIM_TEXT),
        );
        let shown_name = if self.design.name.trim().is_empty() {
            "(no name yet)"
        } else {
            self.design.name.trim()
        };
        text(
            pixmap,
            shown_name,
            RIGHT + 50.0,
            ICON_Y + 23.0,
            to_color(TEXT),
        );

        // Status and buttons
        if let Some((msg, ok)) = &self.status {
            text(
                pixmap,
                msg,
                RIGHT,
                STATUS_Y,
                if *ok { to_color(OK) } else { to_color(ERROR) },
            );
        } else if self.original.is_some() {
            text(
                pixmap,
                "Saving updates this coat.",
                RIGHT,
                STATUS_Y,
                to_color(DIM_TEXT),
            );
        } else {
            text(
                pixmap,
                "Saving adds a new coat.",
                RIGHT,
                STATUS_Y,
                to_color(DIM_TEXT),
            );
        }
        for (hit, r, label) in button_rects() {
            let is_hover = hovered == Some(hit);
            let (bg, fg) = match hit {
                Hit::Save if is_hover => (Color::WHITE, to_color(BG)),
                Hit::Save => (to_color(ACCENT), to_color(BG)),
                _ if is_hover => (to_color(HOVER), Color::WHITE),
                _ => (to_color(FIELD_BG), to_color(TEXT)),
            };
            fill(pixmap, r, 5.0, bg);
            if hit != Hit::Save {
                outline(pixmap, r, 5.0, to_color(BORDER), 1.0);
            }
            let tx = r.x() + (r.width() - label.len() as f32 * 6.0) * 0.5;
            text(pixmap, label, tx, r.y() + 9.0, fg);
        }
    }
}

/// An opaque `w` x `h` pixmap whose pixel at fractions (`fx`, `fy`) of the size is `f(fx, fy)`
fn gradient_pixmap(w: u32, h: u32, f: impl Fn(f32, f32) -> Rgba8) -> Option<Pixmap> {
    let mut pm = Pixmap::new(w, h)?;
    let (wf, hf) = ((w.max(2) - 1) as f32, (h.max(2) - 1) as f32);
    for (i, px) in pm.pixels_mut().iter_mut().enumerate() {
        let (x, y) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
        let c = f(x / wf, y / hf);
        *px = PremultipliedColorU8::from_rgba(c.r, c.g, c.b, 255)?;
    }
    Some(pm)
}

#[cfg(test)]
impl Designer {
    /// Window position of a point given in content coordinates
    fn at(&self, x: f32, y: f32) -> (f32, f32) {
        let (ox, oy) = self.origin();
        (ox + x, oy + y)
    }

    fn click(&mut self, x: f32, y: f32, coats: &mut CoatLibrary, cat: &mut CatSim) -> bool {
        let (x, y) = self.at(x, y);
        let close = self.handle(DesignerInput::Press(x, y), coats, cat);
        self.handle(DesignerInput::Release, coats, cat);
        close
    }

    fn type_text(&mut self, s: &str, coats: &mut CoatLibrary, cat: &mut CatSim) {
        for c in s.chars() {
            self.handle(DesignerInput::Char(c), coats, cat);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coat::{COATS_FILE, CoatStyle};
    use glam::Vec2;

    fn centre(r: Rect) -> (f32, f32) {
        (r.x() + r.width() * 0.5, r.y() + r.height() * 0.5)
    }

    #[test]
    fn hsv_roundtrips_every_preset() {
        for c in PRESETS {
            assert_eq!(Hsv::from_rgb(c).to_rgb(), c);
        }
        let grey = Hsv::from_rgb(rgb(128, 128, 128));
        assert_eq!(grey.s, 0.0);
    }

    #[test]
    fn every_label_uses_glyphs_the_font_has() {
        let mut labels: Vec<&str> = vec![
            "COAT DESIGNER",
            "PICK A COLOUR",
            "Type a name",
            "Tabby rings on the tail",
            "(not used)",
            "(no name yet)",
            "Saving updates this coat.",
            "Saving adds a new coat.",
            "Give the coat a name first.",
            "That name belongs to a built-in coat.",
            "Another coat already has that name.",
            "Could not save coats.toml (see terminal).",
            "Names cannot contain quote marks.",
            "15 hand-tuned",
            "Reset",
            "Saved . Your cat is wearing it.",
        ];
        for p in Pattern::ALL {
            labels.extend(Slot::ALL.map(|s| s.label(p)));
        }
        labels.extend(button_rects().map(|(_, _, l)| l));
        for label in labels {
            assert!(
                label.chars().all(|c| c == ' ' || glyph_5x7(c) != [0; 7]),
                "{label}"
            );
            // Status lines must fit the right column
            assert!(label.len() as f32 * 6.0 <= RIGHT_W, "{label}");
        }
    }

    #[test]
    fn designing_and_saving_a_coat_writes_coats_toml_and_dresses_the_cat() {
        let dir = std::env::temp_dir().join(format!("catsor_designer_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(COATS_FILE);
        std::fs::write(
            &path,
            "# my notes\n[[coat]]\nname = \"Old\"\nbase = \"#101010\"\n",
        )
        .unwrap();
        let mut coats = CoatLibrary::load_from(Some(path.clone()));
        let mut cat = CatSim::new(Vec2::ZERO, 1.0, CoatStyle::Silver);
        let mut d = Designer::new(&cat.coat, &coats);
        assert_eq!(d.design.name, "My Silver");

        // Rename it
        d.click(LEFT + 10.0, NAME_Y + 10.0, &mut coats, &mut cat);
        for _ in 0.."My Silver".len() {
            d.handle(DesignerInput::Backspace, &mut coats, &mut cat);
        }
        d.type_text("Mochi:\"", &mut coats, &mut cat);
        assert_eq!(d.design.name, "Mochi", "':' and '\"' are rejected");
        d.handle(DesignerInput::Enter, &mut coats, &mut cat);

        // Calico pattern, fur from a preset, patches typed as hex, spots from the picker
        let (px, py) = centre(pattern_rect(1));
        d.click(px, py, &mut coats, &mut cat);
        let (px, py) = centre(preset_rect(1));
        d.click(px, py, &mut coats, &mut cat);
        assert_eq!(d.design.base, PRESETS[1]);
        let (hx, hy) = centre(hex_rect(1));
        d.click(hx, hy, &mut coats, &mut cat);
        assert_eq!(d.active, Slot::Patch);
        for _ in 0..6 {
            d.handle(DesignerInput::Backspace, &mut coats, &mut cat);
        }
        d.type_text("3366AA", &mut coats, &mut cat);
        assert_eq!(d.design.patch, rgb(0x33, 0x66, 0xaa));
        let (sx, sy) = centre(slot_rect(2));
        d.click(sx, sy, &mut coats, &mut cat);
        assert_eq!(d.active, Slot::Spot);
        // Top-right of the saturation/value square is the pure hue (red at the top of the strip)
        let (hx, hy) = d.at(HUE_X + 5.0, PICKER_Y);
        d.handle(DesignerInput::Press(hx, hy), &mut coats, &mut cat);
        d.handle(DesignerInput::Release, &mut coats, &mut cat);
        let (vx, vy) = d.at(LEFT + SV_W - 0.5, PICKER_Y);
        d.handle(DesignerInput::Press(vx, vy), &mut coats, &mut cat);
        // Dragging past the corner clamps to full saturation and value
        let (vx, vy) = d.at(LEFT + SV_W + 10.0, PICKER_Y - 10.0);
        d.handle(DesignerInput::Move(vx, vy), &mut coats, &mut cat);
        assert_eq!(d.design.spot, rgb(255, 0, 0));
        // Dragging down to black keeps the hue for when it is dragged back up
        let (bx, by) = d.at(LEFT + SV_W - 0.5, PICKER_Y + PICKER_H + 20.0);
        d.handle(DesignerInput::Move(bx, by), &mut coats, &mut cat);
        assert_eq!(d.design.spot, rgb(0, 0, 0));
        d.handle(DesignerInput::Move(vx, vy), &mut coats, &mut cat);
        assert_eq!(d.design.spot, rgb(255, 0, 0));
        d.handle(DesignerInput::Release, &mut coats, &mut cat);

        let mut pixmap = Pixmap::new(DESIGNER_W + 40, DESIGNER_H).unwrap();
        d.set_window_size(DESIGNER_W + 40, DESIGNER_H);
        d.render(&mut pixmap.as_mut());
        assert!(d.take_dirty());

        // Enter with nothing focused saves
        assert!(!d.handle(DesignerInput::Enter, &mut coats, &mut cat));
        assert_eq!(d.status.as_ref().map(|s| s.1), Some(true), "{:?}", d.status);
        assert_eq!(cat.coat.id, CoatId::Custom("Mochi".into()));
        assert_eq!(cat.coat.patch, rgb(0x33, 0x66, 0xaa));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.starts_with("# my notes\n[[coat]]\nname = \"Old\""),
            "{written}"
        );
        assert!(
            written.contains("name = \"Mochi\"\npattern = \"calico\""),
            "{written}"
        );
        assert_eq!(coats.custom().len(), 2);

        // Renaming and saving again updates the same entry instead of adding one
        d.click(LEFT + 10.0, NAME_Y + 10.0, &mut coats, &mut cat);
        d.type_text("2", &mut coats, &mut cat);
        d.handle(DesignerInput::Enter, &mut coats, &mut cat);
        d.handle(DesignerInput::Enter, &mut coats, &mut cat);
        assert_eq!(coats.custom().len(), 2);
        assert_eq!(&*cat.coat.name, "Mochi2");

        // A name clash is refused without touching the file
        d.click(LEFT + 10.0, NAME_Y + 10.0, &mut coats, &mut cat);
        for _ in 0..6 {
            d.handle(DesignerInput::Backspace, &mut coats, &mut cat);
        }
        d.type_text("old", &mut coats, &mut cat);
        d.handle(DesignerInput::Enter, &mut coats, &mut cat);
        let before = std::fs::read_to_string(&path).unwrap();
        d.handle(DesignerInput::Enter, &mut coats, &mut cat);
        assert_eq!(d.status.as_ref().map(|s| s.1), Some(false));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);

        // Escape closes once nothing is focused
        assert!(d.handle(DesignerInput::Escape, &mut coats, &mut cat));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn solid_pattern_disables_patch_rows_and_randomize_keeps_the_name() {
        let mut coats = CoatLibrary::from_toml("");
        let mut cat = CatSim::new(Vec2::ZERO, 1.0, CoatStyle::Calico);
        let mut d = Designer::new(&cat.coat, &coats);
        let (sx, sy) = centre(slot_rect(1));
        d.click(sx, sy, &mut coats, &mut cat);
        assert_eq!(d.active, Slot::Patch);
        let (px, py) = centre(pattern_rect(0));
        d.click(px, py, &mut coats, &mut cat);
        assert_eq!(d.active, Slot::Base, "patch row is unused for solid coats");
        d.click(sx, sy, &mut coats, &mut cat);
        assert_eq!(d.active, Slot::Base);

        let name = d.design.name.clone();
        for _ in 0..10 {
            let (rx, ry) = centre(button_rects()[0].1);
            d.click(rx, ry, &mut coats, &mut cat);
            assert_eq!(d.design.name, name);
        }
        let (cx, cy) = centre(button_rects()[1].1);
        assert!(d.click(cx, cy, &mut coats, &mut cat));
    }
}
