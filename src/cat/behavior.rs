use super::{
    Activity, BODY_NODES, CatSim, FeedPreset, FeedingPhase, Temperament, pounce_stretch,
    pounce_travel, smoothstep, wrap_angle,
};
use glam::Vec2;

/// Per-temperament pacing and radii for one `CatSim::update` tick (radii already scaled).
struct TemperamentTuning {
    sit_to_lie_time: f32,
    lie_to_sleep_time: f32,
    aloof_chance: f32,
    speed_mult: f32,
    walk_cruise: f32,
    trot_cruise: f32,
    stop_radius: f32,
    wake_radius_sit: f32,
    wake_radius_sleep: f32,
}

/// Cursor and target measurements for one `CatSim::update` tick, taken from the head
/// position before the activity state machine runs.
struct Steering {
    cursor_delta: f32,
    raw_dist_to_head: f32,
    raw_cursor_dist: f32,
    /// Distance that wakes or moves the cat: from the wake anchor if set, and 0 when not following
    dist_to_cursor: f32,
    to_target: Vec2,
    dist_to_target: f32,
    angle_to_cursor: f32,
    wants_to_pounce: bool,
}

impl CatSim {
    pub fn update(&mut self, cursor_pos: Vec2, dt: f32) {
        let dt = dt.clamp(0.001, 0.05);
        self.time += dt;
        self.advance_breathing(dt);
        self.rebase_carrier_nodes();

        let cursor_step = cursor_pos - self.raw_cursor;
        let cursor_delta = cursor_step.length();
        self.cursor_speed = self.cursor_speed * 0.7 + (cursor_delta / dt) * 0.3;
        self.raw_cursor = cursor_pos;

        if let Some(FeedingPhase::Carrying { preset, .. }) = self.feeding_phase {
            self.feeding_phase = Some(FeedingPhase::Carrying {
                preset,
                bowl_pos: cursor_pos,
            });
        }
        let raw_dist_to_head = self.raw_cursor.distance(self.nodes[0]);
        self.track_wiggle(cursor_step, raw_dist_to_head, dt);
        self.track_shake(cursor_step, dt);
        self.update_sprint_trigger();

        // 1. Low-pass filter cursor position so jittery typing/clicking movements
        // blend into a smooth trajectory when the cat follows.
        let smooth_rate = if self.sprinting { 8.5 } else { 6.0 };
        let smooth_alpha = 1.0 - (-dt * smooth_rate).exp();
        self.smooth_cursor = self.smooth_cursor.lerp(self.raw_cursor, smooth_alpha);
        if self.latch_wake_anchor {
            self.latch_wake_anchor = false;
            self.smooth_cursor = cursor_pos;
            self.wake_anchor = Some(cursor_pos);
        }
        if self.latch_pounce_target {
            self.latch_pounce_target = false;
            self.aim_pounce_target();
        }

        let tuning = self.temperament_tuning();
        let head_pos = self.nodes[0];
        self.check_bowl_arrival(head_pos);
        let steer = self.steer(head_pos, cursor_delta, raw_dist_to_head, &tuning);

        // Reset transient modifiers before state machine applies them
        self.hip_wiggle_offset = 0.0;
        let target_puff = if matches!(self.activity, Activity::Zoomies { .. }) {
            1.65
        } else if self.gallop_weight > 0.05 {
            1.0 + 0.25 * self.gallop_weight
        } else {
            1.0
        };
        self.tail_puff += (target_puff - self.tail_puff) * (1.0 - (-dt * 6.0).exp());

        // 2. State machine transitions
        self.step_activity(&steer, &tuning, dt);
        self.apply_sprint_override(steer.dist_to_target);
        self.clear_stale_rest_flags();

        // 3. Sleep blend, belly roll, then locomotion & steering
        self.blend_sleep_and_belly_roll(dt);
        self.update_locomotion(&steer, &tuning, dt);

        // 4. Posture extension, sagittal gallop coil/uncoil wave, and spine curl
        let prev_extension = self.posture_extension;
        self.update_posture(dt);
        if self.activity == Activity::Walking {
            // Couple head vs. hindquarter translation to spinal flexion-extension:
            // - Uncoiling (delta_ext > 0): planted hindlimbs drive the head/shoulders rapidly forward.
            // - Coiling (delta_ext < 0): forelimbs pivot while hindquarters snap forward under the waist.
            let delta_ext = (self.posture_extension - prev_extension) * self.gallop_weight;
            let accordion_head_step = 22.6 * self.scale * (delta_ext / 1.60);
            let forward = Vec2::from_angle(self.body_angle);
            self.nodes[0] += forward * (self.speed * dt + accordion_head_step);
        }
        // 5. Head Gaze & Ear Twitches
        self.update_head_and_ears(
            steer.angle_to_cursor,
            steer.dist_to_cursor,
            cursor_delta,
            dt,
        );

        // 6. Solve Spine, Scapular/Pelvic Sway, Tail Counterbalance, and 4-Paw Kinematics
        self.solve_spine_and_tail(dt);
        self.update_paws(dt);
    }

    fn advance_breathing(&mut self, dt: f32) {
        // ~18 breaths/min asleep, ~18 on the back, ~22 resting awake
        let breath_freq = if self.activity == Activity::Sleeping {
            1.9
        } else if self.belly_roll > 0.1 {
            1.85
        } else {
            2.3
        };
        self.breath_phase = (self.breath_phase + dt * breath_freq) % std::f32::consts::TAU;
    }

    /// Snaps the head back onto its carrier node when only last frame's head offset separates
    /// them, and re-anchors the carrier chain once the waist has wandered too far from it.
    fn rebase_carrier_nodes(&mut self) {
        if (self.nodes[0] - (self.carrier_nodes[0] + self.prev_head_offset)).length_squared() < 1e-3
        {
            self.nodes[0] = self.carrier_nodes[0];
        }
        self.prev_head_offset = Vec2::ZERO;
        if (self.nodes[3] - self.carrier_nodes[3]).length_squared() > (8.0 * self.scale).powi(2) {
            self.carrier_nodes
                .copy_from_slice(&self.nodes[..BODY_NODES]);
            self.window_anchor = self.carrier_nodes[3];
        }
    }

    /// Builds `play_excitement` from back-and-forth cursor reversals near the cat.
    fn track_wiggle(&mut self, cursor_step: Vec2, raw_dist_to_head: f32, dt: f32) {
        let cursor_delta = cursor_step.length();
        let inst_speed = cursor_delta / dt;
        let is_wiggle_reversal = if cursor_delta > 4.0 * self.scale {
            let step_dir = cursor_step / cursor_delta;
            let reversed = self.last_cursor_dir.length_squared() > 0.5
                && step_dir.dot(self.last_cursor_dir) < -0.25;
            self.last_cursor_dir = step_dir;
            reversed
        } else {
            false
        };

        // Only true back-and-forth cursor reversals (like wiggling a laser pointer) build hunting excitement;
        // a sudden point-to-point mouse move must never be misclassified as a wiggle.
        if self.behaviors.playful_quirks
            && is_wiggle_reversal
            && inst_speed > 250.0 * self.scale
            && cursor_delta < 75.0 * self.scale
            && raw_dist_to_head > 75.0 * self.scale
            && raw_dist_to_head < 230.0 * self.scale
        {
            let gain = match self.behaviors.temperament {
                Temperament::Playful => 2.5,
                Temperament::Balanced => 1.8,
                Temperament::Lazy => 0.9,
                Temperament::Aloof => 1.1,
                Temperament::Realistic => 1.5,
            };
            self.play_excitement = (self.play_excitement + (dt * gain).max(0.06 * gain)).min(1.5);
        } else if cursor_delta >= 75.0 * self.scale || raw_dist_to_head >= 230.0 * self.scale {
            self.play_excitement = 0.0;
        } else {
            self.play_excitement = (self.play_excitement - dt * 0.65).max(0.0);
        }
    }

    /// Detects rapid horizontal (left-right) mouse shakes to make the cat run to the cursor.
    /// Each completed stroke that reverses a fast, long-enough stroke adds to `shake_energy`.
    fn track_shake(&mut self, cursor_step: Vec2, dt: f32) {
        let dx = cursor_step.x;
        let dy = cursor_step.y;
        let vx = dx.abs() / dt;
        let is_horiz_step =
            dx.abs() > 2.5 * self.scale && dx.abs() >= dy.abs() * 0.75 && vx > 180.0 * self.scale;
        self.shake_energy = (self.shake_energy - dt * 1.35).max(0.0);
        if is_horiz_step {
            self.shake_idle_timer = 0.0;
            let dir_x = dx.signum();
            if self.shake_dir_x == 0.0 {
                self.shake_dir_x = dir_x;
                self.shake_stroke_dx = dx.abs();
                self.shake_stroke_peak_vx = vx;
            } else if dir_x == self.shake_dir_x {
                self.shake_stroke_dx += dx.abs();
                self.shake_stroke_peak_vx = self.shake_stroke_peak_vx.max(vx);
            } else {
                let prev_valid = self.shake_stroke_dx >= 12.0 * self.scale
                    && self.shake_stroke_peak_vx > 240.0 * self.scale;
                if prev_valid {
                    self.shake_energy = (self.shake_energy + 0.68).min(2.0);
                }
                self.shake_dir_x = dir_x;
                self.shake_stroke_dx = dx.abs();
                self.shake_stroke_peak_vx = vx;
            }
        } else {
            self.shake_idle_timer += dt;
            if (dy.abs() > 20.0 * self.scale && dy.abs() > dx.abs() * 2.0)
                || self.shake_idle_timer > 0.25
            {
                self.shake_dir_x = 0.0;
                self.shake_stroke_dx = 0.0;
                self.shake_stroke_peak_vx = 0.0;
            }
            if self.shake_idle_timer > 0.08 {
                self.shake_energy = (self.shake_energy - dt * 1.2).max(0.0);
            }
        }
    }

    /// Feeding suppresses play and sprinting; otherwise enough shake energy starts a sprint
    /// that cancels any pending pounce or rest.
    fn update_sprint_trigger(&mut self) {
        if self.is_feeding() {
            self.play_excitement = 0.0;
            self.shake_energy = 0.0;
            self.sprinting = false;
        } else if self.behaviors.follow_cursor && self.shake_energy >= 1.0 {
            if !self.sprinting {
                self.sprinting = true;
                self.smooth_cursor = self.smooth_cursor.lerp(self.raw_cursor, 0.75);
            }
            self.manual_pounce = false;
            self.latch_pounce_target = false;
            self.wake_anchor = None;
            self.pinned_rest = false;
            self.play_excitement = 0.0;
        }
    }

    fn temperament_tuning(&self) -> TemperamentTuning {
        let (
            stop_mult,
            sit_mult,
            sleep_mult,
            sit_to_lie_time,
            lie_to_sleep_time,
            aloof_chance,
            speed_mult,
        ) = match self.behaviors.temperament {
            Temperament::Balanced => (1.0, 1.0, 1.0, 12.0, 28.0, 0.28, 1.0),
            Temperament::Playful => (0.85, 0.80, 0.80, 18.0, 40.0, 0.08, 1.22),
            Temperament::Lazy => (1.05, 1.55, 1.60, 5.5, 12.0, 0.45, 0.78),
            Temperament::Aloof => (1.35, 1.30, 1.25, 11.0, 26.0, 0.65, 0.90),
            Temperament::Realistic => (1.0, 1.0, 1.1, 45.0, 120.0, 0.35, 1.0),
        };
        // Walk and trot cruise speeds (unscaled px/s). With the cat ~45 px long (~1 px per cm),
        // Realistic uses real-cat pacing: a relaxed ~0.65 m/s walk at ~1.6 strides/s, trotting
        // only toward distant targets. The others scale a brisker walk by `speed_mult`.
        let (walk_cruise, trot_cruise) = match self.behaviors.temperament {
            Temperament::Realistic => (66.0, 135.0),
            _ => (104.0 * speed_mult, 152.0 * speed_mult),
        };
        TemperamentTuning {
            sit_to_lie_time,
            lie_to_sleep_time,
            aloof_chance,
            speed_mult,
            walk_cruise,
            trot_cruise,
            stop_radius: 38.0 * self.scale * stop_mult,
            wake_radius_sit: 72.0 * self.scale * sit_mult,
            wake_radius_sleep: 92.0 * self.scale * sleep_mult,
        }
    }

    /// Starts eating once the head reaches the bowl the cat is walking to.
    fn check_bowl_arrival(&mut self, head_pos: Vec2) {
        if let Some(FeedingPhase::WalkingToBowl { preset, bowl_pos }) = self.feeding_phase {
            let to_bowl = bowl_pos - head_pos;
            let dist_to_bowl = to_bowl.length();
            let eat_offset = 10.2 * self.scale;
            if dist_to_bowl <= eat_offset + 6.5 * self.scale {
                let waist_to_bowl = bowl_pos - self.nodes[3];
                if waist_to_bowl.length() > 1.5 {
                    self.body_angle = waist_to_bowl.y.atan2(waist_to_bowl.x);
                }
                self.enter_eating_at(preset, bowl_pos);
            }
        }
    }

    /// Picks what the cat is following this tick (the smoothed cursor, or the bowl while
    /// feeding), refreshes `rest_target` from it, and measures distances and headings from
    /// `head_pos` for the state machine and locomotion.
    fn steer(
        &mut self,
        head_pos: Vec2,
        cursor_delta: f32,
        raw_dist_to_head: f32,
        tuning: &TemperamentTuning,
    ) -> Steering {
        let (effective_target_cursor, force_follow) = match self.feeding_phase {
            Some(FeedingPhase::Carrying { bowl_pos, .. }) => (bowl_pos, true),
            Some(FeedingPhase::WalkingToBowl { bowl_pos, .. }) => (bowl_pos, true),
            _ => (self.smooth_cursor, self.behaviors.follow_cursor),
        };

        let to_cursor = effective_target_cursor - head_pos;
        let raw_cursor_dist = to_cursor.length();
        let dist_to_cursor = if force_follow {
            if let Some(anchor) = self.wake_anchor {
                effective_target_cursor.distance(anchor)
            } else {
                raw_cursor_dist
            }
        } else {
            0.0
        };

        let effective_stop_radius =
            if matches!(self.feeding_phase, Some(FeedingPhase::WalkingToBowl { .. })) {
                10.2 * self.scale
            } else {
                tuning.stop_radius
            };

        if force_follow && self.wake_anchor.is_none() {
            if !matches!(
                self.activity,
                Activity::StalkWiggle { .. } | Activity::Pouncing { .. } | Activity::Eating { .. }
            ) {
                if raw_cursor_dist > effective_stop_radius {
                    let dir = to_cursor / raw_cursor_dist;
                    self.rest_target = effective_target_cursor - dir * effective_stop_radius;
                } else {
                    self.rest_target = head_pos;
                }
            }
        } else if !matches!(
            self.activity,
            Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
        ) {
            self.rest_target = head_pos;
        }

        let to_target = self.rest_target - head_pos;
        let angle_to_cursor = if raw_cursor_dist > 1.0 {
            to_cursor.y.atan2(to_cursor.x)
        } else {
            self.body_angle
        };

        // Only pounce when the user is actively wiggling the cursor like a laser toy at pounce range
        let in_pounce_range = |d: f32| d > 85.0 * self.scale && d < 230.0 * self.scale;
        let wants_to_pounce = self.behaviors.playful_quirks
            && self.play_excitement > 0.55
            && in_pounce_range(dist_to_cursor)
            && in_pounce_range(raw_dist_to_head);

        Steering {
            cursor_delta,
            raw_dist_to_head,
            raw_cursor_dist,
            dist_to_cursor,
            to_target,
            dist_to_target: to_target.length(),
            angle_to_cursor,
            wants_to_pounce,
        }
    }

    fn step_activity(&mut self, steer: &Steering, tuning: &TemperamentTuning, dt: f32) {
        let dist_to_cursor = steer.dist_to_cursor;
        let wake_radius_sit = tuning.wake_radius_sit;
        match self.activity {
            Activity::Sleeping => {
                self.idle_time += dt;
                if dist_to_cursor > tuning.wake_radius_sleep {
                    // Waking from sleep: do an anterior wake-up stretch before standing & walking
                    self.activity = Activity::Stretching {
                        progress: 0.0,
                        after_walk: true,
                    };
                    self.idle_time = 0.0;
                }
            }
            Activity::Lying | Activity::DramaticFlop { .. } | Activity::BellyUp { .. } => {
                self.step_lying(steer, tuning, dt);
            }
            Activity::Sitting => self.step_sitting(steer, tuning, dt),
            Activity::AloofNotice {
                mut timer,
                pause_duration,
            } => {
                timer += dt;
                // Radar ear twitch while judging cursor
                self.ear_left_twitch = 0.25;
                if timer >= pause_duration {
                    if dist_to_cursor > wake_radius_sit {
                        self.activity = Activity::StandingUp { progress: 0.0 };
                    } else {
                        self.activity = Activity::Sitting;
                    }
                } else {
                    self.activity = Activity::AloofNotice {
                        timer,
                        pause_duration,
                    };
                }
            }
            Activity::Grooming {
                side,
                mut timer,
                duration,
            } => {
                self.idle_time += dt;
                timer += dt;
                if dist_to_cursor > wake_radius_sit * 1.05 {
                    self.activity = Activity::StandingUp { progress: 0.25 };
                    self.idle_time = 0.0;
                } else if timer >= duration {
                    self.activity = Activity::Sitting;
                } else {
                    self.activity = Activity::Grooming {
                        side,
                        timer,
                        duration,
                    };
                }
            }
            Activity::PawBatting {
                side,
                mut timer,
                duration,
            } => {
                self.idle_time += dt;
                timer += dt;
                if dist_to_cursor > wake_radius_sit {
                    self.activity = Activity::StandingUp { progress: 0.2 };
                    self.idle_time = 0.0;
                } else if timer >= duration {
                    // Look away innocently after swatting at the cursor!
                    self.head_yaw = -side * 0.48;
                    self.activity = Activity::Sitting;
                } else {
                    self.activity = Activity::PawBatting {
                        side,
                        timer,
                        duration,
                    };
                }
            }
            Activity::TailChasing {
                center,
                dir,
                mut timer,
                duration,
            } => {
                timer += dt;
                if dist_to_cursor > wake_radius_sit * 1.2 {
                    self.activity = Activity::StandingUp { progress: 0.4 };
                    self.idle_time = 0.0;
                } else if timer >= duration {
                    // Stop dizzy and immediately pretend to groom a paw ("I meant to do that")
                    self.activity = Activity::Grooming {
                        side: dir,
                        timer: 0.0,
                        duration: 2.2,
                    };
                } else {
                    // Spin in a tight circle chasing own tail around a fixed pivot!
                    self.curl_amount = 0.42 * dir;
                    self.orbit_step(center, 9.2 * dir, 9.5 * self.scale, 0.85, dt);
                    self.activity = Activity::TailChasing {
                        center,
                        dir,
                        timer,
                        duration,
                    };
                }
            }
            Activity::Circling {
                center,
                dir,
                mut timer,
                mut turned,
                total,
            } => {
                timer += dt;
                self.idle_time += dt;
                if dist_to_cursor > wake_radius_sit * 1.15 {
                    self.activity = Activity::StandingUp { progress: 0.5 };
                    self.idle_time = 0.0;
                } else if turned >= total {
                    // Lie down curled the same way it was turning
                    self.curl_side = dir;
                    self.activity = Activity::Sleeping;
                } else {
                    // Slow walk around a tight circle: ease into the first steps, slow for the last
                    let ease_in = smoothstep(timer / 0.8);
                    let ease_out = 1.0 - 0.7 * smoothstep((turned - (total - 1.2)) / 1.2);
                    let spin_speed = Self::CIRCLING_SPIN * ease_in.max(0.15) * ease_out * dir;
                    turned += spin_speed.abs() * dt;
                    self.orbit_step(center, spin_speed, 11.0 * self.scale, 0.7, dt);
                    self.activity = Activity::Circling {
                        center,
                        dir,
                        timer,
                        turned,
                        total,
                    };
                }
            }
            Activity::StalkWiggle {
                timer,
                wiggle_duration,
            } => self.step_stalk(steer, timer + dt, wiggle_duration, dt),
            Activity::Pouncing {
                start_pos,
                target_pos,
                progress,
            } => self.step_pounce(start_pos, target_pos, progress, dt),
            Activity::Zoomies {
                center,
                dir,
                mut timer,
                duration,
            } => {
                timer += dt;
                self.ear_left_twitch = -0.42;
                self.ear_right_twitch = -0.42;
                if timer >= duration {
                    self.activity = Activity::StandingIdle { timer: 0.4 };
                } else {
                    self.zoomies_step(center, dir, timer, dt);
                    self.activity = Activity::Zoomies {
                        center,
                        dir,
                        timer,
                        duration,
                    };
                }
            }
            Activity::Stretching {
                mut progress,
                after_walk,
            } => {
                progress += dt * 1.45;
                if progress >= 1.0 {
                    if after_walk || dist_to_cursor > wake_radius_sit {
                        self.activity = Activity::StandingUp { progress: 0.45 };
                    } else {
                        self.activity = Activity::Lying;
                    }
                } else {
                    self.activity = Activity::Stretching {
                        progress,
                        after_walk,
                    };
                }
            }
            Activity::StandingUp { mut progress } => {
                // If user is wiggling the cursor enticingly at pounce range, drop into StalkWiggle!
                if steer.wants_to_pounce {
                    self.activity = Activity::StalkWiggle {
                        timer: 0.0,
                        wiggle_duration: self.crouch_duration(),
                    };
                } else {
                    // Smoothly stand up over ~0.48s before taking a single step
                    progress += dt * 2.1;
                    if progress >= 1.0 {
                        if steer.dist_to_target > 8.0 * self.scale {
                            self.activity = Activity::Walking;
                        } else {
                            self.activity = Activity::StandingIdle { timer: 0.0 };
                        }
                    } else {
                        self.activity = Activity::StandingUp { progress };
                    }
                }
            }
            Activity::Walking => {
                self.idle_time = 0.0;
                if steer.dist_to_target < 6.0 * self.scale && self.speed < 18.0 * self.scale {
                    self.speed = 0.0;
                    if self.shake_energy < 0.8 {
                        self.sprinting = false;
                    }
                    self.activity = Activity::StandingIdle { timer: 0.0 };
                }
            }
            Activity::StandingIdle { mut timer } => {
                timer += dt;
                self.idle_time += dt;
                // If cursor moves while still standing, immediately resume smooth walking
                if self.wake_anchor.is_some() && dist_to_cursor > wake_radius_sit {
                    self.wake_anchor = None;
                }
                if steer.dist_to_target > 18.0 * self.scale {
                    self.activity = Activity::Walking;
                } else if timer >= 1.6 {
                    self.curl_side = self.rand_side();
                    self.activity = Activity::Sitting;
                } else {
                    self.activity = Activity::StandingIdle { timer };
                }
            }
            Activity::Eating {
                preset,
                elapsed,
                duration,
            } => self.step_eating(preset, elapsed + dt, duration, dt),
        }
    }

    /// Lying, DramaticFlop and BellyUp: roll between the lying poses, wake when the cursor
    /// leaves, or settle (via circling, for Realistic) into sleep.
    fn step_lying(&mut self, steer: &Steering, tuning: &TemperamentTuning, dt: f32) {
        self.idle_time += dt;
        match self.activity {
            Activity::DramaticFlop { side, mut timer } => {
                timer += dt;
                // A realistic cat lying on its side stays there until it sleeps or wakes
                if timer > 6.5 && self.behaviors.temperament != Temperament::Realistic {
                    if self.rand_f32() < 0.45 {
                        self.activity = Activity::BellyUp { side, timer: 0.0 };
                    } else {
                        self.activity = Activity::Lying;
                    }
                } else {
                    self.activity = Activity::DramaticFlop { side, timer };
                }
            }
            Activity::BellyUp { side, mut timer } => {
                timer += dt;
                if timer > 9.5 && !self.pinned_rest {
                    self.activity = Activity::Lying;
                } else {
                    self.activity = Activity::BellyUp { side, timer };
                }
            }
            _ => {}
        }

        let dist_to_cursor = steer.dist_to_cursor;
        if dist_to_cursor > tuning.wake_radius_sit * 1.15 {
            // Aloof cat characteristic: sometimes pauses briefly to judge the cursor first
            if self.rand_f32() < tuning.aloof_chance && dist_to_cursor < 210.0 * self.scale {
                self.activity = Activity::AloofNotice {
                    timer: 0.0,
                    pause_duration: self.rand_range(0.22, 0.38),
                };
            } else {
                self.activity = Activity::StandingUp { progress: 0.0 };
            }
            self.idle_time = 0.0;
        } else if self.idle_time > tuning.lie_to_sleep_time && !self.pinned_rest {
            self.curl_side = self.rand_side();
            self.activity = if self.behaviors.temperament == Temperament::Realistic {
                Activity::Circling {
                    center: self.window_anchor,
                    dir: self.curl_side,
                    timer: 0.0,
                    turned: 0.0,
                    total: self.rand_range(1.0, 1.8) * std::f32::consts::TAU,
                }
            } else {
                Activity::Sleeping
            };
        } else if !self.pinned_rest {
            self.tick_relaxing_quirks(dt, steer.cursor_delta, steer.raw_cursor_dist);
        }
    }

    fn step_sitting(&mut self, steer: &Steering, tuning: &TemperamentTuning, dt: f32) {
        self.idle_time += dt;
        if steer.dist_to_cursor > tuning.wake_radius_sit {
            // Normal or sudden point-to-point cursor moves always stand up and walk
            if steer.wants_to_pounce {
                self.activity = Activity::StalkWiggle {
                    timer: 0.0,
                    wiggle_duration: self.crouch_duration(),
                };
            } else {
                // First stand up before walking!
                self.activity = Activity::StandingUp { progress: 0.0 };
            }
            self.idle_time = 0.0;
        } else if self.idle_time > tuning.sit_to_lie_time {
            let roll = self.rand_f32();
            let side = self.rand_side();
            if self.behaviors.temperament == Temperament::Realistic {
                // Real cats settle from sitting into a loaf, sometimes straight onto a side,
                // and rarely go belly-up from sitting
                if roll < 0.2 {
                    self.activity = Activity::DramaticFlop { side, timer: 0.0 };
                } else {
                    self.curl_side = side;
                    self.activity = Activity::Lying;
                }
            } else if roll < 0.34 {
                self.activity = Activity::DramaticFlop { side, timer: 0.0 };
            } else if roll < 0.64 {
                self.curl_side = side;
                self.activity = Activity::BellyUp { side, timer: 0.0 };
            } else {
                self.activity = Activity::Lying;
            }
        } else {
            // Shuffle the front paws round, pivoting on the haunches, when the cursor goes
            // behind the cat (past what the head can follow)
            let raw_cursor_dist = steer.raw_cursor_dist;
            let rel = wrap_angle(steer.angle_to_cursor - self.body_angle);
            if raw_cursor_dist > 6.0 * self.scale && rel.abs() > Self::MAX_HEAD_YAW + 0.35 {
                self.sit_turning = true;
            } else if rel.abs() < 0.3 || raw_cursor_dist <= 6.0 * self.scale {
                self.sit_turning = false;
            }
            if self.sit_turning {
                let turn = rel.signum() * Self::SIT_TURN_RATE * smoothstep(rel.abs() / 0.6);
                self.body_angle = wrap_angle(self.body_angle + turn * dt);
            } else {
                self.tick_relaxing_quirks(dt, steer.cursor_delta, raw_cursor_dist);
            }
        }
    }

    /// Spins the body around `center` at `spin_speed` rad/s with the head on an `orbit_r`
    /// circle, stepping the paws to keep pace with the head's speed around the orbit.
    fn orbit_step(
        &mut self,
        center: Vec2,
        spin_speed: f32,
        orbit_r: f32,
        gait_weight: f32,
        dt: f32,
    ) {
        self.body_angle = wrap_angle(self.body_angle + spin_speed * dt);
        self.angular_vel = spin_speed;
        self.nodes[0] = center + Vec2::from_angle(self.body_angle) * orbit_r;
        self.speed = spin_speed.abs() * orbit_r;
        self.advance_gait_cycle(dt);
        self.gait_weight = gait_weight;
    }

    /// Advances the normalized gait cycle [0, 1) in proportion to the ground covered, so each
    /// planted paw's stance sweep matches how far the body travels over it.
    fn advance_gait_cycle(&mut self, dt: f32) {
        self.gait_cycle = (self.gait_cycle + self.speed / self.stride_length() * dt).fract();
    }

    /// Pre-pounce crouch. `timer` already includes this tick.
    fn step_stalk(&mut self, steer: &Steering, timer: f32, wiggle_duration: f32, dt: f32) {
        // Lock eyes/head onto target/cursor and flatten airplane ears. The crouch starts with a
        // still stare, then the hindquarters shift weight from one hind paw to the other
        // (~3 Hz) right up to the launch.
        let to_target = steer.to_target;
        let target_angle = if self.manual_pounce && steer.dist_to_target > 1.0 {
            to_target.y.atan2(to_target.x)
        } else {
            steer.angle_to_cursor
        };
        let angle_err = wrap_angle(target_angle - self.body_angle);
        self.body_angle = wrap_angle(self.body_angle + angle_err * dt * 6.5);
        self.ear_left_twitch = -0.36;
        self.ear_right_twitch = -0.36;
        let env = smoothstep((timer / wiggle_duration - 0.3) / 0.3);
        self.hip_wiggle_offset = (timer * Self::WIGGLE_RATE).sin() * 2.2 * self.scale * env;

        if !self.manual_pounce && steer.raw_dist_to_head > 240.0 * self.scale {
            // Cursor moved far away during pre-pounce crouch: walk normally instead of leaping across screen
            self.play_excitement = 0.0;
            self.activity = Activity::StandingUp { progress: 0.6 };
        } else if timer >= wiggle_duration {
            self.manual_pounce = false;
            self.play_excitement = 0.0;
            let start_pos = self.nodes[0];
            self.activity = Activity::Pouncing {
                start_pos,
                target_pos: self.clamp_pounce_target(start_pos, self.rest_target),
                progress: 0.0,
            };
        } else {
            self.activity = Activity::StalkWiggle {
                timer,
                wiggle_duration,
            };
        }
    }

    fn step_pounce(&mut self, start_pos: Vec2, target_pos: Vec2, progress: f32, dt: f32) {
        // Track moving target slightly during leap
        let target_pos =
            self.clamp_pounce_target(start_pos, target_pos.lerp(self.rest_target, dt * 4.0));
        let leap_dist = start_pos.distance(target_pos).max(40.0 * self.scale);
        let leap_rate = (260.0 * self.scale / leap_dist).clamp(1.6, 3.4);
        let progress = progress + dt * leap_rate;

        if progress >= 1.0 {
            self.speed = 25.0 * self.scale;
            if self.behaviors.follow_cursor
                && self.rest_target.distance(self.nodes[0]) > 22.0 * self.scale
            {
                self.activity = Activity::Walking;
            } else {
                self.rest_target = self.nodes[0];
                self.activity = Activity::StandingIdle { timer: 0.0 };
            }
        } else {
            let s = pounce_travel(progress);
            let prev_head = self.nodes[0];
            self.nodes[0] = start_pos.lerp(target_pos, s);
            let step_vec = self.nodes[0] - prev_head;
            if step_vec.length_squared() > 0.01 {
                self.body_angle = step_vec.y.atan2(step_vec.x);
            }
            self.speed = step_vec.length() / dt;
            self.activity = Activity::Pouncing {
                start_pos,
                target_pos,
                progress,
            };
        }
    }

    /// Fast figure-8 / orbit sprint around `center`
    fn zoomies_step(&mut self, center: Vec2, dir: f32, timer: f32, dt: f32) {
        let phase = timer * 5.2 * dir;
        let radius = 30.0 * self.scale;
        let target = center + Vec2::new(phase.cos() * radius, (phase * 2.0).sin() * radius * 0.55);
        let to_z = target - self.nodes[0];
        if to_z.length_squared() > 1.0 {
            let desired_angle = to_z.y.atan2(to_z.x);
            let diff = wrap_angle(desired_angle - self.body_angle);
            self.body_angle = wrap_angle(self.body_angle + diff * dt * 14.0);
            self.angular_vel = diff * 6.0;
        }
        self.speed = 175.0 * self.scale;
        self.nodes[0] += Vec2::from_angle(self.body_angle) * (self.speed * dt);
        // Zoomies are a flat-out gallop, not a trot
        self.gallop_weight += (1.0 - self.gallop_weight) * (1.0 - (-dt * 12.0).exp());
        self.advance_gait_cycle(dt);
        self.gait_weight = 1.0;
    }

    /// `elapsed` already includes this tick.
    fn step_eating(&mut self, preset: FeedPreset, elapsed: f32, duration: Option<f32>, dt: f32) {
        self.idle_time = 0.0;
        // The placed bowl stays fixed on screen: creep the waist anchor forward until the
        // eating pose lines up with it instead of letting the bowl slide toward the cat.
        if let Some(bowl_pos) = self.eating_bowl_pos() {
            let desired_anchor =
                bowl_pos - Vec2::from_angle(self.body_angle) * (Self::BOWL_REACH * self.scale);
            let rate = 1.0 - (-dt * 5.0).exp();
            self.window_anchor = self.window_anchor.lerp(desired_anchor, rate);
        }
        if let Some(total) = duration
            && elapsed >= total
        {
            self.feeding_phase = None;
            let side = self.rand_side();
            self.smooth_cursor = self.raw_cursor;
            self.wake_anchor = Some(self.raw_cursor);
            self.rest_target = self.nodes[0];
            self.activity = Activity::Grooming {
                side,
                timer: 0.0,
                duration: 2.6,
            };
        } else {
            self.activity = Activity::Eating {
                preset,
                elapsed,
                duration,
            };
        }
    }

    /// While sprinting, any non-moving activity is overridden by a run to the target; the
    /// sprint ends once the cat has arrived and the shake energy has drained.
    fn apply_sprint_override(&mut self, dist_to_target: f32) {
        if !self.sprinting {
            return;
        }
        if !self.behaviors.follow_cursor {
            self.sprinting = false;
        } else if dist_to_target > 6.0 * self.scale
            && !matches!(self.activity, Activity::Walking | Activity::Pouncing { .. })
        {
            self.manual_pounce = false;
            self.latch_pounce_target = false;
            self.idle_time = 0.0;
            self.speed = self.speed.max(120.0 * self.scale);
            self.activity = Activity::Walking;
        } else if self.activity != Activity::Walking && self.shake_energy <= 0.0 {
            self.sprinting = false;
        }
    }

    /// Drops rest-related flags that only hold for the activity that set them.
    fn clear_stale_rest_flags(&mut self) {
        if matches!(
            self.activity,
            Activity::StandingUp { .. }
                | Activity::Walking
                | Activity::AloofNotice { .. }
                | Activity::Stretching {
                    after_walk: true,
                    ..
                }
        ) {
            self.wake_anchor = None;
        }
        if !matches!(self.activity, Activity::BellyUp { .. }) {
            self.pinned_rest = false;
        }
        if self.activity != Activity::Sitting {
            self.sit_turning = false;
        }
    }

    fn blend_sleep_and_belly_roll(&mut self, dt: f32) {
        let target_sleep_blend = if self.activity == Activity::Sleeping {
            1.0
        } else {
            0.0
        };
        self.sleep_blend += (target_sleep_blend - self.sleep_blend) * (1.0 - (-dt * 5.0).exp());

        let target_belly_roll = if let Activity::BellyUp { side, .. } = self.activity {
            self.belly_roll_side = side;
            1.0
        } else {
            0.0
        };
        let roll_speed = if target_belly_roll > self.belly_roll {
            2.4
        } else {
            2.8
        };
        if self.belly_roll < target_belly_roll {
            self.belly_roll = (self.belly_roll + dt * roll_speed).min(1.0);
        } else if self.belly_roll > target_belly_roll {
            self.belly_roll = (self.belly_roll - dt * roll_speed).max(0.0);
        }
    }

    /// Locomotion & steering (4-beat walk, 2-beat trot, and sagittal rotary gallop) while
    /// walking; otherwise lets speed and gait decay, except for activities that drive their
    /// own motion.
    fn update_locomotion(&mut self, steer: &Steering, tuning: &TemperamentTuning, dt: f32) {
        if self.activity == Activity::Walking {
            self.steer_walk(steer, tuning, dt);
        } else if !matches!(
            self.activity,
            Activity::TailChasing { .. }
                | Activity::Circling { .. }
                | Activity::Zoomies { .. }
                | Activity::Pouncing { .. }
        ) {
            self.speed *= (-dt * 10.0).exp();
            self.angular_vel *= (-dt * 8.0).exp();
            self.gait_weight *= (-dt * 7.0).exp();
            self.gallop_weight *= (-dt * 10.0).exp();

            if let Activity::StandingUp { .. } = self.activity {
                let angle_err = wrap_angle(steer.angle_to_cursor - self.body_angle);
                self.body_angle = wrap_angle(self.body_angle + angle_err * dt * 0.85);
            }
        } else if !matches!(self.activity, Activity::Zoomies { .. }) {
            self.gallop_weight *= (-dt * 10.0).exp();
        }
    }

    fn steer_walk(&mut self, steer: &Steering, tuning: &TemperamentTuning, dt: f32) {
        let dist_to_target = steer.dist_to_target;
        let desired_dir = if dist_to_target > 0.5 {
            steer.to_target / dist_to_target
        } else {
            Vec2::from_angle(self.body_angle)
        };
        let desired_angle = desired_dir.y.atan2(desired_dir.x);
        let angle_err = wrap_angle(desired_angle - self.body_angle);

        // Smooth, compact turn trajectory: scale turn rate up slightly when close to target
        // and slow forward speed smoothly during sharp heading changes so the turning radius
        // (R = v / omega) stays graceful and compact rather than sweeping wide loops.
        let proximity_boost =
            1.0 + ((90.0 * self.scale - dist_to_target).max(0.0) / (90.0 * self.scale)) * 0.65;
        let base_turn = if self.sprinting { 10.5 } else { 5.6 };
        let turn_gain = if self.sprinting { 13.5 } else { 7.5 };
        let max_turn_rate = base_turn * proximity_boost;
        let desired_turn =
            (angle_err * turn_gain * proximity_boost).clamp(-max_turn_rate, max_turn_rate);
        let turn_smooth = 1.0 - (-dt * if self.sprinting { 18.0 } else { 14.0 }).exp();
        self.angular_vel += (desired_turn - self.angular_vel) * turn_smooth;
        self.body_angle = wrap_angle(self.body_angle + self.angular_vel * dt);

        // Leisurely speed profile (Walk at <300px distance, relaxed Trot when further away),
        // or bounding rotary gallop when triggered by rapid left-right cursor shake.
        let turn_slowdown = ((1.0 + angle_err.cos()) * 0.5).max(0.22);
        let slow_radius = if self.sprinting {
            68.0 * self.scale
        } else {
            95.0 * self.scale
        };
        let trot_ramp = smoothstep(((dist_to_target / self.scale - 220.0) / 140.0).clamp(0.0, 1.0));
        let cruise_speed = if self.sprinting {
            (290.0 + 75.0 * trot_ramp) * self.scale * tuning.speed_mult
        } else {
            (tuning.walk_cruise + (tuning.trot_cruise - tuning.walk_cruise) * trot_ramp)
                * self.scale
        };

        let brake_exp = if self.sprinting { 0.68 } else { 0.92 };
        let target_speed = if dist_to_target < slow_radius {
            cruise_speed * (dist_to_target / slow_radius).powf(brake_exp)
        } else {
            cruise_speed
        } * turn_slowdown;

        let speed_rate = if dist_to_target < 12.0 * self.scale {
            24.0
        } else if target_speed < self.speed {
            if self.sprinting { 16.0 } else { 9.0 }
        } else if self.sprinting {
            16.0
        } else {
            5.5
        };
        let accel = 1.0 - (-dt * speed_rate).exp();
        self.speed += (target_speed - self.speed) * accel;

        let target_gallop = if self.sprinting {
            smoothstep(((self.speed / self.scale - 90.0) / 80.0).clamp(0.0, 1.0))
        } else {
            0.0
        };
        self.gallop_weight += (target_gallop - self.gallop_weight) * (1.0 - (-dt * 12.0).exp());

        self.advance_gait_cycle(dt);

        // Smooth S-curve speed envelope: keeps full lively sway during normal walks (>60 px/s)
        // while tapering smoothly down as the cat decelerates to a stop (<45 px/s)
        let speed_norm = (self.speed / (78.0 * self.scale)).clamp(0.0, 1.0);
        let target_gait_weight = smoothstep(speed_norm);
        let gw_rate = if target_gait_weight < self.gait_weight {
            12.0
        } else {
            8.5
        };
        self.gait_weight += (target_gait_weight - self.gait_weight) * (1.0 - (-dt * gw_rate).exp());
    }

    /// Eases posture extension and spine curl toward the current activity's pose.
    fn update_posture(&mut self, dt: f32) {
        let (target_extension, target_curl) = match self.activity {
            Activity::Sleeping => (0.98, 0.56 * self.curl_side),
            Activity::Lying => (1.26, 0.07 * self.curl_side),
            Activity::BellyUp { side, timer } => {
                // Initial side-roll twist followed by gentle belly-up wiggle
                let roll_twist = (self.belly_roll * std::f32::consts::PI).sin() * (-0.18 * side);
                let roll_wave = (timer * 1.8).sin() * 0.08 * self.belly_roll;
                (1.30, side * 0.12 * self.belly_roll + roll_twist + roll_wave)
            }
            Activity::DramaticFlop { side, .. } => (1.38, -0.10 * side),
            Activity::Sitting
            | Activity::AloofNotice { .. }
            | Activity::Grooming { .. }
            | Activity::PawBatting { .. } => (1.16, 0.09 * self.curl_side),
            Activity::Eating { elapsed, .. } => {
                let cycle = elapsed % 16.0;
                if cycle < 13.2 {
                    // Rhythmic forward-and-downward head/neck bob into the bowl while nibbling
                    let nibble_bob = (elapsed * 7.4).sin() * 0.032;
                    (1.20 + nibble_bob, 0.05 * self.curl_side)
                } else {
                    // Brief pause between mouthfuls: sits slightly back to chew/swallow
                    (1.14, 0.07 * self.curl_side)
                }
            }
            Activity::TailChasing { dir, .. } => (1.28, 0.42 * dir),
            // Standing a little low, body curved around the circle it walks
            Activity::Circling { dir, .. } => (1.45, 0.22 * dir),
            // Low crouch: the legs fold under the body, so seen from above it stays nearly full length
            Activity::StalkWiggle { .. } => (1.50, 0.0),
            Activity::Pouncing { progress, .. } => {
                // Spine snaps to full extension during push-off, stays long in the air and gathers
                // on landing
                (1.50 + 0.42 * pounce_stretch(progress), 0.0)
            }
            Activity::Zoomies { .. } => (self.gallop_extension(), 0.0),
            Activity::Stretching { progress, .. } => {
                let stretch_bell = (progress * std::f32::consts::PI).sin();
                (1.52 + 0.32 * stretch_bell, 0.0)
            }
            Activity::StandingUp { .. } | Activity::Walking | Activity::StandingIdle { .. } => {
                // During a feline rotary gallop the spine is most flexed (~1.24) as the hind paws
                // land under the belly (RH touchdown at cycle 0.03), uncoils through hind stance to full extension
                // (~1.92) at hind lift-off / fore touchdown, then coils again through fore stance and
                // the gathered flight. The phase lead offsets the posture smoothing lag.
                (self.gallop_extension(), 0.0)
            }
        };

        let ext_speed = if self.gallop_weight > 0.05 {
            11.0 + 36.0 * self.gallop_weight
        } else if matches!(self.activity, Activity::Pouncing { .. }) {
            16.0
        } else if matches!(self.activity, Activity::StandingUp { .. }) {
            9.5
        } else {
            6.5
        };
        let posture_rate = 1.0 - (-dt * ext_speed).exp();
        self.posture_extension += (target_extension - self.posture_extension) * posture_rate;
        self.curl_amount += (target_curl - self.curl_amount) * (1.0 - (-dt * 4.5).exp());
    }

    fn tick_relaxing_quirks(&mut self, dt: f32, cursor_delta: f32, dist_to_cursor: f32) {
        // If cursor is close and twitches slightly, the cat might mischievously bat at it with a paw!
        if self.behaviors.playful_quirks
            && cursor_delta > 1.5
            && cursor_delta < 28.0 * self.scale
            && dist_to_cursor < 52.0 * self.scale
            && matches!(self.activity, Activity::Sitting)
            && self.rand_f32() < 0.18
        {
            let rel_angle = wrap_angle(
                (self.smooth_cursor - self.nodes[0])
                    .y
                    .atan2((self.smooth_cursor - self.nodes[0]).x)
                    - self.body_angle,
            );
            let side = if rel_angle >= 0.0 { 1.0 } else { -1.0 };
            self.activity = Activity::PawBatting {
                side,
                timer: 0.0,
                duration: self.rand_range(0.95, 1.45),
            };
            self.next_quirk_timer = self.rand_range(4.0, 7.5);
            return;
        }

        // Small ear reaction if user nudges cursor nearby
        if cursor_delta > 3.0 && self.rand_f32() < 0.25 {
            if self.rand_f32() < 0.5 {
                self.ear_left_twitch = self.rand_range(-0.32, 0.25);
            } else {
                self.ear_right_twitch = self.rand_range(-0.25, 0.32);
            }
        }

        let quirk_rate = match self.behaviors.temperament {
            Temperament::Balanced => 1.0,
            Temperament::Playful => 1.65,
            Temperament::Lazy => 0.55,
            Temperament::Aloof => 0.75,
            Temperament::Realistic => 0.45,
        };
        self.next_quirk_timer -= dt * quirk_rate;
        if self.next_quirk_timer <= 0.0 {
            let roll = self.rand_f32();
            if roll < 0.28 {
                // Spontaneous asymmetric "radar ear" flick
                self.ear_left_twitch = self.rand_range(-0.35, 0.28);
                if self.rand_f32() < 0.45 {
                    self.ear_right_twitch = self.rand_range(-0.28, 0.35);
                }
                self.next_quirk_timer = self.rand_range(2.2, 5.0);
            } else if self.behaviors.playful_quirks
                && roll < 0.48
                && matches!(self.activity, Activity::Sitting)
                && dist_to_cursor < 54.0 * self.scale
            {
                let side = self.rand_side();
                let duration = self.rand_range(0.9, 1.4);
                self.activity = Activity::PawBatting {
                    side,
                    timer: 0.0,
                    duration,
                };
                self.next_quirk_timer = duration + self.rand_range(4.0, 7.5);
            } else if roll < 0.72 && matches!(self.activity, Activity::Sitting) {
                // Groom a front paw / face
                let side = self.rand_side();
                let duration = self.rand_range(2.6, 4.2);
                self.activity = Activity::Grooming {
                    side,
                    timer: 0.0,
                    duration,
                };
                self.next_quirk_timer = duration + self.rand_range(4.0, 8.0);
            } else if self.behaviors.playful_quirks
                && roll < 0.84
                && matches!(self.activity, Activity::Sitting)
            {
                let dir = self.rand_side();
                let duration = self.rand_range(1.3, 2.1);
                self.activity = Activity::TailChasing {
                    center: self.window_anchor,
                    dir,
                    timer: 0.0,
                    duration,
                };
                self.next_quirk_timer = duration + self.rand_range(7.0, 12.0);
            } else if roll < 0.93
                && matches!(self.activity, Activity::Lying)
                && self.behaviors.temperament == Temperament::Realistic
            {
                // A resting cat mostly stays put: an occasional roll onto its side or a stretch,
                // and only rarely onto its back
                let pick = self.rand_f32();
                let side = self.rand_side();
                if pick < 0.10 {
                    self.curl_side = side;
                    self.activity = Activity::BellyUp { side, timer: 0.0 };
                } else if pick < 0.30 {
                    self.activity = Activity::Stretching {
                        progress: 0.0,
                        after_walk: false,
                    };
                } else if pick < 0.55 {
                    self.activity = Activity::DramaticFlop { side, timer: 0.0 };
                } else {
                    self.ear_left_twitch = self.rand_range(-0.35, 0.28);
                }
                self.next_quirk_timer = self.rand_range(8.0, 16.0);
            } else if roll < 0.93 && matches!(self.activity, Activity::Lying) {
                if self.rand_f32() < 0.52 {
                    let side = self.rand_side();
                    self.curl_side = side;
                    self.activity = Activity::BellyUp { side, timer: 0.0 };
                    self.next_quirk_timer = self.rand_range(7.0, 12.0);
                } else {
                    // Lazy front-paw stretch while lying down
                    self.activity = Activity::Stretching {
                        progress: 0.0,
                        after_walk: false,
                    };
                    self.next_quirk_timer = self.rand_range(6.0, 11.0);
                }
            } else if self.behaviors.playful_quirks
                && roll < 0.97
                && matches!(self.activity, Activity::Sitting)
            {
                let dir = self.rand_side();
                let duration = self.rand_range(1.4, 2.2);
                self.activity = Activity::Zoomies {
                    center: self.nodes[2],
                    dir,
                    timer: 0.0,
                    duration,
                };
                self.next_quirk_timer = duration + self.rand_range(10.0, 18.0);
            } else {
                // Shift tail curl side
                self.curl_side = -self.curl_side;
                self.next_quirk_timer = self.rand_range(3.5, 7.5);
            }
        }
    }
}
