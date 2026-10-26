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

    fn update_head_and_ears(
        &mut self,
        angle_to_cursor: f32,
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
                // Intently watches the cursor while batting at it, then glances away
                let rel = wrap_angle(angle_to_cursor - self.body_angle).clamp(-0.6, 0.6);
                rel * 0.7 + (timer * 14.0).sin() * 0.06 * side
            }
            Activity::TailChasing { dir, .. } => {
                // Head craned all the way toward its own tail!
                dir * 0.78
            }
            // Looking down and into the circle at the spot it is about to lie on
            Activity::Circling { dir, .. } => dir * 0.3,
            Activity::StalkWiggle { .. } | Activity::Pouncing { .. } => {
                // Dead-locked gaze on the target
                wrap_angle(angle_to_cursor - self.body_angle).clamp(-0.65, 0.65)
            }
            Activity::Stretching { progress, .. } => {
                (progress * std::f32::consts::TAU).sin() * 0.06
            }
            _ => {
                let rel_cursor = wrap_angle(angle_to_cursor - self.body_angle);
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
                let raw_yaw = rel_cursor * cursor_attention + wander * (1.0 - cursor_attention);
                // Cats turn their heads up to ~80° to either side
                raw_yaw.clamp(-Self::MAX_HEAD_YAW, Self::MAX_HEAD_YAW)
            }
        };

        let yaw_rate = 1.0 - (-dt * 8.0).exp();
        self.head_yaw += wrap_angle(desired_yaw - self.head_yaw) * yaw_rate;
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

    /// Standing spine extension, plus the gallop's flex wave blended in by `gallop_weight`
    fn gallop_extension(&self) -> f32 {
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
    fn stride_length(&self) -> f32 {
        let v = self.speed / self.scale;
        let walk = 30.0 + 0.155 * v;
        let gallop = 40.0 + 0.16 * v;
        let stride = (walk + (gallop - walk) * self.gallop_weight) * self.scale;
        let step_scale = 0.5 + 0.5 * self.gait_weight.clamp(0.0, 1.0);
        let mut stride = (stride * step_scale).max(self.speed / Self::MAX_CADENCE);
        if matches!(
            self.activity,
            Activity::TailChasing { .. } | Activity::Circling { .. }
        ) && self.angular_vel.abs() > 0.1
        {
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

    fn solve_spine_and_tail(&mut self, dt: f32) {
        let len_scale = (self.posture_extension / 1.60) * self.scale;
        // Spine sway, head bob and tail wave are timed to the left hind paw's own step cycle, so
        // they stay in step with the footfalls in every gait and through gait changes
        let phase_rad = (self.gait_cycle - self.leg_offsets[2]) * std::f32::consts::TAU;
        let prev_carrier_waist = self.carrier_nodes[3];

        if matches!(
            self.activity,
            Activity::Walking
                | Activity::Pouncing { .. }
                | Activity::Zoomies { .. }
                | Activity::TailChasing { .. }
                | Activity::Circling { .. }
        ) {
            let (sway_weight, align_rate) = match self.activity {
                Activity::Walking => {
                    let speed_norm = (self.speed / (78.0 * self.scale)).clamp(0.0, 1.0);
                    let speed_env = smoothstep(speed_norm);
                    let trot_blend = ((self.speed / self.scale - 115.0) / 35.0).clamp(0.0, 1.0);
                    let gallop_suppress = 1.0 - 0.85 * self.gallop_weight;
                    let sw = self.gait_weight.min(speed_env)
                        * (1.0 - 0.22 * trot_blend)
                        * gallop_suppress;
                    (sw, 1.5 + speed_norm * 6.0 + self.gallop_weight * 6.0)
                }
                Activity::Zoomies { .. } => (
                    self.gait_weight * 0.70 * (1.0 - 0.85 * self.gallop_weight),
                    7.0,
                ),
                Activity::TailChasing { .. } | Activity::Circling { .. } => {
                    (self.gait_weight * 0.40, 0.0)
                }
                _ => (0.0, 7.5),
            };

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
                let align_decay = if matches!(
                    self.activity,
                    Activity::TailChasing { .. } | Activity::Circling { .. }
                ) {
                    1.0
                } else {
                    (-dt * align_rate).exp()
                };
                let raw_diff = wrap_angle(seg_angle - prev_carrier_angle) * align_decay
                    + self.curl_amount * 0.35;
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
            let head_bob = if matches!(
                self.activity,
                Activity::TailChasing { .. } | Activity::Circling { .. }
            ) {
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
        } else if matches!(self.activity, Activity::StandingIdle { .. }) {
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
        } else {
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

        if matches!(
            self.activity,
            Activity::Walking | Activity::Pouncing { .. } | Activity::Zoomies { .. }
        ) {
            let waist_step = self.carrier_nodes[3] - prev_carrier_waist;
            self.window_anchor += waist_step;
            let sync_rate = 1.0 - (-dt * 10.0).exp();
            self.window_anchor = self.window_anchor.lerp(self.carrier_nodes[3], sync_rate);
        }

        // Tail kinematics (nodes 6..12) with dynamic turn counterbalance
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
            | Activity::Eating { .. } => (0.18, 1.15, -self.curl_side * 0.28),
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
            Activity::Sitting | Activity::Grooming { .. } | Activity::AloofNotice { .. }
        ) && !self.sit_turning;
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
            let seg_len = Self::TAIL_LENGTHS[i] * tail_seg_scale;
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
            let target_seg_angle = wrap_angle(tail_angle + wave + tip_hook + sit_wrap);
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
    fn update_paws(&mut self, dt: f32) {
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
        let spinning = matches!(
            self.activity,
            Activity::TailChasing { .. } | Activity::Circling { .. }
        );
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
mod tests {
    use super::*;

    #[test]
    fn stands_up_before_walking_and_settles_at_target() {
        let start = Vec2::new(500.0, 500.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        assert_eq!(cat.activity, Activity::Sitting);

        // 1. Small cursor nudge inside wake radius: cat stays Sitting and turns head
        let nudge = start + Vec2::new(10.0, -6.0);
        for _ in 0..15 {
            cat.update(nudge, 1.0 / 60.0);
        }
        assert!(matches!(
            cat.activity,
            Activity::Sitting | Activity::PawBatting { .. }
        ));
        assert_eq!(cat.speed, 0.0);

        // 2. Large cursor move from A to B (350px): cat enters StandingUp with zero speed
        let point_b = Vec2::new(900.0, 350.0);
        for _ in 0..10 {
            cat.update(point_b, 1.0 / 60.0);
        }
        assert!(
            matches!(cat.activity, Activity::StandingUp { .. }),
            "Expected StandingUp before locomotion, got {:?}",
            cat.activity
        );
        assert!(cat.speed < 0.01, "Cat should not move while standing up");

        // 3. After ~0.6s of standing up, cat transitions to Walking
        for _ in 0..45 {
            cat.update(point_b, 1.0 / 60.0);
        }
        assert_eq!(cat.activity, Activity::Walking);
        assert!(cat.speed > 10.0);

        // 4. Mid-walk target change to point C: cat continues Walking smoothly along curve
        let point_c = Vec2::new(750.0, 650.0);
        for _ in 0..35 {
            cat.update(point_c, 1.0 / 60.0);
        }
        assert_eq!(cat.activity, Activity::Walking);

        // 5. Let cat arrive at point C and verify it settles into relaxing postures
        for _ in 0..(60 * 7) {
            cat.update(point_c, 1.0 / 60.0);
        }
        assert!(
            matches!(
                cat.activity,
                Activity::Sitting
                    | Activity::Grooming { .. }
                    | Activity::PawBatting { .. }
                    | Activity::TailChasing { .. }
            ),
            "Expected relaxing sitting behavior after arrival, got {:?}",
            cat.activity
        );

        for _ in 0..(60 * 14) {
            cat.update(point_c, 1.0 / 60.0);
        }
        assert!(
            matches!(
                cat.activity,
                Activity::Lying
                    | Activity::DramaticFlop { .. }
                    | Activity::BellyUp { .. }
                    | Activity::Stretching { .. }
            ),
            "Expected Lying/DramaticFlop/BellyUp/Stretching after extended idle, got {:?}",
            cat.activity
        );
        for _ in 0..(60 * 20) {
            cat.update(point_c, 1.0 / 60.0);
        }
        assert_eq!(cat.activity, Activity::Sleeping);
    }

    #[test]
    fn laser_pointer_wiggle_triggers_butt_wiggle_and_pounce() {
        let start = Vec2::new(400.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);

        // Rapidly wiggle cursor at pounce range (~150px away) like a laser toy
        for i in 0..25 {
            let wiggle_y = if i % 2 == 0 { -18.0 } else { 18.0 };
            cat.update(Vec2::new(550.0, 400.0 + wiggle_y), 1.0 / 60.0);
        }

        assert!(
            matches!(
                cat.activity,
                Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
            ),
            "Expected StalkWiggle or Pouncing when cursor wiggles at pounce range, got {:?}",
            cat.activity
        );
    }

    #[test]
    fn tail_chasing_orbits_fixed_center_without_drifting() {
        let start = Vec2::new(400.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        let center = cat.nodes[3];
        cat.activity = Activity::TailChasing {
            center,
            dir: 1.0,
            timer: 0.0,
            duration: 1.8,
        };

        for _ in 0..60 {
            cat.update(start, 1.0 / 60.0);
        }
        assert!(matches!(cat.activity, Activity::TailChasing { .. }));
        let head_dist_from_center = cat.nodes[0].distance(center);
        assert!(
            (head_dist_from_center - 9.5).abs() < 0.1,
            "Expected head to stay on 9.5px orbit around fixed center, got {head_dist_from_center:.2}"
        );
    }

    #[test]
    fn walk_gait_schedules_lateral_footfall_sequence() {
        let start = Vec2::new(200.0, 200.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.activity = Activity::Walking;
        cat.gait_weight = 1.0;
        cat.speed = 80.0;

        // Expected lateral sequence: LH (2) at 0.0, LF (0) at 0.25, RH (3) at 0.50, RF (1) at 0.75
        for &(cycle, expected_paw) in &[
            (0.05, 2usize),
            (0.30, 0usize),
            (0.55, 3usize),
            (0.80, 1usize),
        ] {
            cat.gait_cycle = cycle;
            cat.update_paws(1.0 / 60.0);
            assert!(
                cat.paws[expected_paw].swing_progress > 0.0,
                "Expected paw {expected_paw} to be in swing at gait_cycle {cycle}"
            );
        }
    }

    #[test]
    fn planted_paws_stay_put_at_walk_trot_and_gallop_speeds() {
        // (cursor lead ahead of the head, sprinting): steady walk ~104 px/s, trot ~152 px/s, gallop ~365 px/s
        for &(lead, sprint) in &[(160.0, false), (460.0, false), (460.0, true)] {
            let mut cat = CatSim::new(Vec2::new(0.0, 0.0), 1.0, CoatStyle::Calico);
            let dt = 1.0 / 120.0;
            let mut prev_pos = cat.paws.map(|p| p.world_pos);
            let mut prev_swing = [0.0f32; 4];
            let mut stances = 0;
            for frame in 0..1800 {
                if sprint {
                    cat.sprinting = true;
                    cat.shake_energy = 2.0;
                }
                let cursor = cat.nodes[0] + Vec2::new(lead, 0.0);
                cat.update(cursor, dt);
                for i in 0..4 {
                    let swing = cat.paws[i].swing_progress;
                    if frame > 600 && swing == 0.0 && prev_swing[i] == 0.0 {
                        let slip = cat.paws[i].world_pos.distance(prev_pos[i]);
                        assert!(
                            slip < 0.05,
                            "paw {i} slid {slip:.2}px in one stance frame at speed {:.0} px/s",
                            cat.speed
                        );
                    }
                    if frame > 600 && swing > 0.0 && prev_swing[i] == 0.0 {
                        stances += 1;
                    }
                }
                prev_pos = cat.paws.map(|p| p.world_pos);
                prev_swing = cat.paws.map(|p| p.swing_progress);
            }
            assert!(
                stances > 40,
                "expected steady stepping, saw {stances} steps"
            );
        }
    }

    #[test]
    fn hind_paws_stay_close_behind_the_haunches_in_every_gait() {
        // (cursor lead, sprinting, how far behind the tail base a hind paw may go)
        for &(lead, sprint, limit) in &[
            (160.0, false, 10.0),
            (460.0, false, 10.0),
            (460.0, true, 12.0),
        ] {
            let mut cat = CatSim::new(Vec2::new(0.0, 0.0), 1.0, CoatStyle::Calico);
            let dt = 1.0 / 120.0;
            let mut furthest = f32::MIN;
            for frame in 0..1800 {
                if sprint {
                    cat.sprinting = true;
                    cat.shake_energy = 2.0;
                }
                let cursor = cat.nodes[0] + Vec2::new(lead, 0.0);
                cat.update(cursor, dt);
                if frame < 600 {
                    continue;
                }
                let fwd = (cat.nodes[3] - cat.nodes[5]).normalize();
                for paw in &cat.paws[2..] {
                    furthest = furthest.max(-(paw.world_pos - cat.nodes[5]).dot(fwd));
                }
            }
            assert!(
                furthest <= limit,
                "hind paw reached {furthest:.1}px behind the tail base at {:?} ({:.0} px/s)",
                cat.gait,
                cat.speed
            );
        }
    }

    #[test]
    fn realistic_temperament_walks_at_real_cat_pace_and_trots_only_when_far() {
        // (cursor lead ahead of the head) -> steady (speed px/s, strides/s, gait)
        let steady = |lead: f32| {
            let mut cat = CatSim::new(Vec2::new(0.0, 0.0), 1.0, CoatStyle::Calico);
            cat.behaviors.temperament = Temperament::Realistic;
            let dt = 1.0 / 120.0;
            let (mut start_x, mut cycles, mut last_cycle) = (0.0, 0.0, 0.0);
            for frame in 0..1800 {
                let cursor = cat.nodes[0] + Vec2::new(lead, 0.0);
                cat.update(cursor, dt);
                if frame == 600 {
                    start_x = cat.nodes[3].x;
                    last_cycle = cat.gait_cycle;
                } else if frame > 600 {
                    cycles += (cat.gait_cycle - last_cycle).rem_euclid(1.0);
                    last_cycle = cat.gait_cycle;
                }
            }
            let secs = 1199.0 * dt;
            ((cat.nodes[3].x - start_x) / secs, cycles / secs, cat.gait)
        };

        // ~1 px per cm: a relaxed cat walks ~0.5-0.8 m/s at ~1-2 strides/s
        let (speed, cadence, gait) = steady(160.0);
        assert!((55.0..80.0).contains(&speed), "walk speed {speed:.0} px/s");
        assert!(
            (1.2..2.0).contains(&cadence),
            "walk cadence {cadence:.2} strides/s"
        );
        assert_eq!(gait, Gait::Walk);

        let (speed, _, gait) = steady(460.0);
        assert!(
            speed > 128.0,
            "distant targets should be trotted to, got {speed:.0} px/s"
        );
        assert_eq!(gait, Gait::Trot);
    }

    #[test]
    fn gait_changes_never_cut_swings_short_or_start_them_midway() {
        let mut cat = CatSim::new(Vec2::new(0.0, 0.0), 1.0, CoatStyle::Calico);
        let dt = 1.0 / 120.0;
        // (seconds, cursor lead, sprinting): walk -> trot -> walk -> gallop -> trot -> walk,
        // plus a stretch that hovers around the walk/trot threshold
        let phases = [
            (1.5, 160.0, false),
            (2.0, 460.0, false),
            (2.0, 160.0, false),
            (2.5, 460.0, true),
            (2.0, 460.0, false),
            (2.0, 160.0, false),
            (3.0, 330.0, false),
        ];
        let mut prev_swing = [0.0f32; 4];
        let mut prev_pos = cat.paws.map(|p| p.world_pos);
        let mut planted_slip = 0.0;
        let (mut swings, mut late_starts, mut early_ends) = (0, 0, 0);
        let mut saw_trot = false;
        let mut saw_gallop = false;
        for &(secs, lead, sprint) in &phases {
            for _ in 0..(secs / dt) as usize {
                cat.sprinting = sprint;
                cat.shake_energy = if sprint { 2.0 } else { 0.0 };
                let cursor = cat.nodes[0] + Vec2::new(lead, 0.0);
                cat.update(cursor, dt);
                saw_trot |= cat.gait == Gait::Trot;
                saw_gallop |= cat.gait == Gait::Gallop;
                for i in 0..4 {
                    let swing = cat.paws[i].swing_progress;
                    if swing > 0.0 && prev_swing[i] == 0.0 {
                        swings += 1;
                        if swing > 0.15 {
                            late_starts += 1;
                        }
                    }
                    if swing == 0.0 && prev_swing[i] > 0.0 && prev_swing[i] < 0.85 {
                        early_ends += 1;
                    }
                    if swing == 0.0 && prev_swing[i] == 0.0 && cat.gait_weight > 0.35 {
                        planted_slip += cat.paws[i].world_pos.distance(prev_pos[i]);
                    }
                    prev_swing[i] = swing;
                }
                prev_pos = cat.paws.map(|p| p.world_pos);
            }
        }
        assert!(
            saw_trot && saw_gallop,
            "scenario should exercise every gait"
        );
        assert!(
            swings > 100,
            "expected steady stepping, saw {swings} swings"
        );
        assert_eq!(
            (late_starts, early_ends),
            (0, 0),
            "paws jumped when the gait changed ({swings} swings)"
        );
        assert!(
            planted_slip < 10.0,
            "planted paws slid {planted_slip:.1}px in total across the gait changes"
        );
    }

    #[test]
    fn gallop_has_gathered_flight_and_spine_flexes_at_hind_touchdown() {
        let mut cat = CatSim::new(Vec2::new(0.0, 0.0), 1.0, CoatStyle::Calico);
        let dt = 1.0 / 120.0;
        let (mut flight_frames, mut frames) = (0, 0);
        let mut ext_range = (f32::MAX, f32::MIN);
        let mut ext_at_hind_touchdown = vec![];
        let mut prev_swing = [0.0f32; 4];
        for frame in 0..1800 {
            cat.sprinting = true;
            cat.shake_energy = 2.0;
            let cursor = cat.nodes[0] + Vec2::new(460.0, 0.0);
            cat.update(cursor, dt);
            let swing = cat.paws.map(|p| p.swing_progress);
            if frame > 600 {
                frames += 1;
                if swing.iter().all(|&s| s > 0.0) {
                    flight_frames += 1;
                }
                ext_range = (
                    ext_range.0.min(cat.posture_extension),
                    ext_range.1.max(cat.posture_extension),
                );
                for i in 2..4 {
                    if swing[i] == 0.0 && prev_swing[i] > 0.0 {
                        ext_at_hind_touchdown.push(cat.posture_extension);
                    }
                }
            }
            prev_swing = swing;
        }
        assert!(cat.gallop_weight > 0.95);
        let flight_share = flight_frames as f32 / frames as f32;
        assert!(
            (0.12..0.30).contains(&flight_share),
            "expected a gathered suspension phase, all paws airborne {:.0}% of the time",
            flight_share * 100.0
        );
        // Hind paws land while the spine is bunched up, in the lower part of the flex range
        let mid = (ext_range.0 + ext_range.1) * 0.5;
        assert!(!ext_at_hind_touchdown.is_empty());
        for ext in ext_at_hind_touchdown {
            assert!(
                ext < mid,
                "hind paw landed with spine extended ({ext:.2}, range {ext_range:?})"
            );
        }
    }

    #[test]
    fn curved_walk_trajectory_remains_compact_and_smooth() {
        let start = Vec2::new(500.0, 500.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.activity = Activity::Walking;
        cat.posture_extension = 1.60;
        cat.gait_weight = 1.0;
        cat.speed = 100.0; // Already walking right (body_angle = 0.0)

        let start_head = cat.nodes[0];
        // Suddenly move cursor behind-left of the walking cat (requiring ~155 deg reversal)
        let reversed_cursor = start_head + Vec2::new(-160.0, -70.0);
        let mut trajectory = Vec::new();
        let mut arrived = false;

        for _ in 0..240 {
            cat.update(reversed_cursor, 1.0 / 60.0);
            trajectory.push(cat.nodes[0]);
            if matches!(
                cat.activity,
                Activity::StandingIdle { .. } | Activity::Sitting
            ) {
                arrived = true;
                break;
            }
        }

        assert!(
            arrived,
            "Expected cat to complete turn and arrive within 4s, still in {:?} at {:?}",
            cat.activity, cat.nodes[0]
        );

        let end_head = *trajectory.last().unwrap();
        let chord = end_head - start_head;
        let chord_len = chord.length();
        let chord_normal = Vec2::new(-chord.y, chord.x) / chord_len;

        let max_lateral_dev = trajectory
            .iter()
            .map(|&p| (p - start_head).dot(chord_normal).abs())
            .fold(0.0_f32, f32::max);

        assert!(
            max_lateral_dev < 28.0,
            "Expected compact curved trajectory (<28px lateral deviation), got {max_lateral_dev:.2}px"
        );
    }

    #[test]
    fn follow_cursor_toggle_anchors_cat_in_place() {
        let start = Vec2::new(300.0, 300.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        for _ in 0..15 {
            cat.update(start, 1.0 / 60.0);
        }
        cat.behaviors.follow_cursor = false;
        let anchored_waist = cat.nodes[3];

        // Move cursor far away; cat should stay sitting/relaxing at its current spot
        for _ in 0..120 {
            cat.update(Vec2::new(900.0, 700.0), 1.0 / 60.0);
        }
        assert!(
            !matches!(
                cat.activity,
                Activity::Walking | Activity::StandingUp { .. }
            ),
            "Expected cat not to walk when follow_cursor is disabled, got {:?}",
            cat.activity
        );
        assert!(
            cat.nodes[3].distance(anchored_waist) < 1.0,
            "Cat waist drifted {:.2}px while follow_cursor was disabled",
            cat.nodes[3].distance(anchored_waist)
        );
    }

    #[test]
    fn set_scale_preserves_head_anchor_and_rescales_chain() {
        let start = Vec2::new(400.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        let head_before = cat.nodes[0];
        let tail_dist_before = cat.nodes[5].distance(head_before);

        cat.set_scale(1.35);
        assert_eq!(cat.nodes[0], head_before);
        let tail_dist_after = cat.nodes[5].distance(head_before);
        assert!((tail_dist_after / tail_dist_before - 1.35).abs() < 1e-4);
    }

    #[test]
    fn sudden_mouse_moves_always_walk_instead_of_linear_pounce() {
        for temperament in Temperament::ALL {
            // Test both 1-frame cursor teleports and fast multi-frame cursor flicks across distances
            for &(dist, flick_frames) in &[
                (140.0_f32, 1usize),
                (160.0, 8),
                (210.0, 10),
                (350.0, 12),
                (750.0, 6),
            ] {
                let start = Vec2::new(400.0, 400.0);
                let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
                cat.behaviors.temperament = temperament;

                let target = start + Vec2::new(dist, -40.0);
                for frame in 1..=flick_frames {
                    let t = frame as f32 / (flick_frames as f32);
                    cat.update(start.lerp(target, t), 1.0 / 60.0);
                    assert!(
                        !matches!(
                            cat.activity,
                            Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
                        ),
                        "Unexpected {:?} during sudden mouse flick (temperament={:?}, dist={dist}, frames={flick_frames})",
                        cat.activity,
                        temperament
                    );
                }

                let mut saw_walking = false;
                for _ in 0..180 {
                    cat.update(target, 1.0 / 60.0);
                    assert!(
                        !matches!(
                            cat.activity,
                            Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
                        ),
                        "Unexpected {:?} after sudden mouse move (temperament={:?}, dist={dist}, frames={flick_frames})",
                        cat.activity,
                        temperament
                    );
                    if cat.activity == Activity::Walking {
                        saw_walking = true;
                    }
                }
                assert!(
                    saw_walking,
                    "Expected cat to walk to target after sudden mouse move (temperament={:?}, dist={dist})",
                    temperament
                );
            }
        }
    }

    #[test]
    fn pounce_pushes_off_hard_flies_steadily_and_realistic_leaps_are_short() {
        for (temperament, max_leap) in [
            (Temperament::Balanced, 190.0),
            (Temperament::Realistic, 95.0),
        ] {
            let start = Vec2::new(300.0, 300.0);
            let distant_cursor = Vec2::new(850.0, 620.0);
            let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
            cat.behaviors.temperament = temperament;
            cat.behaviors.follow_cursor = false;
            cat.update(distant_cursor, 1.0 / 120.0);
            cat.trigger(TriggerKind::Pounce);
            cat.update(distant_cursor, 1.0 / 120.0);
            let head_before = cat.nodes[0];

            let mut speeds = vec![];
            for _ in 0..240 {
                cat.update(distant_cursor, 1.0 / 120.0);
                if matches!(cat.activity, Activity::Pouncing { .. }) {
                    speeds.push(cat.speed);
                }
            }
            assert!(
                speeds.len() > 20,
                "{temperament:?}: pounce too short to sample"
            );
            let peak = speeds.iter().cloned().fold(0.0, f32::max);
            // Explosive push-off: near top speed within the first third, and a steady flight
            // until the landing (a smoothstep only reaches its peak at mid-leap)
            let n = speeds.len() as f32;
            for (i, &v) in speeds.iter().enumerate() {
                let p = i as f32 / n;
                if (0.3..0.7).contains(&p) {
                    assert!(
                        v > 0.9 * peak,
                        "{temperament:?}: speed {v:.0} at {:.0}% of the leap (peak {peak:.0})",
                        p * 100.0
                    );
                }
            }
            let leap = cat.nodes[0].distance(head_before);
            assert!(
                leap > 0.8 * (max_leap - 10.0) && leap <= max_leap,
                "{temperament:?}: leap of {leap:.1}px"
            );
        }
    }

    #[test]
    fn realistic_cat_settles_slowly_and_circles_before_curling_up_to_sleep() {
        let start = Vec2::new(500.0, 500.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.behaviors.temperament = Temperament::Realistic;
        let dt = 1.0 / 60.0;
        let cursor = start + Vec2::new(20.0, 0.0);
        // Let it take its first few steps toward the cursor and sit down
        for _ in 0..(60 * 6) {
            cat.update(cursor, dt);
        }
        assert_eq!(cat.activity, Activity::Sitting);
        let anchor = cat.window_center();

        let kind = |a: &Activity| std::mem::discriminant(a);
        let mut sequence: Vec<Activity> = vec![cat.activity];
        let (mut still_secs, mut first_sleep_at) = (0.0, None);
        let mut circled = 0.0f32;
        let mut max_drift = 0.0f32;
        for frame in 0..(60 * 600) {
            cat.update(cursor, dt);
            if kind(&cat.activity) != kind(sequence.last().unwrap()) {
                sequence.push(cat.activity);
            }
            if let Activity::Circling { turned, .. } = cat.activity {
                circled = circled.max(turned);
            }
            if cat.activity == Activity::Sleeping && first_sleep_at.is_none() {
                first_sleep_at = Some(frame as f32 * dt);
            }
            if matches!(
                cat.activity,
                Activity::Sitting
                    | Activity::Lying
                    | Activity::DramaticFlop { .. }
                    | Activity::Sleeping
            ) {
                still_secs += dt;
            }
            max_drift = max_drift.max(cat.window_center().distance(anchor));
        }

        for pair in sequence.windows(2) {
            assert!(
                !(pair[0] == Activity::Sitting && matches!(pair[1], Activity::BellyUp { .. })),
                "rolled belly-up straight from sitting: {sequence:?}"
            );
        }
        let sleep_at = first_sleep_at.expect("cat never fell asleep");
        assert!(sleep_at >= 120.0, "fell asleep after only {sleep_at:.0}s");
        let first_sleep = sequence
            .iter()
            .position(|a| *a == Activity::Sleeping)
            .unwrap();
        assert!(
            matches!(sequence[first_sleep - 1], Activity::Circling { .. }),
            "expected circling right before curling up: {sequence:?}"
        );
        assert!(
            sequence[..first_sleep].contains(&Activity::Lying),
            "expected to lie down before sleeping: {sequence:?}"
        );
        assert!(
            circled >= std::f32::consts::TAU,
            "circled only {circled:.1} rad"
        );
        assert!(
            still_secs > 0.85 * 600.0,
            "only still for {still_secs:.0}s of 600s"
        );
        assert!(
            max_drift < 12.0,
            "cat drifted {max_drift:.1}px while settling"
        );
    }

    #[test]
    fn sitting_cat_wraps_its_tail_and_shuffles_round_to_face_a_cursor_behind_it() {
        let start = Vec2::new(300.0, 300.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.behaviors.playful_quirks = false;
        let dt = 1.0 / 60.0;
        let cursor = start + Vec2::new(20.0, 0.0);
        for _ in 0..(60 * 6) {
            cat.update(cursor, dt);
            cat.idle_time = 0.0;
        }
        assert!(matches!(
            cat.activity,
            Activity::Sitting | Activity::Grooming { .. }
        ));
        let fwd = Vec2::from_angle(cat.body_angle);
        let tip_forward = (cat.nodes[TOTAL_NODES - 1] - cat.nodes[BODY_NODES - 1]).dot(fwd);
        assert!(
            tip_forward > 8.0,
            "tail tip should wrap forward along the flank, got {tip_forward:.1}px"
        );

        // Cursor moves round behind the cat but stays close: it turns on the spot to face it
        let anchor = cat.window_center();
        let behind = anchor - fwd * 30.0 + Vec2::new(-fwd.y, fwd.x) * 8.0;
        for _ in 0..(60 * 4) {
            cat.update(behind, dt);
            cat.idle_time = 0.0;
            assert!(
                matches!(cat.activity, Activity::Sitting | Activity::Grooming { .. }),
                "turned by getting up and walking: {:?}",
                cat.activity
            );
        }
        let to_cursor = behind - cat.nodes[0];
        let facing_err = wrap_angle(to_cursor.y.atan2(to_cursor.x) - cat.body_angle);
        assert!(
            facing_err.abs() < 0.5,
            "still {facing_err:.2} rad off the cursor"
        );
        assert!(
            cat.window_center().distance(anchor) < 3.0,
            "slid while turning"
        );
    }

    #[test]
    fn zoomies_gallop_and_spinning_paws_stay_planted() {
        let start = Vec2::new(300.0, 300.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.update(start, 1.0 / 120.0);
        cat.activity = Activity::Zoomies {
            center: cat.window_center(),
            dir: 1.0,
            timer: 0.0,
            duration: 2.0,
        };
        for _ in 0..120 {
            cat.update(start, 1.0 / 120.0);
        }
        assert_eq!(cat.gait, Gait::Gallop, "zoomies should be a gallop");

        for chase in [true, false] {
            let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
            cat.update(start, 1.0 / 120.0);
            if chase {
                cat.trigger(TriggerKind::ChaseTail);
            } else {
                cat.activity = Activity::Circling {
                    center: cat.window_center(),
                    dir: 1.0,
                    timer: 0.0,
                    turned: 0.0,
                    total: 20.0,
                };
            }
            let mut prev_pos = cat.paws.map(|p| p.world_pos);
            let mut prev_swing = [0.0f32; 4];
            let (mut slip, mut steps) = (0.0, 0);
            for _ in 0..240 {
                cat.update(start, 1.0 / 120.0);
                for i in 0..4 {
                    let swing = cat.paws[i].swing_progress;
                    if swing == 0.0 && prev_swing[i] == 0.0 {
                        slip += cat.paws[i].world_pos.distance(prev_pos[i]);
                    }
                    if swing > 0.0 && prev_swing[i] == 0.0 {
                        steps += 1;
                    }
                    prev_swing[i] = swing;
                }
                prev_pos = cat.paws.map(|p| p.world_pos);
            }
            assert!(steps > 8, "expected stepping while spinning, saw {steps}");
            let per_step = slip / steps as f32;
            assert!(
                per_step < 1.5,
                "{}: planted paws slid {per_step:.2}px per step",
                if chase { "tail chase" } else { "circling" }
            );
        }
    }

    #[test]
    fn butt_wiggle_shifts_weight_a_few_times_per_second() {
        let start = Vec2::new(300.0, 300.0);
        let cursor = start + Vec2::new(150.0, 0.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.update(cursor, 1.0 / 120.0);
        cat.activity = Activity::StalkWiggle {
            timer: 0.0,
            wiggle_duration: 2.0,
        };
        let (mut crossings, mut wiggling_secs, mut last) = (0, 0.0, 0.0f32);
        while matches!(cat.activity, Activity::StalkWiggle { .. }) {
            cat.update(cursor, 1.0 / 120.0);
            if cat.hip_wiggle_offset.abs() > 0.01 {
                wiggling_secs += 1.0 / 120.0;
                if cat.hip_wiggle_offset.signum() != last.signum() && last != 0.0 {
                    crossings += 1;
                }
                last = cat.hip_wiggle_offset;
            }
        }
        let hz = crossings as f32 / 2.0 / wiggling_secs;
        assert!((2.0..4.5).contains(&hz), "butt wiggle at {hz:.1} Hz");
    }

    #[test]
    fn explicit_pounce_trigger_executes_when_cursor_is_distant() {
        let start = Vec2::new(300.0, 300.0);
        let distant_cursor = Vec2::new(850.0, 620.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.behaviors.follow_cursor = false;
        cat.update(distant_cursor, 1.0 / 60.0);
        let head_before = cat.nodes[0];

        cat.trigger(TriggerKind::Pounce);
        cat.update(distant_cursor, 1.0 / 60.0);
        assert!(
            matches!(cat.activity, Activity::StalkWiggle { .. }),
            "Expected StalkWiggle to persist on distant cursor for explicit pounce, got {:?}",
            cat.activity
        );

        let mut saw_pouncing = false;
        for _ in 0..90 {
            cat.update(distant_cursor, 1.0 / 60.0);
            if matches!(cat.activity, Activity::Pouncing { .. }) {
                saw_pouncing = true;
            }
        }
        assert!(saw_pouncing, "Expected explicit pounce to enter Pouncing");
        let leap_displacement = cat.nodes[0].distance(head_before);
        assert!(
            leap_displacement > 120.0 && leap_displacement <= 190.0,
            "Expected bounded leap displacement toward distant cursor, got {leap_displacement:.1}px"
        );
    }

    #[test]
    fn explicit_sleep_trigger_stays_asleep_when_cursor_is_distant_until_moved() {
        let start = Vec2::new(300.0, 300.0);
        let menu_cursor = Vec2::new(820.0, 580.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.behaviors.follow_cursor = true;
        cat.update(menu_cursor, 1.0 / 60.0);

        cat.trigger(TriggerKind::Sleep);
        for _ in 0..120 {
            cat.update(menu_cursor, 1.0 / 60.0);
            assert_eq!(
                cat.activity,
                Activity::Sleeping,
                "Expected explicit sleep not to wake while cursor remains at menu position"
            );
        }

        // Moving the cursor away from the menu position should wake the cat
        let moved_cursor = Vec2::new(500.0, 180.0);
        for _ in 0..30 {
            cat.update(moved_cursor, 1.0 / 60.0);
        }
        assert!(
            matches!(
                cat.activity,
                Activity::Stretching { .. } | Activity::StandingUp { .. } | Activity::Walking
            ),
            "Expected cat to wake after cursor moves away from sleep baseline, got {:?}",
            cat.activity
        );
    }

    #[test]
    fn explicit_belly_up_trigger_stays_on_back_until_cursor_moves() {
        let start = Vec2::new(300.0, 300.0);
        let menu_cursor = Vec2::new(820.0, 580.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.behaviors.follow_cursor = true;
        cat.update(menu_cursor, 1.0 / 60.0);

        cat.trigger(TriggerKind::BellyUp);
        assert_eq!(cat.belly_roll, 0.0);

        // First few frames should smoothly roll onto the back (0.0 < belly_roll < 1.0)
        for _ in 0..8 {
            cat.update(menu_cursor, 1.0 / 60.0);
        }
        assert!(
            cat.belly_roll > 0.15 && cat.belly_roll < 0.85,
            "Expected gradual belly_roll transition animation, got {:.2}",
            cat.belly_roll
        );

        for _ in 0..112 {
            cat.update(menu_cursor, 1.0 / 60.0);
            assert!(
                matches!(cat.activity, Activity::BellyUp { .. }),
                "Expected explicit BellyUp trigger to remain BellyUp while cursor stays at menu position, got {:?}",
                cat.activity
            );
        }
        assert!((cat.belly_roll - 1.0).abs() < 1e-4);

        // Moving cursor far away should make the cat smoothly roll back over and stand up / walk
        let moved_cursor = Vec2::new(480.0, 160.0);
        for _ in 0..8 {
            cat.update(moved_cursor, 1.0 / 60.0);
        }
        assert!(
            cat.belly_roll > 0.1 && cat.belly_roll < 0.9,
            "Expected gradual roll-back animation when waking from BellyUp, got {:.2}",
            cat.belly_roll
        );
        for _ in 0..22 {
            cat.update(moved_cursor, 1.0 / 60.0);
        }
        assert!(
            matches!(
                cat.activity,
                Activity::AloofNotice { .. } | Activity::StandingUp { .. } | Activity::Walking
            ),
            "Expected cat to wake from BellyUp when cursor moves away, got {:?}",
            cat.activity
        );
        assert_eq!(cat.belly_roll, 0.0);
    }

    #[test]
    fn curled_sleeping_wraps_spine_and_tail_into_compact_ball() {
        let start = Vec2::new(400.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        let standing_head_to_rump = cat.nodes[0].distance(cat.nodes[5]);

        cat.trigger(TriggerKind::Sleep);
        assert_eq!(cat.sleep_blend, 0.0);

        // First few frames should smoothly contract sleep_blend rather than snapping tail length in 1 tick
        for _ in 0..8 {
            cat.update(start, 1.0 / 60.0);
        }
        assert!(
            cat.sleep_blend > 0.15 && cat.sleep_blend < 0.85,
            "Expected gradual sleep_blend tail interpolation, got {:.2}",
            cat.sleep_blend
        );

        for _ in 0..82 {
            cat.update(start, 1.0 / 60.0);
        }

        assert_eq!(cat.activity, Activity::Sleeping);
        assert!(cat.sleep_blend > 0.99);
        let curled_head_to_rump = cat.nodes[0].distance(cat.nodes[5]);
        assert!(
            curled_head_to_rump < standing_head_to_rump * 0.45,
            "Expected curled sleeping pose to bring head and rump close together (<45% of standing), got {curled_head_to_rump:.1}px vs {standing_head_to_rump:.1}px"
        );

        let tail_tip_to_head = cat.nodes[TOTAL_NODES - 1].distance(cat.nodes[0]);
        assert!(
            tail_tip_to_head < 14.0,
            "Expected wrapped tail tip to tuck near the head (<14px), got {tail_tip_to_head:.1}px"
        );
    }

    #[test]
    fn transient_groom_trigger_still_progresses_to_sleeping_when_idle() {
        let start = Vec2::new(400.0, 400.0);
        let menu_cursor = Vec2::new(820.0, 580.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.update(menu_cursor, 1.0 / 60.0);

        cat.trigger(TriggerKind::Groom);
        for _ in 0..(60 * 35) {
            cat.update(menu_cursor, 1.0 / 60.0);
        }
        assert_eq!(
            cat.activity,
            Activity::Sleeping,
            "Expected cat to eventually fall asleep after Groom trigger when cursor stays still"
        );
    }

    #[test]
    fn hip_sway_decreases_smoothly_as_cat_slows_down() {
        let start = Vec2::new(200.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        let target = cat.nodes[0] + Vec2::new(380.0, 0.0);
        let mut cruise_max_hip_lat: f32 = 0.0;
        let mut slow_decel_max_hip_lat: f32 = 0.0;
        let mut near_stop_max_hip_lat: f32 = 0.0;
        let mut reached_cruise = false;

        for _ in 0..360 {
            cat.update(target, 1.0 / 60.0);
            if cat.activity == Activity::Walking {
                let hip_sway = cat.nodes[4].distance(cat.carrier_nodes[4]);

                if cat.speed > 85.0 {
                    reached_cruise = true;
                    cruise_max_hip_lat = cruise_max_hip_lat.max(hip_sway);
                } else if reached_cruise && (22.0..=45.0).contains(&cat.speed) {
                    slow_decel_max_hip_lat = slow_decel_max_hip_lat.max(hip_sway);
                } else if reached_cruise && cat.speed < 22.0 {
                    near_stop_max_hip_lat = near_stop_max_hip_lat.max(hip_sway);
                }
            } else if reached_cruise && matches!(cat.activity, Activity::StandingIdle { .. }) {
                break;
            }
        }

        assert!(reached_cruise, "Expected cat to reach cruising walk speed");
        assert!(
            cruise_max_hip_lat > 2.8 && cruise_max_hip_lat < 4.5,
            "Expected clearly visible feline hip sway at cruise speed (2.8..4.5px), got {cruise_max_hip_lat:.2}px"
        );
        assert!(
            slow_decel_max_hip_lat < cruise_max_hip_lat * 0.60,
            "Expected hip sway when slowing down ({slow_decel_max_hip_lat:.2}px) to be smaller than at cruise speed ({cruise_max_hip_lat:.2}px)"
        );
        assert!(
            near_stop_max_hip_lat < 0.65,
            "Expected hip sway right before stopping to settle below 0.65px, got {near_stop_max_hip_lat:.2}px"
        );
    }

    #[test]
    fn window_center_stays_stationary_across_resting_and_in_place_activities() {
        let scale = 1.35_f32;
        let mut cat = CatSim::new(Vec2::new(500.0, 500.0), scale, CoatStyle::Charcoal);
        let target = Vec2::new(800.0, 450.0);

        let mut prev_center = cat.window_center();
        for _ in 0..3600 {
            cat.update(target, 1.0 / 60.0);
            let center = cat.window_center();
            let is_translating = matches!(
                cat.activity,
                Activity::Walking | Activity::Pouncing { .. } | Activity::Zoomies { .. }
            );
            if !is_translating {
                assert!(
                    center.distance(prev_center) < 1e-4,
                    "Expected stationary window_center during {:?}, shifted by {:.4}px",
                    cat.activity,
                    center.distance(prev_center)
                );
            }
            prev_center = center;
        }
    }

    #[test]
    fn high_uptime_tail_motion_and_close_cursor_stops_stay_smooth() {
        let scale = 1.35_f32;
        let mut cat = CatSim::new(Vec2::new(500.0, 500.0), scale, CoatStyle::Charcoal);
        cat.time = 3600.0; // Simulate 1 hour of uptime

        let mut prev_tail = cat.nodes[TOTAL_NODES - 1];
        for f in 0..200 {
            let cursor = if f < 80 {
                Vec2::new(800.0, 500.0)
            } else {
                Vec2::new(570.0, 520.0)
            };
            cat.update(cursor, 1.0 / 60.0);
            let tail_step = cat.nodes[TOTAL_NODES - 1].distance(prev_tail);
            assert!(
                tail_step < 6.0 * scale,
                "Unexpected tail jump ({tail_step:.2}px) at frame {f} ({:?}) under high uptime",
                cat.activity
            );
            prev_tail = cat.nodes[TOTAL_NODES - 1];
        }

        assert!(
            matches!(
                cat.activity,
                Activity::StandingIdle { .. } | Activity::Sitting
            ),
            "Expected cat to settle smoothly when cursor moves inside stop_radius, got {:?}",
            cat.activity
        );
        assert!(
            cat.body_angle.abs() < 0.6,
            "Expected cat not to U-turn away from close cursor, body_angle={:.2}",
            cat.body_angle
        );
    }

    #[test]
    fn repeated_tail_chasing_and_phase_wraps_do_not_drift_or_jump() {
        let scale = 1.35_f32;
        let mut cat = CatSim::new(Vec2::new(500.0, 500.0), scale, CoatStyle::Charcoal);
        let initial_anchor = cat.window_center();

        // Trigger ChaseTail multiple times and let each finish into Grooming/Sitting
        for _ in 0..3 {
            cat.trigger(TriggerKind::ChaseTail);
            for _ in 0..180 {
                cat.update(Vec2::new(500.0, 500.0), 1.0 / 60.0);
                assert!(
                    cat.window_center().distance(initial_anchor) < 1e-4,
                    "Expected window_center to stay pinned during/after ChaseTail"
                );
            }
        }
        assert!(
            cat.carrier_nodes[3].distance(initial_anchor) < 0.25,
            "Expected torso waist to realign with window_anchor after ChaseTail, drift={:.2}px",
            cat.carrier_nodes[3].distance(initial_anchor)
        );

        // Verify seamless tail motion across the 1024.0 tail_phase wrap boundary
        cat.tail_phase = 1023.95;
        let mut prev_tail = cat.nodes[TOTAL_NODES - 1];
        for _ in 0..20 {
            cat.update(Vec2::new(500.0, 500.0), 1.0 / 60.0);
            let step = cat.nodes[TOTAL_NODES - 1].distance(prev_tail);
            assert!(
                step < 1.5 * scale,
                "Expected smooth tail step across tail_phase wrap boundary, got {step:.2}px"
            );
            prev_tail = cat.nodes[TOTAL_NODES - 1];
        }
    }

    #[test]
    fn rapid_left_right_mouse_shake_makes_cat_run_to_cursor() {
        let start = Vec2::new(200.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.trigger(TriggerKind::Sleep);
        cat.update(start, 1.0 / 60.0);
        assert_eq!(cat.activity, Activity::Sleeping);

        // Shake the mouse left and right rapidly around X=750 (3 strokes: right -> left -> right)
        let center_x = 750.0;
        let shake_offsets = [
            -40.0, -20.0, 0.0, 25.0, 45.0, // Stroke 1: Right (+85px)
            20.0, -10.0, -35.0, -50.0, // Stroke 2: Left (-95px, reversal #1)
            -20.0, 10.0, 35.0, 50.0, // Stroke 3: Right (+100px, reversal #2 -> sprint!)
        ];
        for &dx in &shake_offsets {
            cat.update(Vec2::new(center_x + dx, 400.0), 1.0 / 60.0);
        }

        assert!(
            cat.sprinting,
            "Expected rapid left-right mouse shake to engage sprinting mode"
        );
        assert_eq!(
            cat.activity,
            Activity::Walking,
            "Expected cat to immediately start running without waiting in Stretching/StandingUp, got {:?}",
            cat.activity
        );

        let target = Vec2::new(center_x, 400.0);
        let mut peak_speed = cat.speed;
        let mut peak_gallop = cat.gallop_weight;
        let mut min_ext = f32::MAX;
        let mut max_ext = f32::MIN;
        let mut arrived_frame = None;
        for frame in 1..=180 {
            cat.update(target, 1.0 / 60.0);
            peak_speed = peak_speed.max(cat.speed);
            peak_gallop = peak_gallop.max(cat.gallop_weight);
            if cat.gallop_weight > 0.75 {
                min_ext = min_ext.min(cat.posture_extension);
                max_ext = max_ext.max(cat.posture_extension);
            }
            if matches!(
                cat.activity,
                Activity::StandingIdle { .. } | Activity::Sitting
            ) {
                arrived_frame = Some(frame);
                break;
            }
        }

        assert!(
            peak_speed > 240.0,
            "Expected running speed (>240 px/s) after left-right mouse shake, got {peak_speed:.1} px/s"
        );
        assert!(
            peak_gallop > 0.85 && min_ext < 1.38 && max_ext > 1.80,
            "Expected sagittal gallop spine flexion/extension cycle (min_ext={min_ext:.2}, max_ext={max_ext:.2}, gallop={peak_gallop:.2})"
        );
        let frames = arrived_frame.expect("Expected cat to arrive at cursor within 3s");
        assert!(
            frames < 135,
            "Expected sprinting cat to cover 550px quickly (<135 frames), took {frames} frames"
        );
        assert!(
            !cat.sprinting,
            "Expected sprinting flag to clear once cat arrives at cursor"
        );
    }

    #[test]
    fn shake_detector_ignores_single_stroke_endpoint_nudges_and_slow_sweeps() {
        let start = Vec2::new(300.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);

        // 1. Single large stroke flanked by tiny 4px endpoint corrections (+4, -100, +4)
        let mut x = 300.0;
        for step in [4.0, -100.0, 4.0] {
            x += step;
            cat.update(Vec2::new(x, 400.0), 1.0 / 60.0);
        }
        assert!(
            !cat.sprinting,
            "Single stroke with tiny endpoint nudges must not double-count into sprinting"
        );

        // 2. Prolonged multi-second straight sweeps (+5px/frame for 2s, -5px/frame for 2s, then +5px)
        let mut cat2 = CatSim::new(start, 1.0, CoatStyle::Calico);
        let mut x2 = 300.0;
        for _ in 0..120 {
            x2 += 5.0;
            cat2.update(Vec2::new(x2, 400.0), 1.0 / 60.0);
        }
        for _ in 0..120 {
            x2 -= 5.0;
            cat2.update(Vec2::new(x2, 400.0), 1.0 / 60.0);
        }
        x2 += 5.0;
        cat2.update(Vec2::new(x2, 400.0), 1.0 / 60.0);
        assert!(
            !cat2.sprinting,
            "Multi-second straight sweeps must decay shake energy and not trigger sprinting"
        );

        // 3. Interrupting explicit Pounce crouch with a shake clears manual_pounce
        let mut cat3 = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat3.trigger(TriggerKind::Pounce);
        assert!(cat3.manual_pounce);
        for &dx in &[-40.0, 40.0, -40.0, 40.0] {
            cat3.update(Vec2::new(700.0 + dx, 400.0), 1.0 / 60.0);
        }
        assert!(cat3.sprinting);
        assert!(
            !cat3.manual_pounce,
            "Expected manual_pounce to be cleared when sprinting cancels StalkWiggle"
        );
    }

    #[test]
    fn feeding_mode_ignores_cursor_depletes_food_and_resumes_following_when_done() {
        let start = Vec2::new(300.0, 300.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        let initial_anchor = cat.window_center();

        cat.start_feeding(FeedPreset::Mins15);
        assert!(cat.is_feeding());
        assert_eq!(cat.feeding_preset(), Some(FeedPreset::Mins15));
        assert_eq!(cat.feeding_remaining_secs(), Some(900));
        assert!((cat.feeding_fill_ratio().unwrap() - 1.0).abs() < 1e-4);

        // Even if the user moves or violently shakes the cursor far away, the cat stays at its bowl
        for step in 0..120 {
            let shake_x = if step % 2 == 0 { 800.0 } else { 860.0 };
            cat.update(Vec2::new(shake_x, 600.0), 0.05);
        }
        assert!(
            cat.is_feeding(),
            "Cat must remain eating despite distant cursor moves and shakes"
        );
        assert!(
            !cat.sprinting,
            "Cursor shakes must not trigger sprinting while eating"
        );
        assert!(
            cat.window_center().distance(initial_anchor) < 0.5,
            "Cat window anchor must remain stationary at the bowl while eating"
        );

        // Verify that rendering the cat + food bowl at any heading angle stays strictly inside the 108x108 window
        let win_dim = 108u32;
        let mut pixmap = tiny_skia::Pixmap::new(win_dim, win_dim).unwrap();
        let mut mask = tiny_skia::Mask::new(win_dim, win_dim).unwrap();
        for step in 0..8 {
            let mut angle_cat = CatSim::new(start, 1.0, CoatStyle::Calico);
            angle_cat.body_angle = (step as f32) * std::f32::consts::FRAC_PI_4;
            angle_cat.start_feeding(FeedPreset::Mins30);
            for _ in 0..30 {
                angle_cat.update(start, 1.0 / 60.0);
            }
            let origin = angle_cat.window_center() - Vec2::splat((win_dim as f32) * 0.5);
            crate::render::render_cat(&angle_cat, &mut pixmap.as_mut(), &mut mask, origin);
            let data = pixmap.data();
            for i in 0..win_dim {
                assert_eq!(
                    data[(i as usize) * 4 + 3],
                    0,
                    "Top border clipped at angle step {step}"
                );
                assert_eq!(
                    data[((win_dim - 1) * win_dim + i) as usize * 4 + 3],
                    0,
                    "Bottom border clipped at angle step {step}"
                );
                assert_eq!(
                    data[(i * win_dim) as usize * 4 + 3],
                    0,
                    "Left border clipped at angle step {step}"
                );
                assert_eq!(
                    data[(i * win_dim + (win_dim - 1)) as usize * 4 + 3],
                    0,
                    "Right border clipped at angle step {step}"
                );
            }
        }

        // Advance halfway through the 15-minute (900s) feeding duration
        if let Activity::Eating {
            preset, duration, ..
        } = cat.activity
        {
            cat.activity = Activity::Eating {
                preset,
                elapsed: 450.0,
                duration,
            };
        }
        let mid_fill = cat.feeding_fill_ratio().unwrap();
        assert!(
            (mid_fill - 0.5).abs() < 0.02,
            "Expected ~50% food remaining halfway through feeding, got {mid_fill:.3}"
        );

        // Advance past the end of the 900s duration
        if let Activity::Eating {
            preset, duration, ..
        } = cat.activity
        {
            cat.activity = Activity::Eating {
                preset,
                elapsed: 899.98,
                duration,
            };
        }
        cat.update(Vec2::new(800.0, 600.0), 0.05);
        assert!(
            !cat.is_feeding(),
            "Feeding should end automatically when duration expires"
        );
        assert!(cat.feeding_fill_ratio().is_none());
        assert!(
            matches!(cat.activity, Activity::Grooming { .. }),
            "Expected post-meal grooming after finishing bowl, got {:?}",
            cat.activity
        );

        // Move cursor to a new position after meal finishes: cat stands up and follows cursor again
        for _ in 0..60 {
            cat.update(Vec2::new(950.0, 350.0), 0.05);
        }
        assert!(
            matches!(
                cat.activity,
                Activity::StandingUp { .. } | Activity::Walking | Activity::StandingIdle { .. }
            ),
            "Expected cat to resume following cursor after feeding ends, got {:?}",
            cat.activity
        );
    }

    #[test]
    fn bowl_placement_carries_under_cursor_and_walks_cat_to_clicked_bowl_spot() {
        let start = Vec2::new(200.0, 200.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);

        // 1. User selects "Feed 30m" from tray menu -> bowl attaches to cursor
        cat.begin_bowl_placement(FeedPreset::Mins30, start);
        assert!(cat.is_carrying_bowl());
        assert_eq!(cat.detached_bowl_pos(), Some(start));
        assert!(!cat.keeps_awake());

        // 2. User moves cursor to (420, 260) on the Teams window -> detached bowl follows cursor
        let click_spot = Vec2::new(420.0, 260.0);
        cat.update(click_spot, 1.0 / 60.0);
        assert_eq!(cat.detached_bowl_pos(), Some(click_spot));

        // 3. User left-clicks at `click_spot` to place the bowl on the floor
        cat.place_bowl(click_spot);
        assert!(!cat.is_carrying_bowl());
        assert!(cat.keeps_awake());
        assert_eq!(cat.detached_bowl_pos(), Some(click_spot));

        // 4. Even if the user moves the mouse cursor far away to (900, 700), the cat walks to `click_spot` and starts eating there
        let mut started_eating = false;
        for _ in 0..240 {
            cat.update(Vec2::new(900.0, 700.0), 1.0 / 60.0);
            if cat.is_feeding() {
                started_eating = true;
                break;
            }
        }
        assert!(
            started_eating,
            "Cat should walk to placed bowl at click_spot and start eating"
        );
        assert!(
            cat.detached_bowl_pos().is_none(),
            "Detached bowl overlay should hide once cat arrives and eats from its main window"
        );
        assert!(
            cat.keeps_awake(),
            "Keep-awake must stay active while eating"
        );
        assert_eq!(
            cat.eating_bowl_pos(),
            Some(click_spot),
            "Bowl must remain pinned at the placed coordinate"
        );
    }
}
