//! Coat colours and markings.
//!
//! A [`Coat`] is plain data: a marking [`Pattern`] plus every colour the renderer and the tray
//! icon use. There are three sources:
//! - **built-in** coats ([`CoatStyle`]), hand-tuned presets;
//! - **custom** coats, read from `coats.toml` next to the config file. Each one needs only a
//!   name and a base colour; everything it leaves out is derived ([`Coat::derived`]);
//! - **random** coats, rolled from a `u32` seed with simplified cat colour genetics
//!   ([`Coat::random`]). The seed is what gets saved, so the same cat comes back after a restart.

use crate::cat::CatSim;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// File name of the custom coat list, in the same directory as `config.toml`
pub const COATS_FILE: &str = "coats.toml";
/// Longest coat name kept, so the tray row still fits the menu
pub(crate) const MAX_NAME_CHARS: usize = 22;
/// How often `coats.toml` is checked for edits while the app runs
const RELOAD_CHECK_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> Rgba8 {
    Rgba8 { r, g, b, a: 255 }
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Rgba8 {
    Rgba8 { r, g, b, a }
}

const WHITE: Rgba8 = rgb(255, 255, 255);
const CREAM_WHITE: Rgba8 = rgb(250, 247, 242);
const SKIN_PINK: Rgba8 = rgb(240, 196, 204);
const ICON_EAR_PINK: Rgba8 = rgb(214, 136, 136);
const ICON_NOSE_PINK: Rgba8 = rgb(214, 128, 140);

impl Rgba8 {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    fn map_rgb(self, f: impl Fn(f32) -> f32) -> Self {
        let ch = |v: u8| f(v as f32).round().clamp(0.0, 255.0) as u8;
        Self {
            r: ch(self.r),
            g: ch(self.g),
            b: ch(self.b),
            a: self.a,
        }
    }

    /// Multiplies the colour channels by `f`, keeping alpha
    fn scale(self, f: f32) -> Self {
        self.map_rgb(|v| v * f)
    }

    /// Moves the colour channels `t` of the way toward `other`, keeping alpha
    fn mix(self, other: Rgba8, t: f32) -> Self {
        Self {
            r: (self.r as f32 + (other.r as f32 - self.r as f32) * t).round() as u8,
            g: (self.g as f32 + (other.g as f32 - self.g as f32) * t).round() as u8,
            b: (self.b as f32 + (other.b as f32 - self.b as f32) * t).round() as u8,
            a: self.a,
        }
    }

    fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// Perceived brightness in [0, 1]
    fn luma(self) -> f32 {
        (0.2126 * self.r as f32 + 0.7152 * self.g as f32 + 0.0722 * self.b as f32) / 255.0
    }

    /// Parses `#rrggbb` or `#rrggbbaa` (the `#` is optional)
    pub fn parse_hex(s: &str) -> Option<Self> {
        let hex = s.trim().trim_start_matches('#');
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        Some(Self {
            r: byte(0)?,
            g: byte(2)?,
            b: byte(4)?,
            a: if hex.len() == 8 { byte(6)? } else { 255 },
        })
    }

    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

/// Where the patches go. The shapes are drawn by `render.rs`; the coat only picks the layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pattern {
    /// One colour all over
    Solid,
    /// Patches on the back, flank and one side of the head, with a patch-coloured ringed tail
    Calico,
    /// A mask over both sides of the head and a saddle on the back
    Tuxedo,
}

impl Pattern {
    pub const ALL: [Pattern; 3] = [Pattern::Solid, Pattern::Calico, Pattern::Tuxedo];

    pub const fn as_config_str(self) -> &'static str {
        match self {
            Pattern::Solid => "solid",
            Pattern::Calico => "calico",
            Pattern::Tuxedo => "tuxedo",
        }
    }

    pub fn from_config_str(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|p| p.as_config_str().eq_ignore_ascii_case(s.trim()))
    }
}

/// The built-in coats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoatStyle {
    /// White base with warm ginger & charcoal patches (centerpiece from reference)
    Calico,
    /// Warm cinnamon/taupe brown with darker flank shading
    Brown,
    /// Sleek dark slate charcoal with deep edge shading
    Charcoal,
    /// White & charcoal tuxedo with nose blaze
    Tuxedo,
    /// Pearl silver-white with cool gray shading
    Silver,
}

impl CoatStyle {
    pub const ALL: [CoatStyle; 5] = [
        CoatStyle::Calico,
        CoatStyle::Brown,
        CoatStyle::Charcoal,
        CoatStyle::Tuxedo,
        CoatStyle::Silver,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            CoatStyle::Calico => "Calico",
            CoatStyle::Brown => "Brown",
            CoatStyle::Charcoal => "Charcoal",
            CoatStyle::Tuxedo => "Tuxedo",
            CoatStyle::Silver => "Silver",
        }
    }

    pub const fn as_config_str(self) -> &'static str {
        match self {
            CoatStyle::Calico => "calico",
            CoatStyle::Brown => "brown",
            CoatStyle::Charcoal => "charcoal",
            CoatStyle::Tuxedo => "tuxedo",
            CoatStyle::Silver => "silver",
        }
    }

    pub fn from_config_str(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.as_config_str().eq_ignore_ascii_case(s))
    }
}

/// Which coat is selected; this is what the config file stores
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoatId {
    Builtin(CoatStyle),
    /// A coat from `coats.toml`, by name
    Custom(Arc<str>),
    Random(u32),
}

impl CoatId {
    /// `calico`, a custom coat's name, `random:<8 hex digits>`, or `random` for a fresh roll
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        if let Some(style) = CoatStyle::from_config_str(s) {
            return Some(CoatId::Builtin(style));
        }
        if s.eq_ignore_ascii_case("random") {
            return Some(CoatId::Random(fresh_seed()));
        }
        if let Some((prefix, hex)) = s.split_once(':')
            && prefix.eq_ignore_ascii_case("random")
        {
            return u32::from_str_radix(hex.trim(), 16).ok().map(CoatId::Random);
        }
        Some(CoatId::Custom(s.into()))
    }

    pub fn to_config_string(&self) -> String {
        match self {
            CoatId::Builtin(style) => style.as_config_str().to_string(),
            CoatId::Custom(name) => name.to_string(),
            CoatId::Random(seed) => format!("random:{seed:08x}"),
        }
    }
}

impl Default for CoatId {
    fn default() -> Self {
        CoatId::Builtin(CoatStyle::Calico)
    }
}

/// Colours of the front-facing tray icon head. They are tuned separately from the top-down
/// body because the icon is tiny and sits on a dark panel
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconColors {
    pub base: Rgba8,
    pub shade: Rgba8,
    pub outline: Rgba8,
    pub ear_inner: Rgba8,
    pub nose: Rgba8,
    pub eyes: Rgba8,
    /// Calico and Tuxedo markings
    pub patch: Rgba8,
    pub spot: Rgba8,
    /// Calico: the patch-coloured left ear
    pub patch_ear: Rgba8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Coat {
    pub id: CoatId,
    /// Shown in the tray menu
    pub name: Arc<str>,
    pub pattern: Pattern,
    pub base: Rgba8,
    /// Darker rim along the flanks and around the head
    pub shade: Rgba8,
    pub outline: Rgba8,
    /// Muscle creases on the body
    pub crease: Rgba8,
    /// Outer ear, and its inner triangle
    pub ears: Rgba8,
    pub ear_inner: Rgba8,
    pub nose: Rgba8,
    pub paws: Rgba8,
    /// Exposed belly and its pink tummy tint when rolled over
    pub belly: Rgba8,
    pub tummy: Rgba8,
    pub chin: Rgba8,
    /// Patterned coats only: the back patch, the flank patch and the shoulder spot
    pub patch: Rgba8,
    pub patch_shade: Rgba8,
    pub spot: Rgba8,
    pub tail: Rgba8,
    pub tail_shade: Rgba8,
    pub tail_rings: Option<Rgba8>,
    pub icon: IconColors,
}

impl From<CoatStyle> for Coat {
    fn from(style: CoatStyle) -> Self {
        Coat::builtin(style)
    }
}

impl Coat {
    pub fn builtin(style: CoatStyle) -> Self {
        let id = CoatId::Builtin(style);
        let name: Arc<str> = style.label().into();
        match style {
            CoatStyle::Calico => Coat {
                id,
                name,
                pattern: Pattern::Calico,
                base: rgb(238, 235, 230),
                shade: rgb(210, 205, 198),
                outline: rgb(68, 56, 48),
                crease: rgba(168, 160, 150, 210),
                ears: rgb(226, 134, 66),
                ear_inner: rgb(178, 88, 34),
                nose: rgb(68, 52, 46),
                paws: rgb(236, 234, 230),
                belly: rgba(248, 245, 240, 248),
                tummy: rgba(244, 198, 198, 155),
                chin: rgba(246, 245, 242, 245),
                patch: rgb(220, 124, 54),
                patch_shade: rgb(190, 96, 34),
                spot: rgb(58, 60, 66),
                tail: rgb(220, 124, 54),
                tail_shade: rgb(182, 92, 32),
                tail_rings: Some(rgba(178, 82, 24, 235)),
                icon: IconColors {
                    base: rgb(242, 239, 234),
                    shade: rgb(218, 212, 204),
                    outline: rgb(68, 56, 48),
                    ear_inner: rgb(218, 138, 132),
                    nose: rgb(216, 126, 136),
                    eyes: rgb(58, 48, 42),
                    patch: rgb(226, 132, 60),
                    spot: rgb(62, 64, 72),
                    patch_ear: rgb(186, 96, 40),
                },
            },
            CoatStyle::Brown => {
                let base = rgb(154, 118, 88);
                let shade = rgb(126, 94, 68);
                Coat {
                    id,
                    name,
                    pattern: Pattern::Solid,
                    base,
                    shade,
                    outline: rgb(56, 38, 24),
                    crease: rgba(94, 66, 44, 225),
                    ears: rgb(146, 110, 80),
                    ear_inner: rgb(98, 66, 42),
                    nose: rgb(44, 28, 18),
                    paws: rgb(138, 102, 74),
                    belly: rgba(218, 188, 156, 244),
                    tummy: rgba(236, 192, 174, 145),
                    chin: rgba(216, 186, 154, 240),
                    patch: base,
                    patch_shade: shade,
                    spot: base,
                    tail: base,
                    tail_shade: shade,
                    tail_rings: None,
                    icon: solid_icon(
                        rgb(162, 124, 92),
                        rgb(134, 100, 72),
                        rgb(56, 38, 24),
                        rgb(196, 132, 118),
                        rgb(74, 46, 34),
                        rgb(46, 30, 18),
                    ),
                }
            }
            CoatStyle::Charcoal => {
                let base = rgb(62, 62, 66);
                let shade = rgb(42, 42, 46);
                Coat {
                    id,
                    name,
                    pattern: Pattern::Solid,
                    base,
                    shade,
                    outline: rgb(18, 18, 20),
                    crease: rgba(26, 26, 30, 235),
                    ears: rgb(56, 56, 60),
                    ear_inner: rgb(32, 32, 36),
                    nose: rgb(16, 16, 18),
                    paws: rgb(48, 48, 52),
                    belly: rgba(108, 108, 116, 242),
                    tummy: rgba(152, 132, 138, 135),
                    chin: rgba(102, 102, 110, 235),
                    patch: base,
                    patch_shade: shade,
                    spot: base,
                    tail: base,
                    tail_shade: shade,
                    tail_rings: None,
                    icon: solid_icon(
                        rgb(112, 116, 128),
                        rgb(88, 92, 104),
                        rgb(28, 30, 36),
                        rgb(168, 122, 132),
                        rgb(46, 48, 56),
                        rgb(26, 28, 34),
                    ),
                }
            }
            CoatStyle::Tuxedo => {
                let base = rgb(232, 233, 236);
                let shade = rgb(202, 204, 210);
                Coat {
                    id,
                    name,
                    pattern: Pattern::Tuxedo,
                    base,
                    shade,
                    outline: rgb(36, 38, 42),
                    crease: rgba(156, 158, 166, 210),
                    ears: rgb(46, 48, 52),
                    ear_inner: rgb(26, 28, 32),
                    nose: rgb(54, 54, 58),
                    paws: rgb(236, 234, 230),
                    belly: rgba(246, 247, 250, 252),
                    tummy: rgba(240, 198, 204, 145),
                    chin: rgba(246, 245, 242, 245),
                    patch: rgb(48, 50, 56),
                    patch_shade: rgb(48, 50, 56),
                    spot: rgb(78, 80, 88),
                    tail: base,
                    tail_shade: shade,
                    tail_rings: None,
                    icon: IconColors {
                        patch: rgb(46, 48, 56),
                        ..solid_icon(
                            rgb(238, 240, 244),
                            rgb(212, 215, 222),
                            rgb(34, 36, 42),
                            rgb(164, 118, 128),
                            rgb(216, 132, 146),
                            rgb(34, 36, 42),
                        )
                    },
                }
            }
            CoatStyle::Silver => {
                let base = rgb(222, 223, 226);
                let shade = rgb(194, 196, 202);
                Coat {
                    id,
                    name,
                    pattern: Pattern::Solid,
                    base,
                    shade,
                    outline: rgb(110, 112, 118),
                    crease: rgba(162, 164, 170, 215),
                    ears: rgb(216, 218, 222),
                    ear_inner: rgb(172, 174, 182),
                    nose: rgb(92, 92, 98),
                    paws: rgb(236, 234, 230),
                    belly: rgba(245, 246, 249, 250),
                    tummy: rgba(240, 196, 206, 150),
                    chin: rgba(246, 245, 242, 245),
                    patch: base,
                    patch_shade: shade,
                    spot: base,
                    tail: base,
                    tail_shade: shade,
                    tail_rings: None,
                    icon: solid_icon(
                        rgb(224, 226, 232),
                        rgb(196, 198, 206),
                        rgb(96, 100, 110),
                        rgb(206, 152, 162),
                        rgb(198, 126, 138),
                        rgb(58, 60, 68),
                    ),
                }
            }
        }
    }

    /// A full coat worked out from a base colour and, for patterned coats, the patch colours.
    /// Missing patch colours fall back to the ones the built-in Calico and Tuxedo use
    pub fn derived(
        id: CoatId,
        name: Arc<str>,
        pattern: Pattern,
        base: Rgba8,
        patch: Option<Rgba8>,
        spot: Option<Rgba8>,
    ) -> Self {
        let base = base.with_alpha(255);
        let luma = base.luma();
        let shade = base.scale(0.68 + 0.2 * luma);
        let (patch, patch_shade, spot) = match pattern {
            Pattern::Solid => (base, shade, base),
            Pattern::Calico => {
                let patch = patch.unwrap_or(rgb(220, 124, 54)).with_alpha(255);
                let spot = spot.unwrap_or(rgb(58, 60, 66)).with_alpha(255);
                (patch, patch.scale(0.86), spot)
            }
            Pattern::Tuxedo => {
                let patch = patch.unwrap_or(rgb(48, 50, 56)).with_alpha(255);
                let spot = spot.unwrap_or(patch.mix(WHITE, 0.15)).with_alpha(255);
                (patch, patch, spot)
            }
        };
        let patterned = pattern != Pattern::Solid;

        // The outline follows the darkest colour on the coat, darkened so its brightest
        // channel lands near 64 (never brighter than 3/4 of the source), and is kept at most
        // 40% as bright as the base so dark fur still gets a visible edge
        let darkest = if patterned {
            [base, patch, spot]
                .into_iter()
                .min_by(|a, b| a.luma().total_cmp(&b.luma()))
                .unwrap_or(base)
        } else {
            base
        };
        let peak = darkest.r.max(darkest.g).max(darkest.b) as f32;
        let mut outline = if peak > 0.0 {
            darkest.scale((0.75 * peak).min(64.0) / peak)
        } else {
            darkest
        };
        if outline.luma() > 0.4 * luma {
            outline = outline.scale(0.4 * luma / outline.luma());
        }

        let (ears, ear_inner) = if patterned {
            (patch, patch.scale(0.72))
        } else {
            (base.scale(0.95), base.scale(0.64))
        };
        let (tail, tail_shade, tail_rings) = if pattern == Pattern::Calico {
            (
                patch,
                patch.scale(0.83),
                Some(patch.scale(0.8).with_alpha(235)),
            )
        } else {
            (base, shade, None)
        };

        let icon_base = base.mix(WHITE, if luma < 0.35 { 0.3 } else { 0.06 });
        let icon = IconColors {
            base: icon_base,
            shade: icon_base.scale(0.86),
            outline,
            ear_inner: icon_base.mix(ICON_EAR_PINK, 0.6),
            nose: if luma > 0.6 {
                ICON_NOSE_PINK
            } else {
                base.scale(0.45)
            },
            eyes: outline,
            patch,
            spot,
            patch_ear: patch.scale(0.84),
        };

        Coat {
            id,
            name,
            pattern,
            base,
            shade,
            outline,
            crease: base.scale(0.64).with_alpha(215),
            ears,
            ear_inner,
            nose: outline.scale(0.85),
            paws: if patterned { base } else { base.scale(0.9) },
            belly: base.mix(CREAM_WHITE, 0.45).with_alpha(246),
            tummy: base.mix(SKIN_PINK, 0.5 + 0.3 * luma).with_alpha(145),
            chin: base.mix(CREAM_WHITE, 0.4).with_alpha(240),
            patch,
            patch_shade,
            spot,
            tail,
            tail_shade,
            tail_rings,
            icon,
        }
    }

    /// A random but plausible coat. Colours follow simplified cat colour genetics: one
    /// eumelanin colour (black, chocolate, cinnamon) or orange, the dilute gene for their pale
    /// versions (blue, lilac, fawn, cream), tortoiseshell mixes of both, and three amounts of
    /// white spotting (none, tuxedo, and bicolor or calico). Orange cats always carry tabby
    /// rings on the tail
    pub fn random(seed: u32) -> Self {
        let mut rng = SeedRng::new(seed);
        let dilute = rng.chance(0.25);
        let (dark_name, dark) = {
            let roll = rng.unit();
            match (roll < 0.74, roll < 0.92, dilute) {
                (true, _, false) => ("Black", rgb(46, 45, 49)),
                (true, _, true) => ("Blue", rgb(118, 124, 136)),
                (false, true, false) => ("Chocolate", rgb(96, 64, 46)),
                (false, true, true) => ("Lilac", rgb(170, 158, 160)),
                (false, false, false) => ("Cinnamon", rgb(142, 94, 62)),
                (false, false, true) => ("Fawn", rgb(196, 168, 148)),
            }
        };
        let (orange_name, orange) = if dilute {
            ("Cream", rgb(228, 190, 150))
        } else {
            ("Ginger", rgb(214, 128, 62))
        };
        let dark = rng.jitter(dark);
        let orange = rng.jitter(orange);
        let white = rng.jitter_light(rgb(240, 238, 234));

        // Dominant white masks everything else
        let all_white = rng.chance(0.04);
        let tortie = rng.chance(0.3);
        let is_orange = !tortie && rng.chance(0.3);
        let spotting = match rng.unit() {
            u if u < 0.45 => 0,
            u if u < 0.70 => 1,
            _ => 2,
        };
        let (main_name, main) = if is_orange {
            (orange_name, orange)
        } else {
            (dark_name, dark)
        };

        let (pattern, base, patch, spot, name) = if all_white {
            (Pattern::Solid, white, None, None, "White".to_string())
        } else if tortie {
            let prefix = if dilute { "Dilute " } else { "" };
            match spotting {
                // Brindled tortoiseshell: orange and a blend of both over the dark base
                0 => (
                    Pattern::Calico,
                    dark,
                    Some(orange),
                    Some(orange.mix(dark, 0.5)),
                    format!("{prefix}tortie"),
                ),
                1 => (
                    Pattern::Tuxedo,
                    white,
                    Some(dark),
                    Some(orange),
                    format!("{prefix}tortie & white"),
                ),
                _ => (
                    Pattern::Calico,
                    white,
                    Some(orange),
                    Some(dark),
                    format!("{prefix}calico"),
                ),
            }
        } else {
            match spotting {
                0 => (Pattern::Solid, main, None, None, main_name.to_string()),
                1 => (
                    Pattern::Tuxedo,
                    white,
                    Some(main),
                    None,
                    format!("{main_name} tuxedo"),
                ),
                _ => (
                    Pattern::Calico,
                    white,
                    Some(main),
                    Some(main.scale(0.9)),
                    format!("{main_name} & white"),
                ),
            }
        };

        let mut coat = Coat::derived(
            CoatId::Random(seed),
            capitalize(&name).into(),
            pattern,
            base,
            patch,
            spot,
        );
        let orange_tail = coat.tail == orange;
        if is_orange || orange_tail {
            let tabby_base = if pattern == Pattern::Solid {
                base
            } else {
                orange
            };
            coat.tail_rings = Some(tabby_base.scale(0.78).with_alpha(235));
        } else if pattern == Pattern::Calico {
            // Bicolors keep a plain tail in their colour
            coat.tail_rings = None;
        }
        coat
    }

    /// Swatch colours for the tray menu: `(base, accent, outline)`
    pub fn swatch(&self) -> (Rgba8, Rgba8, Rgba8) {
        let accent = match self.pattern {
            Pattern::Solid => self.shade,
            _ => self.patch,
        };
        (self.base, accent, self.outline)
    }

    /// This coat as a `[[coat]]` entry for `coats.toml`, with every colour written out, so it
    /// can be used as the starting point for a custom coat
    pub fn to_toml_entry(&self, name: &str) -> String {
        let mut out = String::from("[[coat]]\n");
        let _ = writeln!(out, "name = \"{}\"", name.replace('"', "'"));
        let _ = writeln!(out, "pattern = \"{}\"", self.pattern.as_config_str());
        let mut color = |key: &str, c: Rgba8| {
            let _ = writeln!(out, "{key} = \"{}\"", c.to_hex());
        };
        color("base", self.base);
        if self.pattern != Pattern::Solid {
            color("patch", self.patch);
            color("patch_shade", self.patch_shade);
            color("spot", self.spot);
        }
        color("shade", self.shade);
        color("outline", self.outline);
        color("crease", self.crease);
        color("ears", self.ears);
        color("ear_inner", self.ear_inner);
        color("nose", self.nose);
        color("paws", self.paws);
        color("belly", self.belly);
        color("tummy", self.tummy);
        color("chin", self.chin);
        color("tail", self.tail);
        color("tail_shade", self.tail_shade);
        color("eyes", self.icon.eyes);
        let rings = self
            .tail_rings
            .map_or_else(|| "none".to_string(), Rgba8::to_hex);
        let _ = writeln!(out, "tail_rings = \"{rings}\"");
        out
    }
}

const fn solid_icon(
    base: Rgba8,
    shade: Rgba8,
    outline: Rgba8,
    ear_inner: Rgba8,
    nose: Rgba8,
    eyes: Rgba8,
) -> IconColors {
    IconColors {
        base,
        shade,
        outline,
        ear_inner,
        nose,
        eyes,
        patch: base,
        spot: base,
        patch_ear: ear_inner,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// A seed for a new random coat, different on every call
pub fn fresh_seed() -> u32 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    let mixed = splitmix64(
        nanos ^ ((std::process::id() as u64) << 32) ^ COUNTER.fetch_add(1, Ordering::Relaxed),
    );
    (mixed >> 32) as u32
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// Small deterministic RNG for rolling random coats from their seed
struct SeedRng(u64);

impl SeedRng {
    fn new(seed: u32) -> Self {
        Self(seed as u64 ^ 0x5eed_c0a7_0000_0000)
    }

    fn unit(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(1);
        (splitmix64(self.0) >> 40) as f32 / (1u64 << 24) as f32
    }

    fn chance(&mut self, p: f32) -> bool {
        self.unit() < p
    }

    fn range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.unit()
    }

    /// Varies brightness by up to ±8% and each channel by a few levels, less on dark colours
    /// where a small shift already reads as a tint
    fn jitter(&mut self, c: Rgba8) -> Rgba8 {
        let bright = self.range(0.92, 1.08);
        let c = c.scale(bright);
        let spread = 2.0 + 4.0 * c.luma();
        let mut ch = |v: u8| {
            (v as f32 + self.range(-spread, spread))
                .round()
                .clamp(0.0, 255.0) as u8
        };
        rgb(ch(c.r), ch(c.g), ch(c.b))
    }

    /// A gentler jitter for white, which would otherwise turn grey or clip
    fn jitter_light(&mut self, c: Rgba8) -> Rgba8 {
        let mut ch = |v: u8| (v as f32 + self.range(-5.0, 4.0)).round().clamp(0.0, 255.0) as u8;
        rgb(ch(c.r), ch(c.g), ch(c.b))
    }
}

/// Tabby rings derived from the tail colour (`tail_rings = "auto"`)
fn auto_tail_rings(coat: &Coat) -> Rgba8 {
    coat.tail.scale(0.8).with_alpha(235)
}

/// The parts of a coat the in-app designer edits. Everything else is derived from them, the
/// same way as for a hand-written `coats.toml` entry, apart from any hand-tuned `overrides`
/// the entry already had, which are kept and applied on top
#[derive(Debug, Clone, PartialEq)]
pub struct CoatDesign {
    pub name: String,
    pub pattern: Pattern,
    pub base: Rgba8,
    pub patch: Rgba8,
    pub spot: Rgba8,
    pub tail_rings: bool,
    /// `(key, value)` colour overrides (see [`OVERRIDE_KEYS`]) from the coat's entry
    pub overrides: Vec<(String, String)>,
}

impl CoatDesign {
    /// Starts a design from an existing coat. A custom coat keeps its name (saving updates it);
    /// any other coat starts a new one named after it. Use [`CoatLibrary::design_for`] to also
    /// pick up a custom coat's hand-tuned colours
    pub fn from_coat(coat: &Coat) -> Self {
        let (patch, spot) = match coat.pattern {
            // Sensible patch colours ready for when the pattern is switched
            Pattern::Solid => (rgb(220, 124, 54), rgb(58, 60, 66)),
            _ => (coat.patch, coat.spot),
        };
        let name = match &coat.id {
            CoatId::Custom(name) => name.to_string(),
            _ => canonical_name(&format!("My {}", coat.name)).to_string(),
        };
        Self {
            name,
            pattern: coat.pattern,
            base: coat.base,
            patch,
            spot,
            tail_rings: coat.tail_rings.is_some(),
            overrides: Vec::new(),
        }
    }

    /// The coat this design saves as, exactly as it will load back from `coats.toml`
    pub fn to_coat(&self) -> Coat {
        let name: Arc<str> = canonical_name(&self.name).into();
        let patterned = self.pattern != Pattern::Solid;
        let mut coat = Coat::derived(
            CoatId::Custom(name.clone()),
            name,
            self.pattern,
            self.base,
            patterned.then_some(self.patch),
            patterned.then_some(self.spot),
        );
        coat.tail_rings = self.tail_rings.then(|| auto_tail_rings(&coat));
        // Overrides were validated when the entry was loaded
        let _ = apply_overrides(&mut coat, &self.overrides);
        coat
    }

    /// The `key = "value"` lines this design writes, apart from its overrides
    fn entry_lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!("name = \"{}\"", canonical_name(&self.name)),
            format!("pattern = \"{}\"", self.pattern.as_config_str()),
            format!("base = \"{}\"", self.base.to_hex()),
        ];
        if self.pattern != Pattern::Solid {
            lines.push(format!("patch = \"{}\"", self.patch.to_hex()));
            lines.push(format!("spot = \"{}\"", self.spot.to_hex()));
        }
        // A hand-tuned ring colour is one of the overrides
        if !self.overrides.iter().any(|(k, _)| k == "tail_rings") {
            let rings = if self.tail_rings { "auto" } else { "none" };
            lines.push(format!("tail_rings = \"{rings}\""));
        }
        lines
    }
}

/// Keys a [`CoatDesign`] always writes
const DESIGN_KEYS: [&str; 6] = ["name", "pattern", "base", "patch", "spot", "tail_rings"];

/// Colour keys that fine-tune a coat on top of what `base`, `patch` and `spot` derive.
/// `tail_rings` also takes `"auto"` or `"none"`
const OVERRIDE_KEYS: [&str; 15] = [
    "shade",
    "outline",
    "crease",
    "ears",
    "ear_inner",
    "nose",
    "paws",
    "belly",
    "tummy",
    "chin",
    "patch_shade",
    "tail",
    "tail_shade",
    "tail_rings",
    "eyes",
];

/// The name as Catsor uses it: trimmed and cut to [`MAX_NAME_CHARS`]. Applied both when
/// loading entries and when finding an entry to update, so long names still match
fn canonical_name(name: &str) -> &str {
    let name = name.trim();
    let end = name
        .char_indices()
        .nth(MAX_NAME_CHARS)
        .map_or(name.len(), |(i, _)| i);
    name[..end].trim_end()
}

/// Strips one pair of matching surrounding quotes (`"…"` or `'…'`), leaving any quote that
/// is part of the value itself
pub(crate) fn unquote(val: &str) -> &str {
    let val = val.trim();
    val.strip_prefix('"')
        .and_then(|v| v.strip_suffix('"'))
        .or_else(|| val.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
        .unwrap_or(val)
}

/// `key` and unquoted `value` of a `key = value` line, ignoring comments
fn split_entry_line(line: &str) -> Option<(String, String)> {
    let (key, val) = strip_comment(line).split_once('=')?;
    Some((
        key.trim().to_ascii_lowercase(),
        unquote(val).trim().to_string(),
    ))
}

/// Returns `contents` with `design` written into the `[[coat]]` entry named one of
/// `names` (case-insensitively), or appended as a new entry. The design's own keys are
/// rewritten; override lines are kept as written (comments included) when the design still
/// has them, and dropped when it doesn't. Comments, other entries and unknown keys stay
fn upsert_coat_entry(contents: &str, names: &[&str], design: &CoatDesign) -> String {
    let lines: Vec<&str> = contents.lines().collect();
    let is_header = |l: &str| l.trim_start().starts_with('[');
    let target = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| strip_comment(l).trim().replace(' ', "") == "[[coat]]")
        .map(|(start, _)| {
            let end = (start + 1..lines.len())
                .find(|&i| is_header(lines[i]))
                .unwrap_or(lines.len());
            (start, end)
        })
        .find(|&(start, end)| {
            lines[start + 1..end].iter().any(|l| {
                split_entry_line(l).is_some_and(|(k, v)| {
                    k == "name"
                        && names
                            .iter()
                            .any(|n| canonical_name(n).eq_ignore_ascii_case(canonical_name(&v)))
                })
            })
        });

    let is_known = |k: &str| DESIGN_KEYS.contains(&k) || OVERRIDE_KEYS.contains(&k);
    let mut out: Vec<String> = Vec::with_capacity(lines.len() + 12);
    let (kept, before, after): (Vec<String>, &[&str], &[&str]) = match target {
        Some((start, end)) => {
            let kept = lines[start + 1..end]
                .iter()
                .filter(|l| {
                    split_entry_line(l).is_none_or(|(k, v)| {
                        !is_known(&k) || design.overrides.iter().any(|o| o.0 == k && o.1 == v)
                    })
                })
                .map(|l| l.to_string())
                .collect();
            (kept, &lines[..start], &lines[end..])
        }
        None => (Vec::new(), &lines[..], &[]),
    };
    out.extend(before.iter().map(|l| l.to_string()));
    if target.is_none() {
        if out.is_empty() {
            out.push(
                "# Custom coats for Catsor. Edit by hand or with \"Design Coat\" in the".into(),
            );
            out.push("# tray menu; see the README for every key.".into());
        }
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
    }
    out.push("[[coat]]".into());
    out.extend(design.entry_lines());
    // Overrides the old entry didn't have a line for (e.g. it was deleted meanwhile)
    let kept_pairs: Vec<(String, String)> =
        kept.iter().filter_map(|l| split_entry_line(l)).collect();
    for (k, v) in &design.overrides {
        if !kept_pairs.iter().any(|(kk, vv)| kk == k && vv == v) {
            out.push(format!("{k} = \"{v}\""));
        }
    }
    out.extend(kept);
    out.extend(after.iter().map(|l| l.to_string()));
    let mut text = out.join("\n");
    text.push('\n');
    text
}

/// Why a coat name can't be used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    Empty,
    /// A double quote would break the TOML string
    Quote,
    /// Taken by a built-in coat, or looks like a random coat id
    Reserved,
}

fn name_problem(name: &str) -> Option<NameProblem> {
    if name.is_empty() {
        Some(NameProblem::Empty)
    } else if name.contains('"') {
        Some(NameProblem::Quote)
    } else if !matches!(CoatId::parse(name), Some(CoatId::Custom(_))) || name.contains(':') {
        Some(NameProblem::Reserved)
    } else {
        None
    }
}

/// Why saving a design failed
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    Name(NameProblem),
    /// Another custom coat already has the name
    Taken,
    /// Reading, writing or reloading `coats.toml` failed
    Failed(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Name(NameProblem::Empty) => write!(f, "the coat needs a name"),
            SaveError::Name(NameProblem::Quote) => write!(f, "names cannot contain '\"'"),
            SaveError::Name(NameProblem::Reserved) => {
                write!(f, "the name is reserved for a built-in or random coat")
            }
            SaveError::Taken => write!(f, "another coat already has that name"),
            SaveError::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// A custom coat and the colour overrides its entry sets
struct CustomEntry {
    coat: Coat,
    overrides: Vec<(String, String)>,
}

/// The custom coats from `coats.toml`, plus lookups across built-in, custom and random coats
pub struct CoatLibrary {
    custom: Vec<Coat>,
    /// Each custom coat's hand-tuned overrides, in the same order as `custom`
    overrides: Vec<Vec<(String, String)>>,
    path: Option<PathBuf>,
    modified: Option<SystemTime>,
    next_check: Instant,
}

impl CoatLibrary {
    /// Reads `coats.toml` from the config directory; a missing file means no custom coats
    pub fn load() -> Self {
        let path = crate::prefs::default_config_path().map(|p| p.with_file_name(COATS_FILE));
        Self::load_from(path)
    }

    pub fn load_from(path: Option<PathBuf>) -> Self {
        let mut lib = Self {
            custom: Vec::new(),
            overrides: Vec::new(),
            path,
            modified: None,
            next_check: Instant::now() + RELOAD_CHECK_INTERVAL,
        };
        lib.reload();
        lib
    }

    #[cfg(test)]
    pub fn from_toml(input: &str) -> Self {
        let mut lib = Self::load_from(None);
        lib.set_entries(parse_entries(input).0);
        lib
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn custom(&self) -> &[Coat] {
        &self.custom
    }

    fn set_entries(&mut self, entries: Vec<CustomEntry>) {
        (self.custom, self.overrides) = entries.into_iter().map(|e| (e.coat, e.overrides)).unzip();
    }

    pub fn reload(&mut self) {
        let Some(path) = &self.path else {
            return;
        };
        self.modified = file_modified(path);
        let Ok(contents) = std::fs::read_to_string(path) else {
            self.set_entries(Vec::new());
            return;
        };
        let (entries, warnings) = parse_entries(&contents);
        for w in warnings {
            eprintln!("catsor: {}: {w}", path.display());
        }
        self.set_entries(entries);
    }

    /// A design to start the designer from: the coat's colours, plus the hand-tuned overrides
    /// of its `coats.toml` entry for a custom coat
    pub fn design_for(&self, coat: &Coat) -> CoatDesign {
        let mut design = CoatDesign::from_coat(coat);
        if let CoatId::Custom(name) = &coat.id
            && let Some(i) = self
                .custom
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(name))
        {
            design.overrides = self.overrides[i].clone();
        }
        design
    }

    /// Writes `design` to `coats.toml`, updating the entry named `original` (the coat being
    /// edited, if any) or the design's own name, and reloads. Returns the saved coat
    pub fn save_design(
        &mut self,
        design: &CoatDesign,
        original: Option<&str>,
    ) -> Result<Coat, SaveError> {
        let name = canonical_name(&design.name);
        if let Some(problem) = name_problem(name) {
            return Err(SaveError::Name(problem));
        }
        let clashes = self.custom.iter().any(|c| {
            c.name.eq_ignore_ascii_case(name)
                && original.is_none_or(|o| !canonical_name(o).eq_ignore_ascii_case(&c.name))
        });
        if clashes {
            return Err(SaveError::Taken);
        }
        let path = self
            .path
            .clone()
            .ok_or_else(|| SaveError::Failed("no config directory to save coats.toml in".into()))?;
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => {
                return Err(SaveError::Failed(format!(
                    "can't read {}: {e}",
                    path.display()
                )));
            }
        };
        let names: Vec<&str> = original.into_iter().chain([name]).collect();
        let updated = upsert_coat_entry(&contents, &names, design);
        crate::prefs::write_atomic(&path, &updated)
            .map_err(|e| SaveError::Failed(format!("can't write {}: {e}", path.display())))?;
        self.reload();
        // Make sure the file now really yields the coat that was designed (an earlier
        // duplicate entry, say, would shadow it)
        match self.find(&CoatId::Custom(name.into())) {
            Some(coat) if coat == design.to_coat() => Ok(coat),
            _ => Err(SaveError::Failed(format!(
                "{} did not load back the saved coat; see the warnings above",
                path.display()
            ))),
        }
    }

    /// The coat `id` refers to, or `None` for a custom coat that no longer exists
    pub fn find(&self, id: &CoatId) -> Option<Coat> {
        match id {
            CoatId::Builtin(style) => Some(Coat::builtin(*style)),
            CoatId::Random(seed) => Some(Coat::random(*seed)),
            CoatId::Custom(name) => self
                .custom
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .cloned(),
        }
    }

    /// Like [`find`](Self::find), falling back to the default coat
    pub fn resolve(&self, id: &CoatId) -> Coat {
        self.find(id)
            .unwrap_or_else(|| Coat::builtin(CoatStyle::Calico))
    }

    /// The coat after `id` when cycling: the built-in coats, then the custom ones, then around
    /// again. A random coat continues with the first built-in one
    pub fn next_after(&self, id: &CoatId) -> Coat {
        let all: Vec<CoatId> = CoatStyle::ALL
            .into_iter()
            .map(CoatId::Builtin)
            .chain(self.custom.iter().map(|c| c.id.clone()))
            .collect();
        let idx = all.iter().position(|c| match (c, id) {
            (CoatId::Custom(a), CoatId::Custom(b)) => a.eq_ignore_ascii_case(b),
            _ => c == id,
        });
        let next = idx.map_or(0, |i| (i + 1) % all.len());
        self.resolve(&all[next])
    }

    /// Reloads `coats.toml` when it changed on disk (checked at most once a second) and
    /// re-applies the cat's coat, so edits to a custom coat show up live. Returns whether the
    /// cat's coat changed
    pub fn refresh(&mut self, cat: &mut CatSim) -> bool {
        let now = Instant::now();
        if now < self.next_check {
            return false;
        }
        self.next_check = now + RELOAD_CHECK_INTERVAL;
        let Some(path) = &self.path else {
            return false;
        };
        if file_modified(path) == self.modified {
            return false;
        }
        self.reload();
        let updated = self.resolve(&cat.coat.id);
        if updated == cat.coat {
            return false;
        }
        cat.coat = updated;
        true
    }
}

fn file_modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Parses the `[[coat]]` tables of `coats.toml` (a small TOML subset: `key = "value"` lines).
/// Returns the valid coats and a warning for every entry or value that was skipped
#[cfg(test)]
pub fn parse_coats(input: &str) -> (Vec<Coat>, Vec<String>) {
    let (entries, warnings) = parse_entries(input);
    (entries.into_iter().map(|e| e.coat).collect(), warnings)
}

fn parse_entries(input: &str) -> (Vec<CustomEntry>, Vec<String>) {
    let mut entries: Vec<CustomEntry> = Vec::new();
    let mut warnings = Vec::new();
    let mut current: Option<(usize, Vec<(String, String)>)> = None;

    let finish = |entry: Option<(usize, Vec<(String, String)>)>,
                  entries: &mut Vec<CustomEntry>,
                  warnings: &mut Vec<String>| {
        let Some((line_no, fields)) = entry else {
            return;
        };
        match build_custom_coat(&fields, warnings) {
            Ok(entry) => {
                if entries
                    .iter()
                    .any(|e| e.coat.name.eq_ignore_ascii_case(&entry.coat.name))
                {
                    warnings.push(format!(
                        "line {line_no}: a coat named \"{}\" already exists; skipped",
                        entry.coat.name
                    ));
                } else {
                    entries.push(entry);
                }
            }
            Err(e) => warnings.push(format!("line {line_no}: {e}; coat skipped")),
        }
    };

    for (idx, raw_line) in input.lines().enumerate() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            finish(current.take(), &mut entries, &mut warnings);
            if line.replace(' ', "") == "[[coat]]" {
                current = Some((idx + 1, Vec::new()));
            }
            continue;
        }
        let Some((_, fields)) = current.as_mut() else {
            continue;
        };
        let Some((key, val)) = line.split_once('=') else {
            warnings.push(format!("line {}: expected `key = \"value\"`", idx + 1));
            continue;
        };
        fields.push((
            key.trim().to_ascii_lowercase(),
            unquote(val).trim().to_string(),
        ));
    }
    finish(current.take(), &mut entries, &mut warnings);
    (entries, warnings)
}

/// Drops a trailing `# comment`, keeping `#` inside quoted strings (hex colours)
pub(crate) fn strip_comment(line: &str) -> &str {
    let mut quote = None;
    for (i, ch) in line.char_indices() {
        match (ch, quote) {
            ('"' | '\'', None) => quote = Some(ch),
            (c, Some(q)) if c == q => quote = None,
            ('#', None) => return &line[..i],
            _ => {}
        }
    }
    line
}

/// Applies colour overrides to a derived coat. Overriding a colour also updates the colours
/// derived from it unless they are overridden too: `shade` the tail's shade band (when the tail
/// is in the base colour), `tail` its shade band, and `outline` the nose and tray-icon eyes
fn apply_overrides(coat: &mut Coat, overrides: &[(String, String)]) -> Result<(), String> {
    let has = |key: &str| overrides.iter().any(|(k, _)| k == key);
    let mut auto_rings = false;
    for (key, val) in overrides {
        if key == "tail_rings" && val.eq_ignore_ascii_case("none") {
            coat.tail_rings = None;
            auto_rings = false;
            continue;
        }
        if key == "tail_rings" && val.eq_ignore_ascii_case("auto") {
            auto_rings = true;
            continue;
        }
        if !OVERRIDE_KEYS.contains(&key.as_str()) {
            continue;
        }
        let c = Rgba8::parse_hex(val).ok_or(format!("`{key}` is not a #rrggbb colour: {val}"))?;
        match key.as_str() {
            "shade" => coat.shade = c,
            "outline" => coat.outline = c,
            "crease" => coat.crease = c,
            "ears" => coat.ears = c,
            "ear_inner" => coat.ear_inner = c,
            "nose" => coat.nose = c,
            "paws" => coat.paws = c,
            "belly" => coat.belly = c,
            "tummy" => coat.tummy = c,
            "chin" => coat.chin = c,
            "patch_shade" => coat.patch_shade = c,
            "tail" => coat.tail = c,
            "tail_shade" => coat.tail_shade = c,
            "tail_rings" => {
                coat.tail_rings = Some(c);
                auto_rings = false;
            }
            "eyes" => coat.icon.eyes = c,
            _ => {}
        }
    }

    if !has("tail_shade") {
        if has("tail") {
            coat.tail_shade = coat.tail.scale(0.83);
        } else if has("shade") && coat.pattern != Pattern::Calico {
            coat.tail_shade = coat.shade;
        }
    }
    if has("outline") {
        coat.icon.outline = coat.outline;
        if !has("nose") {
            coat.nose = coat.outline.scale(0.85);
        }
        if !has("eyes") {
            coat.icon.eyes = coat.outline;
        }
    }
    if auto_rings {
        coat.tail_rings = Some(auto_tail_rings(coat));
    }
    if coat.pattern == Pattern::Solid {
        // Unused without patches; kept in step with the overrides so printed coats compare equal
        coat.patch_shade = coat.shade;
    }
    Ok(())
}

/// Rejects names a custom coat can't have
fn check_name(name: &str) -> Result<(), String> {
    match name_problem(name) {
        None => Ok(()),
        Some(NameProblem::Empty) => Err("empty `name`".into()),
        Some(NameProblem::Quote) => Err(format!("`name` cannot contain a double quote: {name}")),
        Some(NameProblem::Reserved) => Err(format!(
            "\"{name}\" is reserved for a built-in or random coat"
        )),
    }
}

fn build_custom_coat(
    fields: &[(String, String)],
    warnings: &mut Vec<String>,
) -> Result<CustomEntry, String> {
    let get = |key: &str| {
        fields
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    };
    let name = canonical_name(get("name").ok_or("missing `name`")?);
    check_name(name)?;
    let color = |key: &str| -> Result<Option<Rgba8>, String> {
        get(key)
            .map(|v| Rgba8::parse_hex(v).ok_or(format!("`{key}` is not a #rrggbb colour: {v}")))
            .transpose()
    };
    let base = color("base")?.ok_or("missing `base`")?;
    let pattern = match get("pattern") {
        None => Pattern::Solid,
        Some(p) => Pattern::from_config_str(p).ok_or(format!(
            "unknown pattern \"{p}\" (use solid, calico or tuxedo)"
        ))?,
    };
    let patch = color("patch")?;
    let spot = color("spot")?;
    for (key, _) in fields {
        if !DESIGN_KEYS.contains(&key.as_str()) && !OVERRIDE_KEYS.contains(&key.as_str()) {
            warnings.push(format!("unknown coat key `{key}` ignored"));
        }
    }

    let name: Arc<str> = name.into();
    let mut coat = Coat::derived(
        CoatId::Custom(name.clone()),
        name,
        pattern,
        base,
        patch,
        spot,
    );
    let all_overrides: Vec<(String, String)> = fields
        .iter()
        .filter(|(k, _)| OVERRIDE_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    apply_overrides(&mut coat, &all_overrides)?;
    // `tail_rings = "auto"/"none"` is the designer's rings toggle, not a hand-tuned colour
    let overrides = all_overrides
        .into_iter()
        .filter(|(k, v)| k != "tail_rings" || Rgba8::parse_hex(v).is_some())
        .collect();
    Ok(CustomEntry { coat, overrides })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colours_roundtrip() {
        assert_eq!(Rgba8::parse_hex("#d9884a"), Some(rgb(0xd9, 0x88, 0x4a)));
        assert_eq!(
            Rgba8::parse_hex("D9884A80"),
            Some(rgba(0xd9, 0x88, 0x4a, 0x80))
        );
        assert_eq!(Rgba8::parse_hex("#d9884"), None);
        assert_eq!(Rgba8::parse_hex("#zz884a"), None);
        for c in [rgb(1, 2, 3), rgba(250, 128, 0, 17)] {
            assert_eq!(Rgba8::parse_hex(&c.to_hex()), Some(c));
        }
    }

    #[test]
    fn coat_ids_roundtrip_through_config_strings() {
        for id in [
            CoatId::Builtin(CoatStyle::Tuxedo),
            CoatId::Custom("Marmalade".into()),
            CoatId::Random(0x0012_abcd),
        ] {
            assert_eq!(CoatId::parse(&id.to_config_string()), Some(id));
        }
        assert_eq!(
            CoatId::parse("SILVER"),
            Some(CoatId::Builtin(CoatStyle::Silver))
        );
        assert!(matches!(CoatId::parse("random"), Some(CoatId::Random(_))));
        assert_eq!(CoatId::parse("  "), None);
    }

    #[test]
    fn custom_coats_parse_with_derived_colours_and_overrides() {
        let input = r##"
            # my cats
            scale = 2.0                   # keys outside [[coat]] are ignored

            [[coat]]
            name = "Marmalade"            # orange tabby
            base = "#d98a4a"
            tail_rings = "#a85a24"

            [[coat]]
            name = 'Oreo'
            pattern = "tuxedo"
            base = "#f4f4f4"
            patch = "#1c1c1e"
            nose = "#e08890"
        "##;
        let (coats, warnings) = parse_coats(input);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(coats.len(), 2);

        let marmalade = &coats[0];
        assert_eq!(marmalade.id, CoatId::Custom("Marmalade".into()));
        assert_eq!(marmalade.pattern, Pattern::Solid);
        assert_eq!(marmalade.base, rgb(0xd9, 0x8a, 0x4a));
        assert_eq!(marmalade.tail_rings, Some(rgb(0xa8, 0x5a, 0x24)));
        // Derived colours stay darker than the base and keep its hue order (r > g > b)
        for c in [marmalade.shade, marmalade.outline, marmalade.ear_inner] {
            assert!(c.luma() < marmalade.base.luma());
            assert!(c.r >= c.g && c.g >= c.b, "{c:?}");
        }

        let oreo = &coats[1];
        assert_eq!(oreo.pattern, Pattern::Tuxedo);
        assert_eq!(oreo.patch, rgb(0x1c, 0x1c, 0x1e));
        assert_eq!(oreo.nose, rgb(0xe0, 0x88, 0x90));
        // A white coat with a black mask gets the dark outline of its darkest colour
        assert!(oreo.outline.luma() < 0.1);
    }

    #[test]
    fn invalid_custom_coats_are_skipped_with_warnings() {
        let input = r##"
            [[coat]]
            name = "No base"

            [[coat]]
            name = "Calico"
            base = "#ffffff"

            [[coat]]
            name = "random:12"
            base = "#ffffff"

            [[coat]]
            name = "Bad colour"
            base = "orange"

            [[coat]]
            name = "Odd pattern"
            base = "#ffffff"
            pattern = "zebra"

            [[coat]]
            name = "Fine"
            base = "#808080"
            whiskers = "long"

            [[coat]]
            name = "fine"
            base = "#000000"
        "##;
        let (coats, warnings) = parse_coats(input);
        assert_eq!(coats.len(), 1);
        assert_eq!(&*coats[0].name, "Fine");
        // 5 bad entries, 1 unknown key, 1 duplicate name
        assert_eq!(warnings.len(), 7, "{warnings:?}");
    }

    #[test]
    fn printed_coats_parse_back_to_the_same_colours() {
        let coats = CoatStyle::ALL
            .into_iter()
            .map(Coat::builtin)
            .chain((0..40).map(Coat::random));
        for coat in coats {
            let (parsed, warnings) = parse_coats(&coat.to_toml_entry("Copy"));
            assert!(warnings.is_empty(), "{warnings:?}");
            let copy = &parsed[0];
            // Everything but the identity and the separately tuned tray icon carries over
            let expected = Coat {
                id: copy.id.clone(),
                name: copy.name.clone(),
                icon: copy.icon,
                ..coat.clone()
            };
            assert_eq!(*copy, expected, "{}", coat.name);
        }
    }

    #[test]
    fn random_coats_are_deterministic_varied_and_readable() {
        assert_eq!(Coat::random(7), Coat::random(7));
        let coats: Vec<Coat> = (0..400).map(Coat::random).collect();
        for pattern in Pattern::ALL {
            let n = coats.iter().filter(|c| c.pattern == pattern).count();
            assert!(n > 40, "{pattern:?} rolled only {n} times");
        }
        let names: std::collections::HashSet<&str> = coats.iter().map(|c| &*c.name).collect();
        assert!(names.len() > 20, "only {} distinct coats", names.len());
        for coat in &coats {
            assert!(coat.name.chars().count() <= MAX_NAME_CHARS, "{}", coat.name);
            // The outline must stand out from the fur so the silhouette stays readable
            assert!(
                coat.base.luma() - coat.outline.luma() > 0.08,
                "{}: base {:?} outline {:?}",
                coat.name,
                coat.base,
                coat.outline
            );
        }
    }

    #[test]
    fn library_cycles_builtins_then_customs_and_falls_back() {
        let lib = CoatLibrary::from_toml(
            "[[coat]]\nname = \"Mochi\"\nbase = \"#eeeeee\"\n[[coat]]\nname = \"Pepper\"\nbase = \"#333333\"\n",
        );
        let mut id = CoatId::default();
        let mut seen = Vec::new();
        for _ in 0..CoatStyle::ALL.len() + 2 {
            seen.push(lib.resolve(&id).name.to_string());
            id = lib.next_after(&id).id;
        }
        assert_eq!(
            seen,
            [
                "Calico", "Brown", "Charcoal", "Tuxedo", "Silver", "Mochi", "Pepper"
            ]
        );
        assert_eq!(id, CoatId::Builtin(CoatStyle::Calico));

        assert_eq!(
            lib.next_after(&CoatId::Random(3)).id,
            CoatId::Builtin(CoatStyle::Calico)
        );
        assert_eq!(&*lib.resolve(&CoatId::Custom("mochi".into())).name, "Mochi");
        assert_eq!(
            lib.resolve(&CoatId::Custom("Gone".into())).id,
            CoatId::Builtin(CoatStyle::Calico)
        );
    }

    #[test]
    fn saving_a_design_rewrites_only_its_own_keys() {
        let file = r##"# my coats
[[coat]]
name = "Mochi"   # the first one
base = "#eeeeee"
outline = "#112233"  # hand-tuned
paws = "#ffffff"
tail_rings = "#000000"

[[coat]]
name = "Pepper"
base = "#333333"
"##;
        let lib = CoatLibrary::from_toml(file);
        let mut design = lib.design_for(&lib.custom()[0]);
        assert_eq!(
            design.overrides,
            [
                ("outline".to_string(), "#112233".to_string()),
                ("paws".into(), "#ffffff".into()),
                ("tail_rings".into(), "#000000".into()),
            ]
        );
        // The designer previews exactly what loads back
        assert_eq!(design.to_coat(), lib.custom()[0]);

        // Rename, recolour, drop the paws override and the hand-tuned rings
        design.name = "Mochi Two".into();
        design.pattern = Pattern::Tuxedo;
        design.base = rgb(250, 250, 250);
        design.patch = rgb(20, 20, 20);
        design.spot = rgb(60, 60, 60);
        design.overrides.retain(|(k, _)| k == "outline");
        let updated = upsert_coat_entry(file, &["mochi", "Mochi Two"], &design);
        assert_eq!(
            updated,
            r##"# my coats
[[coat]]
name = "Mochi Two"
pattern = "tuxedo"
base = "#fafafa"
patch = "#141414"
spot = "#3c3c3c"
tail_rings = "auto"
outline = "#112233"  # hand-tuned

[[coat]]
name = "Pepper"
base = "#333333"
"##
        );
        let (coats, warnings) = parse_coats(&updated);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(coats[0], design.to_coat());
        assert_eq!(coats[0].outline, rgb(0x11, 0x22, 0x33));

        // A new name is appended as its own entry, overrides included; an empty file gets a
        // header comment
        let salt = CoatDesign {
            name: "Salt".into(),
            ..design.clone()
        };
        let appended = upsert_coat_entry(file, &["Salt"], &salt);
        assert!(appended.starts_with(file));
        assert!(appended.ends_with(
            "\n[[coat]]\nname = \"Salt\"\npattern = \"tuxedo\"\nbase = \"#fafafa\"\npatch = \"#141414\"\nspot = \"#3c3c3c\"\ntail_rings = \"auto\"\noutline = \"#112233\"\n"
        ));
        assert_eq!(parse_coats(&appended).0[2], salt.to_coat());
        let fresh = upsert_coat_entry("", &["Salt"], &salt);
        assert!(fresh.starts_with("# Custom coats for Catsor"));
        assert_eq!(parse_coats(&fresh).0.len(), 1);
    }

    #[test]
    fn long_names_are_cut_the_same_way_when_loading_and_saving() {
        let file = "[[coat]]\nname = \"Sir Fluffington the Third\"\nbase = \"#808080\"\n";
        let lib = CoatLibrary::from_toml(file);
        assert_eq!(&*lib.custom()[0].name, "Sir Fluffington the Th");
        let mut design = lib.design_for(&lib.custom()[0]);
        design.base = rgb(10, 10, 10);
        let updated = upsert_coat_entry(file, &[&design.name.clone()], &design);
        let (coats, warnings) = parse_coats(&updated);
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(coats.len(), 1, "updated in place, not appended:\n{updated}");
        assert_eq!(coats[0], design.to_coat());
    }

    #[test]
    fn overriding_a_colour_updates_the_colours_derived_from_it() {
        let (coats, _) = parse_coats(
            "[[coat]]\nname = \"A\"\nbase = \"#c0c0c0\"\nshade = \"#202020\"\noutline = \"#ff0000\"\n\
             [[coat]]\nname = \"B\"\nbase = \"#c0c0c0\"\noutline = \"#ff0000\"\nnose = \"#00ff00\"\neyes = \"#0000ff\"\ntail = \"#804020\"\n",
        );
        let a = &coats[0];
        assert_eq!(a.tail_shade, rgb(0x20, 0x20, 0x20));
        assert_eq!(a.nose, rgb(0xff, 0, 0).scale(0.85));
        assert_eq!(a.icon.eyes, rgb(0xff, 0, 0));
        assert_eq!(a.icon.outline, rgb(0xff, 0, 0));
        let b = &coats[1];
        assert_eq!(b.nose, rgb(0, 0xff, 0), "explicit values win");
        assert_eq!(b.icon.eyes, rgb(0, 0, 0xff));
        assert_eq!(b.tail_shade, rgb(0x80, 0x40, 0x20).scale(0.83));
    }

    #[test]
    fn save_design_reports_typed_errors() {
        let mut lib = CoatLibrary::from_toml("[[coat]]\nname = \"Taken\"\nbase = \"#808080\"\n");
        let mut design = CoatDesign::from_coat(&Coat::builtin(CoatStyle::Brown));
        for (name, err) in [
            ("  ", SaveError::Name(NameProblem::Empty)),
            ("Tuxedo", SaveError::Name(NameProblem::Reserved)),
            ("random:1", SaveError::Name(NameProblem::Reserved)),
            ("a\"b", SaveError::Name(NameProblem::Quote)),
            ("taken", SaveError::Taken),
        ] {
            design.name = name.into();
            assert_eq!(lib.save_design(&design, None), Err(err), "{name}");
        }
        // Renaming the coat being edited to its own name is fine; with no file it fails cleanly
        design.name = "Taken".into();
        assert!(matches!(
            lib.save_design(&design, Some("taken")),
            Err(SaveError::Failed(_))
        ));
    }

    #[test]
    fn library_reloads_edits_and_updates_the_cat() {
        let dir = std::env::temp_dir().join(format!("catsor_coats_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(COATS_FILE);
        std::fs::write(&path, "[[coat]]\nname = \"Mochi\"\nbase = \"#eeeeee\"\n").unwrap();

        let mut lib = CoatLibrary::load_from(Some(path.clone()));
        let mut cat = CatSim::new(
            glam::Vec2::ZERO,
            1.0,
            lib.resolve(&CoatId::Custom("Mochi".into())),
        );
        assert_eq!(cat.coat.base, rgb(0xee, 0xee, 0xee));

        // Unchanged file: nothing to do, even once the check interval has passed
        lib.next_check = Instant::now();
        assert!(!lib.refresh(&mut cat));

        std::fs::write(&path, "[[coat]]\nname = \"Mochi\"\nbase = \"#aa7744\"\n").unwrap();
        // Force a different mtime even on coarse-grained filesystems
        lib.modified = None;
        lib.next_check = Instant::now();
        assert!(lib.refresh(&mut cat));
        assert_eq!(cat.coat.base, rgb(0xaa, 0x77, 0x44));

        // Deleting the coat falls back to the default one
        std::fs::remove_file(&path).unwrap();
        lib.next_check = Instant::now();
        assert!(lib.refresh(&mut cat));
        assert_eq!(cat.coat.id, CoatId::Builtin(CoatStyle::Calico));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
