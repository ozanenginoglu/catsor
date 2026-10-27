use super::{Activity, BODY_NODES, CatSim, Gait, TAIL_NODES, smoothstep, wrap_angle};
use glam::Vec2;

impl CatSim {
    pub(super) fn update_head_and_ears(
        &mut self,
        look_angle: f32,
        watch_angle: Option<f32>,
        dist_to_cursor: f32,
        cursor_delta: f32,
        dt: f32,
    ) {
        let decay = 1.0 - (-dt * 7.5).exp();
        let ambient_l = Self::fbm(self.time * 0.45, 11) * 0.06;
        let ambient_r = Self::fbm(self.time * 0.48 + 9.2, 23) * 0.06;
        self.ear_left_twitch += (ambient_l - self.ear_left_twitch) * decay;
        self.ear_right_twitch += (ambient_r - self.ear_right_twitch) * decay;

        if self.activity == Activity::Walking && self.sprinting {
            self.ear_left_twitch = -0.24;
            self.ear_right_twitch = -0.24;
        }
        // Ears flattened sideways at a cat it is cross with
        if self.bristle > 0.05 {
            let flat = -0.5 * self.bristle;
            self.ear_left_twitch = self.ear_left_twitch.min(flat);
            self.ear_right_twitch = self.ear_right_twitch.min(flat);
        }
        let desired_yaw = match self.activity {
            Activity::Sleeping => self.curl_side * 2.38 + (self.time * 1.2).sin() * 0.02,
            Activity::BellyUp { side, timer } => -side * 0.22 + (timer * 1.6).sin() * 0.12,
            Activity::Grooming { side, timer, .. } => {
                let lick = (timer * 7.5).sin() * 0.16;
                side * 0.42 + lick
            }
            Activity::Eating { elapsed, .. } => {
                let cycle = elapsed % 16.0;
                if cycle < 13.2 {
                    // Small side-to-side nibbling motions across the food bowl
                    self.ear_left_twitch -= 0.08 * decay;
                    self.ear_right_twitch -= 0.08 * decay;
                    (elapsed * 2.5).sin() * 0.10 + (elapsed * 8.2).sin() * 0.04
                } else {
                    // Brief pause looking up contentedly
                    (elapsed * 1.4).sin() * 0.24
                }
            }
            Activity::PawBatting { side, timer, .. } => {
                // Intently watches the cursor (or the cat) while batting at it, then glances away
                let at = match (self.social, watch_angle) {
                    (Some(_), Some(angle)) => angle,
                    _ => look_angle,
                };
                let rel = wrap_angle(at - self.body_angle).clamp(-0.6, 0.6);
                rel * 0.7 + (timer * 14.0).sin() * 0.06 * side
            }
            Activity::TailChasing { dir, .. } => {
                // Head craned all the way toward its own tail!
                dir * 0.78
            }
            // Looking down and into the circle at the spot it is about to lie on
            Activity::Circling { dir, .. } => dir * 0.3,
            // Nose to nose with the other cat, or glaring at it
            Activity::Greeting { .. } | Activity::Standoff { .. } => {
                watch_angle.map_or(0.0, |a| wrap_angle(a - self.body_angle).clamp(-0.9, 0.9))
            }
            // Nose toward the other cat's rear, inside the circle
            Activity::Sniffing { dir, .. } => dir * 0.4,
            Activity::GroomingOther { timer, .. } => {
                let lick = (timer * 7.5).sin() * 0.12;
                watch_angle.map_or(0.0, |a| {
                    wrap_angle(a - self.body_angle).clamp(-Self::MAX_HEAD_YAW, Self::MAX_HEAD_YAW)
                }) + lick
            }
            Activity::Kneading { timer, .. } => (timer * 1.3).sin() * 0.05,
            Activity::StalkWiggle { .. } | Activity::Pouncing { .. } => {
                // Dead-locked gaze on the target
                wrap_angle(look_angle - self.body_angle).clamp(-0.65, 0.65)
            }
            Activity::Stretching { progress, .. } => {
                (progress * std::f32::consts::TAU).sin() * 0.06
            }
            _ => {
                let rel_cursor = wrap_angle(look_angle - self.body_angle);
                let wander = Self::fbm(self.time * 0.32, 77) * 0.38;
                let cursor_attention = if matches!(
                    self.activity,
                    Activity::Walking
                        | Activity::StandingUp { .. }
                        | Activity::StandingIdle { .. }
                        | Activity::AloofNotice { .. }
                ) || cursor_delta > 0.5
                    || dist_to_cursor > 55.0 * self.scale
                {
                    0.85
                } else {
                    0.35
                };
                let raw_yaw = match watch_angle {
                    // Calm, with the cursor still nearby: watches the other cat instead
                    Some(angle) if cursor_attention < 0.5 => self.watch_yaw(angle, wander),
                    _ => rel_cursor * cursor_attention + wander * (1.0 - cursor_attention),
                };
                // Cats turn their heads up to ~80° to either side
                raw_yaw.clamp(-Self::MAX_HEAD_YAW, Self::MAX_HEAD_YAW)
            }
        };

        let yaw_rate = 1.0 - (-dt * 8.0).exp();
        self.head_yaw += wrap_angle(desired_yaw - self.head_yaw) * yaw_rate;
    }

    /// Standing spine extension, plus the gallop's flex wave blended in by `gallop_weight`
    pub(super) fn gallop_extension(&self) -> f32 {
        let bound_wave =
            -((self.gait_cycle + Self::GALLOP_FLEX_LEAD) * std::f32::consts::TAU).cos();
        let gallop_ext = 1.58 + 0.34 * bound_wave;
        1.60 + (gallop_ext - 1.60) * self.gallop_weight
    }

    /// Ground distance covered by one full gait cycle at the current speed. Like real cats, the
    /// stride lengthens as speed rises (~46 px walking at 104 px/s, ~98 px galloping at 365 px/s)
    /// rather than only stepping faster, and the first and last steps of a walk are short ones
    /// (down to half length as `gait_weight` fades). Both the gait cycle and the paw sweep use it,
    /// so planted paws stay put while the cat starts, stops or changes gait.
    pub(super) fn stride_length(&self) -> f32 {
        let v = self.speed / self.scale;
        let walk = 30.0 + 0.155 * v;
        let gallop = 40.0 + 0.16 * v;
        let stride = (walk + (gallop - walk) * self.gallop_weight) * self.scale;
        let step_scale = 0.5 + 0.5 * self.gait_weight.clamp(0.0, 1.0);
        let mut stride = (stride * step_scale).max(self.speed / Self::MAX_CADENCE);
        if self.activity.is_spinning() && self.angular_vel.abs() > 0.1 {
            // Spinning on the spot takes quick little steps: the body turns at most
            // `MAX_SPIN_PER_STRIDE` per stride, so a planted paw isn't swept round with it
            stride = stride.min(self.speed * Self::MAX_SPIN_PER_STRIDE / self.angular_vel.abs());
        }
        stride
    }

    #[inline]
    fn gait_spine_sway(phase_rad: f32, seg_idx: usize, sway_weight: f32) -> f32 {
        match seg_idx {
            // Thorax / shoulders: epaxial musculature stabilizes the anterior trunk with slight counter-angle
            2 => -0.055 * (phase_rad - 0.20).sin() * sway_weight,
            // Cranial-mid lumbar spine: lateral flexion shifting the pelvic girdle over the stance hindlimb
            3 => 0.195 * (phase_rad - 0.90).sin() * sway_weight,
            // Caudal lumbar / pelvis: continues lateral translation and pelvic yaw into the hips
            4 => 0.230 * (phase_rad - 1.20).sin() * sway_weight,
            // Lumbosacral junction (L7-S1): aligns sacrum/rump with the pelvic girdle S-curve
            5 => 0.065 * (phase_rad - 1.50).sin() * sway_weight,
            _ => 0.0,
        }
    }

    pub(super) fn solve_spine_and_tail(&mut self, dt: f32) {
        let len_scale = (self.posture_extension / 1.60) * self.scale;
        // Spine sway, head bob and tail wave are timed to the left hind paw's own step cycle, so
        // they stay in step with the footfalls in every gait and through gait changes
        let phase_rad = (self.gait_cycle - self.leg_offsets[2]) * std::f32::consts::TAU;
        let prev_carrier_waist = self.carrier_nodes[3];

        if matches!(
            self.activity,
            Activity::Walking | Activity::Pouncing { .. } | Activity::Zoomies { .. }
        ) || self.activity.is_spinning()
        {
            self.solve_moving_spine(dt, len_scale, phase_rad);
        } else if matches!(self.activity, Activity::StandingIdle { .. }) {
            self.ease_spine_onto_carrier(dt);
        } else {
            self.solve_resting_spine(dt, len_scale);
        }

        if matches!(
            self.activity,
            Activity::Walking | Activity::Pouncing { .. } | Activity::Zoomies { .. }
        ) {
            let waist_step = self.carrier_nodes[3] - prev_carrier_waist;
            self.window_anchor += waist_step;
            let sync_rate = 1.0 - (-dt * 10.0).exp();
            self.window_anchor = self.window_anchor.lerp(self.carrier_nodes[3], sync_rate);
        }

        self.solve_tail(dt, phase_rad);
    }

    /// Moving: the carrier spine follows the head's path, then the gait's lateral S-wave and
    /// head bob are laid over it
    fn solve_moving_spine(&mut self, dt: f32, len_scale: f32, phase_rad: f32) {
        let (sway_weight, align_rate) = match self.activity {
            Activity::Walking => {
                let speed_norm = (self.speed / (78.0 * self.scale)).clamp(0.0, 1.0);
                let speed_env = smoothstep(speed_norm);
                let trot_blend = ((self.speed / self.scale - 115.0) / 35.0).clamp(0.0, 1.0);
                let gallop_suppress = 1.0 - 0.85 * self.gallop_weight;
                let sw =
                    self.gait_weight.min(speed_env) * (1.0 - 0.22 * trot_blend) * gallop_suppress;
                (sw, 1.5 + speed_norm * 6.0 + self.gallop_weight * 6.0)
            }
            Activity::Zoomies { .. } => (
                self.gait_weight * 0.70 * (1.0 - 0.85 * self.gallop_weight),
                7.0,
            ),
            _ if self.activity.is_spinning() => (self.gait_weight * 0.40, 0.0),
            _ => (0.0, 7.5),
        };
        let spinning = self.activity.is_spinning();

        if self.nodes[0].distance_squared(self.carrier_nodes[0]) > (24.0 * self.scale).powi(2) {
            self.carrier_nodes
                .copy_from_slice(&self.nodes[..BODY_NODES]);
        } else {
            self.carrier_nodes[0] = self.nodes[0];
        }

        // 1. Solve pure path-following carrier spine from Head (0) to TailBase (5)
        let mut prev_carrier_angle = self.body_angle + std::f32::consts::PI;
        let mut carrier_angles = [prev_carrier_angle; BODY_NODES - 1];
        for i in 1..BODY_NODES {
            let seg_len = Self::STANDING_BODY_LENGTHS[i - 1] * len_scale;
            let delta = self.carrier_nodes[i] - self.carrier_nodes[i - 1];
            let mut seg_angle = if delta.length_squared() > 0.001 {
                delta.y.atan2(delta.x)
            } else {
                prev_carrier_angle
            };

            let max_bend = if matches!(self.activity, Activity::TailChasing { .. }) {
                0.52
            } else {
                0.35
            };
            let align_decay = if spinning {
                1.0
            } else {
                (-dt * align_rate).exp()
            };
            let raw_diff =
                wrap_angle(seg_angle - prev_carrier_angle) * align_decay + self.curl_amount * 0.35;
            let diff = raw_diff.clamp(-max_bend, max_bend);
            seg_angle = wrap_angle(prev_carrier_angle + diff);

            self.carrier_nodes[i] =
                self.carrier_nodes[i - 1] + Vec2::from_angle(seg_angle) * seg_len;
            carrier_angles[i - 1] = seg_angle;
            prev_carrier_angle = seg_angle;
        }

        // 2. Superimpose exact biomechanical feline spinal S-wave onto the carrier spine
        let forward = Vec2::from_angle(self.body_angle);
        let side = Vec2::new(-forward.y, forward.x);
        let head_bob = if spinning {
            0.0
        } else {
            phase_rad.sin() * 0.32 * self.scale * sway_weight
        };
        self.prev_head_offset = side * head_bob;
        self.nodes[0] = self.carrier_nodes[0] + self.prev_head_offset;
        for i in 1..BODY_NODES {
            let seg_len = Self::STANDING_BODY_LENGTHS[i - 1] * len_scale;
            let sway_angle = Self::gait_spine_sway(phase_rad, i, sway_weight);
            let seg_angle = wrap_angle(carrier_angles[i - 1] + sway_angle);
            self.nodes[i] = self.nodes[i - 1] + Vec2::from_angle(seg_angle) * seg_len;
        }
    }

    /// Standing still: the body relaxes onto the carrier spine and the heading follows it
    fn ease_spine_onto_carrier(&mut self, dt: f32) {
        let rate = 1.0 - (-dt * 12.0).exp();
        for i in 0..BODY_NODES {
            self.nodes[i] = self.nodes[i].lerp(self.carrier_nodes[i], rate);
        }
        let chord = self.carrier_nodes[0] - self.carrier_nodes[3];
        if chord.length_squared() > 0.1 {
            let chord_angle = chord.y.atan2(chord.x);
            self.body_angle =
                wrap_angle(self.body_angle + wrap_angle(chord_angle - self.body_angle) * rate);
        }
    }

    /// Resting, crouching or standing up: the spine curls out both ways from the waist
    fn solve_resting_spine(&mut self, dt: f32, len_scale: f32) {
        // Anchor at carrier_nodes[3] (waist) so resting/crouching/standing-up posture changes
        // never slide the cat across the screen and hip_wiggle_offset never accumulates drift.
        let anchor_idx = 3;
        let rate = 1.0 - (-dt * 14.0).exp();
        let anchor_pos = self.carrier_nodes[anchor_idx].lerp(self.window_anchor, rate);
        self.carrier_nodes[anchor_idx] = anchor_pos;
        let is_sleeping = self.activity == Activity::Sleeping;
        let mut fwd_angle = self.body_angle;
        let mut chain_pos = anchor_pos;
        for i in (0..anchor_idx).rev() {
            let seg_len = Self::STANDING_BODY_LENGTHS[i] * len_scale;
            let curl_mult = if is_sleeping {
                if i == 0 { 1.15 } else { 0.96 }
            } else {
                0.85
            };
            fwd_angle = wrap_angle(fwd_angle + self.curl_amount * curl_mult);
            let desired_dir = Vec2::from_angle(fwd_angle);
            let cur_dir = (self.carrier_nodes[i] - chain_pos).normalize_or_zero();
            let dir = if cur_dir.length_squared() > 0.1 {
                cur_dir.lerp(desired_dir, rate).normalize_or_zero()
            } else {
                desired_dir
            };
            chain_pos += dir * seg_len;
            self.carrier_nodes[i] = chain_pos;
            self.nodes[i] = chain_pos;
        }

        let mut back_angle = self.body_angle + std::f32::consts::PI;
        let side_normal = Vec2::new(-back_angle.sin(), back_angle.cos());
        self.nodes[anchor_idx] = anchor_pos + side_normal * (self.hip_wiggle_offset * 0.35);

        let back_curl_mult = if is_sleeping { 1.04 } else { 0.95 };
        let mut carrier_back_pos = anchor_pos;
        for i in (anchor_idx + 1)..BODY_NODES {
            let seg_len = Self::STANDING_BODY_LENGTHS[i - 1] * len_scale;
            back_angle = wrap_angle(back_angle - self.curl_amount * back_curl_mult);
            let desired_dir = Vec2::from_angle(back_angle);
            let cur_dir = (self.carrier_nodes[i] - carrier_back_pos).normalize_or_zero();
            let dir = if cur_dir.length_squared() > 0.1 {
                cur_dir.lerp(desired_dir, rate).normalize_or_zero()
            } else {
                desired_dir
            };
            carrier_back_pos += dir * seg_len;
            self.carrier_nodes[i] = carrier_back_pos;
            let wiggle = if i == 4 {
                side_normal * self.hip_wiggle_offset
            } else {
                side_normal * (self.hip_wiggle_offset * 1.35)
            };
            self.nodes[i] = self.carrier_nodes[i] + wiggle;
        }
    }

    /// Tail kinematics (nodes 6..12) with dynamic turn counterbalance
    fn solve_tail(&mut self, dt: f32, phase_rad: f32) {
        let rump_vec = self.nodes[5] - self.nodes[4];
        let mut tail_angle = if rump_vec.length_squared() > 0.001 {
            rump_vec.y.atan2(rump_vec.x)
        } else {
            self.body_angle + std::f32::consts::PI
        };
        let turn_counterbalance = (-self.angular_vel * 0.065).clamp(-0.28, 0.28);
        let (sway_amp, sway_speed, wrap_bias) = match self.activity {
            Activity::Sleeping => (0.02, 0.35, -self.curl_side * 0.38),
            Activity::Lying => (0.14, 0.85, -self.curl_side * 0.24),
            Activity::BellyUp { side, .. } => (0.22, 1.30, side * 0.18),
            Activity::DramaticFlop { side, .. } => (0.24, 1.35, side * 0.22),
            Activity::Sitting
            | Activity::AloofNotice { .. }
            | Activity::Grooming { .. }
            | Activity::GroomingOther { .. }
            | Activity::Eating { .. } => (0.18, 1.15, -self.curl_side * 0.28),
            Activity::Kneading { .. } => (0.14, 0.85, -self.curl_side * 0.24),
            Activity::Greeting { .. } => (0.10, 1.0, 0.0),
            Activity::Sniffing { .. } => (0.12, 0.9, turn_counterbalance),
            Activity::PawBatting { .. } => (0.28, 2.6, 0.0),
            Activity::TailChasing { dir, .. } => (0.25, 4.0, dir * 0.48),
            Activity::Circling { .. } => (0.12, 0.9, turn_counterbalance),
            Activity::StalkWiggle { .. } => (0.34, 5.5, 0.0), // Excited tail tip twitch before pounce!
            Activity::Pouncing { .. } | Activity::Zoomies { .. } => {
                (0.20, 3.2, turn_counterbalance)
            }
            Activity::Stretching { progress, .. } => {
                (0.22, 1.8, (progress * std::f32::consts::PI).sin() * 0.18)
            }
            Activity::Walking => (
                0.11 + 0.05 * self.gait_weight,
                1.35 + 0.75 * self.gait_weight,
                turn_counterbalance,
            ),
            Activity::StandingUp { .. } | Activity::StandingIdle { .. } => (0.12, 1.35, 0.0),
            Activity::Standoff { .. } => (0.08, 1.0, 0.0),
        };
        // Lashing: wide, quick sweeps (and out from round the paws, for a kitten to pounce on)
        let (sway_amp, sway_speed) = if self.tail_lash > 0.0 {
            (
                sway_amp.max(0.42 * self.tail_lash),
                sway_speed + (4.2 - sway_speed).max(0.0) * self.tail_lash,
            )
        } else {
            (sway_amp, sway_speed)
        };

        // Held up (to greet a friend) it sways less; held still near a cat it dislikes, hardly at all
        let sway_amp = if self.tail_raise > 0.0 || self.tail_still > 0.0 {
            sway_amp * (1.0 - 0.6 * self.tail_raise) * (1.0 - 0.7 * self.tail_still)
        } else {
            sway_amp
        };
        let is_sleeping = self.activity == Activity::Sleeping;
        let is_tail_chasing = matches!(self.activity, Activity::TailChasing { .. });
        let tail_rate = 1.0 - (-dt * 8.0).exp();
        self.tail_wrap_smooth += (wrap_bias - self.tail_wrap_smooth) * tail_rate;
        self.tail_amp_smooth += (sway_amp - self.tail_amp_smooth) * tail_rate;
        let wrap_bias = self.tail_wrap_smooth;
        let sway_amp = self.tail_amp_smooth;
        // A settled sitting cat slowly wraps its tail along its flank and around its front paws
        let wraps_tail = matches!(
            self.activity,
            Activity::Sitting
                | Activity::Grooming { .. }
                | Activity::GroomingOther { .. }
                | Activity::AloofNotice { .. }
        ) && !self.sit_turning
            && self.tail_lash < 0.3;
        let wrap_rate = if wraps_tail { 1.6 } else { 5.0 };
        self.sit_tail_wrap += (f32::from(u8::from(wraps_tail)) - self.sit_tail_wrap)
            * (1.0 - (-dt * wrap_rate).exp());
        let sit_wrap_side = -self.curl_side;
        let tail_seg_scale = (1.0 - 0.48 * self.sleep_blend) * self.scale;
        const TAIL_PHASE_PERIOD: f32 = 1024.0;
        self.tail_phase = (self.tail_phase + dt * sway_speed).rem_euclid(TAIL_PHASE_PERIOD);
        let u0 = self.tail_phase / TAIL_PHASE_PERIOD;
        let p0 = self.tail_phase;
        let p1 = (u0 + 0.5).fract() * TAIL_PHASE_PERIOD;
        let w0 = 0.5 - 0.5 * (u0 * std::f32::consts::TAU).cos();
        let w1 = 1.0 - w0;

        for i in 0..TAIL_NODES {
            let idx = BODY_NODES + i;
            let mut seg_len = Self::TAIL_LENGTHS[i] * tail_seg_scale;
            // Seen from above, a tail held up looks short, with its tip hooked over
            let mut raised_hook = 0.0;
            if self.tail_raise > 0.0 {
                seg_len *= 1.0 - self.tail_raise * Self::TAIL_RAISE_FORESHORTEN[i];
                raised_hook = self.tail_raise * Self::TAIL_RAISE_HOOK[i] * self.curl_side;
            }
            let phase_delay = (i as f32) * 0.42;
            let gait_tail_wave = if self.activity == Activity::Walking {
                (phase_rad - 1.8 - (i as f32) * 0.55).sin() * 0.065 * self.gait_weight
            } else {
                0.0
            };
            let seed = 300 + (i as u32) * 17;
            let fbm_blend =
                w0 * Self::fbm(p0 - phase_delay, seed) + w1 * Self::fbm(p1 - phase_delay, seed);
            // While wrapped, only the tip keeps flicking
            let wrap_calm = if i < 4 {
                1.0 - 0.7 * self.sit_tail_wrap
            } else {
                1.0
            };
            let wave = fbm_blend * sway_amp * wrap_calm + gait_tail_wave;

            let tip_hook = if is_sleeping {
                // Tuck the tail tightly along the lower rump and underneath the body/chin so it never crowds the head
                if i == 0 {
                    wrap_bias * 2.10
                } else {
                    wrap_bias * 1.05
                }
            } else if is_tail_chasing {
                wrap_bias * if i >= 3 { 0.88 } else { 1.0 }
            } else if i >= 3 {
                let tip_noise = w0 * Self::noise1d(p0 * 0.8 - phase_delay, 991)
                    + w1 * Self::noise1d(p1 * 0.8 - phase_delay, 991);
                -wrap_bias * 0.42 + tip_noise * sway_amp * 0.85
            } else {
                wrap_bias
            };

            let sit_wrap = sit_wrap_side * Self::SIT_TAIL_WRAP[i] * self.sit_tail_wrap;
            let target_seg_angle =
                wrap_angle(tail_angle + wave + tip_hook + sit_wrap + raised_hook);
            let desired_pos = self.nodes[idx - 1] + Vec2::from_angle(target_seg_angle) * seg_len;

            let follow_rate = 1.0 - (-dt * (16.0 - (i as f32) * 1.4)).exp();
            let blended = self.nodes[idx].lerp(desired_pos, follow_rate);
            let constrained_dir = (blended - self.nodes[idx - 1]).normalize_or_zero();
            self.nodes[idx] = self.nodes[idx - 1] + constrained_dir * seg_len;
            tail_angle = constrained_dir.y.atan2(constrained_dir.x);
        }
    }

    /// Picks the gait with hysteresis and slides each leg's phase toward its pattern. Offsets move
    /// at most half as far as the gait cycle advanced, so every leg's own phase keeps moving
    /// forward and a gait change only makes some steps briefly quicker or slower.
    fn update_gait_pattern(&mut self) {
        let advance = (self.gait_cycle - self.prev_gait_cycle).rem_euclid(1.0);
        self.prev_gait_cycle = self.gait_cycle;
        let speed = self.speed / self.scale;
        self.gait = match self.gait {
            _ if self.gallop_weight > 0.5 => Gait::Gallop,
            Gait::Gallop if self.gallop_weight > 0.35 => Gait::Gallop,
            Gait::Trot | Gait::Gallop if speed > 114.0 => Gait::Trot,
            _ if speed > 128.0 => Gait::Trot,
            _ => Gait::Walk,
        };

        let (target_offsets, target_swing) = self.gait.pattern();
        let max_shift = 0.5 * advance;
        for (offset, target) in self.leg_offsets.iter_mut().zip(target_offsets) {
            let diff = (target - *offset + 0.5).rem_euclid(1.0) - 0.5;
            *offset = (*offset + diff.clamp(-max_shift, max_shift)).rem_euclid(1.0);
        }
        self.swing_fraction += (target_swing - self.swing_fraction).clamp(-max_shift, max_shift);
    }

    /// Updates 4 world-planted paws following the current `Gait` pattern: 4-beat walk
    /// (`LH -> LF -> RH -> RF`), 2-beat diagonal trot (`LF+RH` vs `RF+LH`), or rotary gallop.
    pub(super) fn update_paws(&mut self, dt: f32) {
        let s = self.scale;
        self.update_gait_pattern();

        let shoulder_pos = self.nodes[2];
        let hip_pos = self.nodes[4];
        let fwd_sh = (self.nodes[1] - self.nodes[2]).normalize_or_zero();
        let side_sh = Vec2::new(-fwd_sh.y, fwd_sh.x);
        let fwd_hip = (self.nodes[3] - self.nodes[4]).normalize_or_zero();
        let side_hip = Vec2::new(-fwd_hip.y, fwd_hip.x);

        // Neutral girdle centers for [0: LF, 1: RF, 2: LH, 3: RH]: each paw sweeps symmetrically
        // about its socket, from touchdown ahead of it to lift-off behind it
        // Galloping forelegs reach out past the chin at touchdown
        let fore_fwd = Self::FORE_SOCKET_FWD + Self::GALLOP_FORE_REACH * self.gallop_weight;
        let sockets = [
            shoulder_pos + fwd_sh * (fore_fwd * s) + side_sh * (4.8 * s),
            shoulder_pos + fwd_sh * (fore_fwd * s) - side_sh * (4.8 * s),
            hip_pos + fwd_hip * (Self::HIND_SOCKET_FWD * s) + side_hip * (Self::HIND_TRACK * s),
            hip_pos + fwd_hip * (Self::HIND_SOCKET_FWD * s) - side_hip * (Self::HIND_TRACK * s),
        ];
        let forwards = [fwd_sh, fwd_sh, fwd_hip, fwd_hip];

        let swing_window = self.swing_fraction;
        // While a paw is planted the body travels `stance fraction × stride` over it, so landing half
        // that sweep ahead of the socket lets it stay put until lift-off instead of being dragged.
        let stride = self.stride_length();
        let half_sweep = 0.5 * (1.0 - swing_window) * stride;
        // Safety clamp sized for the longest stance of any gait, so a stance that started in a
        // slower gait isn't cut short while the swing fraction slides toward a faster one
        let mut max_reach =
            (1.0 - Gait::Walk.pattern().1) * stride * Self::HIND_LEAD_SHARE * 1.3 + 2.0 * s;

        // When spinning on the spot, each socket moves at its own speed and direction, so each paw
        // lands half a stance ahead along its own socket's path instead of straight ahead
        let spinning = self.activity.is_spinning();
        let half_stance_secs = if self.speed > 1.0 {
            0.5 * (1.0 - swing_window) * stride / self.speed
        } else {
            0.0
        };
        // Light smoothing only: a slower filter lags behind the turning direction of a fast spin
        let vel_rate = 1.0 - (-dt * 40.0).exp();
        let mut leads = [Vec2::ZERO; 4];
        for i in 0..4 {
            let paw = &mut self.paws[i];
            let vel = if paw.prev_socket == Vec2::ZERO {
                Vec2::ZERO
            } else {
                (sockets[i] - paw.prev_socket) / dt
            };
            paw.prev_socket = sockets[i];
            paw.socket_vel += (vel - paw.socket_vel) * vel_rate;
            leads[i] = if spinning {
                paw.socket_vel * half_stance_secs
            } else {
                // A hind paw lands well ahead under the belly and pushes off only a little behind
                // the haunch; a forepaw sweeps evenly about its shoulder
                let share = if i >= 2 { Self::HIND_LEAD_SHARE } else { 0.5 };
                forwards[i] * (2.0 * half_sweep * share)
            };
            if spinning {
                max_reach = max_reach.max(leads[i].length() * 1.3 + 2.0 * s);
            }
        }

        for i in 0..4 {
            let mut limb_phase = (self.gait_cycle - self.leg_offsets[i]).rem_euclid(1.0);
            let prev_phase = self.paws[i].limb_phase;
            let ideal_target = sockets[i] + leads[i];

            if self.gait_weight > 0.05 {
                if prev_phase < 0.0 {
                    // First step after standing: the gathered paw sits under its socket, which is
                    // mid-stance. A leg due to swing lifts off now (a short first step) and a leg in
                    // early stance counts as mid-stance; the offsets slide back to the pattern.
                    let mid_stance = swing_window + 0.5 * (1.0 - swing_window);
                    let entry_phase = if limb_phase < swing_window {
                        0.0
                    } else {
                        limb_phase.max(mid_stance)
                    };
                    self.leg_offsets[i] = (self.gait_cycle - entry_phase).rem_euclid(1.0);
                    limb_phase = entry_phase;
                }
                self.paws[i].limb_phase = limb_phase;
                // A swing only starts as the paw's own cycle wraps past lift-off (or right at the
                // first step) and runs until touchdown, so a gait change can't pop a planted paw
                // into a half-finished swing or cut a swing short.
                let swinging = self.paws[i].swing_progress > 0.0;
                let starts_swing = if prev_phase < 0.0 {
                    limb_phase == 0.0
                } else {
                    limb_phase < prev_phase
                };
                if (swinging || starts_swing) && limb_phase < swing_window {
                    // Swing phase (protraction): latch where the paw lifted off relative to its socket
                    if !swinging {
                        let delta = self.paws[i].world_pos - sockets[i];
                        self.paws[i].step_start = if delta.length() > max_reach {
                            delta.normalize() * max_reach
                        } else {
                            delta
                        };
                    }
                    let t = (limb_phase / swing_window).clamp(0.0, 1.0);
                    self.paws[i].swing_progress = t.max(1e-4);
                    self.paws[i].step_target = ideal_target;
                    // The start point rides along with the socket: the leg folds as it lifts, so the
                    // paw comes forward with the body at once instead of trailing behind it, and it
                    // still settles onto its world-space landing spot with zero speed
                    let s_curve = smoothstep(t);
                    let lift_point = sockets[i] + self.paws[i].step_start;
                    self.paws[i].world_pos = lift_point.lerp(self.paws[i].step_target, s_curve);
                } else {
                    // Stance phase (retraction): paw stays planted in world space while body glides over it
                    self.paws[i].swing_progress = 0.0;
                    if self.gait_weight < 0.35 {
                        let gather = (1.0 - self.gait_weight / 0.35) * 6.0;
                        self.paws[i].world_pos = self.paws[i]
                            .world_pos
                            .lerp(sockets[i], 1.0 - (-dt * gather).exp());
                    }
                }
                let delta = self.paws[i].world_pos - sockets[i];
                if delta.length() > max_reach {
                    self.paws[i].world_pos = sockets[i] + delta.normalize() * max_reach;
                }
            } else {
                self.paws[i].swing_progress = 0.0;
                self.paws[i].limb_phase = -1.0;
                self.paws[i].world_pos = self.paws[i]
                    .world_pos
                    .lerp(sockets[i], 1.0 - (-dt * 12.0).exp());
            }
        }
    }
}
