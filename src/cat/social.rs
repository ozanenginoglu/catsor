//! How a cat reacts to the other cats around it (`CatSim::neighbors`, filled in by `Clowder`):
//! watching them, holding its tail up for friends, copying a friend's grooming and dozing,
//! being startled awake, a kitten keeping close to its mother, and its half of what two cats do
//! together (`Engagement`). With no other cats none of this does anything.

use super::{
    Activity, CatSim, Engagement, Kinship, Neighbor, Pose, SocialKind, SocialRole, wrap_angle,
};
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
        if let Some((_, p)) = self.social_partner() {
            let to = p.head - self.nodes[0];
            return Some(to.y.atan2(to.x));
        }
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
        if self.neighbors.is_empty() || self.social.is_some() {
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
        // Tail up to say hello, and hooked over the other cat during a head bunt
        if self.social.is_some_and(|e| {
            matches!(
                e.kind,
                SocialKind::NoseBoop | SocialKind::HeadBunt | SocialKind::SniffCircle
            )
        }) {
            raise = 1.0;
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

    /// Starts this cat's half of something it does with another cat
    pub(crate) fn begin_social(
        &mut self,
        kind: SocialKind,
        role: SocialRole,
        partner: usize,
        side: f32,
    ) {
        self.social = Some(Engagement {
            kind,
            role,
            partner,
            side,
            reached_side: false,
            acting: false,
            done: false,
            anchor: self.smooth_cursor,
            timer: 0.0,
        });
    }

    /// Ends whatever the cat was doing with another cat; it then carries on as before (back to
    /// its spot by the cursor). A groomed cat sometimes grooms itself straight after, as if to
    /// set its fur right.
    pub(crate) fn end_social(&mut self) {
        let Some(e) = self.social.take() else {
            return;
        };
        let groomed = e.kind == SocialKind::Groom && e.role == SocialRole::Passive && e.acting;
        let huff = groomed && self.activity == Activity::Sitting && self.rand_f32() < 0.4;
        match self.activity {
            Activity::Greeting { .. } | Activity::Sniffing { .. } => {
                self.activity = Activity::StandingIdle { timer: 0.0 };
            }
            Activity::GroomingOther { .. } => self.activity = Activity::Sitting,
            Activity::Kneading { .. } => self.activity = Activity::Lying,
            Activity::Sitting if huff => {
                let side = self.rand_side();
                self.activity = Activity::Grooming {
                    side,
                    timer: 0.0,
                    duration: self.rand_range(1.8, 2.8),
                };
            }
            _ => {}
        }
    }

    /// The cat it's doing something with, as last seen
    pub(super) fn social_partner(&self) -> Option<(Engagement, Neighbor)> {
        let e = self.social?;
        Some((e, *self.neighbors.get(e.partner)?))
    }

    /// Where an active cat heads for its half: up to the partner, then (for a head bunt) along
    /// its flank. `None` while it does its part in place, and for the passive cat.
    pub(super) fn social_target(&self) -> Option<Vec2> {
        let (e, p) = self.social_partner()?;
        if e.role == SocialRole::Passive || e.done {
            return None;
        }
        let head = self.nodes[0];
        let s = 0.5 * (self.scale + p.scale);
        let heading = (p.head - p.center).normalize_or(Vec2::X);
        let flank = heading.perp() * e.side;
        let toward = (p.head - head).normalize_or(heading);
        let point = match (e.kind, e.acting) {
            // Aims a little inside touching distance, so stopping short still gets it there
            (SocialKind::NoseBoop, false) => p.head - toward * ((Self::NOSE_GAP - 4.0) * s),
            // Nose to the other cat's rear, on whichever side it's on: as both do, they end up
            // side by side, head to tail
            (SocialKind::SniffCircle, false) => {
                let near_side = if heading.perp().dot(head - p.center) >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                p.center - heading * (12.0 * s) + heading.perp() * (near_side * 9.0 * s)
            }
            // In front of the partner's face, a little to one side, then along its flank to its
            // hips, cheek against its side
            (SocialKind::HeadBunt, false) => p.head + heading * (14.0 * s) + flank * (9.0 * s),
            (SocialKind::HeadBunt, true) => p.center - heading * (16.0 * s) + flank * (10.0 * s),
            (SocialKind::Groom, false) => p.head + flank * (13.0 * s) - heading * (3.0 * s),
            // Comes up from behind along its friend's side, so it lies down alongside it
            (SocialKind::SleepTogether, false) if e.reached_side => {
                p.head + flank * (13.0 * s) - heading * (2.0 * s)
            }
            (SocialKind::SleepTogether, false) => {
                p.center - heading * (22.0 * s) + flank * (15.0 * s)
            }
            (SocialKind::Nurse, false) => p.center + flank * (18.0 * s),
            _ => return None,
        };
        Some(point)
    }

    /// Advances this cat's half of what it does with another cat: notices getting there and
    /// starts its part, and marks it done when it's over, given up, or the cat got up
    pub(super) fn step_social(&mut self, dt: f32) {
        let Some(mut e) = self.social else {
            return;
        };
        e.timer += dt;
        // Called away by something more pressing
        e.done |= self.sprinting
            || self.feeding_phase.is_some()
            || (self.roaming && !self.roaming_by_menu)
            || self.fetch_target.is_some();
        let Some(p) = self.neighbors.get(e.partner).copied() else {
            e.done = true;
            self.social = Some(e);
            return;
        };
        let resting = Pose::of(self).is_resting();
        if e.role == SocialRole::Passive {
            // The passive cat only has to stay put (an approaching cat counts, as for a sniff)
            e.done |= match e.kind {
                SocialKind::Groom | SocialKind::SleepTogether | SocialKind::Nurse => !resting,
                SocialKind::HeadBunt => self.activity == Activity::Walking,
                SocialKind::NoseBoop | SocialKind::SniffCircle => false,
            };
        } else if !e.acting {
            let s = 0.5 * (self.scale + p.scale);
            let arrived = if e.kind == SocialKind::NoseBoop {
                self.nodes[0].distance(p.head) < (Self::NOSE_GAP + 2.0) * s
            } else {
                self.social_target()
                    .is_some_and(|t| t.distance(self.nodes[0]) < 8.0 * self.scale)
            };
            if e.timer > Self::SOCIAL_GIVE_UP {
                e.done = true;
            } else if arrived && e.kind == SocialKind::SleepTogether && !e.reached_side {
                e.reached_side = true;
                self.social = Some(e);
                return;
            } else if arrived {
                e.acting = true;
                self.start_social_act(e, p);
            }
        } else {
            e.done |= match e.kind {
                SocialKind::HeadBunt => self
                    .social_target()
                    .is_none_or(|t| t.distance(self.nodes[0]) < 8.0 * self.scale),
                SocialKind::SleepTogether | SocialKind::Nurse => !matches!(
                    self.activity,
                    Activity::Sleeping | Activity::Kneading { .. }
                ),
                // These finish in their own activity (or end if it was cut short)
                SocialKind::NoseBoop => !matches!(self.activity, Activity::Greeting { .. }),
                SocialKind::SniffCircle => !matches!(self.activity, Activity::Sniffing { .. }),
                SocialKind::Groom => !matches!(self.activity, Activity::GroomingOther { .. }),
            };
        }
        self.social = Some(e);
    }

    fn start_social_act(&mut self, e: Engagement, p: Neighbor) {
        let head = self.nodes[0];
        match e.kind {
            SocialKind::NoseBoop => {
                self.speed = 0.0;
                self.activity = Activity::Greeting {
                    timer: 0.0,
                    duration: self.rand_range(0.9, 1.5),
                };
            }
            SocialKind::SniffCircle => {
                self.speed = 0.0;
                // Both walk round the middle between their bodies
                self.activity = Activity::Sniffing {
                    center: 0.5 * (self.body_point() + p.center),
                    dir: e.side,
                    turned: 0.0,
                };
            }
            // Carries on walking, slowly, along the partner's flank
            SocialKind::HeadBunt => {}
            SocialKind::Groom => {
                self.speed = 0.0;
                self.activity = Activity::GroomingOther {
                    timer: 0.0,
                    duration: self.rand_range(5.0, 9.0),
                };
            }
            SocialKind::SleepTogether => {
                self.speed = 0.0;
                // Curls round toward the cat it sleeps against
                let to_partner = p.center - head;
                self.curl_side = if Vec2::from_angle(self.body_angle).perp().dot(to_partner) >= 0.0
                {
                    1.0
                } else {
                    -1.0
                };
                self.idle_time = self.idle_time.max(60.0);
                self.activity = Activity::Sleeping;
            }
            SocialKind::Nurse => {
                self.speed = 0.0;
                self.activity = Activity::Kneading {
                    timer: 0.0,
                    duration: self.rand_range(6.0, 12.0),
                };
            }
        }
    }

    /// Whether the cursor has moved far enough since the cats began to break it up
    pub(super) fn social_interrupted(&self, radius: f32) -> bool {
        self.social
            .is_some_and(|e| self.smooth_cursor.distance(e.anchor) > radius)
    }

    /// Eyes shut in contentment: kneading, or being groomed
    pub fn blissful(&self) -> bool {
        matches!(self.activity, Activity::Kneading { .. })
            || self.social.is_some_and(|e| {
                e.kind == SocialKind::Groom && e.role == SocialRole::Passive && e.acting
            })
    }
}
