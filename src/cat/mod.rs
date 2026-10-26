mod behavior;
mod body;

use glam::Vec2;

/// Number of spine nodes from head (0) to tail base (5)
pub const BODY_NODES: usize = 6;
/// Number of tail segments after the tail base (6..12)
pub const TAIL_NODES: usize = 6;
pub const TOTAL_NODES: usize = BODY_NODES + TAIL_NODES;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Temperament {
    /// Balanced companion: natural mix of strolling, lounging, and occasional quirks
    Balanced,
    /// Playful & hyper: eager pounces, zoomies, tail-chasing, and brisker gait
    Playful,
    /// Lazy & sleepy: unhurried strolls, quick to flop and curl up for long naps
    Lazy,
    /// Aloof & independent: wider personal space, judges cursor movement before moving
    Aloof,
    /// True to life: real-cat pacing, with an unhurried ~0.65 m/s walk, long rests and rare quirks
    Realistic,
}

impl Temperament {
    pub const ALL: [Temperament; 5] = [
        Temperament::Balanced,
        Temperament::Playful,
        Temperament::Lazy,
        Temperament::Aloof,
        Temperament::Realistic,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Temperament::Balanced => "Balanced",
            Temperament::Playful => "Playful",
            Temperament::Lazy => "Lazy",
            Temperament::Aloof => "Aloof",
            Temperament::Realistic => "Realistic",
        }
    }

    pub const fn as_config_str(self) -> &'static str {
        match self {
            Temperament::Balanced => "balanced",
            Temperament::Playful => "playful",
            Temperament::Lazy => "lazy",
            Temperament::Aloof => "aloof",
            Temperament::Realistic => "realistic",
        }
    }

    pub fn from_config_str(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.as_config_str().eq_ignore_ascii_case(s))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BehaviorConfig {
    pub temperament: Temperament,
    /// Whether the cat walks/trots across the screen to follow the mouse cursor
    pub follow_cursor: bool,
    /// Whether playful hunting quirks (pounce, butt-wiggle, zoomies, tail-chase, paw-bat) occur
    pub playful_quirks: bool,
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            temperament: Temperament::Balanced,
            follow_cursor: true,
            playful_quirks: true,
        }
    }
}

/// Behaviors the tests can start on demand to set up a scenario
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Pounce,
    ChaseTail,
    Groom,
    BellyUp,
    Sleep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedPreset {
    Mins15,
    Mins30,
    Hour1,
    Hours2,
    Indefinite,
}

impl FeedPreset {
    pub const ALL: [FeedPreset; 5] = [
        FeedPreset::Mins15,
        FeedPreset::Mins30,
        FeedPreset::Hour1,
        FeedPreset::Hours2,
        FeedPreset::Indefinite,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            FeedPreset::Mins15 => "Feed 15m",
            FeedPreset::Mins30 => "Feed 30m",
            FeedPreset::Hour1 => "Feed 1h",
            FeedPreset::Hours2 => "Feed 2h",
            FeedPreset::Indefinite => "Feed Unlimited",
        }
    }

    pub const fn duration_secs(self) -> Option<f32> {
        match self {
            FeedPreset::Mins15 => Some(15.0 * 60.0),
            FeedPreset::Mins30 => Some(30.0 * 60.0),
            FeedPreset::Hour1 => Some(60.0 * 60.0),
            FeedPreset::Hours2 => Some(120.0 * 60.0),
            FeedPreset::Indefinite => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FeedingPhase {
    /// Bowl is attached to the cursor waiting for the user's left-click to place it on screen
    Carrying { preset: FeedPreset, bowl_pos: Vec2 },
    /// Bowl is placed at `bowl_pos`; cat is walking over to it before eating
    WalkingToBowl { preset: FeedPreset, bowl_pos: Vec2 },
    /// Cat has arrived at `bowl_pos` and is actively eating from it
    Eating { preset: FeedPreset, bowl_pos: Vec2 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Activity {
    /// Curled up in a tight ball asleep; breathing slowly with tail wrapped around
    Sleeping,
    /// Lying down on belly ("loaf" / sphinx rest)
    Lying,
    /// Lying on back (belly-up / supine) with soft belly exposed and all 4 paws in the air
    BellyUp {
        side: f32,
        timer: f32,
    },
    /// Dramatic side-flop with paws extended to one side
    DramaticFlop {
        side: f32,
        timer: f32,
    },
    Sitting,
    /// Aloof "judging" pause: turns ear/head toward moved cursor before deciding to get up
    AloofNotice {
        timer: f32,
        pause_duration: f32,
    },
    /// Sitting and grooming a front paw / face
    Grooming {
        side: f32,
        timer: f32,
        duration: f32,
    },
    /// Playfully swatting/batting at the nearby mouse cursor with one front paw ("tap-tap!")
    PawBatting {
        side: f32,
        timer: f32,
        duration: f32,
    },
    /// Spinning in circles trying to catch its own tail, then stopping dizzy
    TailChasing {
        center: Vec2,
        dir: f32,
        timer: f32,
        duration: f32,
    },
    /// Getting up and turning once or twice on the spot before curling up to sleep, as cats do
    /// when settling into a resting place (Realistic temperament)
    Circling {
        center: Vec2,
        dir: f32,
        timer: f32,
        turned: f32,
        total: f32,
    },
    /// Pre-pounce crouch with rhythmic hindquarter "butt wiggle" & airplane ears
    StalkWiggle {
        timer: f32,
        wiggle_duration: f32,
    },
    /// Explosive pounce leap toward the cursor target
    Pouncing {
        start_pos: Vec2,
        target_pos: Vec2,
        progress: f32,
    },
    /// Burst of playful FRAPs ("zoomies") orbiting near the cursor
    Zoomies {
        center: Vec2,
        dir: f32,
        timer: f32,
        duration: f32,
    },
    /// Anterior front-paw stretch when waking up or relaxing
    Stretching {
        progress: f32,
        after_walk: bool,
    },
    /// Rising from sit/lie/sleep before walking (speed is 0 until complete)
    StandingUp {
        progress: f32,
    },
    /// Leisurely 4-beat walk or 2-beat diagonal trot toward the smoothed cursor target
    Walking,
    /// Briefly standing after reaching the cursor before sitting down
    StandingIdle {
        timer: f32,
    },
    /// Eating from a food bowl in front of the cat while keeping the system/screen awake
    Eating {
        preset: FeedPreset,
        elapsed: f32,
        duration: Option<f32>,
    },
}

/// World-space articulated paw state for realistic 4-beat walk & 2-beat trot
#[derive(Debug, Clone, Copy)]
pub struct PawState {
    /// Current world position of the paw pad
    pub world_pos: Vec2,
    /// Where the current swing step started, as an offset from the paw's socket at lift-off
    pub step_start: Vec2,
    /// Target of current swing step
    pub step_target: Vec2,
    /// 0.0 = planted in stance, >0.0..1.0 = lifting & swinging forward
    pub swing_progress: f32,
    /// This paw's position in its own step cycle [0, 1) last frame (0 = lift-off), or -1.0 while
    /// the paws are gathered and not stepping
    pub limb_phase: f32,
    /// This paw's shoulder/hip socket last frame, and its smoothed velocity (px/s)
    pub prev_socket: Vec2,
    pub socket_vel: Vec2,
}

/// Footfall pattern the paws converge toward. Switching has hysteresis, and each leg's phase
/// slides to the new pattern over about a stride instead of jumping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gait {
    Walk,
    Trot,
    Gallop,
}

impl Gait {
    /// Lift-off phase of each paw `[LF, RF, LH, RH]` within the gait cycle, and the swing fraction.
    /// Each pattern's overall phase is chosen so moving between neighbouring gaits shifts every
    /// leg as little as possible.
    fn pattern(self) -> ([f32; 4], f32) {
        match self {
            // 4-beat lateral walk: LH (0.00) -> LF (0.25) -> RH (0.50) -> RF (0.75)
            Gait::Walk => ([0.25, 0.75, 0.0, 0.5], 0.34),
            // 2-beat diagonal trot: LF & RH together (0.375), RF & LH together (0.875)
            Gait::Trot => ([0.375, 0.875, 0.875, 0.375], 0.42),
            // Right-lead rotary gallop, by touchdown: RH (0.03) -> LH (0.13) -> LF (0.35) -> RF (0.45),
            // with a gathered flight (no paw down) from RF lift-off at 0.83 until RH lands again
            Gait::Gallop => ([0.73, 0.83, 0.51, 0.41], 0.62),
        }
    }
}

pub struct CatSim {
    pub scale: f32,
    pub coat: CoatStyle,
    pub activity: Activity,
    pub behaviors: BehaviorConfig,
    pub feeding_phase: Option<FeedingPhase>,

    /// Raw cursor position in screen coordinates
    pub raw_cursor: Vec2,
    /// Previous meaningful cursor step direction, used to distinguish back-and-forth wiggles from point-to-point moves
    last_cursor_dir: Vec2,
    /// Instantaneous cursor velocity (px/s) to detect laser-pointer wiggles
    pub cursor_speed: f32,
    /// Accumulated "play/wiggle" excitement from rapid back-and-forth cursor oscillations
    pub play_excitement: f32,
    /// Last significant horizontal cursor step sign (-1.0 left, +1.0 right) for shake detection
    shake_dir_x: f32,
    /// Accumulated horizontal distance in the current left/right stroke
    shake_stroke_dx: f32,
    /// Peak horizontal speed (px/s) observed in the current stroke
    shake_stroke_peak_vx: f32,
    /// Time elapsed since the last fast horizontal cursor step
    shake_idle_timer: f32,
    /// Accumulated energy from rapid left-right cursor shakes
    pub shake_energy: f32,
    /// Whether the cat is actively running/sprinting toward the cursor after a left-right shake
    pub sprinting: bool,
    /// Low-pass smoothed cursor target so rapid mouse jitter is ignored
    pub smooth_cursor: Vec2,
    /// Target resting spot near the cursor (offset so cat doesn't cover pointer)
    pub rest_target: Vec2,

    /// World positions of spine nodes [0..TOTAL_NODES]
    /// 0 = Head center, 1 = Neck, 2 = Shoulders, 3 = Waist, 4 = Hips, 5 = TailBase, 6..11 = Tail
    pub nodes: [Vec2; TOTAL_NODES],
    /// 4 paws: [0: Left Fore, 1: Right Fore, 2: Left Hind, 3: Right Hind]
    pub paws: [PawState; 4],

    /// Forward heading angle (radians) of the body/shoulders
    pub body_angle: f32,
    /// Angular velocity of body turn (rad/s), used for tail counterbalance
    pub angular_vel: f32,
    /// Head yaw offset relative to body_angle (clamped to realistic neck range)
    pub head_yaw: f32,
    /// Current forward linear speed (px/s)
    pub speed: f32,
    /// Continuous gait cycle phase in [0, 1)
    pub gait_cycle: f32,
    /// Blend factor: 0.0 = stationary, 1.0 = full stride
    pub gait_weight: f32,
    /// Blend factor for sagittal bounding gallop (0.0 = walk/trot, 1.0 = full rotary gallop)
    pub gallop_weight: f32,
    /// Footfall pattern currently in use (see `Gait`)
    pub gait: Gait,
    /// Per-paw lift-off phases `[LF, RF, LH, RH]`, sliding toward `gait`'s pattern
    leg_offsets: [f32; 4],
    /// Fraction of each paw's cycle spent swinging, sliding toward `gait`'s pattern
    swing_fraction: f32,
    /// `gait_cycle` at the previous paw update, to measure how far the cycle advanced
    prev_gait_cycle: f32,

    /// Posture dimensions blend:
    /// 1.12 = curled sleep, 1.16 = sitting/compact, 1.60 = full standing/walking
    pub posture_extension: f32,
    /// Curl curvature bias (radians per spine segment, non-zero when sleeping/curling/chasing tail)
    pub curl_amount: f32,
    /// Which side (+1.0 or -1.0) the cat prefers to curl/tuck tail toward
    pub curl_side: f32,
    /// Lateral butt-wiggle offset at hips (used during StalkWiggle)
    pub hip_wiggle_offset: f32,
    /// Tail puffiness multiplier (1.0 = sleek normal, 1.65 = zoomies bottlebrush tail)
    pub tail_puff: f32,
    /// Smooth 0.0..=1.0 roll progress onto the back for `Activity::BellyUp` transitions
    pub belly_roll: f32,
    /// Which side (+1.0 or -1.0) the cat rolls through when entering/exiting `Activity::BellyUp`
    pub belly_roll_side: f32,
    /// Smooth 0.0..=1.0 blend factor for curled sleeping tail compaction
    pub sleep_blend: f32,

    /// Independent ear angles (radians: negative = flattened "airplane ears", positive = perked forward)
    pub ear_left_twitch: f32,
    pub ear_right_twitch: f32,

    /// Continuous simulation clock (seconds)
    pub time: f32,
    /// Continuous phase for breathing animation so frequency changes never jump phase
    pub breath_phase: f32,
    /// Continuous phase for tail noise so sway_speed changes never jump phase at high uptime
    pub tail_phase: f32,
    /// Smoothed tail wrap bias and sway amplitude across activity transitions
    tail_wrap_smooth: f32,
    tail_amp_smooth: f32,
    /// 0..1: how far the tail has wrapped around the front paws while sitting
    sit_tail_wrap: f32,
    /// Whether a sitting cat is shuffling round to face a cursor behind it
    sit_turning: bool,
    /// Time spent idle near the cursor
    pub idle_time: f32,
    /// Countdown until next spontaneous cat quirk
    pub next_quirk_timer: f32,
    /// Deterministic pseudo-random state for non-repeating behavior choices
    rng_state: u64,
    /// Whether the active StalkWiggle was explicitly requested via `trigger(TriggerKind::Pounce)`
    manual_pounce: bool,
    /// Recompute explicit pounce target on the next `update` once `raw_cursor` reflects current cursor
    latch_pounce_target: bool,
    /// Cursor baseline latched when an explicit rest/quirk command is triggered
    wake_anchor: Option<Vec2>,
    /// Snap `smooth_cursor` and `wake_anchor` on the first `update` following an explicit rest trigger
    latch_wake_anchor: bool,
    /// Whether the current `BellyUp` pose was explicitly pinned via `trigger(TriggerKind::BellyUp)`
    pinned_rest: bool,
    /// Pure path-following carrier spine (without transient lateral gait oscillation)
    carrier_nodes: [Vec2; BODY_NODES],
    /// Transient lateral head-bob offset applied on the last spine solve step
    prev_head_offset: Vec2,
    /// Stable screen-space torso anchor for positioning the overlay window without jitter
    window_anchor: Vec2,
}

impl CatSim {
    /// Rest lengths between consecutive body nodes (0..5) in unscaled standing pose.
    /// Total body length (head center to tail base) is ~38px at scale 1.0.
    const STANDING_BODY_LENGTHS: [f32; BODY_NODES - 1] = [6.2, 7.6, 8.8, 8.2, 6.8];
    /// Distance from the waist node to the food bowl centre in the eating pose (unscaled)
    pub const BOWL_REACH: f32 = 27.2;
    /// Rest lengths for tail segments (5..11) in unscaled pose (~24px total).
    const TAIL_LENGTHS: [f32; TAIL_NODES] = [4.5, 4.3, 4.1, 3.8, 3.5, 3.2];
    /// Extra bend per tail segment (rad) when fully wrapped while sitting: turn sideways off the
    /// rump, run forward along the flank, then hook in toward the front paws
    const SIT_TAIL_WRAP: [f32; TAIL_NODES] = [0.78, 0.74, 0.36, 0.24, 0.22, 0.40];

    pub fn new(start_pos: Vec2, scale: f32, coat: CoatStyle) -> Self {
        let initial_offset = Vec2::new(-42.0 * scale, 24.0 * scale);
        let head_pos = start_pos + initial_offset;
        let mut nodes = [head_pos; TOTAL_NODES];

        for i in 1..BODY_NODES {
            let len = Self::STANDING_BODY_LENGTHS[i - 1] * scale;
            nodes[i] = nodes[i - 1] + Vec2::new(-len, 0.0);
        }
        for i in 0..TAIL_NODES {
            let idx = BODY_NODES + i;
            let len = Self::TAIL_LENGTHS[i] * scale;
            nodes[idx] = nodes[idx - 1] + Vec2::new(-len, 0.0);
        }
        let mut carrier_nodes = [head_pos; BODY_NODES];
        carrier_nodes.copy_from_slice(&nodes[..BODY_NODES]);
        // Paw order: [LF, RF, LH, RH]
        let paws = [
            PawState {
                world_pos: nodes[2] + Vec2::new(3.0, 5.5) * scale,
                step_start: nodes[2],
                step_target: nodes[2],
                swing_progress: 0.0,
                limb_phase: -1.0,
                prev_socket: Vec2::ZERO,
                socket_vel: Vec2::ZERO,
            },
            PawState {
                world_pos: nodes[2] + Vec2::new(3.0, -5.5) * scale,
                step_start: nodes[2],
                step_target: nodes[2],
                swing_progress: 0.0,
                limb_phase: -1.0,
                prev_socket: Vec2::ZERO,
                socket_vel: Vec2::ZERO,
            },
            PawState {
                world_pos: nodes[4] + Vec2::new(-1.0, 5.2) * scale,
                step_start: nodes[4],
                step_target: nodes[4],
                swing_progress: 0.0,
                limb_phase: -1.0,
                prev_socket: Vec2::ZERO,
                socket_vel: Vec2::ZERO,
            },
            PawState {
                world_pos: nodes[4] + Vec2::new(-1.0, -5.2) * scale,
                step_start: nodes[4],
                step_target: nodes[4],
                swing_progress: 0.0,
                limb_phase: -1.0,
                prev_socket: Vec2::ZERO,
                socket_vel: Vec2::ZERO,
            },
        ];

        Self {
            scale,
            coat,
            activity: Activity::Sitting,
            behaviors: BehaviorConfig::default(),
            feeding_phase: None,
            raw_cursor: start_pos,
            last_cursor_dir: Vec2::ZERO,
            cursor_speed: 0.0,
            play_excitement: 0.0,
            shake_dir_x: 0.0,
            shake_stroke_dx: 0.0,
            shake_stroke_peak_vx: 0.0,
            shake_idle_timer: 0.0,
            shake_energy: 0.0,
            sprinting: false,
            smooth_cursor: start_pos,
            rest_target: head_pos,
            nodes,
            paws,
            body_angle: 0.0,
            angular_vel: 0.0,
            head_yaw: 0.0,
            speed: 0.0,
            gait_cycle: 0.0,
            gait_weight: 0.0,
            gallop_weight: 0.0,
            gait: Gait::Walk,
            leg_offsets: Gait::Walk.pattern().0,
            swing_fraction: Gait::Walk.pattern().1,
            prev_gait_cycle: 0.0,
            posture_extension: 1.16,
            curl_amount: 0.0,
            curl_side: 1.0,
            hip_wiggle_offset: 0.0,
            tail_puff: 1.0,
            belly_roll: 0.0,
            belly_roll_side: 1.0,
            sleep_blend: 0.0,
            ear_left_twitch: 0.0,
            ear_right_twitch: 0.0,
            time: 0.0,
            breath_phase: 0.0,
            tail_phase: 0.0,
            tail_wrap_smooth: -0.28,
            tail_amp_smooth: 0.18,
            sit_tail_wrap: 0.0,
            sit_turning: false,
            idle_time: 2.5,
            next_quirk_timer: 3.2,
            rng_state: 0x853c_49e6_748f_ea9b,
            manual_pounce: false,
            latch_pounce_target: false,
            wake_anchor: None,
            latch_wake_anchor: false,
            pinned_rest: false,
            carrier_nodes,
            prev_head_offset: Vec2::ZERO,
            window_anchor: carrier_nodes[3],
        }
    }

    /// Dynamically rescales the cat in-place without teleporting or kinking the spine
    pub fn set_scale(&mut self, new_scale: f32) {
        let clamped = new_scale.clamp(0.5, 2.5);
        if (clamped - self.scale).abs() < 1e-4 {
            return;
        }
        let ratio = clamped / self.scale;
        let anchor = self.nodes[0];
        for node in &mut self.nodes[1..] {
            *node = anchor + (*node - anchor) * ratio;
        }
        for node in &mut self.carrier_nodes {
            *node = anchor + (*node - anchor) * ratio;
        }
        self.prev_head_offset *= ratio;
        self.window_anchor = anchor + (self.window_anchor - anchor) * ratio;
        for paw in &mut self.paws {
            paw.world_pos = anchor + (paw.world_pos - anchor) * ratio;
            paw.step_start *= ratio;
            paw.step_target = anchor + (paw.step_target - anchor) * ratio;
        }
        self.scale = clamped;
    }

    /// Immediately starts a specific cat behavior/animation (test scenario setup)
    #[cfg(test)]
    pub fn trigger(&mut self, kind: TriggerKind) {
        let side = self.rand_side();
        self.idle_time = 0.0;
        self.reset_sprint();
        self.pinned_rest = matches!(kind, TriggerKind::BellyUp);
        self.feeding_phase = None;
        if kind == TriggerKind::Pounce {
            self.manual_pounce = true;
            self.latch_pounce_target = true;
            self.wake_anchor = None;
            self.latch_wake_anchor = false;
            self.aim_pounce_target();
        } else {
            self.manual_pounce = false;
            self.latch_pounce_target = false;
            self.settle_in_place();
        }
        if !matches!(kind, TriggerKind::Pounce | TriggerKind::ChaseTail) {
            self.speed = 0.0;
            self.gait_weight = 0.0;
        }
        if matches!(kind, TriggerKind::BellyUp | TriggerKind::Sleep) {
            self.curl_side = side;
        }
        self.activity = match kind {
            TriggerKind::Pounce => Activity::StalkWiggle {
                timer: 0.0,
                wiggle_duration: 0.55,
            },
            TriggerKind::ChaseTail => Activity::TailChasing {
                center: self.window_anchor,
                dir: side,
                timer: 0.0,
                duration: 2.0,
            },
            TriggerKind::Groom => Activity::Grooming {
                side,
                timer: 0.0,
                duration: 3.4,
            },
            TriggerKind::BellyUp => Activity::BellyUp { side, timer: 0.0 },
            TriggerKind::Sleep => Activity::Sleeping,
        };
    }

    /// Clears sprint state and any in-progress left/right shake stroke
    fn reset_sprint(&mut self) {
        self.sprinting = false;
        self.shake_energy = 0.0;
        self.gallop_weight = 0.0;
        self.shake_dir_x = 0.0;
        self.shake_stroke_dx = 0.0;
        self.shake_stroke_peak_vx = 0.0;
        self.shake_idle_timer = 0.0;
    }

    /// Pins the cat where it is and latches the current cursor as the baseline it must move away from to wake it
    fn settle_in_place(&mut self) {
        self.smooth_cursor = self.raw_cursor;
        self.wake_anchor = Some(self.raw_cursor);
        self.latch_wake_anchor = true;
        self.rest_target = self.nodes[0];
    }

    /// Aims the pounce leap at the cursor, or straight ahead when the cursor is too close to leap at
    /// Longest pounce (unscaled px). Realistic hunting pounces are short, about two body lengths;
    /// a cat with farther to go leaps short and walks the rest.
    fn max_pounce_leap(&self) -> f32 {
        if self.behaviors.temperament == Temperament::Realistic {
            90.0
        } else {
            180.0
        }
    }

    /// How long the cat crouches, staring and then wiggling, before it leaps. Real cats hold the
    /// crouch for a second or more; the other temperaments keep it short and snappy.
    fn crouch_duration(&mut self) -> f32 {
        if self.behaviors.temperament == Temperament::Realistic {
            self.rand_range(1.4, 2.4)
        } else {
            self.rand_range(0.6, 0.85)
        }
    }

    /// Keeps a pounce landing within `max_pounce_leap` of where the leap started
    fn clamp_pounce_target(&self, start: Vec2, target: Vec2) -> Vec2 {
        let max_leap = self.max_pounce_leap() * self.scale;
        let leap = target - start;
        if leap.length() > max_leap {
            start + leap.normalize() * max_leap
        } else {
            target
        }
    }

    fn aim_pounce_target(&mut self) {
        let to_cursor = self.raw_cursor - self.nodes[0];
        let dist = to_cursor.length();
        let stop_r = 32.0 * self.scale;
        self.rest_target = if dist > stop_r * 1.6 {
            let leap_dist = (dist - stop_r).min(self.max_pounce_leap() * self.scale);
            self.nodes[0] + (to_cursor / dist) * leap_dist
        } else {
            self.nodes[0] + Vec2::from_angle(self.body_angle) * (95.0 * self.scale)
        };
    }

    /// Starts interactive bowl placement: attaches the food bowl to the cursor until left-click places it
    pub fn begin_bowl_placement(&mut self, preset: FeedPreset, cursor_pos: Vec2) {
        self.idle_time = 0.0;
        self.manual_pounce = false;
        self.latch_pounce_target = false;
        self.wake_anchor = None;
        self.latch_wake_anchor = false;
        self.pinned_rest = false;
        self.raw_cursor = cursor_pos;
        self.smooth_cursor = cursor_pos;
        self.feeding_phase = Some(FeedingPhase::Carrying {
            preset,
            bowl_pos: cursor_pos,
        });
        if matches!(
            self.activity,
            Activity::Sleeping
                | Activity::Lying
                | Activity::BellyUp { .. }
                | Activity::DramaticFlop { .. }
                | Activity::Sitting
                | Activity::Eating { .. }
        ) {
            self.activity = Activity::StandingUp { progress: 0.25 };
        }
    }

    /// Places the carried food bowl at `bowl_pos`; the cat walks up to `bowl_pos` and begins eating
    pub fn place_bowl(&mut self, bowl_pos: Vec2) {
        let preset = match self.feeding_phase {
            Some(
                FeedingPhase::Carrying { preset, .. }
                | FeedingPhase::WalkingToBowl { preset, .. }
                | FeedingPhase::Eating { preset, .. },
            ) => preset,
            None => FeedPreset::Mins30,
        };
        let to_bowl = bowl_pos - self.nodes[0];
        let dist = to_bowl.length();
        if dist <= 18.0 * self.scale {
            if dist > 1.5 {
                self.body_angle = to_bowl.y.atan2(to_bowl.x);
            }
            self.enter_eating_at(preset, bowl_pos);
        } else {
            self.wake_anchor = None;
            self.latch_wake_anchor = false;
            self.pinned_rest = false;
            self.manual_pounce = false;
            self.feeding_phase = Some(FeedingPhase::WalkingToBowl { preset, bowl_pos });
            if !matches!(
                self.activity,
                Activity::Walking | Activity::StandingUp { .. }
            ) {
                self.activity = Activity::StandingUp { progress: 0.35 };
            }
        }
    }

    fn enter_eating_at(&mut self, preset: FeedPreset, bowl_pos: Vec2) {
        self.idle_time = 0.0;
        self.reset_sprint();
        self.play_excitement = 0.0;
        self.manual_pounce = false;
        self.latch_pounce_target = false;
        self.pinned_rest = false;
        self.hip_wiggle_offset = 0.0;
        self.settle_in_place();
        self.speed = 0.0;
        self.gait_weight = 0.0;
        self.feeding_phase = Some(FeedingPhase::Eating { preset, bowl_pos });
        self.activity = Activity::Eating {
            preset,
            elapsed: 0.0,
            duration: preset.duration_secs(),
        };
    }

    /// Places a food bowl immediately in front of the cat and begins eating for the selected preset duration
    pub fn start_feeding(&mut self, preset: FeedPreset) {
        let bowl_pos =
            self.nodes[3] + Vec2::from_angle(self.body_angle) * (Self::BOWL_REACH * self.scale);
        self.enter_eating_at(preset, bowl_pos);
    }

    /// Removes the food bowl and transitions the cat into a brief post-meal grooming before resuming mouse follow
    pub fn stop_feeding(&mut self) {
        self.feeding_phase = None;
        if matches!(self.activity, Activity::Eating { .. }) {
            let side = self.rand_side();
            self.settle_in_place();
            self.speed = 0.0;
            self.gait_weight = 0.0;
            self.activity = Activity::Grooming {
                side,
                timer: 0.0,
                duration: 2.6,
            };
        }
    }

    pub fn is_feeding(&self) -> bool {
        matches!(self.activity, Activity::Eating { .. })
    }

    /// Returns the fixed screen position of the bowl the cat is currently eating from
    pub fn eating_bowl_pos(&self) -> Option<Vec2> {
        match self.feeding_phase {
            Some(FeedingPhase::Eating { bowl_pos, .. }) if self.is_feeding() => Some(bowl_pos),
            _ => None,
        }
    }

    pub fn is_carrying_bowl(&self) -> bool {
        matches!(self.feeding_phase, Some(FeedingPhase::Carrying { .. }))
    }

    /// Returns the world position of the bowl when it should be rendered in its standalone overlay window
    /// (while attached to the cursor or sitting on the floor waiting for the cat to walk up to it).
    pub fn detached_bowl_pos(&self) -> Option<Vec2> {
        match self.feeding_phase {
            Some(
                FeedingPhase::Carrying { bowl_pos, .. }
                | FeedingPhase::WalkingToBowl { bowl_pos, .. },
            ) => Some(bowl_pos),
            _ => None,
        }
    }

    /// Whether keep-awake should be active: the bowl is placed and the cat is walking to it or eating.
    pub fn keeps_awake(&self) -> bool {
        matches!(
            self.feeding_phase,
            Some(FeedingPhase::WalkingToBowl { .. } | FeedingPhase::Eating { .. })
        )
    }

    pub fn feeding_preset(&self) -> Option<FeedPreset> {
        match self.feeding_phase {
            Some(
                FeedingPhase::Carrying { preset, .. }
                | FeedingPhase::WalkingToBowl { preset, .. }
                | FeedingPhase::Eating { preset, .. },
            ) => Some(preset),
            None => None,
        }
    }

    /// Returns the food fill fraction in `[0.0, 1.0]` while the cat is eating (`None` when not eating)
    pub fn feeding_fill_ratio(&self) -> Option<f32> {
        match self.activity {
            Activity::Eating {
                elapsed,
                duration: Some(total),
                ..
            } if total > 0.0 => Some((1.0 - elapsed / total).clamp(0.0, 1.0)),
            Activity::Eating { duration: None, .. } => Some(1.0),
            _ => None,
        }
    }

    /// Returns remaining whole seconds for a timed feeding session (`None` if not feeding or unlimited)
    pub fn feeding_remaining_secs(&self) -> Option<u32> {
        if let Activity::Eating {
            elapsed,
            duration: Some(total),
            ..
        } = self.activity
        {
            Some((total - elapsed).max(0.0).ceil() as u32)
        } else if let Some(
            FeedingPhase::Carrying { preset, .. } | FeedingPhase::WalkingToBowl { preset, .. },
        ) = self.feeding_phase
        {
            preset.duration_secs().map(|s| s.ceil() as u32)
        } else {
            None
        }
    }

    #[inline]
    fn rand_u32(&mut self) -> u32 {
        let old = self.rng_state;
        self.rng_state = old
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    #[inline]
    fn rand_f32(&mut self) -> f32 {
        (self.rand_u32() as f64 / (u32::MAX as f64)) as f32
    }

    #[inline]
    fn rand_range(&mut self, min: f32, max: f32) -> f32 {
        min + (max - min) * self.rand_f32()
    }

    /// Random side/direction sign: +1.0 or -1.0
    #[inline]
    fn rand_side(&mut self) -> f32 {
        if self.rand_f32() > 0.5 { 1.0 } else { -1.0 }
    }

    /// Smooth 1D gradient noise for non-repeating organic motions
    pub fn noise1d(x: f32, seed: u32) -> f32 {
        let xi = x.floor() as i32;
        let xf = x - x.floor();
        let u = xf * xf * xf * (xf * (xf * 6.0 - 15.0) + 10.0);

        let grad = |n: i32| -> f32 {
            let mut h = (n as u32).wrapping_mul(0x27d4_eb2d) ^ seed;
            h = (h ^ 61) ^ (h >> 16);
            h = h.wrapping_add(h << 3);
            h ^= h >> 4;
            h = h.wrapping_mul(0x27d4_eb2d);
            h ^= h >> 15;
            ((h & 0xffff) as f32 / 32768.0) - 1.0
        };

        let g0 = grad(xi) * xf;
        let g1 = grad(xi + 1) * (xf - 1.0);
        (g0 + u * (g1 - g0)) * 1.85
    }

    /// Layered FBM noise for fluid, never-repeating tail/head/ear motion
    pub fn fbm(t: f32, seed: u32) -> f32 {
        0.60 * Self::noise1d(t, seed)
            + 0.28 * Self::noise1d(t * 2.17 + 13.7, seed.wrapping_add(101))
            + 0.12 * Self::noise1d(t * 4.53 + 7.1, seed.wrapping_add(202))
    }

    /// Forward offsets (unscaled px) of the shoulder and hip paw sockets from their spine nodes
    const FORE_SOCKET_FWD: f32 = 4.0;
    const HIND_SOCKET_FWD: f32 = -1.0;
    /// Share of a hind paw's stance sweep that lies ahead of its socket (the rest is behind it)
    const HIND_LEAD_SHARE: f32 = 0.62;
    /// Sideways offset (unscaled px) of the hind paws from the midline: cats walk with a narrow
    /// track, hind feet tucked under the haunches
    const HIND_TRACK: f32 = 3.4;
    /// Extra forward shift (unscaled px) of the fore sockets at full gallop
    const GALLOP_FORE_REACH: f32 = 7.0;
    /// Gait-cycle phase lead of the gallop spine flex wave over the footfalls (see `update`)
    const GALLOP_FLEX_LEAD: f32 = 0.05;
    /// Turning rate (rad/s) while circling before sleep: about one turn every 2.5 s
    const CIRCLING_SPIN: f32 = 2.5;
    /// How fast (rad/s) a sitting cat shuffles round to face a cursor behind it
    const SIT_TURN_RATE: f32 = 1.8;
    /// How far (rad) the head turns to either side of the body
    const MAX_HEAD_YAW: f32 = 1.4;
    /// Butt-wiggle rate (rad/s, ~3 Hz): visible weight shifts between the hind paws, not a vibration
    const WIGGLE_RATE: f32 = 19.0;
    /// Most the body turns (rad) during one stride when spinning on the spot
    const MAX_SPIN_PER_STRIDE: f32 = 1.6;
    /// Highest stride frequency (strides/s) when moving forward; faster speeds lengthen the stride
    /// instead
    const MAX_CADENCE: f32 = 4.2;

    /// Stable torso anchor in screen space for positioning the compact ARGB overlay window
    /// without jittering when the tail sways, hips wiggle, or posture curls/extends.
    pub fn window_center(&self) -> Vec2 {
        self.window_anchor
    }
}

#[inline]
pub fn wrap_angle(mut a: f32) -> f32 {
    while a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    }
    while a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}

/// Fraction of a pounce's distance covered at `progress` (0..1). The hind legs push off hard over
/// the first fifth, the cat flies at a steady speed, and it brakes over the last quarter as the
/// forepaws land: a trapezoidal speed profile, unlike a smoothstep's slow ease-in.
pub fn pounce_travel(progress: f32) -> f32 {
    const PUSH: f32 = 0.2;
    const LAND: f32 = 0.25;
    let p = progress.clamp(0.0, 1.0);
    let top_speed = 1.0 / (1.0 - 0.5 * PUSH - 0.5 * LAND);
    if p < PUSH {
        top_speed * p * p / (2.0 * PUSH)
    } else if p < 1.0 - LAND {
        top_speed * (0.5 * PUSH + (p - PUSH))
    } else {
        let left = 1.0 - p;
        1.0 - top_speed * left * left / (2.0 * LAND)
    }
}

/// How stretched out the cat is during a pounce (0 = crouched, 1 = fully extended): it extends
/// during push-off, stays long in the air and gathers again as it lands.
pub fn pounce_stretch(progress: f32) -> f32 {
    smoothstep(progress / 0.2) * (1.0 - smoothstep((progress - 0.75) / 0.25))
}

#[inline]
pub fn smoothstep(t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

#[cfg(test)]
mod tests;
