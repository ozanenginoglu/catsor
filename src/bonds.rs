//! How well each pair of cats knows and likes the other, kept in `bonds.toml` next to the config
//! so it carries across restarts. Hand-parsed like the prefs: unknown or invalid entries are
//! ignored.

use crate::cat::{Kinship, Temperament};
use crate::coat::{strip_comment, unquote};
use crate::prefs::{default_config_path, write_atomic};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const BONDS_FILE: &str = "bonds.toml";

/// One pair of cats
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bond {
    /// 0–1: grows slowly while both cats are around, faster while they're near each other
    pub familiarity: f32,
    /// -1–1: grows while they rest together and drifts back toward what their temperaments
    /// make of each other
    pub affection: f32,
}

impl Bond {
    /// A newly met pair: strangers, a little wary of each other. Family starts at its floor.
    pub fn first_meeting(baseline: f32, kin: Kinship) -> Self {
        Self {
            familiarity: 0.0,
            affection: baseline - 0.1,
        }
        .with_floor(kin)
    }

    /// Family never drops below a high floor: a mother and her kittens, and siblings
    pub fn with_floor(self, kin: Kinship) -> Self {
        let (familiarity, affection) = match kin {
            Kinship::Mother | Kinship::Kitten => (0.9, 0.8),
            Kinship::Sibling => (0.8, 0.6),
            Kinship::None => (0.0, -1.0),
        };
        Self {
            familiarity: self.familiarity.clamp(familiarity, 1.0),
            affection: self.affection.clamp(affection, 1.0),
        }
    }

    /// Glad to see each other: family, or cats that know and like each other
    pub fn friends(self, kin: Kinship) -> bool {
        kin.is_family() || (self.affection > 0.3 && self.familiarity > 0.3)
    }

    /// Where affection settles without anything happening between the two: the average of how
    /// warm each one is toward other cats
    pub fn baseline(a: (Temperament, bool), b: (Temperament, bool)) -> f32 {
        let warmth = |(temperament, kitten): (Temperament, bool)| {
            let w = match temperament {
                Temperament::Playful => 0.35,
                Temperament::Balanced | Temperament::Lazy => 0.2,
                Temperament::Realistic => 0.15,
                Temperament::Aloof => -0.1,
            };
            if kitten { w + 0.15 } else { w }
        };
        0.5 * (warmth(a) + warmth(b))
    }
}

/// A value for each pair of cats, by the two names in either order. Looking a pair up borrows
/// the names, so the per-frame lookups don't allocate.
#[derive(Debug, Clone, PartialEq)]
pub struct PairMap<T> {
    /// By the lesser name, then the greater
    map: BTreeMap<String, BTreeMap<String, T>>,
}

impl<T> Default for PairMap<T> {
    fn default() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }
}

impl<T> PairMap<T> {
    fn order<'a>(a: &'a str, b: &'a str) -> (&'a str, &'a str) {
        if a <= b { (a, b) } else { (b, a) }
    }

    pub fn get(&self, a: &str, b: &str) -> Option<&T> {
        let (lo, hi) = Self::order(a, b);
        self.map.get(lo)?.get(hi)
    }

    pub fn get_mut(&mut self, a: &str, b: &str) -> Option<&mut T> {
        let (lo, hi) = Self::order(a, b);
        self.map.get_mut(lo)?.get_mut(hi)
    }

    /// The pair's value, first set to `value` if the pair has none
    pub fn get_or_insert(&mut self, a: &str, b: &str, value: T) -> &mut T {
        let (lo, hi) = Self::order(a, b);
        if self.get(lo, hi).is_none() {
            self.map
                .entry(lo.to_string())
                .or_default()
                .insert(hi.to_string(), value);
        }
        self.map.get_mut(lo).and_then(|m| m.get_mut(hi)).unwrap()
    }

    pub fn insert(&mut self, a: &str, b: &str, value: T) {
        match self.get_mut(a, b) {
            Some(slot) => *slot = value,
            None => {
                let (lo, hi) = Self::order(a, b);
                self.map
                    .entry(lo.to_string())
                    .or_default()
                    .insert(hi.to_string(), value);
            }
        }
    }

    /// Drops every pair `name` is in
    pub fn forget(&mut self, name: &str) {
        self.map.remove(name);
        for inner in self.map.values_mut() {
            inner.remove(name);
        }
        self.map.retain(|_, inner| !inner.is_empty());
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.values().map(BTreeMap::len).sum()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.map.values_mut().flat_map(BTreeMap::values_mut)
    }

    /// Every pair, the lesser name first, in order
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, &T)> {
        self.map.iter().flat_map(|(lo, inner)| {
            inner
                .iter()
                .map(move |(hi, v)| (lo.as_str(), hi.as_str(), v))
        })
    }
}

/// Every pair's bond, by the two cats' names. A cat removed from the tray takes its bonds with
/// it, so a new cat that gets its name starts as a stranger; a cat left out of the config by
/// hand keeps them.
#[derive(Debug, Default)]
pub struct Bonds {
    pairs: PairMap<Bond>,
    /// Where `save` writes; `None` (as in tests) doesn't save
    path: Option<PathBuf>,
}

impl Bonds {
    /// Reads `bonds.toml` next to the config; empty if there is none yet
    pub fn load() -> Self {
        let path = default_config_path().map(|p| p.with_file_name(BONDS_FILE));
        let mut bonds = path
            .as_deref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| Self::parse_toml(&text))
            .unwrap_or_default();
        bonds.path = path;
        bonds
    }

    pub fn save(&self) {
        if let Some(path) = &self.path
            && let Err(e) = self.save_to_path(path)
        {
            eprintln!("catsor: failed to save {}: {e}", path.display());
        }
    }

    pub fn save_to_path(&self, path: &Path) -> std::io::Result<()> {
        write_atomic(path, &self.to_toml_string())
    }

    pub fn get(&self, a: &str, b: &str) -> Option<Bond> {
        self.pairs.get(a, b).copied()
    }

    pub fn set(&mut self, a: &str, b: &str, bond: Bond) {
        self.pairs.insert(a, b, bond);
    }

    /// Drops every bond `name` has
    pub fn forget(&mut self, name: &str) {
        self.pairs.forget(name);
    }

    pub fn to_toml_string(&self) -> String {
        let mut out = String::from("# Catsor: how well the cats know and like each other\n");
        for (a, b, bond) in self.pairs.iter() {
            out.push_str(&format!(
                "\n[[bond]]\n\
                 a = \"{a}\"\n\
                 b = \"{b}\"\n\
                 familiarity = {:.4}\n\
                 affection = {:.4}\n",
                bond.familiarity, bond.affection,
            ));
        }
        out
    }

    /// One `[[bond]]` table per pair. A table missing a name, naming the same cat twice or with
    /// no valid numbers is skipped; a missing familiarity is 0 (strangers) and a missing
    /// affection 0 (neutral).
    pub fn parse_toml(input: &str) -> Self {
        #[derive(Default)]
        struct Row {
            a: Option<String>,
            b: Option<String>,
            familiarity: Option<f32>,
            affection: Option<f32>,
        }
        let mut bonds = Self::default();
        let mut finish = |row: Row| {
            if let (Some(a), Some(b)) = (row.a, row.b)
                && a != b
                && (row.familiarity.is_some() || row.affection.is_some())
            {
                bonds.set(
                    &a,
                    &b,
                    Bond {
                        familiarity: row.familiarity.unwrap_or(0.0).clamp(0.0, 1.0),
                        affection: row.affection.unwrap_or(0.0).clamp(-1.0, 1.0),
                    },
                );
            }
        };
        let mut current: Option<Row> = None;
        for raw_line in input.lines() {
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                if let Some(row) = current.take() {
                    finish(row);
                }
                if line.replace(' ', "") == "[[bond]]" {
                    current = Some(Row::default());
                }
                continue;
            }
            let (Some(row), Some((raw_key, raw_val))) = (&mut current, line.split_once('=')) else {
                continue;
            };
            let val = unquote(raw_val).trim();
            let number = val.parse::<f32>().ok().filter(|v| v.is_finite());
            match raw_key.trim() {
                "a" if !val.is_empty() => row.a = Some(val.to_string()),
                "b" if !val.is_empty() => row.b = Some(val.to_string()),
                "familiarity" => row.familiarity = number,
                "affection" => row.affection = number,
                _ => {}
            }
        }
        if let Some(row) = current {
            finish(row);
        }
        bonds
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pairs_are_found_either_way_round_and_forgotten_by_name() {
        let mut pairs = PairMap::default();
        pairs.insert("Pip", "Luna", 1.0);
        *pairs.get_or_insert("Luna", "Milo", 2.0) += 1.0;
        pairs.insert("Milo", "Pip", 4.0);
        assert_eq!(pairs.get("Luna", "Pip"), Some(&1.0));
        assert_eq!(pairs.get("Milo", "Luna"), Some(&3.0));
        pairs.forget("Pip");
        let left: Vec<_> = pairs.iter().collect();
        assert_eq!(left, [("Luna", "Milo", &3.0)]);
    }

    #[test]
    fn bonds_roundtrip_and_skip_invalid_rows() {
        let mut bonds = Bonds::default();
        bonds.set(
            "Pip",
            "Luna",
            Bond {
                familiarity: 0.42,
                affection: -0.3,
            },
        );
        bonds.set(
            "Luna",
            "Milo",
            Bond {
                familiarity: 1.0,
                affection: 0.75,
            },
        );
        let parsed = Bonds::parse_toml(&bonds.to_toml_string());
        assert_eq!(parsed.pairs, bonds.pairs);
        // Order of the two names doesn't matter
        assert_eq!(parsed.get("Luna", "Pip"), bonds.get("Pip", "Luna"));

        let messy = "\
            [[bond]]\na = \"Luna\"\nb = \"Luna\"\nfamiliarity = 0.5\n\
            [[bond]]\na = \"Luna\"\nfamiliarity = 0.5\n\
            [[bond]]\na = \"Luna\"\nb = \"Pip\"\nfamiliarity = lots\naffection = 7 # too much\n\
            [other]\na = \"x\"\n";
        let parsed = Bonds::parse_toml(messy);
        assert_eq!(parsed.pairs.len(), 1);
        assert_eq!(
            parsed.get("Pip", "Luna"),
            Some(Bond {
                familiarity: 0.0,
                affection: 1.0
            })
        );
    }

    #[test]
    fn family_keeps_a_high_floor() {
        let cold = Bond {
            familiarity: 0.0,
            affection: -0.5,
        };
        assert_eq!(cold.with_floor(Kinship::None), cold);
        let mother = cold.with_floor(Kinship::Mother);
        assert!(mother.familiarity >= 0.9 && mother.affection >= 0.8);
        let sibling = cold.with_floor(Kinship::Sibling);
        assert!(sibling.familiarity >= 0.8 && sibling.affection >= 0.6);
        let aloof = Bond::baseline((Temperament::Aloof, false), (Temperament::Aloof, false));
        let playful = Bond::baseline((Temperament::Playful, true), (Temperament::Playful, false));
        assert!(aloof < 0.0 && playful > 0.3);
    }
}
