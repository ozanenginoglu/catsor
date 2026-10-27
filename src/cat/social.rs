//! How a cat reacts to the other cats around it (`CatSim::neighbors`, filled in by `Clowder`):
//! watching them, holding its tail up for friends, copying a friend's grooming and dozing,
//! being startled awake, and a kitten keeping close to its mother. With no other cats none of
//! this does anything.

use super::{Activity, CatSim, Kinship, Neighbor, Pose, wrap_angle};
use glam::Vec2;

impl CatSim {
    /// This kitten's mother, if she's here
    pub(super) fn mother(&self) -> Option<&Neighbor> {
        self.neighbors.iter().find(|n| n.kin == Kinship::Mother)
    }

    /// For a kitten: where to go instead of `desired`, so it stays within the leash of its
    /// mother: on the way toward `desired`, well inside the leash so there's slack when she
    /// moves. `None` while she's near enough to `desired` (or it has no mother here), which
    /// also clears `leashed`.
    pub(super) fn leash(&mut self, desired: Vec2) -> Option<Vec2> {
        let Some(mother) = self.mother().copied().filter(|_| self.kitten) else {
            self.leashed = false;
            return None;
        };
        let leash = Self::KITTEN_LEASH * mother.scale;
        let from_mother = desired - mother.center;
        let d = from_mother.length();
        // Let go a little inside the leash, so it doesn't flick on and off at the edge
        self.leashed = d > leash || (self.leashed && d > 0.85 * leash);
        self.leashed
            .then(|| mother.center + from_mother.clamp_length_max(0.7 * leash))
    }

    /// Whether a roaming kitten has wandered out of its mother's reach and should head back
    pub(super) fn strayed_from_mother(&self) -> bool {
        self.kitten
            && self
                .mother()
                .is_some_and(|m| self.nodes[0].distance(m.center) > Self::KITTEN_LEASH * m.scale)
    }

    /// The direction (world angle) to the cat this one is watching, if any. A cat at rest
    /// watches another one moving nearby, racing past, or one it doesn't know yet; a mother
    /// keeps an eye on her kittens; kittens watch everything.
    pub(super) fn watch_cats(&mut self) -> Option<f32> {
        let at_rest = matches!(
            self.activity,
            Activity::Sitting
                | Activity::Lying
                | Activity::DramaticFlop { .. }
                | Activity::StandingIdle { .. }
        );
        if !at_rest || self.neighbors.is_empty() {
            self.gaze_neighbor = None;
            return None;
        }
        let head = self.nodes[0];
        let range = Self::WATCH_RANGE * self.scale * if self.kitten { 1.3 } else { 1.0 };
        let mut best: Option<(usize, f32)> = None;
        for (i, n) in self.neighbors.iter().enumerate() {
            let watch_range = if n.kin == Kinship::Kitten {
                range * 2.0
            } else {
                range
            };
            let d = head.distance(n.head);
            if d > watch_range || d < 1.0 {
                continue;
            }
            // 1 for a cat walking at 40 px/s (unscaled)
            let moving = (n.speed / (40.0 * n.scale.max(0.1))).min(2.0);
            let mut score = if n.pose == Pose::Racing { 2.0 } else { moving };
            if n.kin == Kinship::Kitten {
                score += 0.8;
            }
            // A stranger is worth keeping an eye on
            score *= 1.5 - 0.5 * n.familiarity;
            if self.kitten {
                score *= 1.4;
            }
            score *= 1.0 - 0.5 * d / watch_range;
            if self.gaze_neighbor == Some(i) {
                score *= 1.4;
            }
            if score > 0.35 && best.is_none_or(|(_, s)| score > s) {
                best = Some((i, score));
            }
        }
        self.gaze_neighbor = best.map(|(i, _)| i);
        best.map(|(i, _)| {
            let to = self.neighbors[i].head - head;
            to.y.atan2(to.x)
        })
    }

    /// A cat racing past (galloping, pouncing, zoomies) close enough to wake this one
    pub(super) fn startled(&self) -> bool {
        let reach = Self::STARTLE_RANGE * self.scale;
        let (head, body) = (self.nodes[0], self.body_point());
        self.neighbors.iter().any(|n| {
            n.pose == Pose::Racing
                && [n.head, n.center]
                    .into_iter()
                    .any(|p| p.distance(head) < reach || p.distance(body) < reach)
        })
    }

    /// Whether a friend within copying range is in a pose that `pred` picks
    fn friend_nearby(&self, pred: impl Fn(Pose) -> bool) -> bool {
        let range = Self::COPY_RANGE * self.scale;
        self.neighbors
            .iter()
            .any(|n| n.is_friend() && pred(n.pose) && n.head.distance(self.nodes[0]) < range)
    }

    /// Cats do what their friends do: one resting near a friend who lies down or sleeps grows
    /// drowsy sooner, and one sitting near a friend who grooms often grooms too
    pub(super) fn copy_friends(&mut self, dt: f32) {
        if self.neighbors.is_empty() {
            return;
        }
        self.copy_cooldown = (self.copy_cooldown - dt).max(0.0);
        let resting = matches!(
            self.activity,
            Activity::Sitting | Activity::Lying | Activity::StandingIdle { .. }
        );
        if resting && self.friend_nearby(|p| matches!(p, Pose::Lying | Pose::Sleeping)) {
            self.idle_time += dt * 0.6;
        }
        if self.activity == Activity::Sitting
            && !self.roaming
            && self.copy_cooldown <= 0.0
            && self.friend_nearby(|p| p == Pose::Grooming)
            && self.rand_f32() < dt * 0.3
        {
            let side = self.rand_side();
            let duration = self.rand_range(2.4, 3.8);
            self.activity = Activity::Grooming {
                side,
                timer: 0.0,
                duration,
            };
            self.copy_cooldown = 20.0;
            self.next_quirk_timer = self.next_quirk_timer.max(duration + 4.0);
        }
    }

    /// Eases the tail up while walking up to a friend (and for a kitten trotting along, all the
    /// more toward its mother), and holds it still near a cat this one dislikes
    pub(super) fn update_social_tail(&mut self, dt: f32) {
        if self.neighbors.is_empty()
            && !self.kitten
            && self.tail_raise == 0.0
            && self.tail_still == 0.0
        {
            return;
        }
        let mut raise: f32 = 0.0;
        let mut still: f32 = 0.0;
        let moving = matches!(
            self.activity,
            Activity::Walking | Activity::StandingIdle { .. } | Activity::StandingUp { .. }
        );
        if moving && !self.sprinting {
            if self.kitten {
                raise = if self.leashed { 1.0 } else { 0.55 };
            }
            let head = self.nodes[0];
            let ahead = Vec2::from_angle(self.body_angle);
            for n in &self.neighbors {
                let to = n.head - head;
                let d = to.length();
                let toward = self.activity != Activity::Walking || to.dot(ahead) > 0.3 * d;
                if n.is_friend() && d < 140.0 * self.scale && toward {
                    raise = 1.0;
                }
                if n.is_disliked() && d < 110.0 * self.scale {
                    still = 1.0;
                }
            }
        }
        let rate = 1.0 - (-dt * 3.0).exp();
        self.tail_raise += (raise - self.tail_raise) * rate;
        self.tail_still += (still - self.tail_still) * rate;
        if self.tail_raise < 1e-3 && raise == 0.0 {
            self.tail_raise = 0.0;
        }
        if self.tail_still < 1e-3 && still == 0.0 {
            self.tail_still = 0.0;
        }
    }

    /// Head yaw (relative to the body) for a calm cat watching `angle`, mixed with a little
    /// of its idle wandering gaze
    pub(super) fn watch_yaw(&self, angle: f32, wander: f32) -> f32 {
        wrap_angle(angle - self.body_angle) * 0.8 + wander * 0.2
    }
}
