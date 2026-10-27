//! Several cats at once (a group of cats is a "clowder"). `Clowder` owns every `CatSim`, tells
//! each one where the others are and what they are to it, keeps track of how well each pair
//! gets on (`Bonds`), sends a mother to fetch a kitten that wandered off, and runs the shared
//! food bowl: one cat eats at a time while the rest sit nearby and wait their turn, kittens
//! first.

use super::{CatSim, FeedPreset, Kinship, Neighbor, Pose, Slot, Temperament, wrap_angle};
use crate::bonds::{Bond, Bonds};
use crate::coat::{Coat, CoatId, CoatLibrary, fresh_seed};
use glam::Vec2;
use std::collections::VecDeque;

/// Most cats running at once
pub const MAX_CATS: usize = 6;
/// A kitten's default size, relative to the size picked in the tray
pub const KITTEN_SIZE: f32 = 0.6;
/// Longest single turn at the shared bowl (seconds) when more than one cat is eating
const TURN_SECS: f32 = 150.0;
/// How far (unscaled px, times the mother's scale) a kitten that wandered off can get before
/// its mother goes and fetches it
const FETCH_RADIUS: f32 = 240.0;
/// How often (seconds) the bonds are written to disk
const BONDS_SAVE_SECS: f32 = 300.0;

/// Names handed to cats added from the tray, in order, skipping names already in use
const NAMES: [&str; 12] = [
    "Luna", "Milo", "Pip", "Mochi", "Olive", "Biscuit", "Nori", "Ziggy", "Maple", "Tofu",
    "Juniper", "Pepper",
];

/// One cat as saved in the config: who it is and how it looks and behaves
#[derive(Debug, Clone, PartialEq)]
pub struct CatProfile {
    /// Unique among the cats; the key for everything else that refers to this cat
    pub name: String,
    pub coat: CoatId,
    pub temperament: Temperament,
    /// Size relative to the tray's Size (the global scale)
    pub size: f32,
    pub kitten: bool,
    /// The name of this kitten's mother: another cat that isn't a kitten itself
    pub mother: Option<String>,
}

impl CatProfile {
    pub fn new(name: impl Into<String>, coat: CoatId, temperament: Temperament) -> Self {
        Self {
            name: name.into(),
            coat,
            temperament,
            size: 1.0,
            kitten: false,
            mother: None,
        }
    }

    pub const MIN_SIZE: f32 = 0.4;
    pub const MAX_SIZE: f32 = 1.5;
}

/// The first cat: a calico called Luna
impl Default for CatProfile {
    fn default() -> Self {
        Self::new(NAMES[0], CoatId::default(), Temperament::Balanced)
    }
}

/// A cat in the clowder: its simulation plus the parts of its profile `CatSim` doesn't hold
pub struct Member {
    pub name: String,
    pub size: f32,
    pub kitten: bool,
    pub mother: Option<String>,
    pub cat: CatSim,
    /// The kitten (index) this mother is on her way to fetch
    fetching: Option<usize>,
}

impl Member {
    pub fn profile(&self) -> CatProfile {
        CatProfile {
            name: self.name.clone(),
            coat: self.cat.coat.id.clone(),
            temperament: self.cat.behaviors.temperament,
            size: self.size,
            kitten: self.kitten,
            mother: self.mother.clone(),
        }
    }
}

/// The shared bowl from the moment the user picks a feed preset until feeding ends
struct FeedSession {
    preset: FeedPreset,
    /// `None` while the bowl still hangs from the cursor
    bowl_pos: Option<Vec2>,
    /// Seconds eaten so far, by all cats together; the preset's duration counts these
    eaten: f32,
    /// The cat whose turn it is (walking to the bowl or eating)
    eater: Option<usize>,
    /// Cats waiting for a turn, next first
    queue: VecDeque<usize>,
}

pub struct Clowder {
    members: Vec<Member>,
    /// The tray's Size; each cat's `scale` is this times its `size`
    scale: f32,
    follow_cursor: bool,
    playful_quirks: bool,
    feeding: Option<FeedSession>,
    bonds: Bonds,
    /// Seconds since the bonds were last saved
    bonds_unsaved: f32,
}

impl Clowder {
    /// Builds the cats from saved profiles near `start` (the cursor). Duplicate names get a
    /// number, and a `mother` that isn't another grown cat here is dropped. An empty list
    /// gives one default cat.
    pub fn new(
        profiles: &[CatProfile],
        scale: f32,
        follow_cursor: bool,
        playful_quirks: bool,
        coats: &CoatLibrary,
        start: Vec2,
    ) -> Self {
        let mut clowder = Self {
            members: Vec::new(),
            scale,
            follow_cursor,
            playful_quirks,
            feeding: None,
            bonds: Bonds::default(),
            bonds_unsaved: 0.0,
        };
        let default_profile = [CatProfile::default()];
        let profiles = if profiles.is_empty() {
            &default_profile[..]
        } else {
            profiles
        };
        for profile in profiles.iter().take(MAX_CATS) {
            let mut profile = profile.clone();
            if profile.name.trim().is_empty() || clowder.index_of(&profile.name).is_some() {
                profile.name = clowder.fresh_name();
            }
            let i = clowder.members.len();
            // Spread out a little so they don't all start in one heap
            let offset = Vec2::new(-34.0 * i as f32, 20.0 * (i % 2) as f32) * scale;
            let coat = coats.resolve(&profile.coat);
            clowder.push_member(profile, coat, start + offset);
        }
        clowder.validate_mothers();
        clowder
    }

    fn push_member(&mut self, profile: CatProfile, coat: Coat, start: Vec2) -> usize {
        let size = profile
            .size
            .clamp(CatProfile::MIN_SIZE, CatProfile::MAX_SIZE);
        let mut cat = CatSim::new(start, self.scale * size, coat);
        cat.behaviors.temperament = profile.temperament;
        cat.behaviors.follow_cursor = self.follow_cursor;
        cat.behaviors.playful_quirks = self.playful_quirks;
        cat.kitten = profile.kitten;
        // The first cat keeps the default seed, so a lone cat behaves exactly as it always has
        if !self.members.is_empty() {
            cat.seed_rng(name_seed(&profile.name));
        }
        self.members.push(Member {
            name: profile.name,
            size,
            kitten: profile.kitten,
            mother: profile.mother,
            cat,
            fetching: None,
        });
        self.members.len() - 1
    }

    /// Drops any `mother` that isn't another grown cat in the clowder
    fn validate_mothers(&mut self) {
        for i in 0..self.members.len() {
            let Some(mother) = self.members[i].mother.clone() else {
                continue;
            };
            let valid = self
                .index_of(&mother)
                .is_some_and(|m| m != i && !self.members[m].kitten);
            if !valid || !self.members[i].kitten {
                self.members[i].mother = None;
            }
        }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.members.iter().position(|m| m.name == name)
    }

    /// The first name from `NAMES` nobody uses yet, or "Cat N"
    fn fresh_name(&self) -> String {
        NAMES
            .iter()
            .map(|n| n.to_string())
            .chain((2..).map(|n| format!("Cat {n}")))
            .find(|n| self.index_of(n).is_none())
            .unwrap_or_default()
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn cat(&self, i: usize) -> &CatSim {
        &self.members[i].cat
    }

    pub fn cat_mut(&mut self, i: usize) -> &mut CatSim {
        &mut self.members[i].cat
    }

    pub fn cats(&self) -> impl Iterator<Item = &CatSim> {
        self.members.iter().map(|m| &m.cat)
    }

    pub fn cats_mut(&mut self) -> impl Iterator<Item = &mut CatSim> {
        self.members.iter_mut().map(|m| &mut m.cat)
    }

    pub fn profiles(&self) -> Vec<CatProfile> {
        self.members.iter().map(Member::profile).collect()
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn follow_cursor(&self) -> bool {
        self.follow_cursor
    }

    pub fn playful_quirks(&self) -> bool {
        self.playful_quirks
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
        for m in &mut self.members {
            m.cat.set_scale(scale * m.size);
        }
    }

    pub fn set_follow_cursor(&mut self, on: bool) {
        self.follow_cursor = on;
        for cat in self.cats_mut() {
            cat.behaviors.follow_cursor = on;
        }
    }

    pub fn set_playful_quirks(&mut self, on: bool) {
        self.playful_quirks = on;
        for cat in self.cats_mut() {
            cat.behaviors.playful_quirks = on;
        }
    }

    pub fn can_add(&self) -> bool {
        self.members.len() < MAX_CATS
    }

    /// Adds a grown cat with a random coat, walking in from the side of the screen nearest the
    /// cursor. Returns its index, or `None` when the clowder is full.
    pub fn add_cat(&mut self) -> Option<usize> {
        if !self.can_add() {
            return None;
        }
        let profile = CatProfile::new(
            self.fresh_name(),
            CoatId::Random(fresh_seed()),
            Temperament::Balanced,
        );
        let coat = Coat::random(match profile.coat {
            CoatId::Random(seed) => seed,
            _ => 0,
        });
        let start = self.edge_spawn_point();
        let i = self.push_member(profile, coat, start);
        self.join_feeding(i);
        Some(i)
    }

    /// Adds a playful kitten of the grown cat `mother`, next to her. Returns its index, or
    /// `None` when the clowder is full or `mother` is a kitten.
    pub fn add_kitten(&mut self, mother: usize) -> Option<usize> {
        if !self.can_add() || self.members.get(mother).is_none_or(|m| m.kitten) {
            return None;
        }
        let seed = fresh_seed();
        let mut profile = CatProfile::new(
            self.fresh_name(),
            CoatId::Random(seed),
            Temperament::Playful,
        );
        profile.size = KITTEN_SIZE;
        profile.kitten = true;
        profile.mother = Some(self.members[mother].name.clone());
        let mom = &self.members[mother].cat;
        let start = mom.window_center() - Vec2::from_angle(mom.body_angle) * (30.0 * mom.scale);
        let i = self.push_member(profile, Coat::random(seed), start);
        self.join_feeding(i);
        Some(i)
    }

    /// Just off the edge of the first cat's screen nearest the cursor, at the cursor's height
    fn edge_spawn_point(&self) -> Vec2 {
        let first = &self.members[0].cat;
        let cursor = first.raw_cursor;
        let (min, max) = first.roam_bounds.unwrap_or((
            cursor - Vec2::new(600.0, 400.0),
            cursor + Vec2::new(600.0, 400.0),
        ));
        let margin = 40.0 * self.scale;
        let x = if cursor.x - min.x < max.x - cursor.x {
            min.x - margin
        } else {
            max.x + margin
        };
        Vec2::new(x, cursor.y.clamp(min.y, max.y.max(min.y)))
    }

    /// Removes cat `i`. The last cat can't be removed. Its kittens keep living, motherless.
    pub fn remove(&mut self, i: usize) -> bool {
        if self.members.len() <= 1 || i >= self.members.len() {
            return false;
        }
        let removed = self.members.remove(i);
        for m in &mut self.members {
            if m.mother.as_deref() == Some(&removed.name) {
                m.mother = None;
            }
            // Indices moved; a mother still fetching starts again next tick if she needs to
            m.fetching = None;
            m.cat.fetch_target = None;
        }
        if let Some(session) = &mut self.feeding {
            if session.eater == Some(i) {
                session.eater = None;
            }
            session.queue.retain(|&q| q != i);
            let shift = |q: &mut usize| {
                if *q > i {
                    *q -= 1;
                }
            };
            session.queue.iter_mut().for_each(shift);
            if let Some(e) = &mut session.eater {
                shift(e);
            }
        }
        true
    }

    /// Advances every cat by `dt` toward the cursor, then the shared bowl and the bonds
    pub fn update(&mut self, cursor: Vec2, dt: f32) {
        self.share_neighbors();
        self.update_fetching();
        self.assign_slots();
        for m in &mut self.members {
            m.cat.update(cursor, dt);
        }
        self.tick_feeding(dt);
        self.update_bonds(dt);
    }

    /// What cat `j` is to cat `i`
    pub fn kinship(&self, i: usize, j: usize) -> Kinship {
        let (a, b) = (&self.members[i], &self.members[j]);
        if i == j {
            Kinship::None
        } else if a.mother.as_deref() == Some(&b.name) {
            Kinship::Mother
        } else if b.mother.as_deref() == Some(&a.name) {
            Kinship::Kitten
        } else if a.mother.is_some() && a.mother == b.mother {
            Kinship::Sibling
        } else {
            Kinship::None
        }
    }

    /// How cats `i` and `j` get on, from the bond table, or as on first meeting if they haven't
    /// met yet. Family is never below its floor.
    pub fn bond(&self, i: usize, j: usize) -> Bond {
        let (a, b) = (&self.members[i], &self.members[j]);
        let kin = self.kinship(i, j);
        match self.bonds.get(&a.name, &b.name) {
            Some(bond) => bond.with_floor(kin),
            None => Bond::first_meeting(self.bond_baseline(i, j), kin),
        }
    }

    fn bond_baseline(&self, i: usize, j: usize) -> f32 {
        let traits = |m: &Member| (m.cat.behaviors.temperament, m.kitten);
        Bond::baseline(traits(&self.members[i]), traits(&self.members[j]))
    }

    #[cfg(test)]
    pub fn set_bond(&mut self, i: usize, j: usize, bond: Bond) {
        let (a, b) = (self.members[i].name.clone(), self.members[j].name.clone());
        self.bonds.set(&a, &b, bond);
    }

    /// Uses `bonds` (loaded from disk) for how the cats get on
    pub fn with_bonds(mut self, bonds: Bonds) -> Self {
        self.bonds = bonds;
        self
    }

    /// Saves the bonds every few minutes; call once a frame
    pub fn save_bonds_if_due(&mut self) {
        if self.bonds_unsaved >= BONDS_SAVE_SECS {
            self.save_bonds();
        }
    }

    pub fn save_bonds(&mut self) {
        self.bonds_unsaved = 0.0;
        if self.members.len() > 1 {
            self.bonds.save();
        }
    }

    /// Gives every cat a snapshot of the others as they were at the end of the last tick, with
    /// what each is to it and how well they get on
    fn share_neighbors(&mut self) {
        let n = self.members.len();
        let mut all: Vec<Vec<Neighbor>> = Vec::with_capacity(n);
        for i in 0..n {
            let mut seen = Vec::with_capacity(n.saturating_sub(1));
            for (j, other) in self.members.iter().enumerate() {
                if j == i {
                    continue;
                }
                let cat = &other.cat;
                let bond = self.bond(i, j);
                seen.push(Neighbor {
                    head: cat.nodes[0],
                    center: cat.body_point(),
                    radius: CatSim::PERSONAL_SPACE * cat.scale,
                    scale: cat.scale,
                    speed: cat.speed,
                    pose: Pose::of(cat),
                    roaming: cat.roaming,
                    kin: self.kinship(i, j),
                    familiarity: bond.familiarity,
                    affection: bond.affection,
                });
            }
            all.push(seen);
        }
        for (m, seen) in self.members.iter_mut().zip(all) {
            m.cat.neighbors = seen;
        }
    }

    /// The index into cat `i`'s `neighbors` of cat `j`
    fn neighbor_index(i: usize, j: usize) -> usize {
        if j < i { j } else { j - 1 }
    }

    /// Sends a mother to fetch her kitten when it has wandered off on its own (shaken off by the
    /// cursor) out of her reach. Once she reaches it, it comes back with her.
    fn update_fetching(&mut self) {
        for k in 0..self.members.len() {
            let Some(m) = self.members[k]
                .mother
                .as_deref()
                .and_then(|name| self.index_of(name))
            else {
                continue;
            };
            let (mother, kitten) = (&self.members[m].cat, &self.members[k].cat);
            let able = self.follow_cursor
                && kitten.roaming
                && !mother.roaming
                && mother.feeding_phase.is_none()
                && kitten.feeding_phase.is_none()
                && !matches!(Pose::of(mother), Pose::Busy | Pose::Racing);
            let fetching = self.members[m].fetching == Some(k);
            let gap = mother.nodes[0].distance(kitten.nodes[0]);
            let reached = gap < 1.6 * CatSim::PERSONAL_SPACE * (mother.scale + kitten.scale);
            let far = gap > FETCH_RADIUS * mother.scale;
            if able && fetching && reached {
                self.members[k].cat.call_back();
                self.members[m].fetching = None;
                self.members[m].cat.fetch_target = None;
            } else if able && (fetching || (far && self.members[m].fetching.is_none())) {
                self.members[m].fetching = Some(k);
                self.members[m].cat.fetch_target = Some(self.members[k].cat.nodes[0]);
            } else if fetching {
                self.members[m].fetching = None;
                self.members[m].cat.fetch_target = None;
            }
        }
    }

    /// Cats get to know each other over time, faster while near each other, and grow fond of
    /// each other resting side by side. Away from each other, fondness drifts back toward what
    /// their temperaments make of each other.
    fn update_bonds(&mut self, dt: f32) {
        let n = self.members.len();
        if n < 2 {
            return;
        }
        self.bonds_unsaved += dt;
        for i in 0..n {
            for j in (i + 1)..n {
                let (a, b) = (&self.members[i].cat, &self.members[j].cat);
                let s = a.scale.max(b.scale);
                let gap = a.nodes[0].distance(b.nodes[0]);
                let near = gap < 150.0 * s;
                let resting_together = near && Pose::of(a).is_resting() && Pose::of(b).is_resting();
                let mut bond = self.bond(i, j);
                let meet_rate = if near { 1.0 / 2700.0 } else { 0.0 };
                bond.familiarity = (bond.familiarity + dt * (1.0 / 10800.0 + meet_rate)).min(1.0);
                if resting_together {
                    bond.affection += dt / 1500.0;
                }
                let baseline = self.bond_baseline(i, j);
                bond.affection += (baseline - bond.affection) * (dt / 7200.0).min(1.0);
                let bond = bond.with_floor(self.kinship(i, j));
                let (na, nb) = (self.members[i].name.clone(), self.members[j].name.clone());
                self.bonds.set(&na, &nb, bond);
            }
        }
    }

    /// Gives every cat that gathers round the cursor (or the bowl) a spot of its own on a
    /// circle round it, clear of the other cats' heads and bodies, so they settle side by side.
    /// Grown cats choose before kittens, then in list order. A cat near its spot keeps it until
    /// a cat that chooses first needs the room; one further away takes the spot on its way in.
    /// A lone cat has no spot and follows the cursor as it always has.
    fn assign_slots(&mut self) {
        let n = self.members.len();
        if n < 2 {
            for cat in self.cats_mut() {
                cat.slot = None;
            }
            return;
        }
        let wants: Vec<Option<(Vec2, f32)>> = self.cats().map(CatSim::wants_slot).collect();
        // Points already taken and how much room each needs: first the cats that don't take a
        // spot (eating, hunting, resting where they settled) where they are now
        let mut taken: Vec<(Vec2, f32, usize)> = Vec::new();
        for (j, (cat, want)) in self.cats().zip(&wants).enumerate() {
            if want.is_none() {
                let room = CatSim::PERSONAL_SPACE * cat.scale;
                taken.push((cat.nodes[0], room, j));
                taken.push((cat.body_point(), room, j));
            }
        }
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| (self.members[i].kitten, i));
        for i in order {
            let cat = &self.members[i].cat;
            let Some((center, ring)) = wants[i] else {
                self.members[i].cat.slot = None;
                continue;
            };
            let room = CatSim::PERSONAL_SPACE * cat.scale;
            let body = CatSim::BODY_SPACING_OFFSET * cat.scale;
            let head_rel = cat.nodes[0] - center;
            let head_angle = if head_rel.length_squared() > 1.0 {
                head_rel.y.atan2(head_rel.x)
            } else {
                cat.body_angle + std::f32::consts::PI
            };
            let near = head_rel.length() < ring * 2.5;
            let preferred = match cat.slot {
                Some(slot) if near => slot.angle,
                _ => head_angle,
            };
            // A cat at rest faces the centre, so its body lies further out on the same ray. How
            // much room it leaves another cat depends on how well the two get on.
            let neighbors = &cat.neighbors;
            let fits = |angle: f32, radius: f32| {
                let ray = Vec2::from_angle(angle);
                [radius, radius + body].into_iter().all(|r| {
                    let p = center + ray * r;
                    taken.iter().all(|&(q, q_room, j)| {
                        let space = neighbors
                            .get(Self::neighbor_index(i, j))
                            .map_or(1.0, Neighbor::space);
                        p.distance(q) >= (room + q_room) * space
                    })
                })
            };
            // Look round the circle, first on the side the cat is on, then further out
            let first_side = if wrap_angle(head_angle - preferred) >= 0.0 {
                1.0
            } else {
                -1.0
            };
            let chosen = (0..3).find_map(|ring_i| {
                let r = ring + ring_i as f32 * 2.0 * room;
                let step = (0.25 * room / r).clamp(0.03, 0.3);
                let steps = (std::f32::consts::PI / step) as i32;
                (0..=steps).find_map(|k| {
                    [first_side, -first_side].into_iter().find_map(|side| {
                        let angle = wrap_angle(preferred + side * k as f32 * step);
                        fits(angle, r).then_some((angle, r))
                    })
                })
            });
            let (angle, radius) = chosen.unwrap_or((preferred, ring));
            let ray = Vec2::from_angle(angle);
            taken.push((center + ray * radius, room, i));
            taken.push((center + ray * (radius + body), room, i));
            let cat = &mut self.members[i].cat;
            if near
                && cat
                    .slot
                    .is_some_and(|s| wrap_angle(s.angle - angle).abs() > 0.05)
            {
                cat.slot_bumped = true;
            }
            cat.slot = Some(Slot { angle, radius });
        }
    }

    /// Starts carrying the bowl under the cursor; every cat follows the bowl
    pub fn begin_bowl_placement(&mut self, preset: FeedPreset, cursor: Vec2) {
        self.feeding = Some(FeedSession {
            preset,
            bowl_pos: None,
            eaten: 0.0,
            eater: None,
            queue: VecDeque::new(),
        });
        for cat in self.cats_mut() {
            cat.begin_bowl_placement(preset, cursor);
        }
    }

    /// Puts the bowl down at `bowl_pos`. Kittens eat first, then the grown cats, each group
    /// nearest first; the rest wait by the bowl.
    pub fn place_bowl(&mut self, bowl_pos: Vec2) {
        let preset = self
            .feeding
            .as_ref()
            .map_or(FeedPreset::Mins30, |s| s.preset);
        let mut order: Vec<usize> = (0..self.members.len()).collect();
        order.sort_by(|&a, &b| {
            let key = |i: usize| {
                let m = &self.members[i];
                (!m.kitten, m.cat.nodes[0].distance(bowl_pos))
            };
            let (ka, kb) = (key(a), key(b));
            ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        });
        self.feeding = Some(FeedSession {
            preset,
            bowl_pos: Some(bowl_pos),
            eaten: 0.0,
            eater: None,
            queue: order.into(),
        });
        for i in 0..self.members.len() {
            self.members[i].cat.wait_for_bowl(preset, bowl_pos);
        }
        self.next_turn();
    }

    /// A cat added while feeding joins in: it follows the carried bowl, or queues for it
    fn join_feeding(&mut self, i: usize) {
        let Some(session) = &mut self.feeding else {
            return;
        };
        let cat = &mut self.members[i].cat;
        match session.bowl_pos {
            None => cat.begin_bowl_placement(session.preset, cat.raw_cursor),
            Some(bowl_pos) => {
                session.queue.push_back(i);
                cat.wait_for_bowl(session.preset, bowl_pos);
            }
        }
    }

    pub fn stop_feeding(&mut self) {
        self.feeding = None;
        for cat in self.cats_mut() {
            cat.stop_feeding();
        }
    }

    /// Seconds of the preset left to eat (`None` for an unlimited preset)
    fn remaining(session: &FeedSession) -> Option<f32> {
        session
            .preset
            .duration_secs()
            .map(|d| (d - session.eaten).max(0.0))
    }

    /// Gives the bowl to the next cat in the queue, for a turn of at most `TURN_SECS` when
    /// others are waiting
    fn next_turn(&mut self) {
        let shared = self.members.len() > 1;
        let Some(session) = &mut self.feeding else {
            return;
        };
        let (Some(bowl_pos), None) = (session.bowl_pos, session.eater) else {
            return;
        };
        let Some(next) = session.queue.pop_front() else {
            return;
        };
        let remaining = Self::remaining(session);
        let turn = if shared {
            Some(remaining.map_or(TURN_SECS, |r| r.min(TURN_SECS)))
        } else {
            remaining
        };
        session.eater = Some(next);
        self.members[next].cat.place_bowl_for_turn(bowl_pos, turn);
    }

    /// Counts eating time, refills the bowl's fill level, and passes the bowl on when a turn
    /// ends. Feeding ends when the preset's time has been eaten.
    fn tick_feeding(&mut self, dt: f32) {
        let Some(session) = &mut self.feeding else {
            return;
        };
        let Some(bowl_pos) = session.bowl_pos else {
            // Placement cancelled some other way
            if !self.members.iter().any(|m| m.cat.is_carrying_bowl()) {
                self.feeding = None;
            }
            return;
        };
        let mut turn_over = None;
        if let Some(e) = session.eater {
            let cat = &mut self.members[e].cat;
            if cat.is_feeding() {
                session.eaten += dt;
                cat.bowl_fill = Some(match session.preset.duration_secs() {
                    Some(total) if total > 0.0 => 1.0 - session.eaten / total,
                    _ => 1.0,
                });
            } else if cat.feeding_phase.is_none() {
                session.eater = None;
                turn_over = Some(e);
            }
        }
        let finished = Self::remaining(session).is_some_and(|r| r < 1.0)
            || (self.members.len() == 1 && turn_over.is_some());
        if finished {
            // The cat that just ate stays put grooming; the ones still waiting are let go
            self.feeding = None;
            for cat in self.cats_mut() {
                if cat.feeding_phase.is_some() {
                    cat.stop_feeding();
                }
            }
            return;
        }
        if let Some(e) = turn_over {
            // Back of the queue, sitting by the bowl again
            session.queue.push_back(e);
            self.members[e].cat.wait_for_bowl(session.preset, bowl_pos);
        }
        self.next_turn();
    }

    pub fn feeding_preset(&self) -> Option<FeedPreset> {
        self.feeding.as_ref().map(|s| s.preset)
    }

    /// Whole seconds of the timed preset left (`None` when not feeding or unlimited)
    pub fn feeding_remaining_secs(&self) -> Option<u32> {
        self.feeding
            .as_ref()
            .and_then(Self::remaining)
            .map(|r| r.ceil() as u32)
    }

    pub fn is_carrying_bowl(&self) -> bool {
        self.feeding.as_ref().is_some_and(|s| s.bowl_pos.is_none())
    }

    /// The bowl, when it is drawn in its own window: on the cursor, or on the floor while the
    /// next cat walks over
    pub fn detached_bowl_pos(&self) -> Option<Vec2> {
        self.cats().find_map(CatSim::detached_bowl_pos)
    }

    /// Keep-awake runs from the moment the bowl is put down until feeding ends
    pub fn keeps_awake(&self) -> bool {
        self.feeding.as_ref().is_some_and(|s| s.bowl_pos.is_some())
    }

    /// The scale of whichever cat the bowl belongs to right now, for sizing the bowl window
    pub fn bowl_scale(&self) -> f32 {
        self.feeding
            .as_ref()
            .and_then(|s| s.eater)
            .map_or(self.scale, |e| self.members[e].cat.scale)
    }
}

/// A stable RNG seed from a cat's name, so each cat makes its own choices
fn name_seed(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cat::Activity;
    use crate::coat::CoatStyle;

    const DT: f32 = 1.0 / 30.0;

    fn profile(name: &str) -> CatProfile {
        CatProfile::new(
            name,
            CoatId::Builtin(CoatStyle::Tuxedo),
            Temperament::Balanced,
        )
    }

    fn kitten_of(name: &str, mother: &str) -> CatProfile {
        CatProfile {
            size: KITTEN_SIZE,
            kitten: true,
            mother: Some(mother.into()),
            ..profile(name)
        }
    }

    fn clowder(profiles: &[CatProfile], start: Vec2) -> Clowder {
        Clowder::new(
            profiles,
            1.0,
            true,
            true,
            &CoatLibrary::from_toml(""),
            start,
        )
    }

    fn run(clowder: &mut Clowder, cursor: Vec2, secs: f32) {
        for _ in 0..(secs / DT) as usize {
            clowder.update(cursor, DT);
        }
    }

    #[test]
    fn a_lone_cat_behaves_exactly_like_a_bare_cat() {
        let start = Vec2::new(500.0, 400.0);
        let mut group = clowder(&[profile("Luna")], start);
        let mut bare = CatSim::new(start, 1.0, CoatStyle::Tuxedo);
        for frame in 0..1800 {
            let t = frame as f32 * DT;
            let cursor = start + Vec2::new(300.0 * (t * 0.4).sin(), 160.0 * (t * 0.25).cos());
            group.update(cursor, DT);
            bare.update(cursor, DT);
        }
        assert_eq!(group.cat(0).nodes, bare.nodes);
        assert_eq!(group.cat(0).activity, bare.activity);
    }

    #[test]
    fn cats_following_the_cursor_settle_side_by_side() {
        let start = Vec2::new(600.0, 400.0);
        let names = ["Luna", "Milo", "Pip", "Mochi"];
        let profiles: Vec<_> = names.iter().map(|n| profile(n)).collect();
        let mut group = clowder(&profiles, start);
        let cursor = Vec2::new(1100.0, 650.0);
        run(&mut group, cursor, 25.0);

        let heads: Vec<Vec2> = group.cats().map(|c| c.nodes[0]).collect();
        for (i, a) in heads.iter().enumerate() {
            let cat = group.cat(i);
            assert!(
                !matches!(
                    cat.activity,
                    Activity::Walking | Activity::StandingUp { .. }
                ),
                "cat {i} still moving: {:?}",
                cat.activity
            );
            assert!(
                a.distance(cursor) < 110.0,
                "cat {i} too far: {}",
                a.distance(cursor)
            );
            for b in &heads[i + 1..] {
                assert!(a.distance(*b) > 26.0, "heads {a} and {b} overlap");
            }
        }
    }

    /// Frames each cat spends getting up or walking over `secs` with the cursor at `cursor`
    fn frames_moving(group: &mut Clowder, cursor: Vec2, secs: f32) -> Vec<usize> {
        let mut moving = vec![0; group.len()];
        for _ in 0..(secs / DT) as usize {
            group.update(cursor, DT);
            for (i, cat) in group.cats().enumerate() {
                if matches!(
                    cat.activity,
                    Activity::Walking | Activity::StandingUp { .. }
                ) {
                    moving[i] += 1;
                }
            }
        }
        moving
    }

    #[test]
    fn settled_cats_stay_put_round_the_cursor_and_the_bowl() {
        let profiles = [
            profile("Luna"),
            CatProfile {
                temperament: Temperament::Playful,
                ..profile("Milo")
            },
            kitten_of("Pip", "Luna"),
            CatProfile {
                temperament: Temperament::Lazy,
                ..profile("Nori")
            },
        ];
        let mut group = Clowder::new(
            &profiles,
            1.5,
            true,
            true,
            &CoatLibrary::from_toml(""),
            Vec2::new(150.0, 150.0),
        );
        let cursor = Vec2::new(420.0, 300.0);
        run(&mut group, cursor, 20.0);
        // Turning round, lying down or a neighbour's fidgeting never gets them up. The kitten
        // wakes from its naps by itself and stands, but doesn't walk off.
        let moving = frames_moving(&mut group, cursor, 90.0);
        assert_eq!([moving[0], moving[1], moving[3]], [0; 3]);
        assert!(
            moving[2] < 40,
            "the kitten only stands up after naps: {moving:?}"
        );

        let bowl = Vec2::new(300.0, 250.0);
        group.begin_bowl_placement(FeedPreset::Mins15, bowl);
        group.place_bowl(bowl);
        run(&mut group, bowl, 15.0);
        assert_eq!(frames_moving(&mut group, bowl, 90.0), vec![0; 4]);
        for i in [0, 1, 3] {
            let d = group.cat(i).nodes[0].distance(bowl);
            assert!(
                d > 50.0,
                "cat {i} waits clear of the bowl, not {d} px from it"
            );
        }
    }

    #[test]
    fn kittens_eat_first_and_the_cats_take_turns_at_the_bowl() {
        let start = Vec2::new(600.0, 400.0);
        let mut group = clowder(
            &[profile("Luna"), profile("Milo"), kitten_of("Pip", "Luna")],
            start,
        );
        let bowl = Vec2::new(760.0, 480.0);
        group.begin_bowl_placement(FeedPreset::Mins15, bowl);
        assert!(group.is_carrying_bowl());
        assert!(!group.keeps_awake());
        run(&mut group, bowl, 2.0);
        group.place_bowl(bowl);
        assert!(group.keeps_awake());

        run(&mut group, bowl, 12.0);
        assert!(group.cat(2).is_feeding(), "the kitten eats first");
        for i in [0, 1] {
            let cat = group.cat(i);
            assert!(cat.is_waiting_for_bowl());
            assert!(
                cat.nodes[0].distance(bowl) < 90.0,
                "cat {i} waits by the bowl"
            );
            assert!(cat.nodes[0].distance(group.cat(2).nodes[0]) > 20.0);
        }

        // The whole session is one preset long, split into turns
        let mut eaters = Vec::new();
        for _ in 0..(16.0 * 60.0 / DT) as usize {
            group.update(bowl, DT);
            if let Some(i) = (0..3).find(|&i| group.cat(i).is_feeding())
                && eaters.last() != Some(&i)
            {
                eaters.push(i);
            }
        }
        assert_eq!(
            eaters[..3],
            [2, 0, 1],
            "kitten first, then nearest grown cat"
        );
        assert!(eaters.len() >= 6, "turns go round: {eaters:?}");
        assert_eq!(group.feeding_preset(), None);
        assert!(!group.keeps_awake());
        assert!(group.cats().all(|c| c.feeding_phase.is_none()));
    }

    #[test]
    fn a_lone_cat_eats_the_whole_preset_in_one_go() {
        let start = Vec2::new(600.0, 400.0);
        let mut group = clowder(&[profile("Luna")], start);
        let bowl = Vec2::new(700.0, 420.0);
        group.begin_bowl_placement(FeedPreset::Mins15, bowl);
        group.place_bowl(bowl);
        run(&mut group, bowl, 10.0);
        assert!(group.cat(0).is_feeding());
        assert_eq!(
            group.cat(0).activity,
            Activity::Eating {
                preset: FeedPreset::Mins15,
                elapsed: match group.cat(0).activity {
                    Activity::Eating { elapsed, .. } => elapsed,
                    _ => unreachable!(),
                },
                duration: Some(900.0),
            }
        );
        run(&mut group, bowl, 900.0);
        assert_eq!(group.feeding_preset(), None);
        assert!(!group.cat(0).is_feeding());
    }

    #[test]
    fn stopping_feeding_lets_every_cat_go() {
        let mut group = clowder(&[profile("Luna"), profile("Milo")], Vec2::new(600.0, 400.0));
        let bowl = Vec2::new(700.0, 420.0);
        group.begin_bowl_placement(FeedPreset::Indefinite, bowl);
        group.place_bowl(bowl);
        run(&mut group, bowl, 8.0);
        assert_eq!(group.feeding_remaining_secs(), None);
        group.stop_feeding();
        assert!(group.cats().all(|c| c.feeding_phase.is_none()));
        assert!(!group.keeps_awake());
    }

    #[test]
    fn adding_and_removing_cats() {
        let mut group = clowder(&[profile("Luna")], Vec2::new(600.0, 400.0));
        group.cat_mut(0).roam_bounds = Some((Vec2::ZERO, Vec2::new(1920.0, 1080.0)));
        run(&mut group, Vec2::new(600.0, 400.0), 0.1);

        // A new cat walks in from the nearer (left) edge
        let milo = group.add_cat().unwrap();
        assert_eq!(group.members()[milo].name, "Milo");
        assert!(group.cat(milo).window_center().x < 0.0);

        let pip = group.add_kitten(0).unwrap();
        let kitten = &group.members()[pip];
        assert!(kitten.kitten);
        assert_eq!(kitten.mother.as_deref(), Some("Luna"));
        assert!((kitten.cat.scale - KITTEN_SIZE).abs() < 1e-4);
        assert_eq!(kitten.cat.behaviors.temperament, Temperament::Playful);
        assert_eq!(group.add_kitten(pip), None, "kittens have no kittens");

        group.set_scale(1.5);
        assert!((group.cat(pip).scale - 1.5 * KITTEN_SIZE).abs() < 1e-4);

        // Removing the mother leaves the kitten motherless
        assert!(group.remove(0));
        assert_eq!(group.members()[1].mother, None);
        assert!(group.remove(0));
        assert!(!group.remove(0), "the last cat stays");

        while group.add_cat().is_some() {}
        assert_eq!(group.len(), MAX_CATS);
        let mut names: Vec<_> = group.members().iter().map(|m| m.name.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), MAX_CATS);
        // Every name the tray may show can be drawn by its bitmap font
        for name in NAMES {
            assert!(
                name.chars().all(|c| crate::tray::glyph_5x7(c) != [0; 7]),
                "{name}"
            );
        }
    }

    #[test]
    fn saved_cats_are_checked_when_loaded() {
        let profiles = [
            profile("Luna"),
            profile("Luna"),
            kitten_of("Pip", "Pip"),
            kitten_of("Tofu", "Nobody"),
            kitten_of("Nori", "Luna"),
            CatProfile {
                mother: Some("Luna".into()),
                ..profile("Maple")
            },
        ];
        let group = clowder(&profiles, Vec2::new(600.0, 400.0));
        let loaded = group.profiles();
        assert_eq!(loaded[0].name, "Luna");
        assert_ne!(loaded[1].name, "Luna", "duplicate names are renamed");
        assert_eq!(loaded[2].mother, None, "a kitten isn't its own mother");
        assert_eq!(loaded[3].mother, None, "the mother must exist");
        assert_eq!(loaded[4].mother.as_deref(), Some("Luna"));
        assert_eq!(loaded[5].mother, None, "only kittens have mothers");

        // Different cats make different choices
        let mut group = clowder(&[profile("Luna"), profile("Milo")], Vec2::new(600.0, 400.0));
        assert_ne!(group.cat_mut(0).rand_u32(), group.cat_mut(1).rand_u32());
    }

    #[test]
    fn a_kitten_keeps_within_reach_of_its_mother() {
        let start = Vec2::new(900.0, 600.0);
        let mut group = clowder(&[profile("Luna"), kitten_of("Pip", "Luna")], start);
        run(&mut group, start, 8.0);
        // Shaken off, the mother wanders off while the kitten would follow the cursor
        group.cat_mut(0).shoo();
        let leash = CatSim::KITTEN_LEASH;
        let (mut leashed, mut widest, mut from_cursor) = (0, 0.0f32, 0.0f32);
        for frame in 0..(60.0 / DT) as usize {
            group.update(start, DT);
            let (mother, kitten) = (group.cat(0), group.cat(1));
            leashed += usize::from(kitten.leashed);
            if frame as f32 * DT > 5.0 {
                widest = widest.max(kitten.nodes[0].distance(mother.body_point()));
                from_cursor = from_cursor.max(kitten.nodes[0].distance(start));
            }
        }
        assert!(leashed > 0, "the leash never held the kitten back");
        assert!(
            from_cursor > leash,
            "the mother never went far: {from_cursor}"
        );
        assert!(
            widest < leash * 1.35,
            "the kitten got {widest} px from its mother"
        );
    }

    #[test]
    fn a_mother_fetches_her_kitten_that_wandered_off() {
        let start = Vec2::new(900.0, 600.0);
        let mut group = clowder(&[profile("Luna"), kitten_of("Pip", "Luna")], start);
        run(&mut group, start, 8.0);
        group.cat_mut(1).shoo();
        let mut fetched = false;
        for _ in 0..(120.0 / DT) as usize {
            group.update(start, DT);
            fetched |= group.cat(0).fetch_target.is_some();
            if fetched && !group.cat(1).roaming {
                break;
            }
        }
        assert!(fetched, "the mother never went for her kitten");
        assert!(!group.cat(1).roaming, "the kitten is still off on its own");
        // Both come back to the cursor, the kitten at its mother's side
        run(&mut group, start, 20.0);
        let (mother, kitten) = (group.cat(0), group.cat(1));
        assert!(mother.fetch_target.is_none());
        assert!(mother.nodes[0].distance(start) < 100.0);
        assert!(kitten.nodes[0].distance(mother.body_point()) < CatSim::KITTEN_LEASH);
    }

    #[test]
    fn cats_get_to_know_each_other_and_friends_sit_closer() {
        let start = Vec2::new(600.0, 400.0);
        let mut group = clowder(
            &[profile("Luna"), profile("Milo"), kitten_of("Pip", "Luna")],
            start,
        );
        let first = group.bond(0, 1);
        assert_eq!(first.familiarity, 0.0);
        let family = group.bond(0, 2);
        assert!(family.familiarity >= 0.9 && family.affection >= 0.8);
        run(&mut group, start, 600.0);
        let later = group.bond(0, 1);
        assert!(later.familiarity > 0.2, "familiarity {}", later.familiarity);
        assert!(
            later.affection > first.affection + 0.1,
            "affection {}",
            later.affection
        );

        // Strangers leave each other more room than friends
        let gap = |bond: Option<Bond>| {
            let mut group = clowder(&[profile("Luna"), profile("Milo")], start);
            if let Some(bond) = bond {
                group.set_bond(0, 1, bond);
            }
            let cursor = Vec2::new(1000.0, 600.0);
            run(&mut group, cursor, 20.0);
            group.cat(0).nodes[0].distance(group.cat(1).nodes[0])
        };
        let strangers = gap(None);
        let friends = gap(Some(Bond {
            familiarity: 1.0,
            affection: 0.9,
        }));
        assert!(
            friends + 5.0 < strangers,
            "friends {friends:.0} px apart, strangers {strangers:.0} px"
        );
    }
}
