use super::*;
use crate::coat::CoatStyle;

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
fn follow_cursor_off_lets_the_cat_roam_and_back_on_follows_again() {
    const DT: f32 = 1.0 / 60.0;
    let start = Vec2::new(800.0, 500.0);
    let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
    cat.roam_bounds = Some((Vec2::ZERO, Vec2::new(1600.0, 1000.0)));
    for _ in 0..15 {
        cat.update(start, DT);
    }
    cat.behaviors.follow_cursor = false;

    // A far-off cursor doesn't draw the cat over; it settles in, then wanders on its own
    let far = Vec2::new(1400.0, 850.0);
    cat.update(far, DT);
    assert!(
        cat.roaming,
        "Turning Follow Cursor off should start roaming"
    );
    let mut closest = f32::MAX;
    let mut moved = 0.0;
    let mut prev = cat.nodes[0];
    for _ in 0..(60 * 60) {
        cat.update(far, DT);
        moved += cat.nodes[0].distance(prev);
        prev = cat.nodes[0];
        closest = closest.min(cat.nodes[0].distance(far));
    }
    assert!(
        moved > 200.0,
        "Roaming cat should wander, moved only {moved:.0}px"
    );
    assert!(
        closest > 150.0,
        "Cat came within {closest:.0}px of the cursor"
    );

    // It keeps away from a cursor put on it, and shakes don't call it back
    let on_cat = cat.nodes[0];
    for i in 0..300 {
        let shake = if (i / 5) % 2 == 0 { -40.0 } else { 40.0 };
        let cursor = if i < 60 {
            on_cat + Vec2::new(shake, 0.0)
        } else {
            on_cat
        };
        cat.update(cursor, DT);
    }
    assert!(
        cat.roaming && !cat.sprinting,
        "Shakes must not override Follow Cursor being off"
    );
    assert!(
        cat.nodes[0].distance(on_cat) > CatSim::ROAM_FLEE_RADIUS,
        "Roaming cat should flee a cursor placed on it"
    );

    // Turning Follow Cursor back on ends roaming and the cat comes to the cursor
    cat.behaviors.follow_cursor = true;
    for _ in 0..(10 * 60) {
        cat.update(on_cat, DT);
    }
    assert!(!cat.roaming, "Turning Follow Cursor on should end roaming");
    assert!(
        cat.nodes[0].distance(on_cat) < 60.0,
        "Cat should follow the cursor again, head is {:.0}px away",
        cat.nodes[0].distance(on_cat)
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

#[test]
fn shaking_over_the_cat_sends_it_roaming_until_a_shake_calls_it_back() {
    const DT: f32 = 1.0 / 60.0;
    let lo = Vec2::new(0.0, 0.0);
    let hi = Vec2::new(1600.0, 1000.0);
    let start = Vec2::new(800.0, 500.0);
    let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
    cat.roam_bounds = Some((lo, hi));
    let shake = |cat: &mut CatSim, center: Vec2, strokes: usize| {
        for i in 0..strokes {
            let dir = if i % 2 == 0 { 1.0 } else { -1.0 };
            for &dx in &[-40.0, -20.0, 0.0, 20.0, 40.0] {
                cat.update(center + Vec2::new(dir * dx, 0.0), DT);
            }
        }
    };

    // 1. Put the cursor on the cat's back and let it settle, then shake it there
    let on_back = cat.nodes[3];
    for _ in 0..30 {
        cat.update(on_back, DT);
    }
    shake(&mut cat, on_back, 3);
    assert!(cat.roaming, "Shaking over the cat should send it roaming");
    assert!(
        !cat.sprinting,
        "Shaking the cat off must not make it sprint"
    );

    // 2. Shaking on after it was shaken off doesn't call it straight back
    shake(&mut cat, on_back, 6);
    assert!(cat.roaming, "The same shake must not end roaming");

    // 3. It walks away from the still cursor, and keeps away when the cursor follows it
    for _ in 0..240 {
        cat.update(on_back, DT);
    }
    assert!(
        cat.nodes[0].distance(on_back) > 150.0,
        "Roaming cat should move away from the cursor, head is {:.0}px away",
        cat.nodes[0].distance(on_back)
    );
    let chaser = cat.nodes[0];
    for _ in 0..60 {
        cat.update(chaser, DT);
    }
    for _ in 0..240 {
        cat.update(chaser, DT);
    }
    assert!(
        cat.nodes[0].distance(chaser) > CatSim::ROAM_FLEE_RADIUS,
        "Roaming cat should flee a cursor placed on it, head is {:.0}px away",
        cat.nodes[0].distance(chaser)
    );

    // 4. Left alone, it wanders around on its own inside the screen, clear of the cursor
    let still = chaser;
    let mut moved = 0.0;
    let mut prev = cat.nodes[0];
    for _ in 0..(90 * 60) {
        cat.update(still, DT);
        let head = cat.nodes[0];
        moved += head.distance(prev);
        prev = head;
        assert!(
            head.cmpge(lo).all() && head.cmple(hi).all(),
            "Cat left the screen at {head}"
        );
        assert!(
            head.distance(still) > 90.0,
            "Roaming cat came within {:.0}px of the cursor",
            head.distance(still)
        );
        assert!(cat.roaming && !cat.sprinting);
    }
    assert!(
        moved > 300.0,
        "Roaming cat should wander, moved only {moved:.0}px"
    );

    // 5. It ignores the cursor completely: a twin cat whose cursor sweeps around (out of
    // fleeing range) turns its head, ears and body exactly like one whose cursor stays still
    let mut frames = 0;
    while (cat.roam_going || cat.activity != Activity::Sitting || cat.roam_timer < 2.5)
        && frames < 60 * 60
    {
        cat.update(still, DT);
        frames += 1;
    }
    assert!(!cat.roam_going && cat.activity == Activity::Sitting);
    let mut twin = cat.clone();
    let away = (still - cat.nodes[0]).normalize();
    let side = away.perp();
    for frame in 0..120 {
        let t = frame as f32 * DT;
        let sweep =
            still + away * (40.0 + 40.0 * (t * 5.0).sin()) + side * (90.0 * (t * 3.0).sin());
        cat.update(still, DT);
        twin.update(sweep, DT);
        assert_eq!(twin.activity, cat.activity);
        assert_eq!(
            twin.head_yaw, cat.head_yaw,
            "Roaming cat's head followed the cursor"
        );
        assert_eq!(
            twin.body_angle, cat.body_angle,
            "Roaming cat turned toward the cursor"
        );
        assert_eq!(twin.ear_left_twitch, cat.ear_left_twitch);
        assert_eq!(twin.ear_right_twitch, cat.ear_right_twitch);
    }

    // 6. A shake calls it back at a gallop; then it follows the cursor normally
    shake(&mut cat, still, 3);
    assert!(!cat.roaming, "A shake should end roaming");
    assert!(cat.sprinting, "The cat should sprint back when called");
    for _ in 0..240 {
        cat.update(still, DT);
    }
    assert!(!cat.sprinting);
    assert!(
        cat.nodes[0].distance(still) < 60.0,
        "Called cat should come to the cursor, head is {:.0}px away",
        cat.nodes[0].distance(still)
    );
    let moved_cursor = still + Vec2::new(if still.x > 800.0 { -300.0 } else { 300.0 }, 0.0);
    for _ in 0..360 {
        cat.update(moved_cursor, DT);
    }
    assert!(
        cat.nodes[0].distance(moved_cursor) < 60.0,
        "After returning the cat should follow the cursor again"
    );
}

/// A cat roaming because Follow Cursor is off, inside a 1600x1000 screen
fn roaming_cat(start: Vec2) -> CatSim {
    let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
    cat.roam_bounds = Some((Vec2::ZERO, Vec2::new(1600.0, 1000.0)));
    cat.behaviors.follow_cursor = false;
    cat.update(start, 1.0 / 60.0);
    assert!(cat.roaming);
    cat
}

#[test]
fn cornered_roaming_cat_slips_out_along_the_wall() {
    const DT: f32 = 1.0 / 60.0;
    // Head in the top-left corner, cursor between it and the middle of the screen
    let mut cat = roaming_cat(Vec2::new(110.0, 40.0));
    let corner_head = cat.nodes[0];
    let cursor = corner_head + Vec2::new(60.0, 60.0);
    let mut picks = 0;
    let mut prev_target = cat.roam_target;
    for _ in 0..(6 * 60) {
        cat.update(cursor, DT);
        if cat.roam_target != prev_target {
            picks += 1;
            prev_target = cat.roam_target;
        }
    }
    assert!(
        cat.nodes[0].distance(cursor) > CatSim::ROAM_FLEE_RADIUS,
        "Cornered cat should get away from the cursor, head is {:.0}px away",
        cat.nodes[0].distance(cursor)
    );
    assert!(
        picks < 10,
        "Cornered cat kept re-picking where to go ({picks} times)"
    );
}

#[test]
fn roaming_cat_turns_aside_instead_of_walking_through_the_cursor() {
    const DT: f32 = 1.0 / 60.0;
    let mut cat = roaming_cat(Vec2::new(800.0, 500.0));
    let far = Vec2::new(1500.0, 950.0);
    let mut frames = 0;
    while !(cat.roam_going && cat.activity == Activity::Walking && cat.speed > 60.0) {
        cat.update(far, DT);
        frames += 1;
        assert!(frames < 60 * 60, "Cat never set off to wander");
    }
    // Park the cursor on its route, 90 px ahead
    let route = (cat.roam_target - cat.nodes[0]).normalize();
    let in_the_way = cat.nodes[0] + route * 90.0;
    let mut closest = f32::MAX;
    for _ in 0..(4 * 60) {
        cat.update(in_the_way, DT);
        closest = closest.min(cat.nodes[0].distance(in_the_way));
    }
    assert!(
        closest > 45.0,
        "Roaming cat walked up to {closest:.0}px of the cursor in its way"
    );
}

#[test]
fn shaking_just_in_front_of_a_resting_cat_calls_it_instead_of_shooing_it() {
    const DT: f32 = 1.0 / 60.0;
    let start = Vec2::new(800.0, 500.0);
    let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
    // Let it walk over and settle, so the cursor sits about a stop radius ahead of its head
    let cursor = Vec2::new(1000.0, 500.0);
    for _ in 0..(5 * 60) {
        cat.update(cursor, DT);
    }
    for i in 0..30 {
        let dx = if (i / 5) % 2 == 0 { -40.0 } else { 40.0 };
        cat.update(cursor + Vec2::new(dx, 0.0), DT);
    }
    assert!(!cat.roaming, "A shake in front of the cat must not shoo it");
}

#[test]
fn shaking_off_a_crouching_cat_calls_off_its_pounce() {
    const DT: f32 = 1.0 / 60.0;
    let start = Vec2::new(800.0, 500.0);
    let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
    cat.roam_bounds = Some((Vec2::ZERO, Vec2::new(1600.0, 1000.0)));
    let on_back = cat.nodes[3];
    for _ in 0..30 {
        cat.update(on_back, DT);
    }
    cat.trigger(TriggerKind::Pounce);
    cat.update(on_back, DT);
    assert!(matches!(cat.activity, Activity::StalkWiggle { .. }));
    for i in 0..20 {
        let dx = if (i / 5) % 2 == 0 { -40.0 } else { 40.0 };
        cat.update(on_back + Vec2::new(dx, 0.0), DT);
        if cat.roaming {
            break;
        }
    }
    assert!(cat.roaming, "Shake over the crouching cat should shoo it");
    for _ in 0..120 {
        cat.update(on_back, DT);
        assert!(
            !matches!(
                cat.activity,
                Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
            ),
            "Shaken-off cat still went for the cursor: {:?}",
            cat.activity
        );
    }
}

/// Teases a settled cat the way a person would: an aimed move out to `spot`, then flicking the
/// pointer back and forth across the cat's line of sight. Returns whether it stalked and pounced.
fn tease_with_pointer(
    cat: &mut CatSim,
    from: Vec2,
    spot: Vec2,
    axis: Vec2,
    hz: f32,
    dt: f32,
) -> (bool, bool) {
    let (mut stalked, mut pounced) = (false, false);
    let move_steps = (0.3 / dt) as usize;
    for i in 1..=move_steps {
        cat.update(
            from.lerp(spot, smoothstep(i as f32 / move_steps as f32)),
            dt,
        );
    }
    let mut t = 0.0;
    while t < 4.0 && !pounced {
        t += dt;
        let flick = (std::f32::consts::TAU * hz * t).sin() * 20.0;
        cat.update(spot + axis * flick, dt);
        stalked |= matches!(cat.activity, Activity::StalkWiggle { .. });
        pounced |= matches!(cat.activity, Activity::Pouncing { .. });
    }
    (stalked, pounced)
}

#[test]
fn moving_the_pointer_away_and_wiggling_it_makes_a_sitting_cat_stalk_and_pounce() {
    for temperament in Temperament::ALL {
        for (hz, dt, axis) in [
            (4.0, 1.0 / 60.0, Vec2::Y),
            (5.0, 1.0 / 144.0, Vec2::Y),
            (4.0, 1.0 / 30.0, Vec2::new(0.5, 0.87)),
            // A horizontal wiggle in pounce range is play, not a shake calling the cat over
            (4.5, 1.0 / 60.0, Vec2::X),
        ] {
            let start = Vec2::new(400.0, 400.0);
            let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
            cat.behaviors.temperament = temperament;
            let rest = cat.nodes[0] + Vec2::new(38.0, 0.0);
            for _ in 0..(2.0 / dt) as usize {
                cat.update(rest, dt);
            }
            assert_eq!(cat.activity, Activity::Sitting, "{temperament:?}");

            let spot = cat.nodes[0] + Vec2::new(150.0, 0.0);
            let (stalked, pounced) = tease_with_pointer(&mut cat, rest, spot, axis, hz, dt);
            assert!(
                stalked && pounced && !cat.sprinting,
                "{temperament:?} at {hz} Hz, dt {dt:.3}, axis {axis}: stalked {stalked}, pounced {pounced}, sprinting {}",
                cat.sprinting
            );
        }
    }
}

#[test]
fn ordinary_pointing_near_the_cat_never_starts_a_stalk() {
    // Aimed moves between spots around the cat, each overshooting a little and correcting back,
    // with short pauses: busy everyday mouse use, not a laser-pointer tease
    let dt = 1.0 / 60.0;
    for temperament in Temperament::ALL {
        let start = Vec2::new(500.0, 400.0);
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        cat.behaviors.temperament = temperament;
        let mut cursor = cat.nodes[0] + Vec2::new(38.0, 0.0);
        let mut max_excitement = 0.0f32;
        for i in 0..40 {
            let angle = i as f32 * 0.6;
            let target = start + Vec2::from_angle(angle) * (110.0 + (i % 3) as f32 * 30.0);
            let overshoot = target + (target - cursor) * 0.08;
            let from = cursor;
            for k in 1..=15 {
                cursor = from.lerp(overshoot, smoothstep(k as f32 / 15.0));
                cat.update(cursor, dt);
                max_excitement = max_excitement.max(cat.play_excitement);
            }
            for k in 1..=6 {
                cursor = overshoot.lerp(target, smoothstep(k as f32 / 6.0));
                cat.update(cursor, dt);
            }
            for _ in 0..9 {
                cat.update(cursor, dt);
            }
            max_excitement = max_excitement.max(cat.play_excitement);
            assert!(
                !matches!(
                    cat.activity,
                    Activity::StalkWiggle { .. } | Activity::Pouncing { .. }
                ),
                "{temperament:?} stalked ordinary pointing after {i} moves"
            );
        }
        assert!(
            max_excitement < 0.4,
            "{temperament:?} got excited ({max_excitement:.2}) by ordinary pointing"
        );
    }
}

/// A cat that has walked up to `cursor` and settled there
fn settled_cat(cursor: Vec2, kitten: bool) -> CatSim {
    let mut cat = CatSim::new(cursor, if kitten { 0.6 } else { 1.0 }, CoatStyle::Calico);
    cat.kitten = kitten;
    for _ in 0..(60 * 6) {
        cat.update(cursor, 1.0 / 60.0);
    }
    cat
}

fn friend(mut n: Neighbor) -> Neighbor {
    n.familiarity = 0.8;
    n.affection = 0.6;
    n
}

#[test]
fn a_resting_cat_watches_another_cat_walk_past() {
    let start = Vec2::new(500.0, 500.0);
    let mut cat = settled_cat(start, false);
    let head = cat.nodes[0];
    let ahead = Vec2::from_angle(cat.body_angle);
    let across = ahead.perp();
    let (mut sampled, mut watching) = (0, 0);
    for frame in 0..240 {
        let t = frame as f32 / 60.0;
        let other = head + ahead * 90.0 + across * (-120.0 + 60.0 * t);
        cat.neighbors = vec![Neighbor {
            speed: 60.0,
            pose: Pose::Moving,
            ..Neighbor::at(other, across.to_angle(), 1.0)
        }];
        cat.update(start, 1.0 / 60.0);
        if t > 0.5 && matches!(cat.activity, Activity::Sitting | Activity::Lying) {
            sampled += 1;
            let to_other = (other - cat.nodes[0]).to_angle();
            if wrap_angle(cat.body_angle + cat.head_yaw - to_other).abs() < 0.35 {
                watching += 1;
            }
        }
    }
    assert!(sampled > 30, "only {sampled} calm frames");
    assert!(
        watching * 10 >= sampled * 8,
        "watched the passing cat in {watching} of {sampled} frames"
    );
}

#[test]
fn a_cat_racing_past_wakes_a_sleeper_partway() {
    let start = Vec2::new(500.0, 500.0);
    let mut cat = settled_cat(start, false);
    cat.trigger(TriggerKind::Sleep);
    let body = cat.body_point();
    let pass = |cat: &mut CatSim, pose: Pose| {
        for frame in 0..60 {
            let other = body + Vec2::new(-90.0 + 3.0 * frame as f32, 30.0);
            cat.neighbors = vec![Neighbor {
                speed: 180.0,
                pose,
                ..Neighbor::at(other, 0.0, 1.0)
            }];
            cat.update(start, 1.0 / 60.0);
        }
        cat.neighbors.clear();
    };
    pass(&mut cat, Pose::Moving);
    assert_eq!(
        cat.activity,
        Activity::Sleeping,
        "a cat walking by doesn't wake it"
    );
    pass(&mut cat, Pose::Racing);
    assert_eq!(
        cat.activity,
        Activity::Lying,
        "a galloping cat does, halfway"
    );
    // ...and it dozes off again
    for _ in 0..(60 * 30) {
        cat.update(start, 1.0 / 60.0);
    }
    assert_eq!(cat.activity, Activity::Sleeping);
}

#[test]
fn a_friend_lying_nearby_makes_a_cat_drowsy() {
    let start = Vec2::new(500.0, 500.0);
    let time_to_lie_down = |with_friend: bool| {
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        for frame in 0..(60 * 40) {
            if with_friend {
                let spot = cat.nodes[0] + Vec2::new(0.0, 70.0);
                cat.neighbors = vec![friend(Neighbor {
                    pose: Pose::Sleeping,
                    ..Neighbor::at(spot, 0.0, 1.0)
                })];
            }
            cat.update(start, 1.0 / 60.0);
            if matches!(
                cat.activity,
                Activity::Lying | Activity::DramaticFlop { .. } | Activity::BellyUp { .. }
            ) {
                return frame as f32 / 60.0;
            }
        }
        f32::INFINITY
    };
    let alone = time_to_lie_down(false);
    let together = time_to_lie_down(true);
    assert!(
        together + 2.0 < alone,
        "lay down after {together:.1} s next to a friend, {alone:.1} s alone"
    );
}

#[test]
fn a_cat_raises_its_tail_walking_up_to_a_friend_and_holds_it_still_near_a_foe() {
    let start = Vec2::new(300.0, 500.0);
    let walk_past = |other: Neighbor| {
        let mut cat = CatSim::new(start, 1.0, CoatStyle::Calico);
        let (mut raised, mut still) = (0.0f32, 0.0f32);
        let spot = start + Vec2::new(180.0, 50.0);
        for _ in 0..(60 * 4) {
            cat.neighbors = vec![Neighbor {
                pose: Pose::Sitting,
                ..other
            }];
            cat.neighbors[0].head = spot;
            cat.update(start + Vec2::new(400.0, 0.0), 1.0 / 60.0);
            raised = raised.max(cat.tail_raise);
            still = still.max(cat.tail_still);
        }
        (raised, still)
    };
    let stranger = Neighbor::at(Vec2::ZERO, 0.0, 1.0);
    assert_eq!(walk_past(stranger), (0.0, 0.0));
    let (raised, _) = walk_past(friend(stranger));
    assert!(raised > 0.6, "tail up to {raised} for a friend");
    let (raised, still) = walk_past(Neighbor {
        affection: -0.5,
        ..stranger
    });
    assert!(
        raised == 0.0 && still > 0.6,
        "tail held still ({still}) near a foe"
    );
}

#[test]
fn a_kitten_naps_and_wakes_by_itself() {
    let start = Vec2::new(500.0, 500.0);
    for kitten in [false, true] {
        let mut cat = settled_cat(start, kitten);
        cat.trigger(TriggerKind::Sleep);
        let mut woke = false;
        for _ in 0..(60 * 80) {
            cat.update(start, 1.0 / 60.0);
            woke |= cat.activity != Activity::Sleeping;
        }
        assert_eq!(woke, kitten, "kitten: {kitten}");
    }
}
