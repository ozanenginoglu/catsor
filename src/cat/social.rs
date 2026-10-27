//! How a cat reacts to the other cats around it (`CatSim::neighbors`, filled in by `Clowder`):
//! watching them, holding its tail up for friends, copying a friend's grooming and dozing,
//! being startled awake, a kitten keeping close to its mother, and its half of what two cats do
//! together (`Engagement`). With no other cats none of this does anything.

use super::{
    Activity, CatSim, Engagement, Kinship, Neighbor, Pose, SocialKind, SocialRole, dist_to_segment,
    wrap_angle,
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
            && self.tail_lash == 0.0
            && self.bristle == 0.0
            && self.retreat.is_none()
        {
            return;
        }
        self.update_bristle_and_lash(dt);
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
        // Walking off from a cat that annoyed it
        if self.retreat.is_some() {
            raise = 0.0;
            still = 1.0;
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

    /// Puffing up and flattening its ears at another cat, and lashing its tail: watching
    /// another cat chase the laser dot, pestered by a kitten, or twitching it for her own kitten
    /// to pounce on
    fn update_bristle_and_lash(&mut self, dt: f32) {
        let (mut bristle, mut lash): (f32, f32) = (0.0, 0.0);
        if self.laser_rival && self.play_excitement > 0.25 {
            lash = 0.8;
        }
        if let Some((e, p)) = self.social_partner() {
            let active = e.role == SocialRole::Active;
            match e.kind {
                SocialKind::Standoff if e.acting => {
                    bristle = if active { 1.0 } else { 0.5 };
                    lash = 1.0;
                }
                SocialKind::Swat if e.acting => {
                    bristle = if active { 0.45 } else { 0.3 };
                    lash = if active { 1.0 } else { 0.0 };
                }
                // A mother twitches her tail to get her kitten to play; anyone else grows more
                // annoyed with each pounce
                SocialKind::TailStalk if !active => {
                    lash = if p.kin == Kinship::Kitten {
                        1.0
                    } else {
                        0.35 + 0.65 * f32::from(e.rounds) / f32::from(e.limit.max(1))
                    };
                }
                _ => {}
            }
        }
        let rate = 1.0 - (-dt * 4.0).exp();
        self.bristle += (bristle - self.bristle) * rate;
        self.tail_lash += (lash - self.tail_lash) * rate;
        if self.bristle < 1e-3 && bristle == 0.0 {
            self.bristle = 0.0;
        }
        if self.tail_lash < 1e-3 && lash == 0.0 {
            self.tail_lash = 0.0;
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
        limit: u8,
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
            rounds: 0,
            limit,
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
            Activity::Standoff { .. } => self.activity = Activity::StandingIdle { timer: 0.0 },
            Activity::PawBatting { .. } if e.kind.is_play() || e.kind == SocialKind::BowlPush => {
                self.activity = self.swat_return.take().unwrap_or(Activity::Sitting);
            }
            Activity::StalkWiggle { .. } if e.kind == SocialKind::TailStalk => {
                self.manual_pounce = false;
                self.activity = Activity::Sitting;
            }
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
        if e.done {
            return None;
        }
        let head = self.nodes[0];
        if e.kind == SocialKind::Chase && e.role == SocialRole::Passive {
            // Runs round the cursor (as it was when the chase began), a little ahead of itself
            let rel = head - e.anchor;
            let angle = rel.y.atan2(rel.x) + 0.8 * e.side;
            return Some(e.anchor + Vec2::from_angle(angle) * (Self::CHASE_RADIUS * self.scale));
        }
        if e.role == SocialRole::Passive {
            return None;
        }
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
            (SocialKind::HeadBunt, false) => p.head + heading * (14.0 * s) + flank * (12.0 * s),
            (SocialKind::HeadBunt, true) => p.center - heading * (16.0 * s) + flank * (12.0 * s),
            (SocialKind::Groom, false) => p.head + flank * (13.0 * s) - heading * (3.0 * s),
            // Comes up from behind along its friend's side, so it lies down alongside it
            (SocialKind::SleepTogether, false) if e.reached_side => {
                p.head + flank * (13.0 * s) - heading * (2.0 * s)
            }
            (SocialKind::SleepTogether, false) => {
                p.center - heading * (22.0 * s) + flank * (15.0 * s)
            }
            (SocialKind::Nurse, false) => p.center + flank * (18.0 * s),
            // Crouching distance out beyond the tail tip (not across the other cat), toward
            // whichever side of it the cat is
            (SocialKind::TailStalk, false) => {
                let out = (p.tail_tip - p.center).normalize_or(-heading);
                let mine = (head - p.tail_tip).normalize_or(out);
                let away = (2.0 * out + mine).normalize_or(out);
                p.tail_tip + away * (Self::TAIL_STALK_RANGE * self.scale)
            }
            // Straight at the other cat, once it has a head start
            (SocialKind::Chase, _) if e.timer < 0.5 => return None,
            (SocialKind::Chase, _) => p.center,
            (SocialKind::Wrestle, false) => p.head - toward * ((Self::NOSE_GAP + 2.0) * s),
            // Beside the eater's face, at the bowl
            (SocialKind::BowlPush, false) => p.head + flank * (13.0 * s) + heading * (2.0 * s),
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
        // Called away by something more pressing (pushing in at the bowl, or swatting, is
        // part of feeding)
        let at_bowl = matches!(e.kind, SocialKind::BowlPush | SocialKind::Swat);
        e.done |= self.sprinting
            || (self.feeding_phase.is_some() && !at_bowl)
            || (self.roaming && !self.roaming_by_menu)
            || self.fetch_target.is_some();
        let Some(p) = self.neighbors.get(e.partner).copied() else {
            e.done = true;
            self.social = Some(e);
            return;
        };
        let resting = Pose::of(self).is_resting();
        if e.kind == SocialKind::Chase {
            // Both run from the start; the chaser has caught up once it's at the other's body
            e.acting = true;
            if e.role == SocialRole::Active
                && e.timer > 1.5
                && [self.nodes[0], self.body_point()]
                    .into_iter()
                    .any(|q| q.distance(p.center) < Self::TAG_RANGE * self.scale)
            {
                e.rounds = 1;
                e.done = true;
            }
            e.done |= e.timer > 9.0;
        } else if e.role == SocialRole::Passive {
            // The passive cat only has to stay put (an approaching cat counts, as for a sniff)
            e.done |= match e.kind {
                SocialKind::Groom | SocialKind::SleepTogether | SocialKind::Nurse => !resting,
                SocialKind::HeadBunt => self.activity == Activity::Walking,
                SocialKind::BowlPush => !self.is_feeding(),
                SocialKind::Standoff => {
                    e.rounds > 0 && !matches!(self.activity, Activity::Standoff { .. })
                }
                SocialKind::NoseBoop
                | SocialKind::SniffCircle
                | SocialKind::TailStalk
                | SocialKind::Chase
                | SocialKind::Wrestle
                | SocialKind::Swat => false,
            };
            if e.acting {
                self.passive_act(&mut e, p);
            }
        } else if e.kind.in_place() && !e.acting {
            e.acting = true;
            self.start_social_act(&mut e, p);
        } else if !e.acting {
            let s = 0.5 * (self.scale + p.scale);
            let arrived = if e.kind == SocialKind::NoseBoop {
                self.nodes[0].distance(p.head) < (Self::NOSE_GAP + 2.0) * s
            } else {
                // A twitching tail tip is a moving target: near enough will do. Two cats sniffing
                // each other's rear chase each other round, so a nose by the other's flank will
                // do: anywhere along its hindquarters, but beside them, not right behind, where
                // circling round would take it through the other cat.
                let reach = if e.kind == SocialKind::TailStalk {
                    14.0
                } else {
                    8.0
                };
                let across = (p.head - p.center).normalize_or(Vec2::X).perp();
                let sniffing = e.kind == SocialKind::SniffCircle
                    && dist_to_segment(self.nodes[0], p.center, p.rump) < 18.0 * s
                    && (self.nodes[0] - p.center).dot(across).abs() > Self::SNIFF_FLANK * s;
                sniffing
                    || self
                        .social_target()
                        .is_some_and(|t| t.distance(self.nodes[0]) < reach * self.scale)
            };
            if e.timer > Self::SOCIAL_GIVE_UP {
                e.done = true;
            } else if arrived && e.kind == SocialKind::SleepTogether && !e.reached_side {
                e.reached_side = true;
                self.social = Some(e);
                return;
            } else if arrived {
                e.acting = true;
                self.start_social_act(&mut e, p);
            }
        } else if e.kind == SocialKind::TailStalk {
            let hunting = matches!(
                self.activity,
                Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
            );
            if !hunting {
                // Landed (or tumbled): again, or that's enough
                e.rounds += 1;
                if e.rounds >= e.limit {
                    e.done = true;
                } else {
                    e.acting = false;
                    e.timer = 0.0;
                }
            }
        } else if e.kind == SocialKind::Wrestle {
            // Over once the last turn is done
            if e.rounds >= e.limit && !self.in_wrestle_turn() {
                e.done = true;
            } else {
                self.wrestle_turn(&mut e, false);
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
                SocialKind::Standoff => !matches!(self.activity, Activity::Standoff { .. }),
                SocialKind::Swat => !matches!(self.activity, Activity::PawBatting { .. }),
                // `Clowder` settles what happens once it's there
                SocialKind::BowlPush => false,
                SocialKind::TailStalk | SocialKind::Chase | SocialKind::Wrestle => false,
            };
        }
        self.social = Some(e);
    }

    fn start_social_act(&mut self, e: &mut Engagement, p: Neighbor) {
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
            SocialKind::TailStalk => {
                self.speed = 0.0;
                // Crouches facing the tail tip; `steer` keeps the target on the tip as it moves
                self.manual_pounce = true;
                self.rest_target = p.tail_tip;
                self.activity = Activity::StalkWiggle {
                    timer: 0.0,
                    wiggle_duration: self.crouch_duration(),
                };
            }
            SocialKind::Chase | SocialKind::BowlPush => {}
            SocialKind::Wrestle => {
                self.wrestle_turn(e, false);
            }
            SocialKind::Standoff => {
                self.speed = 0.0;
                self.activity = Activity::Standoff {
                    timer: 0.0,
                    duration: self.rand_range(1.8, 3.0),
                };
            }
            SocialKind::Swat => self.swat_at(p.head),
        }
    }

    /// The passive cat's part once the other one is there
    fn passive_act(&mut self, e: &mut Engagement, p: Neighbor) {
        match e.kind {
            SocialKind::Wrestle => {
                self.wrestle_turn(e, true);
            }
            SocialKind::Standoff if !matches!(self.activity, Activity::Standoff { .. }) => {
                if e.rounds == 0 {
                    e.rounds = 1;
                    self.speed = 0.0;
                    self.activity = Activity::Standoff {
                        timer: 0.0,
                        duration: 6.0,
                    };
                }
            }
            SocialKind::Standoff => {}
            // Whips round, ears flat, but stays put until it's over
            SocialKind::Swat => {
                let to = p.head - self.nodes[0];
                self.head_yaw = wrap_angle(to.y.atan2(to.x) - self.body_angle).clamp(-1.0, 1.0);
            }
            _ => {}
        }
    }

    /// In the middle of a turn wrestling: on its back (for a while) or batting
    fn in_wrestle_turn(&self) -> bool {
        match self.activity {
            Activity::BellyUp { timer, .. } => timer <= 1.3,
            Activity::PawBatting { .. } => true,
            _ => false,
        }
    }

    /// Wrestling: on its back pawing at the other cat, then up batting at it, by turns.
    /// Returns whether a new turn began (counted in `rounds`).
    fn wrestle_turn(&mut self, e: &mut Engagement, on_back_first: bool) -> bool {
        if self.in_wrestle_turn() {
            return false;
        }
        let next_on_back = match self.activity {
            Activity::BellyUp { .. } => false,
            _ if e.rounds == 0 => on_back_first,
            // Just finished batting
            _ => true,
        };
        e.rounds = e.rounds.saturating_add(1);
        self.speed = 0.0;
        self.idle_time = 0.0;
        let side = self.side_toward_partner();
        if next_on_back {
            self.curl_side = side;
            self.activity = Activity::BellyUp { side, timer: 0.0 };
        } else {
            self.activity = Activity::PawBatting {
                side,
                timer: 0.0,
                duration: self.rand_range(0.6, 1.0),
            };
        }
        true
    }

    /// +1 when the partner is on this cat's left (world-angle positive) side, else -1
    fn side_toward_partner(&self) -> f32 {
        let Some((_, p)) = self.social_partner() else {
            return self.curl_side;
        };
        let rel = p.head - self.nodes[0];
        if Vec2::from_angle(self.body_angle).perp().dot(rel) >= 0.0 {
            1.0
        } else {
            -1.0
        }
    }

    /// A quick paw swat at a cat at `at`, ears flat. A cat that was eating goes back to it.
    fn swat_at(&mut self, at: Vec2) {
        if matches!(self.activity, Activity::Eating { .. }) {
            self.swat_return = Some(self.activity);
        }
        let rel = at - self.nodes[0];
        let side = if Vec2::from_angle(self.body_angle).perp().dot(rel) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        self.speed = 0.0;
        self.activity = Activity::PawBatting {
            side,
            timer: 0.0,
            duration: self.rand_range(0.55, 0.8),
        };
    }

    /// Backs off `dist` (unscaled px) straight away from `from`, holding its tail still, and
    /// grooms when it gets there
    pub(crate) fn back_off(&mut self, from: Vec2, dist: f32) {
        let away = (self.nodes[0] - from).normalize_or(-Vec2::from_angle(self.body_angle));
        self.retreat = Some(self.nodes[0] + away * (dist * self.scale));
        self.wake_anchor = None;
    }

    /// Grooms itself where it is ("we weren't fighting")
    pub(crate) fn groom_it_off(&mut self) {
        let side = self.rand_side();
        self.speed = 0.0;
        self.activity = Activity::Grooming {
            side,
            timer: 0.0,
            duration: self.rand_range(2.2, 3.4),
        };
    }

    /// Joins another cat's zoomies
    pub(crate) fn join_zoomies(&mut self) {
        let dir = self.rand_side();
        let duration = self.rand_range(1.4, 2.2);
        self.activity = Activity::Zoomies {
            center: self.nodes[2],
            dir,
            timer: 0.0,
            duration,
        };
        self.next_quirk_timer = self.next_quirk_timer.max(duration + 10.0);
    }

    /// Lets another cat have the laser dot: stops crouching and sits watching it
    pub(crate) fn yield_pounce(&mut self) {
        if self.hunting_cursor() && matches!(self.activity, Activity::StalkWiggle { .. }) {
            self.manual_pounce = false;
            self.speed = 0.0;
            self.rest_target = self.nodes[0];
            self.activity = Activity::Sitting;
        }
    }

    /// Whether this cat is crouching at, or leaping on, the cursor (not another cat's tail)
    pub(crate) fn hunting_cursor(&self) -> bool {
        self.social.is_none()
            && matches!(
                self.activity,
                Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
            )
    }

    /// Where a backing-off cat is going; arriving there it settles and grooms
    pub(super) fn retreat_point(&mut self) -> Option<Vec2> {
        let point = self.retreat?;
        let arrived = self.nodes[0].distance(point) < 12.0 * self.scale
            && !matches!(
                self.activity,
                Activity::Walking | Activity::StandingUp { .. }
            );
        if !arrived && !self.sprinting && self.social.is_none() {
            return Some(point);
        }
        self.retreat = None;
        if arrived && self.feeding_phase.is_none() {
            self.wake_anchor = Some(self.smooth_cursor);
            self.groom_it_off();
        }
        None
    }

    /// Whether the cursor has moved far enough since the cats began to break it up. Only a cat
    /// following the cursor minds: one roaming or busy at the bowl doesn't.
    pub(super) fn social_interrupted(&self, radius: f32) -> bool {
        self.behaviors.follow_cursor
            && !self.roaming
            && self.feeding_phase.is_none()
            && self
                .social
                .is_some_and(|e| self.smooth_cursor.distance(e.anchor) > radius)
    }

    /// Eyes shut in contentment: kneading, or being groomed
    pub fn blissful(&self) -> bool {
        matches!(
            self.activity,
            Activity::Kneading { .. } | Activity::Carried { .. }
        ) || self.social.is_some_and(|e| {
            e.kind == SocialKind::Groom && e.role == SocialRole::Passive && e.acting
        })
    }
}
