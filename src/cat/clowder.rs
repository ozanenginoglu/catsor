//! Several cats at once (a group of cats is a "clowder"). `Clowder` owns every `CatSim`, tells
//! each one where the others are and what they are to it, keeps track of how well each pair
//! gets on (`Bonds`), sends a mother to fetch a kitten that wandered off (and sometimes carry it
//! back by the scruff), and runs the shared
//! food bowl: one cat eats at a time while the rest sit nearby and wait their turn, kittens
//! first.

use super::{
    Activity, BODY_NODES, CatSim, FeedPreset, Kinship, Neighbor, Pose, Slot, SocialKind,
    SocialRole, TOTAL_NODES, Temperament, wrap_angle,
};
use crate::bonds::{Bond, Bonds, PairMap};
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
/// How far from her (times the leash) a kitten asleep on its own can be before its mother goes
/// and carries it back
const STRANDED_LEASH: f32 = 1.3;
/// How likely a mother who reaches her kitten that is awake carries it back rather than calling
/// it along
const CARRY_CHANCE: f32 = 0.35;
/// Longest (seconds) a mother carries her kitten before putting it down
const CARRY_SECS: f32 = 30.0;
/// How long (seconds) lifting a kitten into the carrying hold takes; until then it is still
/// drawn in its own window
const LIFT_SECS: f32 = 0.4;
/// How often (seconds) the bonds are written to disk
const BONDS_SAVE_SECS: f32 = 300.0;
/// How often (seconds) `Clowder` looks for two cats to do something together
const DIRECTOR_TICK: f32 = 0.5;
/// How long (seconds) two cats must have been apart to greet each other when they meet again
const GREET_AFTER: f32 = 90.0;
/// How long (seconds) a pair rests from playing after playing, and after a squabble
const PLAY_REST: f32 = 25.0;
const SQUABBLE_REST: f32 = 120.0;
/// How long (seconds) the cat that got the laser dot keeps it after it last crouched or leapt
const LASER_HOLD: f32 = 3.0;
/// How fast (1/s) two cats whose bodies sank into each other are eased apart
const SEPARATION_RATE: f32 = 18.0;
/// How many days a kitten that grows up takes to become a grown cat
pub const GROW_UP_DAYS: f32 = 30.0;
/// The size a kitten grows to (unless it was already bigger)
const GROWN_SIZE: f32 = 1.0;

/// Seconds since the Unix epoch, for kittens' birth dates
pub fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Names handed to cats added from the tray, in order, skipping names already in use
const NAMES: [&str; 12] = [
    "Luna", "Milo", "Pip", "Mochi", "Olive", "Biscuit", "Nori", "Ziggy", "Maple", "Tofu",
    "Juniper", "Pepper",
];

/// Something the director might start between two cats this tick, and the chance it does
struct Idea {
    active: usize,
    passive: usize,
    kind: SocialKind,
    chance: f32,
    /// `Engagement::limit`
    limit: u8,
}

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
    /// The name of this cat's mother: another cat that isn't a kitten itself. A kitten that
    /// grew up keeps her
    pub mother: Option<String>,
    /// A kitten that grows into a grown cat over `GROW_UP_DAYS` from `born`
    pub grows_up: bool,
    /// When a growing kitten was born (seconds since the Unix epoch); set the first time it
    /// grows
    pub born: Option<u64>,
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
            grows_up: false,
            born: None,
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
    pub grows_up: bool,
    pub born: Option<u64>,
    /// How far a growing kitten has grown (0–1), easing its size from `size` toward grown
    growth: f32,
    pub cat: CatSim,
    /// The kitten (index) this mother is on her way to fetch
    fetching: Option<usize>,
    /// The cat (index) this one is doing something with
    partner: Option<usize>,
    /// The kitten this mother has in her mouth
    carry: Option<Carry>,
}

/// A mother carrying her kitten back by the scruff
#[derive(Debug, Clone, Copy)]
struct Carry {
    kitten: usize,
    /// How long she has been carrying it
    time: f32,
    /// Whether she has set off with it yet
    walked: bool,
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
            grows_up: self.grows_up,
            born: self.born,
        }
    }

    /// Its size now: `size`, or for a growing kitten, partway to grown
    fn current_size(&self) -> f32 {
        self.size + (self.size.max(GROWN_SIZE) - self.size) * self.growth
    }

    /// Days left before a growing kitten is grown
    pub fn days_to_grow(&self) -> Option<u32> {
        (self.kitten && self.grows_up)
            .then(|| ((1.0 - self.growth) * GROW_UP_DAYS).ceil().max(0.0) as u32)
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
    /// Seconds each pair (by name) has been apart; a pair not listed hasn't met this session
    apart: PairMap<f32>,
    /// Time until `Clowder` next looks for two cats to do something together
    director_timer: f32,
    /// `Clowder`'s own random choices, kept apart from each cat's so a lone cat is unaffected
    rng_state: u64,
    /// Whether cats start doing things together (off in tests of other mechanics)
    social: bool,
    /// The cat hunting the laser dot (the wiggling cursor); the others leave it to that one
    hunter: Option<usize>,
    /// Time left before the hunter gives the dot up, having stopped hunting it
    hunter_hold: f32,
    /// Seconds each pair (by name) still rests from playing together
    play_rest: PairMap<f32>,
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
            apart: PairMap::default(),
            director_timer: DIRECTOR_TICK,
            rng_state: 0x2545_f491_4f6c_dd1d,
            social: true,
            hunter: None,
            hunter_hold: 0.0,
            play_rest: PairMap::default(),
        };
        let default_profile = [CatProfile::default()];
        let profiles = if profiles.is_empty() {
            &default_profile[..]
        } else {
            profiles
        };
        let profiles = &profiles[..profiles.len().min(MAX_CATS)];
        // A renamed duplicate mustn't take the name of a cat further down the list
        let given: Vec<&str> = profiles.iter().map(|p| p.name.as_str()).collect();
        for profile in profiles {
            let mut profile = profile.clone();
            if profile.name.trim().is_empty() || clowder.index_of(&profile.name).is_some() {
                profile.name = clowder.fresh_name_avoiding(&given);
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
            grows_up: profile.grows_up && profile.kitten,
            born: profile.born,
            growth: 0.0,
            cat,
            fetching: None,
            partner: None,
            carry: None,
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
            if !valid {
                self.members[i].mother = None;
            }
        }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.members.iter().position(|m| m.name == name)
    }

    /// The first name from `NAMES` nobody uses yet, or "Cat N"
    fn fresh_name(&self) -> String {
        self.fresh_name_avoiding(&[])
    }

    /// A name no cat has, and not one of `reserved`
    fn fresh_name_avoiding(&self, reserved: &[&str]) -> String {
        NAMES
            .iter()
            .map(|n| n.to_string())
            .chain((2..).map(|n| format!("Cat {n}")))
            .find(|n| self.index_of(n).is_none() && !reserved.contains(&n.as_str()))
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
            m.cat.set_scale(scale * m.current_size());
        }
    }

    /// Grows the kittens that grow up, at `now` (seconds since the Unix epoch): each eases
    /// toward a grown cat's size and ways, and stops being a kitten once `GROW_UP_DAYS` have
    /// passed since it was born. A growing kitten without a birth date is born now. Returns
    /// whether the profiles changed and should be saved.
    pub fn grow(&mut self, now: u64) -> bool {
        let mut changed = false;
        for m in &mut self.members {
            if !(m.kitten && m.grows_up) {
                continue;
            }
            let born = *m.born.get_or_insert_with(|| {
                changed = true;
                now
            });
            let days = now.saturating_sub(born) as f32 / 86_400.0;
            m.growth = (days / GROW_UP_DAYS).clamp(0.0, 1.0);
            if m.growth >= 1.0 {
                // Grown: from now on a cat like any other, still her mother's
                m.size = m.size.max(GROWN_SIZE);
                m.growth = 0.0;
                m.kitten = false;
                m.grows_up = false;
                m.born = None;
                m.cat.kitten = false;
                m.cat.leashed = false;
                changed = true;
            }
            m.cat.youth = 1.0 - m.growth;
            m.cat.set_scale(self.scale * m.current_size());
        }
        changed
    }

    /// Lets kitten `i` grow up (from now on), or stops it growing at the size it has reached
    pub fn set_grows_up(&mut self, i: usize, on: bool) {
        let Some(m) = self.members.get_mut(i).filter(|m| m.kitten) else {
            return;
        };
        if !on {
            m.size = m.current_size();
            m.growth = 0.0;
            m.born = None;
            m.cat.youth = 1.0;
        }
        m.grows_up = on;
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

    /// Adds a playful kitten of the grown cat `mother`, next to her, with a coat rolled from
    /// hers. Returns its index, or `None` when the clowder is full or `mother` is a kitten.
    pub fn add_kitten(&mut self, mother: usize) -> Option<usize> {
        if !self.can_add() || self.members.get(mother).is_none_or(|m| m.kitten) {
            return None;
        }
        let coat = Coat::inherit(&self.members[mother].cat.coat, fresh_seed());
        let mut profile = CatProfile::new(self.fresh_name(), coat.id.clone(), Temperament::Playful);
        profile.size = KITTEN_SIZE;
        profile.kitten = true;
        profile.mother = Some(self.members[mother].name.clone());
        let mom = &self.members[mother].cat;
        let start = mom.window_center() - Vec2::from_angle(mom.body_angle) * (30.0 * mom.scale);
        let i = self.push_member(profile, coat, start);
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
        for m in 0..self.members.len() {
            self.put_down(m);
        }
        let removed = self.members.remove(i);
        for m in &mut self.members {
            if m.mother.as_deref() == Some(&removed.name) {
                m.mother = None;
            }
            // Indices moved; a mother still fetching starts again next tick if she needs to, and
            // whatever two cats were doing together stops
            m.fetching = None;
            m.cat.fetch_target = None;
            m.partner = None;
            m.cat.end_social();
            m.cat.laser_rival = false;
        }
        self.hunter = None;
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
        // A new cat given this name later starts out a stranger to the others. Saved now, as
        // with one cat left nothing would be saved on quit.
        self.bonds.forget(&removed.name);
        self.apart.forget(&removed.name);
        self.play_rest.forget(&removed.name);
        self.save_bonds_now();
        self.retime_turn();
        true
    }

    /// Advances every cat by `dt` toward the cursor, then the shared bowl and the bonds
    pub fn update(&mut self, cursor: Vec2, dt: f32) {
        self.share_neighbors();
        self.tend_carries(dt);
        self.update_fetching();
        self.referee_laser(dt);
        self.tend_engagements();
        self.direct(dt);
        self.assign_slots();
        for m in &mut self.members {
            if Pose::of(&m.cat) != Pose::Carried {
                m.cat.update(cursor, dt);
            }
        }
        self.hang_carried(cursor, dt);
        self.separate(dt);
        self.tick_feeding(dt);
        self.update_bonds(dt);
    }

    /// Eases apart any two cats whose bodies sank into each other this tick, beyond what
    /// they're doing together allows, the one that gives way more moving further. Walking round
    /// each other is the cats' own job; this catches what steering can't, such as a cat turning
    /// its rump into another or rolling over beside it.
    fn separate(&mut self, dt: f32) {
        let n = self.members.len();
        let ease = 1.0 - (-dt * SEPARATION_RATE).exp();
        let mut shoves = vec![Vec2::ZERO; n];
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (&self.members[i].cat, &self.members[j].cat);
                if Pose::of(a) == Pose::Carried || Pose::of(b) == Pose::Carried {
                    continue;
                }
                let contact = match a.social {
                    Some(e) if self.members[i].partner == Some(j) => {
                        e.kind.contact() * 0.5 * (a.scale + b.scale)
                    }
                    _ => 0.0,
                };
                let (depth, out) = a.body_overlap(b);
                let excess = depth - contact;
                if excess <= 0.0 {
                    continue;
                }
                let (give_a, give_b) = (a.give(), b.give());
                let push = out * (excess * ease / (give_a + give_b));
                shoves[i] += push * give_a;
                shoves[j] -= push * give_b;
            }
        }
        for (m, by) in self.members.iter_mut().zip(shoves) {
            if by != Vec2::ZERO {
                m.cat.shove(by);
            }
        }
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
        if self.members.len() > 1 {
            self.save_bonds_now();
        }
    }

    fn save_bonds_now(&mut self) {
        self.bonds_unsaved = 0.0;
        self.bonds.save();
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
                    rump: cat.nodes[BODY_NODES - 1],
                    tail_tip: cat.nodes[TOTAL_NODES - 1],
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
                .filter(|_| self.members[k].kitten)
                .and_then(|name| self.index_of(name))
            else {
                continue;
            };
            let (mother, kitten) = (&self.members[m].cat, &self.members[k].cat);
            let fetching = self.members[m].fetching == Some(k);
            let gap = mother.nodes[0].distance(kitten.nodes[0]);
            // Asleep on its own, too far from her: she goes and carries it back
            let stranded = kitten.activity == Activity::Sleeping
                && kitten.social.is_none()
                && (fetching || gap > STRANDED_LEASH * CatSim::KITTEN_LEASH * mother.scale);
            let able = self.follow_cursor
                && (kitten.roaming || stranded)
                && self.members[m].carry.is_none()
                && !mother.roaming
                && mother.feeding_phase.is_none()
                && kitten.feeding_phase.is_none()
                && !matches!(Pose::of(mother), Pose::Busy | Pose::Racing);
            let reached = gap < 1.6 * CatSim::PERSONAL_SPACE * (mother.scale + kitten.scale);
            let far = gap > FETCH_RADIUS * mother.scale || stranded;
            if able && fetching && reached {
                // A sleeping kitten is carried back; one awake now and then too, else it's called
                let asleep = kitten.activity == Activity::Sleeping;
                let carry = kitten.social.is_none() && (asleep || self.rand_f32() < CARRY_CHANCE);
                self.members[m].fetching = None;
                self.members[m].cat.fetch_target = None;
                if carry {
                    self.pick_up(m, k);
                } else {
                    self.members[k].cat.call_back();
                }
            } else if able && (fetching || (far && self.members[m].fetching.is_none())) {
                self.members[m].fetching = Some(k);
                self.members[m].cat.fetch_target = Some(self.members[k].cat.nodes[0]);
            } else if fetching {
                self.members[m].fetching = None;
                self.members[m].cat.fetch_target = None;
            }
        }
    }

    /// Mother `m` picks her kitten `k` up by the scruff to carry it back
    fn pick_up(&mut self, m: usize, k: usize) {
        let side = if self.rand_f32() < 0.5 { 1.0 } else { -1.0 };
        let (mouth, _) = self.members[m].cat.mouth();
        self.members[k].cat.pick_up(mouth, side);
        self.members[m].cat.carrying = true;
        self.members[m].carry = Some(Carry {
            kitten: k,
            time: 0.0,
            walked: false,
        });
    }

    /// Mother `m` puts down the kitten she's carrying, if she is
    fn put_down(&mut self, m: usize) {
        if let Some(carry) = self.members[m].carry.take() {
            self.members[carry.kitten].cat.put_down();
        }
        self.members[m].cat.carrying = false;
    }

    /// A mother carries her kitten back to her spot and puts it down once she has stopped there,
    /// or straight away when something else comes up (a sprint, food, play, being shaken off)
    fn tend_carries(&mut self, dt: f32) {
        for m in 0..self.members.len() {
            let Some(mut carry) = self.members[m].carry else {
                continue;
            };
            let mother = &self.members[m].cat;
            let kitten = &self.members[carry.kitten].cat;
            carry.time += dt;
            carry.walked |= mother.activity == Activity::Walking;
            let on_the_way = match mother.activity {
                Activity::Walking | Activity::StandingUp { .. } => true,
                Activity::StandingIdle { .. } => !carry.walked,
                _ => false,
            };
            let keep = on_the_way
                && self.follow_cursor
                && carry.time < CARRY_SECS
                && !mother.roaming
                && !mother.sprinting
                && mother.social.is_none()
                && mother.feeding_phase.is_none()
                && kitten.feeding_phase.is_none();
            if keep {
                self.members[m].carry = Some(carry);
            } else {
                self.put_down(m);
            }
        }
    }

    /// Hangs each carried kitten from its mother's mouth where she is now
    fn hang_carried(&mut self, cursor: Vec2, dt: f32) {
        for m in 0..self.members.len() {
            if let Some(carry) = self.members[m].carry {
                let (mouth, facing) = self.members[m].cat.mouth();
                self.members[carry.kitten]
                    .cat
                    .hang_from(mouth, facing, cursor, dt);
            }
        }
    }

    /// The cats drawn in cat `i`'s window, bottom first. Once lifted into her hold, a carried
    /// kitten is drawn under its mother in her window (big enough for both), and its own
    /// window is empty.
    pub fn drawn_in(&self, i: usize) -> impl Iterator<Item = &CatSim> {
        let held = |c: &Carry| c.time >= LIFT_SECS;
        let carried = self.members[i]
            .carry
            .filter(held)
            .map(|c| &self.members[c.kitten].cat);
        let own = !self
            .members
            .iter()
            .any(|m| m.carry.is_some_and(|c| c.kitten == i && held(&c)));
        carried
            .into_iter()
            .chain(own.then_some(&self.members[i].cat))
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
                self.bonds
                    .set(&self.members[i].name, &self.members[j].name, bond);
            }
        }
    }

    fn rand_f32(&mut self) -> f32 {
        // xorshift64*
        self.rng_state ^= self.rng_state >> 12;
        self.rng_state ^= self.rng_state << 25;
        self.rng_state ^= self.rng_state >> 27;
        let x = self.rng_state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        (x >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Ends what two cats were doing together once either is done with it (or was pulled
    /// away), for both, and starts whatever follows from it. Something that went well warms
    /// them to each other a little; friction cools them a little.
    fn tend_engagements(&mut self) {
        for i in 0..self.members.len() {
            let Some(j) = self.members[i].partner else {
                if self.members[i].cat.social.is_some() {
                    self.members[i].cat.end_social();
                }
                continue;
            };
            let over = |c: &CatSim| c.social.is_none_or(|e| e.done);
            // Pouncing on the tail of a cat busy with the laser dot: only one of them knows
            if self.members[j].partner != Some(i) {
                if over(&self.members[i].cat) {
                    self.members[i].cat.end_social();
                    self.members[i].partner = None;
                }
                continue;
            }
            if j < i {
                continue;
            }
            // Which one went over (the first, when both did)
            let (act, pas) = match self.members[i].cat.social {
                Some(e) if e.role == SocialRole::Passive => (j, i),
                _ => (i, j),
            };
            let (ea, ep) = (self.members[act].cat.social, self.members[pas].cat.social);
            if let (Some(e), Some(_)) = (ea, ep)
                && e.kind == SocialKind::BowlPush
                && e.acting
                && !e.done
            {
                self.settle_bowl_push(act, pas);
                continue;
            }
            let (a, b) = (&self.members[act].cat, &self.members[pas].cat);
            if over(a) || over(b) {
                // Neither was pulled away, and they got to it
                let both = ea.is_some() && ep.is_some();
                let went_well = both && [ea, ep].into_iter().flatten().any(|e| e.acting);
                let kind = ea.or(ep).map(|e| e.kind);
                let warmth = match kind {
                    Some(SocialKind::Groom) => 0.06,
                    Some(SocialKind::NoseBoop | SocialKind::HeadBunt | SocialKind::SniffCircle) => {
                        0.03
                    }
                    _ => 0.0,
                };
                if went_well && warmth > 0.0 {
                    self.nudge_affection(act, pas, warmth);
                }
                for k in [act, pas] {
                    self.members[k].cat.end_social();
                    self.members[k].partner = None;
                }
                if let (Some(kind), Some(e), true) = (kind, ea, both) {
                    self.follow_up(kind, e, act, pas);
                }
                continue;
            }
            // The cat that stays put shares in it once the other one gets there
            let acting = a.social.is_some_and(|e| e.acting) || b.social.is_some_and(|e| e.acting);
            for k in [act, pas] {
                if let Some(e) = &mut self.members[k].cat.social
                    && e.role == SocialRole::Passive
                {
                    e.acting = acting;
                    // How many times its tail was pounced on, to grow more annoyed
                    if e.kind == SocialKind::TailStalk {
                        e.rounds = ea.map_or(0, |e| e.rounds);
                    }
                }
            }
        }
    }

    /// What happens after play or a squabble that ran its course (`e` is the active cat's half)
    fn follow_up(&mut self, kind: SocialKind, e: super::Engagement, act: usize, pas: usize) {
        if kind.is_play() {
            self.rest_from_play(act, pas, PLAY_REST);
        }
        match kind {
            SocialKind::TailStalk if e.rounds >= e.limit => {
                if self.kinship(act, pas) == Kinship::Mother {
                    // Its mother has had enough: a gentle boop, nose to nose
                    self.engage(act, pas, SocialKind::NoseBoop);
                    self.nudge_affection(act, pas, 0.02);
                } else if self.members[act].kitten && !self.members[pas].kitten {
                    // A grown cat pestered by a kitten swats it, or gets up and walks off
                    if self.rand_f32() < 0.5 {
                        self.engage_with(pas, act, SocialKind::Swat, 1);
                    } else {
                        let from = self.members[act].cat.nodes[0];
                        self.members[pas].cat.back_off(from, 90.0);
                        self.nudge_affection(act, pas, -0.02);
                        self.rest_from_play(act, pas, SQUABBLE_REST);
                    }
                } else {
                    self.nudge_affection(act, pas, 0.02);
                }
            }
            // Caught up: they swap round, roll into a wrestle, or it gets too rough
            SocialKind::Chase if e.rounds > 0 => {
                let roll = self.rand_f32();
                if e.limit > 0 && roll < 0.4 {
                    self.engage_with(pas, act, SocialKind::Chase, e.limit - 1);
                } else if roll < 0.7 {
                    let turns = self.wrestle_turns();
                    self.engage_with(act, pas, SocialKind::Wrestle, turns);
                } else if self.rand_f32() < self.escalation(act, pas) {
                    self.start_standoff(act, pas);
                } else {
                    self.nudge_affection(act, pas, 0.02);
                }
            }
            SocialKind::Wrestle if e.rounds >= e.limit => {
                if self.rand_f32() < self.escalation(act, pas) {
                    self.start_standoff(act, pas);
                } else {
                    // "We weren't fighting"
                    for k in [act, pas] {
                        self.members[k].cat.groom_it_off();
                    }
                    self.nudge_affection(act, pas, 0.02);
                }
            }
            // They break apart and both groom
            SocialKind::Standoff if e.acting => {
                let (pa, pb) = (
                    self.members[act].cat.body_point(),
                    self.members[pas].cat.body_point(),
                );
                let (da, db) = (55.0 + 20.0 * self.rand_f32(), 55.0 + 20.0 * self.rand_f32());
                self.members[act].cat.back_off(pb, da);
                self.members[pas].cat.back_off(pa, db);
                self.nudge_affection(act, pas, -0.05);
                self.rest_from_play(act, pas, SQUABBLE_REST);
            }
            // The swatted cat backs off
            SocialKind::Swat if e.acting => {
                let from = self.members[act].cat.nodes[0];
                self.members[pas].cat.back_off(from, 35.0);
                self.nudge_affection(act, pas, -0.04);
                self.rest_from_play(act, pas, SQUABBLE_REST);
            }
            _ => {}
        }
    }

    /// A cat pushing in at the bowl got there: the one eating gives way (a mother always lets
    /// her kitten), or swats it
    fn settle_bowl_push(&mut self, pusher: usize, eater: usize) {
        for k in [pusher, eater] {
            self.members[k].cat.end_social();
            self.members[k].partner = None;
        }
        let gives_way = if self.kinship(pusher, eater) == Kinship::Mother {
            0.9
        } else {
            match self.members[eater].cat.behaviors.temperament {
                Temperament::Lazy => 0.6,
                Temperament::Balanced => 0.45,
                Temperament::Playful | Temperament::Realistic => 0.4,
                Temperament::Aloof => 0.25,
            }
        };
        self.rest_from_play(pusher, eater, 60.0);
        if self.rand_f32() < gives_way {
            self.nudge_affection(pusher, eater, -0.01);
            self.hand_over_bowl(eater, pusher);
        } else {
            self.engage_with(eater, pusher, SocialKind::Swat, 1);
        }
    }

    /// The cat eating steps back for one that pushed in, and goes to the back of the queue
    fn hand_over_bowl(&mut self, eater: usize, pusher: usize) {
        let Some(session) = &mut self.feeding else {
            return;
        };
        let Some(bowl_pos) = session.bowl_pos else {
            return;
        };
        if session.eater != Some(eater) {
            return;
        }
        let turn = Self::remaining(session).map_or(TURN_SECS, |r| r.min(TURN_SECS));
        session.queue.retain(|&q| q != pusher);
        session.queue.push_back(eater);
        session.eater = Some(pusher);
        let preset = session.preset;
        self.members[eater].cat.wait_for_bowl(preset, bowl_pos);
        self.members[pusher]
            .cat
            .place_bowl_for_turn(bowl_pos, Some(turn));
    }

    /// Now and then, a pushy cat waiting for the bowl (a Playful one, or a kitten that hardly
    /// knows the one eating) pushes in beside the cat eating
    fn stir_bowl(&mut self) {
        let Some(e) = self.feeding.as_ref().and_then(|s| s.eater) else {
            return;
        };
        let eating_a_while = matches!(
            self.members[e].cat.activity,
            Activity::Eating { elapsed, .. } if elapsed > 6.0
        );
        if !eating_a_while || self.members[e].partner.is_some() {
            return;
        }
        let per_tick = 1.0 - (-DIRECTOR_TICK / 40.0_f32).exp();
        for w in 0..self.members.len() {
            let m = &self.members[w];
            if w == e
                || m.partner.is_some()
                || !m.cat.is_waiting_for_bowl()
                || !Pose::of(&m.cat).is_resting()
                || self.resting_from_play(w, e)
            {
                continue;
            }
            let kin = self.kinship(w, e);
            let pushy = m.cat.behaviors.temperament == Temperament::Playful
                || (m.kitten && !kin.is_family() && self.bond(w, e).affection < 0.3)
                || (m.kitten && kin == Kinship::Mother);
            if pushy && self.rand_f32() < per_tick {
                self.engage(w, e, SocialKind::BowlPush);
                return;
            }
        }
    }

    /// Picks one cat with the laser dot (the cursor wiggled at pounce range) when several want
    /// it, the one nearest the dot; the others sit back and watch, tails twitching
    fn referee_laser(&mut self, dt: f32) {
        let n = self.members.len();
        if n < 2 {
            return;
        }
        if let Some(h) = self.hunter {
            if h < n && self.members[h].cat.hunting_cursor() {
                self.hunter_hold = LASER_HOLD;
            } else {
                self.hunter_hold -= dt;
                if h >= n || self.hunter_hold <= 0.0 {
                    self.hunter = None;
                }
            }
        }
        if self.hunter.is_none() {
            self.hunter = (0..n)
                .filter(|&i| self.members[i].cat.hunting_cursor())
                .min_by(|&a, &b| {
                    let d = |i: usize| {
                        let c = &self.members[i].cat;
                        c.nodes[0].distance(c.raw_cursor)
                    };
                    d(a).total_cmp(&d(b))
                });
            self.hunter_hold = LASER_HOLD;
        }
        for i in 0..n {
            let rival = self.hunter.is_some_and(|h| h != i);
            let cat = &mut self.members[i].cat;
            cat.laser_rival = rival;
            if rival {
                cat.yield_pounce();
            }
        }
    }

    fn nudge_affection(&mut self, i: usize, j: usize, delta: f32) {
        let mut bond = self.bond(i, j);
        bond.affection = (bond.affection + delta).clamp(-1.0, 1.0);
        let bond = bond.with_floor(self.kinship(i, j));
        self.bonds
            .set(&self.members[i].name, &self.members[j].name, bond);
    }

    fn rest_from_play(&mut self, i: usize, j: usize, secs: f32) {
        let (a, b) = (&self.members[i].name, &self.members[j].name);
        let rest = self.play_rest.get_or_insert(a, b, 0.0);
        *rest = rest.max(secs);
    }

    fn resting_from_play(&self, i: usize, j: usize) -> bool {
        self.play_rest
            .get(&self.members[i].name, &self.members[j].name)
            .is_some_and(|&t| t > 0.0)
    }

    /// How likely play between these two gets too rough: kittens (siblings most of all), and
    /// anyone with an Aloof cat
    fn escalation(&self, i: usize, j: usize) -> f32 {
        let (a, b) = (&self.members[i], &self.members[j]);
        let mut chance = if a.kitten && b.kitten { 0.3 } else { 0.12 };
        if self.kinship(i, j) == Kinship::Sibling {
            chance += 0.05;
        }
        if [a, b]
            .into_iter()
            .any(|m| m.cat.behaviors.temperament == Temperament::Aloof)
        {
            chance += 0.1;
        }
        chance
    }

    /// One of the two puffs up (a kitten, or either), and they face each other down
    fn start_standoff(&mut self, i: usize, j: usize) {
        let (a, b) = (self.members[i].kitten, self.members[j].kitten);
        let i_puffs = if a != b { a } else { self.rand_f32() < 0.5 };
        let (puffed, other) = if i_puffs { (i, j) } else { (j, i) };
        self.engage_with(puffed, other, SocialKind::Standoff, 1);
    }

    fn wrestle_turns(&mut self) -> u8 {
        3 + (self.rand_f32() * 3.0) as u8
    }

    /// Without cats starting things together, to test other mechanics on their own
    #[cfg(test)]
    pub fn without_social(mut self) -> Self {
        self.social = false;
        self
    }

    /// Whether cat `i` is free to start something with another cat: not busy, and not on its
    /// way somewhere (following the cursor, say)
    fn free(&self, i: usize) -> bool {
        let m = &self.members[i];
        let c = &m.cat;
        m.partner.is_none()
            && c.rest_target.distance(c.nodes[0]) < 40.0 * c.scale
            && c.social.is_none()
            && c.feeding_phase.is_none()
            && (!c.roaming || c.roaming_by_menu)
            && !c.sprinting
            && c.fetch_target.is_none()
            && m.carry.is_none()
            && !matches!(Pose::of(c), Pose::Busy | Pose::Racing | Pose::Carried)
    }

    /// How readily a cat of this temperament does things with other cats
    fn sociability(m: &Member) -> f32 {
        match m.cat.behaviors.temperament {
            Temperament::Playful => 1.2,
            Temperament::Balanced | Temperament::Lazy => 1.0,
            Temperament::Aloof => 0.6,
            Temperament::Realistic => 0.5,
        }
    }

    /// Every half second, looks for two free cats to do something together and maybe starts
    /// it: a greeting when they meet after time apart (sniffing a stranger, a nose boop or
    /// head bunt for a friend), grooming each other, curling up against a sleeping friend, or a
    /// kitten coming to knead and nap against its mother
    fn direct(&mut self, dt: f32) {
        let n = self.members.len();
        if n < 2 || !self.social {
            return;
        }
        for i in 0..n {
            for j in (i + 1)..n {
                let (a, b) = (&self.members[i].cat, &self.members[j].cat);
                let far = a.nodes[0].distance(b.nodes[0]) > 250.0 * a.scale.max(b.scale);
                let (na, nb) = (&self.members[i].name, &self.members[j].name);
                let apart = self.apart.get_or_insert(na, nb, GREET_AFTER);
                if far {
                    *apart += dt;
                }
            }
        }
        for rest in self.play_rest.values_mut() {
            *rest = (*rest - dt).max(0.0);
        }
        self.director_timer -= dt;
        if self.director_timer > 0.0 {
            return;
        }
        self.director_timer += DIRECTOR_TICK;
        self.stir_bowl();
        if self.playful_quirks {
            self.spread_zoomies();
            self.pounce_on_hunters_tail();
        }

        let per_tick = |rate: f32| 1.0 - (-rate * DIRECTOR_TICK).exp();
        let mut ideas = Vec::new();
        for i in 0..n {
            for j in 0..n {
                if i == j || !self.free(i) || !self.free(j) {
                    continue;
                }
                let (mi, mj) = (&self.members[i], &self.members[j]);
                let (pi, pj) = (Pose::of(&mi.cat), Pose::of(&mj.cat));
                let gap = mi.cat.nodes[0].distance(mj.cat.nodes[0]);
                let s = mi.cat.scale.max(mj.cat.scale);
                let kin = self.kinship(i, j);
                let bond = self.bond(i, j);
                let friends = bond.friends(kin);
                let warm = Self::sociability(mi) * Self::sociability(mj);
                let awake = |p: Pose| {
                    matches!(
                        p,
                        Pose::Moving | Pose::Sitting | Pose::Grooming | Pose::Lying
                    )
                };
                let resting_awake = |p: Pose| matches!(p, Pose::Sitting | Pose::Lying);

                // Greetings, asked once per pair (i < j); who leads is settled below
                let apart = self.apart.get(&mi.name, &mj.name).copied().unwrap_or(0.0);
                if i < j && apart >= GREET_AFTER && gap < 160.0 * s && awake(pi) && awake(pj) {
                    let kind = if bond.familiarity < 0.3 && !kin.is_family() {
                        SocialKind::SniffCircle
                    } else if friends {
                        SocialKind::HeadBunt
                    } else {
                        SocialKind::NoseBoop
                    };
                    ideas.push(Idea {
                        active: i,
                        passive: j,
                        kind,
                        chance: 0.4,
                        limit: 1,
                    });
                }
                // A kitten goes to knead and nap against its mother lying down
                if kin == Kinship::Mother
                    && mi.kitten
                    && awake(pi)
                    && matches!(pj, Pose::Lying | Pose::Sleeping)
                    && gap < 250.0 * s
                {
                    ideas.push(Idea {
                        active: i,
                        passive: j,
                        kind: SocialKind::Nurse,
                        chance: per_tick(1.0 / 25.0),
                        limit: 1,
                    });
                } else if friends && resting_awake(pi) && pj == Pose::Sleeping && gap < 220.0 * s {
                    let family = if kin.is_family() { 2.0 } else { 1.0 };
                    ideas.push(Idea {
                        active: i,
                        passive: j,
                        kind: SocialKind::SleepTogether,
                        // A drowsy cat soon goes to curl up against a sleeping friend: the chance
                        // only lasts until it dozes off itself
                        chance: per_tick(family * warm / 8.0),
                        limit: 1,
                    });
                }
                if friends && resting_awake(pi) && resting_awake(pj) && gap < 130.0 * s {
                    // A mother grooms her kittens far more than cats groom each other
                    let kin_mult = match kin {
                        Kinship::Kitten => 3.0,
                        Kinship::Sibling => 2.0,
                        Kinship::Mother => 0.5,
                        Kinship::None => 1.0,
                    };
                    let rate = kin_mult * warm * (0.5 + bond.affection) / 60.0;
                    ideas.push(Idea {
                        active: i,
                        passive: j,
                        kind: SocialKind::Groom,
                        chance: per_tick(rate.max(0.0)),
                        limit: 1,
                    });
                }
            }
        }
        if self.playful_quirks {
            self.play_ideas(&mut ideas);
        }
        if ideas.is_empty() {
            return;
        }
        let start = (self.rand_f32() * ideas.len() as f32) as usize % ideas.len();
        for k in 0..ideas.len() {
            let idea = &ideas[(start + k) % ideas.len()];
            if self.rand_f32() < idea.chance {
                let (active, passive, kind, limit) =
                    (idea.active, idea.passive, idea.kind, idea.limit);
                // Either cat may be the one to rub against the other
                let (active, passive) = if kind == SocialKind::HeadBunt && self.rand_f32() < 0.5 {
                    (passive, active)
                } else {
                    (active, passive)
                };
                self.engage_with(active, passive, kind, limit);
                return;
            }
        }
    }

    /// How keen a cat is to play with other cats
    fn playfulness(m: &Member) -> f32 {
        if m.kitten {
            return 1.0;
        }
        match m.cat.behaviors.temperament {
            Temperament::Playful => 0.6,
            Temperament::Balanced => 0.3,
            Temperament::Lazy => 0.12,
            Temperament::Realistic => 0.12,
            Temperament::Aloof => 0.1,
        }
    }

    /// How many pounces on its tail a grown cat puts up with from a kitten not its own
    fn tolerance(m: &Member) -> u8 {
        match m.cat.behaviors.temperament {
            Temperament::Aloof => 1,
            Temperament::Balanced | Temperament::Realistic => 2,
            Temperament::Lazy | Temperament::Playful => 3,
        }
    }

    /// Play between two free cats that aren't resting from it: pouncing on a resting cat's tail
    /// (a kitten pestering a grown cat, or answering its mother's twitching tail), and for
    /// friends and kittens a chase or a wrestle
    fn play_ideas(&mut self, ideas: &mut Vec<Idea>) {
        let n = self.members.len();
        let per_tick = |rate: f32| 1.0 - (-rate * DIRECTOR_TICK).exp();
        for i in 0..n {
            for j in 0..n {
                if i == j || !self.free(i) || !self.free(j) || self.resting_from_play(i, j) {
                    continue;
                }
                let (mi, mj) = (&self.members[i], &self.members[j]);
                let (pi, pj) = (Pose::of(&mi.cat), Pose::of(&mj.cat));
                let gap = mi.cat.nodes[0].distance(mj.cat.nodes[0]);
                let s = mi.cat.scale.max(mj.cat.scale);
                if pi == Pose::Sleeping || pj == Pose::Sleeping || gap > 200.0 * s {
                    continue;
                }
                let kin = self.kinship(i, j);
                let bond = self.bond(i, j);
                let friends = bond.friends(kin);
                let pals = friends || (mi.kitten && mj.kitten);
                let (fi, fj) = (Self::playfulness(mi), Self::playfulness(mj));
                let tolerance = Self::tolerance(mj);

                if matches!(pj, Pose::Sitting | Pose::Lying | Pose::Grooming) {
                    let (rate, limit) = if kin == Kinship::Mother && mi.kitten {
                        // She twitches her tail for it, and puts up with it a long while
                        (1.0 / 20.0, 3 + (self.rand_f32() * 3.0) as u8)
                    } else if mi.kitten && !mj.kitten {
                        (fi * (0.6 + bond.affection).max(0.2) / 25.0, tolerance)
                    } else if pals {
                        (fi * fj / 60.0, 1 + (self.rand_f32() * 2.0) as u8)
                    } else {
                        (0.0, 0)
                    };
                    if rate > 0.0 {
                        ideas.push(Idea {
                            active: i,
                            passive: j,
                            kind: SocialKind::TailStalk,
                            chance: per_tick(rate),
                            limit,
                        });
                    }
                }
                if i < j && pals {
                    let family = if kin == Kinship::Sibling { 2.0 } else { 1.0 };
                    let (chaser, runner) = if self.rand_f32() < 0.5 {
                        (i, j)
                    } else {
                        (j, i)
                    };
                    ideas.push(Idea {
                        active: chaser,
                        passive: runner,
                        kind: SocialKind::Chase,
                        chance: per_tick(fi * fj * family / 35.0),
                        limit: 2,
                    });
                    if gap < 130.0 * s {
                        let turns = self.wrestle_turns();
                        ideas.push(Idea {
                            active: i,
                            passive: j,
                            kind: SocialKind::Wrestle,
                            chance: per_tick(fi * fj * family / 45.0),
                            limit: turns,
                        });
                    }
                }
            }
        }
    }

    /// A Playful friend or a kitten near a cat that just got the zoomies joins in
    fn spread_zoomies(&mut self) {
        let n = self.members.len();
        for i in 0..n {
            let fresh = matches!(
                self.members[i].cat.activity,
                Activity::Zoomies { timer, .. } if timer < DIRECTOR_TICK
            );
            if !fresh {
                continue;
            }
            for j in 0..n {
                if j == i || !self.free(j) {
                    continue;
                }
                let m = &self.members[j];
                let keen = m.kitten || m.cat.behaviors.temperament == Temperament::Playful;
                let bond = self.bond(i, j);
                let friends = bond.friends(self.kinship(i, j));
                let (a, b) = (&self.members[i].cat, &m.cat);
                let near = a.nodes[0].distance(b.nodes[0]) < 220.0 * a.scale.max(b.scale);
                if keen && friends && near && Pose::of(b) != Pose::Sleeping && self.rand_f32() < 0.6
                {
                    self.members[j].cat.join_zoomies();
                }
            }
        }
    }

    /// A kitten or Playful cat that lost the laser dot to another cat sometimes pounces on that
    /// cat's tail instead
    fn pounce_on_hunters_tail(&mut self) {
        let Some(h) = self
            .hunter
            .filter(|&h| self.members[h].cat.hunting_cursor())
        else {
            return;
        };
        let tip = self.members[h].cat.nodes[TOTAL_NODES - 1];
        for i in 0..self.members.len() {
            let m = &self.members[i];
            let keen = m.kitten || m.cat.behaviors.temperament == Temperament::Playful;
            if i == h
                || !keen
                || !self.free(i)
                || m.cat.nodes[0].distance(tip) > 180.0 * m.cat.scale
            {
                continue;
            }
            if self.rand_f32() < 0.1 {
                self.members[i].cat.begin_social(
                    SocialKind::TailStalk,
                    SocialRole::Active,
                    Self::neighbor_index(i, h),
                    1.0,
                    1,
                );
                self.members[i].partner = Some(h);
                return;
            }
        }
    }

    /// Starts `kind` with `active` going over to `passive` (for a nose boop or sniff, both go)
    fn engage(&mut self, active: usize, passive: usize, kind: SocialKind) {
        self.engage_with(active, passive, kind, 1);
    }

    /// `engage`, with `limit` rounds (pounces, wrestling turns, or for a chase the times the two
    /// may swap round)
    fn engage_with(&mut self, active: usize, passive: usize, kind: SocialKind, limit: u8) {
        let symmetric = matches!(kind, SocialKind::NoseBoop | SocialKind::SniffCircle);
        let side = if matches!(
            kind,
            SocialKind::SniffCircle | SocialKind::Chase | SocialKind::Standoff
        ) {
            if self.rand_f32() < 0.5 { 1.0 } else { -1.0 }
        } else {
            // Come up on the side of the other cat it's already on
            let p = &self.members[passive].cat;
            let heading = (p.nodes[0] - p.body_point()).normalize_or(Vec2::X);
            let rel = self.members[active].cat.nodes[0] - p.body_point();
            if heading.perp().dot(rel) >= 0.0 {
                1.0
            } else {
                -1.0
            }
        };
        let passive_role = if symmetric {
            SocialRole::Active
        } else {
            SocialRole::Passive
        };
        self.members[active].cat.begin_social(
            kind,
            SocialRole::Active,
            Self::neighbor_index(active, passive),
            side,
            limit,
        );
        self.members[passive].cat.begin_social(
            kind,
            passive_role,
            Self::neighbor_index(passive, active),
            side,
            limit,
        );
        self.members[active].partner = Some(passive);
        self.members[passive].partner = Some(active);
        if matches!(
            kind,
            SocialKind::NoseBoop | SocialKind::HeadBunt | SocialKind::SniffCircle
        ) {
            let (a, b) = (&self.members[active].name, &self.members[passive].name);
            self.apart.insert(a, b, 0.0);
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
            if want.is_none() && Pose::of(cat) != Pose::Carried {
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
        self.retime_turn();
    }

    /// How long a turn at the bowl lasts: capped at `TURN_SECS` when cats share it, otherwise
    /// what's left of the preset
    fn turn_secs(&self, session: &FeedSession) -> Option<f32> {
        let remaining = Self::remaining(session);
        if self.members.len() > 1 {
            Some(remaining.map_or(TURN_SECS, |r| r.min(TURN_SECS)))
        } else {
            remaining
        }
    }

    /// Fits the current eater's turn to the cats there are now: a lone eater gets the rest of
    /// the preset, and one that has company gets at most a shared turn from now
    fn retime_turn(&mut self) {
        let Some(session) = &self.feeding else {
            return;
        };
        let Some(e) = session.eater else {
            return;
        };
        let turn = self.turn_secs(session);
        self.members[e].cat.retime_eat_turn(turn);
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
        let Some(turn) = self.feeding.as_ref().map(|s| self.turn_secs(s)) else {
            return;
        };
        let Some(session) = &mut self.feeding else {
            return;
        };
        let (Some(bowl_pos), None) = (session.bowl_pos, session.eater) else {
            return;
        };
        let Some(next) = session.queue.pop_front() else {
            return;
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
    use crate::cat::{Activity, TriggerKind};
    use crate::coat::CoatStyle;
    use crate::render::cat_window_dim;

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
        let mut group = clowder(&profiles, start).without_social();
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
        )
        .without_social();
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

    /// Seconds left in the eating cat's turn
    fn turn_left(group: &Clowder) -> Option<f32> {
        let e = group.feeding.as_ref()?.eater?;
        match group.cat(e).activity {
            Activity::Eating {
                elapsed,
                duration: Some(d),
                ..
            } => Some(d - elapsed),
            _ => None,
        }
    }

    #[test]
    fn turns_at_the_bowl_change_as_cats_join_and_leave() {
        let start = Vec2::new(600.0, 400.0);
        let bowl = Vec2::new(700.0, 420.0);
        let mut group = clowder(&[profile("Luna"), profile("Milo")], start);
        group.begin_bowl_placement(FeedPreset::Mins30, bowl);
        group.place_bowl(bowl);
        run(&mut group, bowl, 20.0);
        let left = turn_left(&group).expect("someone eats");
        assert!(left <= TURN_SECS, "a shared turn: {left}");

        // With the other cat gone, the one eating has the rest of the preset
        let eater = group.feeding.as_ref().and_then(|s| s.eater).unwrap();
        assert!(group.remove(1 - eater));
        let left = turn_left(&group).unwrap();
        assert!(left > 1700.0, "the lone cat's turn is still capped: {left}");
        run(&mut group, bowl, 200.0);
        assert!(
            group.cat(0).is_feeding(),
            "feeding stopped after a shared turn"
        );

        // A newcomer shares again: the eater's turn shrinks, and the newcomer gets one
        let pip = group.add_cat().unwrap();
        assert!(turn_left(&group).unwrap() < TURN_SECS + 0.01);
        let mut newcomer_ate = false;
        for _ in 0..(200.0 / DT) as usize {
            group.update(bowl, DT);
            newcomer_ate |= group.cat(pip).is_feeding();
        }
        assert!(newcomer_ate, "the new cat never got a turn");
    }

    #[test]
    fn a_cat_joining_an_unlimited_meal_gets_a_turn() {
        let start = Vec2::new(600.0, 400.0);
        let bowl = Vec2::new(700.0, 420.0);
        let mut group = clowder(&[profile("Luna")], start);
        group.begin_bowl_placement(FeedPreset::Indefinite, bowl);
        group.place_bowl(bowl);
        run(&mut group, bowl, 10.0);
        assert_eq!(
            turn_left(&group),
            None,
            "a lone cat eats as long as it likes"
        );
        let kitten = group.add_kitten(0).unwrap();
        assert!(turn_left(&group).is_some_and(|t| t < TURN_SECS + 0.01));
        let mut kitten_ate = false;
        for _ in 0..(200.0 / DT) as usize {
            group.update(bowl, DT);
            kitten_ate |= group.cat(kitten).is_feeding();
        }
        assert!(kitten_ate, "the kitten never got a turn");
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
    fn a_new_cat_with_a_removed_cats_name_is_a_stranger() {
        let start = Vec2::new(600.0, 400.0);
        let mut group = clowder(&[profile("Luna"), profile("Milo")], start);
        let close = Bond {
            familiarity: 0.9,
            affection: 0.8,
        };
        group.set_bond(0, 1, close);
        assert!(group.remove(1));
        let newcomer = group.add_cat().unwrap();
        assert_eq!(group.members()[newcomer].name, "Milo");
        assert!(group.bond(0, newcomer).familiarity < 0.1);
    }

    #[test]
    fn a_duplicate_name_is_not_renamed_onto_a_later_cat() {
        let profiles = [
            profile("Luna"),
            profile("Luna"),
            profile("Milo"),
            kitten_of("Pip", "Milo"),
        ];
        let group = clowder(&profiles, Vec2::new(600.0, 400.0));
        let names: Vec<_> = group.members().iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names[0], "Luna");
        assert!(!["Luna", "Milo", "Pip"].contains(&names[1]), "{names:?}");
        assert_eq!(names[2..], ["Milo", "Pip"]);
        assert_eq!(group.members()[3].mother.as_deref(), Some("Milo"));
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
        assert_eq!(
            loaded[5].mother.as_deref(),
            Some("Luna"),
            "a kitten that grew up keeps its mother"
        );
        let kittens = [kitten_of("Nori", "Luna"), kitten_of("Olive", "Nori")];
        let group = clowder(&kittens, Vec2::new(600.0, 400.0));
        assert_eq!(group.profiles()[1].mother, None, "kittens have no kittens");

        // Different cats make different choices
        let mut group = clowder(&[profile("Luna"), profile("Milo")], Vec2::new(600.0, 400.0));
        assert_ne!(group.cat_mut(0).rand_u32(), group.cat_mut(1).rand_u32());
    }

    #[test]
    fn a_kitten_grows_up_over_the_days_and_stays_its_mothers() {
        let day = 86_400;
        let born = 1_761_500_000;
        let profiles = [
            profile("Luna"),
            CatProfile {
                grows_up: true,
                ..kitten_of("Pip", "Luna")
            },
            kitten_of("Nori", "Luna"),
        ];
        let mut group = clowder(&profiles, Vec2::new(600.0, 400.0));
        // Born the first time it grows: that needs saving
        assert!(group.grow(born));
        assert_eq!(group.profiles()[1].born, Some(born));
        assert!((group.cat(1).scale - KITTEN_SIZE).abs() < 1e-3);
        // Halfway: halfway to grown size, half its kitten ways left
        let half = born + (GROW_UP_DAYS as u64 / 2) * day;
        assert!(!group.grow(half));
        let mid = 0.5 * (KITTEN_SIZE + 1.0);
        assert!(
            (group.cat(1).scale - mid).abs() < 0.01,
            "{}",
            group.cat(1).scale
        );
        assert!((group.cat(1).youth - 0.5).abs() < 0.01);
        assert_eq!(group.members()[1].days_to_grow(), Some(15));
        // A kitten that doesn't grow up stays one
        assert!((group.cat(2).scale - KITTEN_SIZE).abs() < 1e-3);
        // Grown: a grown cat now, and still its mother's
        assert!(group.grow(born + GROW_UP_DAYS as u64 * day));
        let pip = &group.profiles()[1];
        assert!(!pip.kitten && !pip.grows_up && pip.born.is_none());
        assert!((pip.size - 1.0).abs() < 1e-3 && (group.cat(1).scale - 1.0).abs() < 1e-3);
        assert_eq!(group.kinship(1, 0), Kinship::Mother);
        assert!(group.bond(0, 1).affection >= 0.8);
        assert!(!group.grow(born + 40 * GROW_UP_DAYS as u64 * day));

        // Stopping a kitten growing keeps the size it reached
        let mut group = clowder(&profiles, Vec2::new(600.0, 400.0));
        group.grow(born);
        group.grow(half);
        group.set_grows_up(1, false);
        group.grow(half + 10 * day);
        assert!((group.cat(1).scale - mid).abs() < 0.01);
        assert!(group.profiles()[1].kitten);
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
            widest < leash * 1.5,
            "the kitten got {widest} px from its mother"
        );
    }

    #[test]
    fn a_mother_carries_her_kitten_asleep_on_its_own_back_by_the_scruff() {
        let start = Vec2::new(700.0, 600.0);
        let mut group = clowder(&[profile("Luna"), kitten_of("Pip", "Luna")], start);
        run(&mut group, start, 8.0);
        group.cat_mut(1).trigger(TriggerKind::Sleep);
        // The cursor goes off; the mother follows it and the kitten sleeps on where it was
        let cursor = start + Vec2::new(450.0, 0.0);
        let (mut carried, mut worst_hold, mut worst_reach) = (0.0f32, 0.0f32, 0.0f32);
        let half_window = f32::from(cat_window_dim(group.cat(0).scale)) * 0.5;
        for _ in 0..(90.0 / DT) as usize {
            group.update(cursor, DT);
            let (mother, kitten) = (group.cat(0), group.cat(1));
            if Pose::of(kitten) != Pose::Carried {
                if carried > 0.0 {
                    break;
                }
                continue;
            }
            carried += DT;
            assert!(mother.carrying);
            if carried > 0.5 {
                assert_eq!(
                    group.drawn_in(1).count(),
                    0,
                    "a carried kitten in its own window"
                );
                let drawn: Vec<_> = group.drawn_in(0).map(|c| c.nodes[0]).collect();
                assert_eq!(
                    drawn,
                    [kitten.nodes[0], mother.nodes[0]],
                    "she's drawn over it"
                );
                worst_hold = worst_hold.max(kitten.nodes[1].distance(mother.mouth().0));
                for node in kitten.nodes {
                    let off = (node - mother.window_center()).abs();
                    worst_reach = worst_reach.max(off.x.max(off.y));
                }
            }
        }
        assert!(
            carried > 1.0,
            "the mother never carried her kitten ({carried} s)"
        );
        assert!(
            worst_hold < 2.0,
            "the scruff slipped {worst_hold} px from her mouth"
        );
        assert!(
            worst_reach < half_window - 6.0,
            "the kitten hung {worst_reach} px out, outside her {half_window} px window half"
        );
        // Put down still asleep by its mother's spot near the cursor, and nobody's carrying
        let (mother, kitten) = (group.cat(0), group.cat(1));
        assert_eq!(kitten.activity, Activity::Sleeping);
        assert!(!mother.carrying);
        assert!(group.members[0].carry.is_none());
        assert!(mother.nodes[0].distance(cursor) < 120.0);
        assert!(kitten.nodes[0].distance(mother.body_point()) < CatSim::KITTEN_LEASH * 0.5);
        assert_eq!(group.drawn_in(1).count(), 1);
        // Woken, it goes on as usual
        run(&mut group, cursor, 5.0);
        assert!(group.cat(1).nodes[0].is_finite());
    }

    #[test]
    fn a_carried_kitten_is_put_down_when_its_mother_is_called_away() {
        let start = Vec2::new(700.0, 600.0);
        let mut group = clowder(&[profile("Luna"), kitten_of("Pip", "Luna")], start);
        run(&mut group, start, 8.0);
        run(&mut group, start + Vec2::new(300.0, 0.0), 1.5);
        group.pick_up(0, 1);
        run(&mut group, start + Vec2::new(300.0, 0.0), 1.0);
        assert_eq!(Pose::of(group.cat(1)), Pose::Carried);
        // Shaken off, she drops it and goes off on her own; it sits up where it was let go
        group.cat_mut(0).shoo();
        group.update(start, DT);
        assert!(!group.cat(0).carrying);
        assert_eq!(group.cat(1).activity, Activity::Sitting);
        // Removing a mother mid-carry puts the kitten down too
        group.pick_up(0, 1);
        assert!(group.remove(0));
        assert_ne!(Pose::of(group.cat(0)), Pose::Carried);
        run(&mut group, start, 2.0);
    }

    #[test]
    fn a_mother_fetches_her_kitten_that_wandered_off() {
        let start = Vec2::new(900.0, 600.0);
        let mut group = clowder(&[profile("Luna"), kitten_of("Pip", "Luna")], start);
        run(&mut group, start, 8.0);
        // Shaken off, the kitten stays behind roaming while its mother follows the cursor away
        group.cat_mut(1).shoo();
        let cursor = start + Vec2::new(400.0, 0.0);
        let mut fetched = false;
        for _ in 0..(120.0 / DT) as usize {
            group.update(cursor, DT);
            fetched |= group.cat(0).fetch_target.is_some();
            if fetched && !group.cat(1).roaming {
                break;
            }
        }
        assert!(fetched, "the mother never went for her kitten");
        assert!(!group.cat(1).roaming, "the kitten is still off on its own");
        // Both come back to the cursor, the kitten at its mother's side
        run(&mut group, cursor, 20.0);
        let (mother, kitten) = (group.cat(0), group.cat(1));
        assert!(mother.fetch_target.is_none());
        assert!(mother.nodes[0].distance(cursor) < 100.0);
        assert!(kitten.nodes[0].distance(mother.body_point()) < CatSim::KITTEN_LEASH);
    }

    #[test]
    fn cats_get_to_know_each_other_and_friends_sit_closer() {
        let start = Vec2::new(600.0, 400.0);
        let group = clowder(
            &[profile("Luna"), profile("Milo"), kitten_of("Pip", "Luna")],
            start,
        );
        let family = group.bond(0, 2);
        assert!(family.familiarity >= 0.9 && family.affection >= 0.8);
        // Two grown cats (a kitten would draw its mother off to sleep with it, wherever it is)
        let mut group = clowder(&[profile("Luna"), profile("Milo")], start);
        let first = group.bond(0, 1);
        assert_eq!(first.familiarity, 0.0);
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
            let mut group = clowder(&[profile("Luna"), profile("Milo")], start).without_social();
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

    /// Runs the clowder and lists what each pair started doing together (kind, active cat,
    /// passive cat), in order, plus the frames each cat spent in each social activity
    fn social_log(group: &mut Clowder, cursor: Vec2, secs: f32) -> Vec<(SocialKind, usize, usize)> {
        let mut log = Vec::new();
        let mut was: Vec<Option<SocialKind>> = vec![None; group.len()];
        for _ in 0..(secs / DT) as usize {
            group.update(cursor, DT);
            for (i, was) in was.iter_mut().enumerate() {
                let now = group.cat(i).social.map(|e| e.kind);
                if let (Some(kind), None) = (now, *was)
                    && group
                        .cat(i)
                        .social
                        .is_some_and(|e| e.role == SocialRole::Active)
                    && let Some(j) = group.members[i].partner
                    && !log
                        .iter()
                        .rev()
                        .take(1)
                        .any(|&(k, a, b)| k == kind && ((a, b) == (j, i) || (a, b) == (i, j)))
                {
                    log.push((kind, i, j));
                }
                *was = now;
            }
        }
        log
    }

    fn pair(bond: Option<Bond>, temperament: Temperament) -> Clowder {
        let profiles = [
            CatProfile {
                temperament,
                ..profile("Luna")
            },
            CatProfile {
                temperament,
                ..profile("Milo")
            },
        ];
        let mut group = clowder(&profiles, Vec2::new(600.0, 400.0));
        if let Some(bond) = bond {
            group.set_bond(0, 1, bond);
        }
        group
    }

    #[test]
    fn cats_greet_each_other_as_well_as_they_know_each_other() {
        let cursor = Vec2::new(600.0, 400.0);
        let first_greeting = |bond: Option<Bond>| {
            let mut group = pair(bond, Temperament::Balanced);
            let log = social_log(&mut group, cursor, 15.0);
            // Once is enough while they stay together
            let greetings = log
                .iter()
                .filter(|(k, ..)| {
                    matches!(
                        k,
                        SocialKind::NoseBoop | SocialKind::HeadBunt | SocialKind::SniffCircle
                    )
                })
                .count();
            assert_eq!(greetings, 1, "{log:?}");
            assert!(group.cats().all(|c| c.social.is_none()), "{log:?}");
            log[0].0
        };
        assert_eq!(first_greeting(None), SocialKind::SniffCircle);
        let acquainted = Bond {
            familiarity: 0.6,
            affection: 0.1,
        };
        assert_eq!(first_greeting(Some(acquainted)), SocialKind::NoseBoop);
        let friends = Bond {
            familiarity: 0.9,
            affection: 0.7,
        };
        assert_eq!(first_greeting(Some(friends)), SocialKind::HeadBunt);
    }

    #[test]
    fn a_greeting_brings_noses_together_and_circles_round() {
        let cursor = Vec2::new(600.0, 400.0);
        let closest = |bond: Option<Bond>| {
            let mut group = pair(bond, Temperament::Balanced);
            let mut closest = f32::MAX;
            let mut turned = 0.0f32;
            for _ in 0..(15.0 / DT) as usize {
                group.update(cursor, DT);
                if let Activity::Greeting { .. } = group.cat(0).activity {
                    closest = closest.min(group.cat(0).nodes[0].distance(group.cat(1).nodes[0]));
                }
                if let Activity::Sniffing { turned: t, .. } = group.cat(0).activity {
                    turned = turned.max(t);
                }
            }
            (closest, turned)
        };
        let (nose_to_nose, _) = closest(Some(Bond {
            familiarity: 0.6,
            affection: 0.1,
        }));
        assert!(nose_to_nose < 19.0, "heads {nose_to_nose} px apart");
        let (_, turned) = closest(None);
        assert!(turned > 3.0, "sniffed round only {turned} rad");
    }

    #[test]
    fn friends_resting_together_groom_and_sleep_against_each_other() {
        let cursor = Vec2::new(600.0, 400.0);
        let friends = Bond {
            familiarity: 0.9,
            affection: 0.8,
        };
        // Over 20 minutes the cursor moves now and then, so the cats get up and settle again
        let snuggles = |temperament| {
            let mut group = pair(Some(friends), temperament);
            let (mut snuggled, mut groomed_bliss) = (f32::MAX, false);
            let mut kinds = Vec::new();
            for frame in 0..(1200.0 / DT) as usize {
                let shift = (frame as f32 * DT / 45.0) as usize % 2;
                group.update(cursor + Vec2::new(160.0 * shift as f32, 0.0), DT);
                for i in 0..2 {
                    let cat = group.cat(i);
                    let Some(e) = cat.social.filter(|e| e.role == SocialRole::Active) else {
                        continue;
                    };
                    if kinds.last() != Some(&e.kind) {
                        kinds.push(e.kind);
                    }
                    groomed_bliss |= group.cat(1 - i).blissful();
                    if e.kind == SocialKind::SleepTogether && e.acting {
                        let other = group.cat(1 - i).body_point();
                        snuggled = snuggled.min(cat.body_point().distance(other));
                    }
                }
            }
            (kinds, snuggled, groomed_bliss)
        };
        let (kinds, snuggled, _) = snuggles(Temperament::Lazy);
        assert!(kinds.contains(&SocialKind::SleepTogether), "{kinds:?}");
        assert!(snuggled < 30.0, "slept {snuggled} px from its friend");
        let (kinds, _, groomed_bliss) = snuggles(Temperament::Balanced);
        assert!(kinds.contains(&SocialKind::Groom), "{kinds:?}");
        assert!(groomed_bliss, "the groomed cat never shut its eyes");
    }

    #[test]
    fn a_kitten_kneads_and_naps_against_its_mother() {
        let cursor = Vec2::new(600.0, 400.0);
        let profiles = [
            CatProfile {
                temperament: Temperament::Lazy,
                ..profile("Luna")
            },
            kitten_of("Pip", "Luna"),
        ];
        let mut group = clowder(&profiles, cursor);
        let (mut kneaded, mut napped) = (false, f32::MAX);
        for _ in 0..(300.0 / DT) as usize {
            group.update(cursor, DT);
            let (mother, kitten) = (group.cat(0), group.cat(1));
            kneaded |= matches!(kitten.activity, Activity::Kneading { .. });
            if kitten.activity == Activity::Sleeping
                && kitten.social.is_some_and(|e| e.kind == SocialKind::Nurse)
            {
                napped = napped.min(kitten.nodes[0].distance(mother.body_point()));
            }
        }
        assert!(kneaded, "the kitten never kneaded");
        assert!(napped < 25.0, "napped {napped} px from its mother");
    }

    #[test]
    fn moving_the_cursor_away_breaks_it_up() {
        let cursor = Vec2::new(600.0, 400.0);
        let mut group = pair(None, Temperament::Balanced);
        let mut engaged = false;
        for _ in 0..(15.0 / DT) as usize {
            group.update(cursor, DT);
            if group.cats().all(|c| c.social.is_some_and(|e| e.acting)) {
                engaged = true;
                break;
            }
        }
        assert!(engaged, "they never got round to sniffing each other");
        let away = cursor + Vec2::new(500.0, 0.0);
        run(&mut group, away, 1.0);
        assert!(group.cats().all(|c| c.social.is_none()));
        run(&mut group, away, 12.0);
        for cat in group.cats() {
            assert!(cat.nodes[0].distance(away) < 120.0);
        }
    }

    const FRIENDS: Bond = Bond {
        familiarity: 0.8,
        affection: 0.6,
    };

    /// Where someone at the computer leaves the cursor `secs` in: here, then a little to the
    /// side, and back, every 45 s (getting the cats up each time)
    fn shifting_cursor(home: Vec2, secs: f32) -> Vec2 {
        if ((secs / 45.0) as usize).is_multiple_of(2) {
            home
        } else {
            home + Vec2::new(160.0, 0.0)
        }
    }

    /// A grown cat and a kitten that isn't hers
    fn kitten_and_stranger(temperament: Temperament) -> Clowder {
        let profiles = [
            CatProfile {
                temperament,
                ..profile("Luna")
            },
            CatProfile {
                size: KITTEN_SIZE,
                kitten: true,
                ..profile("Pip")
            },
        ];
        clowder(&profiles, Vec2::new(600.0, 400.0))
    }

    #[test]
    fn a_kitten_pounces_on_a_grown_cats_tail_until_it_has_had_enough() {
        let cursor = Vec2::new(600.0, 400.0);
        let mut group = kitten_and_stranger(Temperament::Balanced);
        let mut affection = group.bond(0, 1).affection;
        let mut cooled = false;
        let (mut landings, mut worst_miss) = (0, 0.0_f32);
        let (mut stalked, mut swatted, mut walked_off) = (false, false, false);
        let mut was_pouncing = false;
        for k in 0..(600.0 / DT) as usize {
            group.update(shifting_cursor(cursor, k as f32 * DT), DT);
            let (luna, pip) = (group.cat(0), group.cat(1));
            let stalking = pip
                .social
                .is_some_and(|e| e.kind == SocialKind::TailStalk && e.role == SocialRole::Active);
            stalked |= stalking;
            let pouncing = matches!(pip.activity, Activity::Pouncing { .. });
            if stalking && was_pouncing && !pouncing {
                landings += 1;
                let tip = luna.nodes[TOTAL_NODES - 1];
                worst_miss = worst_miss.max(pip.nodes[0].distance(tip));
            }
            was_pouncing = pouncing && stalking;
            swatted |= luna
                .social
                .is_some_and(|e| e.kind == SocialKind::Swat && e.acting)
                && matches!(luna.activity, Activity::PawBatting { .. });
            walked_off |= luna.retreat.is_some();
            // Affection otherwise only drifts, far more slowly than this
            let now = group.bond(0, 1).affection;
            cooled |= now < affection - 0.015;
            affection = now;
            if swatted && walked_off {
                break;
            }
        }
        assert!(stalked && landings >= 2, "{landings} pounces on the tail");
        assert!(
            worst_miss < 16.0,
            "landed {worst_miss} px from the tail tip"
        );
        assert!(swatted || walked_off, "Luna put up with it forever");
        assert!(cooled, "pestering cost nothing");
    }

    #[test]
    fn a_mother_twitches_her_tail_for_her_kitten_and_ends_with_a_boop() {
        let cursor = Vec2::new(600.0, 400.0);
        let profiles = [profile("Luna"), kitten_of("Pip", "Luna")];
        let mut group = clowder(&profiles, cursor);
        let (mut lured, mut pounces, mut booped) = (0.0_f32, 0, false);
        let mut was_stalking = false;
        for k in 0..(600.0 / DT) as usize {
            group.update(shifting_cursor(cursor, k as f32 * DT), DT);
            let (luna, pip) = (group.cat(0), group.cat(1));
            let stalking = pip.social.is_some_and(|e| e.kind == SocialKind::TailStalk);
            if stalking && luna.social.is_some_and(|e| e.acting) {
                lured = lured.max(luna.tail_lash);
                if let Some(e) = pip.social {
                    pounces = pounces.max(e.rounds);
                }
            }
            if was_stalking && !stalking {
                booped = pip.social.is_some_and(|e| e.kind == SocialKind::NoseBoop);
                if booped {
                    break;
                }
            }
            was_stalking = stalking;
        }
        assert!(lured > 0.8, "her tail hardly twitched ({lured})");
        assert!(pounces >= 3, "she put up with only {pounces} pounces");
        assert!(booped, "it didn't end with a boop");
    }

    #[test]
    fn only_one_cat_gets_the_laser_dot_and_the_other_twitches_its_tail() {
        let cursor = Vec2::new(600.0, 400.0);
        let mut group = pair(Some(FRIENDS), Temperament::Playful).without_social();
        run(&mut group, cursor, 12.0);
        // Dance the dot round in front of them
        let dot = cursor + Vec2::new(0.0, 150.0);
        let (mut hunted, mut both, mut lash) = ([false; 2], 0, 0.0_f32);
        for k in 0..(25.0 / DT) as usize {
            let jiggle = if k % 2 == 0 { -18.0 } else { 18.0 };
            let drift = Vec2::new(40.0 * (k as f32 * DT * 0.7).sin(), jiggle);
            group.update(dot + drift, DT);
            let hunting: Vec<bool> = group.cats().map(CatSim::hunting_cursor).collect();
            for (h, now) in hunted.iter_mut().zip(&hunting) {
                *h |= now;
            }
            if hunting.iter().all(|&h| h) {
                both += 1;
            }
            for c in group.cats().filter(|c| c.laser_rival) {
                lash = lash.max(c.tail_lash);
            }
        }
        assert!(hunted.iter().any(|&h| h), "neither went for the dot");
        // Two can crouch in the same frame; the second gives way on the next
        assert!(both <= 2, "both hunted the dot for {both} frames");
        assert!(
            lash > 0.5,
            "the one left out didn't twitch its tail ({lash})"
        );
    }

    #[test]
    fn kittens_chase_each_other_and_wrestle() {
        let cursor = Vec2::new(600.0, 400.0);
        let profiles = [
            profile("Luna"),
            kitten_of("Pip", "Luna"),
            kitten_of("Nori", "Luna"),
        ];
        let mut group = clowder(&profiles, cursor);
        let (mut raced, mut caught, mut wrestled, mut farthest) = (false, false, false, 0.0_f32);
        for k in 0..(600.0 / DT) as usize {
            let at = shifting_cursor(cursor, k as f32 * DT);
            group.update(at, DT);
            let (pip, nori) = (group.cat(1), group.cat(2));
            let chase = |c: &CatSim| c.social.is_some_and(|e| e.kind == SocialKind::Chase);
            if chase(pip) && chase(nori) {
                raced |= Pose::of(pip) == Pose::Racing && Pose::of(nori) == Pose::Racing;
                caught |= [pip, nori]
                    .into_iter()
                    .any(|c| c.social.is_some_and(|e| e.rounds > 0));
                for c in [pip, nori] {
                    farthest = farthest.max(c.nodes[0].distance(at));
                }
            }
            let rolling = |c: &CatSim| {
                c.social.is_some_and(|e| e.kind == SocialKind::Wrestle)
                    && matches!(c.activity, Activity::BellyUp { .. })
            };
            let batting = |c: &CatSim| {
                c.social.is_some_and(|e| e.kind == SocialKind::Wrestle)
                    && matches!(c.activity, Activity::PawBatting { .. })
            };
            wrestled |= (rolling(pip) && batting(nori)) || (rolling(nori) && batting(pip));
            if raced && caught && wrestled {
                break;
            }
        }
        assert!(raced, "they never galloped after each other");
        assert!(caught, "the chaser never caught up");
        assert!(
            farthest < 200.0,
            "the chase ran {farthest} px from the cursor"
        );
        assert!(wrestled, "they never wrestled");
    }

    #[test]
    fn only_cats_following_the_cursor_are_pulled_apart_by_it() {
        let cursor = Vec2::new(600.0, 400.0);
        let away = cursor + Vec2::new(400.0, 250.0);
        for follow in [true, false] {
            let mut group = pair(Some(FRIENDS), Temperament::Balanced).without_social();
            run(&mut group, cursor, 12.0);
            group.set_follow_cursor(follow);
            group.engage(0, 1, SocialKind::Groom);
            run(&mut group, cursor, 0.5);
            assert!(group.cat(0).social.is_some());
            run(&mut group, away, 1.0);
            assert_eq!(
                group.cat(0).social.is_some(),
                !follow,
                "follow cursor {follow}: grooming went on {}",
                group.cat(0).social.is_some()
            );
        }
    }

    #[test]
    fn a_squabble_puffs_one_up_then_both_back_off_and_groom() {
        let cursor = Vec2::new(600.0, 400.0);
        let mut group = pair(Some(FRIENDS), Temperament::Playful).without_social();
        run(&mut group, cursor, 12.0);
        let start = group.bond(0, 1).affection;
        let gap_before = group.cat(0).nodes[0].distance(group.cat(1).nodes[0]);
        group.start_standoff(0, 1);
        let (mut puff, mut ears, mut tail) = (0.0_f32, 0.0_f32, 0.0_f32);
        let mut groomed = [false; 2];
        let mut gap_after = 0.0_f32;
        for _ in 0..(15.0 / DT) as usize {
            group.update(cursor, DT);
            for c in group.cats() {
                puff = puff.max(c.bristle);
                ears = ears.min(c.ear_left_twitch);
                tail = tail.max(c.tail_puff);
            }
            for (g, c) in groomed.iter_mut().zip(group.cats()) {
                *g |= matches!(c.activity, Activity::Grooming { .. });
            }
            gap_after = gap_after.max(group.cat(0).nodes[0].distance(group.cat(1).nodes[0]));
        }
        assert!(puff > 0.9 && tail > 1.6, "puffed up {puff}, tail {tail}");
        assert!(ears < -0.4, "ears only went to {ears}");
        assert!(
            gap_after > gap_before + 40.0,
            "only {gap_before} -> {gap_after} px apart"
        );
        assert_eq!(groomed, [true; 2], "both groom it off");
        assert!(group.bond(0, 1).affection < start - 0.04);
        assert!(
            group
                .cats()
                .all(|c| c.social.is_none() && c.retreat.is_none())
        );
    }

    #[test]
    fn zoomies_spread_to_a_playful_friend_but_not_a_stranger() {
        let cursor = Vec2::new(600.0, 400.0);
        let joins = |bond: Bond| {
            let mut group = pair(Some(bond), Temperament::Playful);
            run(&mut group, cursor, 12.0);
            let mut joined = 0;
            for _ in 0..6 {
                let center = group.cat(0).nodes[2];
                group.members[0].cat.activity = Activity::Zoomies {
                    center,
                    dir: 1.0,
                    timer: 0.0,
                    duration: 1.8,
                };
                let mut zoomed = false;
                // Joining in, not a fit of its own later on
                for _ in 0..(1.0 / DT) as usize {
                    group.update(cursor, DT);
                    zoomed |= matches!(group.cat(1).activity, Activity::Zoomies { .. });
                }
                joined += usize::from(zoomed);
                run(&mut group, cursor, 12.0);
            }
            joined
        };
        assert!(joins(FRIENDS) >= 2, "a friend never joined in");
        let stranger = Bond {
            familiarity: 0.0,
            affection: 0.0,
        };
        assert_eq!(joins(stranger), 0);
    }

    #[test]
    fn a_pushy_cat_pushes_in_at_the_bowl() {
        let profiles = [
            profile("Luna"),
            CatProfile {
                temperament: Temperament::Playful,
                ..profile("Milo")
            },
        ];
        let bowl = Vec2::new(600.0, 400.0);
        let mut group = clowder(&profiles, bowl - Vec2::new(0.0, 60.0));
        group.begin_bowl_placement(FeedPreset::Indefinite, bowl);
        group.place_bowl(bowl);
        let (mut pushed, mut outcome) = (false, None);
        for frame in 0..(400.0 / DT) as usize {
            // The cursor goes on with its work: the cats at the bowl don't mind it
            let t = frame as f32 * DT;
            let cursor = bowl + Vec2::new(300.0 + 200.0 * (t * 0.7).sin(), 150.0 * t.cos());
            group.update(cursor, DT);
            let milo = group.cat(1);
            pushed |= milo
                .social
                .is_some_and(|e| e.kind == SocialKind::BowlPush && e.acting);
            if pushed && outcome.is_none() {
                if group
                    .cat(0)
                    .social
                    .is_some_and(|e| e.kind == SocialKind::Swat)
                {
                    outcome = Some("swat");
                } else if group.feeding.as_ref().and_then(|s| s.eater) == Some(1) {
                    outcome = Some("gave way");
                }
            }
            // The bowl stays down for the cat at it, swatting or not
            if let Some(e) = group.feeding.as_ref().and_then(|s| s.eater)
                && group.cat(e).is_feeding()
            {
                assert_eq!(group.cat(e).eating_bowl_pos(), Some(bowl));
            }
            if outcome.is_some() {
                run(&mut group, bowl, 5.0);
                break;
            }
        }
        assert!(pushed, "Milo waited for its turn");
        assert!(outcome.is_some(), "Luna neither gave way nor swatted");
        assert!(group.feeding.is_some(), "feeding stopped");
        let eater = group.feeding.as_ref().and_then(|s| s.eater).unwrap();
        assert!(matches!(group.cat(eater).activity, Activity::Eating { .. }));
    }

    /// A cursor that rests at one spot, glides to the next over 1.5 s, and rests again, round
    /// a loop that sends the cats across and round each other
    fn wandering_cursor(t: f32) -> Vec2 {
        const SPOTS: [Vec2; 6] = [
            Vec2::new(600.0, 400.0),
            Vec2::new(900.0, 420.0),
            Vec2::new(620.0, 450.0),
            Vec2::new(760.0, 250.0),
            Vec2::new(780.0, 560.0),
            Vec2::new(500.0, 300.0),
        ];
        let leg = 18.0;
        let k = (t / leg) as usize;
        let (from, to) = (SPOTS[k % SPOTS.len()], SPOTS[(k + 1) % SPOTS.len()]);
        let glide = ((t - k as f32 * leg - (leg - 1.5)) / 1.5).clamp(0.0, 1.0);
        from + (to - from) * glide
    }

    /// Over `secs` of the cats following `cursor`, how deep (px) two cats' bodies sank into
    /// each other at worst, beyond what they were doing together allows, and how many
    /// frames they did so by more than `slack` px (a kitten in its mother's mouth aside)
    fn overlaps(
        group: &mut Clowder,
        cursor: impl Fn(f32) -> Vec2,
        secs: f32,
        slack: f32,
    ) -> (f32, usize) {
        let (mut worst, mut frames) = (0.0_f32, 0);
        for k in 0..(secs / DT) as usize {
            group.update(cursor(k as f32 * DT), DT);
            let mut sank = false;
            for i in 0..group.len() {
                for j in i + 1..group.len() {
                    let (a, b) = (group.cat(i), group.cat(j));
                    if [a, b].iter().any(|c| Pose::of(c) == Pose::Carried) {
                        continue;
                    }
                    let contact = match a.social {
                        Some(e) if group.members[i].partner == Some(j) => {
                            e.kind.contact() * 0.5 * (a.scale + b.scale)
                        }
                        _ => 0.0,
                    };
                    let excess = a.body_overlap(b).0 - contact;
                    worst = worst.max(excess);
                    sank |= excess > slack;
                }
            }
            frames += usize::from(sank);
        }
        (worst, frames)
    }

    #[test]
    fn cats_walk_round_each_other_rather_than_through() {
        // A brush in passing now and then (under one frame in 100), never half through
        let check = |group: &mut Clowder, cursor: &dyn Fn(f32) -> Vec2, secs: f32| {
            let (worst, frames) = overlaps(group, cursor, secs, 4.0);
            assert!(worst < 10.0, "bodies sank {worst} px into each other");
            let total = (secs / DT) as usize;
            assert!(
                frames * 100 < total,
                "overlapped in {frames} of {total} frames"
            );
        };
        let profiles = [
            profile("Luna"),
            profile("Milo"),
            profile("Mochi"),
            kitten_of("Pip", "Luna"),
        ];
        let start = Vec2::new(600.0, 400.0);
        check(
            &mut clowder(&profiles, start).without_social(),
            &wandering_cursor,
            216.0,
        );
        // Friends getting on and playing, and kittens chasing and wrestling
        let mut group = clowder(&profiles, start);
        for (i, j) in [(0, 1), (0, 2), (1, 2)] {
            group.set_bond(i, j, FRIENDS);
        }
        check(&mut group, &wandering_cursor, 216.0);
        let profiles = [
            profile("Luna"),
            kitten_of("Pip", "Luna"),
            kitten_of("Nori", "Luna"),
        ];
        check(
            &mut clowder(&profiles, start),
            &|t| shifting_cursor(start, t),
            600.0,
        );
    }
}
