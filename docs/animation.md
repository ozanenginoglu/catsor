# Animation & rendering

The cat is a 2D kinematic skeleton viewed from directly above. `CatSim` (`src/cat/`) moves the skeleton every frame, and `render.rs` builds a body around it with tiny-skia paths. There are no sprites. All lengths below are **unscaled px** and get multiplied by `scale` in the code.

## Body model

```
 head  neck  shoulders  waist  hips  tail-base   tail …………………… tip
  [0]───[1]─────[2]──────[3]────[4]─────[5]────[6]─[7]─[8]─[9]─[10]─[11]
      6.2   7.6      8.8     8.2    6.8    4.5 4.3 4.1 3.8 3.5 3.2
        └──────── body ≈ 38 px ───────┘   └──────── tail ≈ 24 px ───────┘
```

- `nodes[0..12]` are world-space (screen) positions. `BODY_NODES = 6`, `TAIL_NODES = 6`.
- Body segment lengths are `STANDING_BODY_LENGTHS × posture_extension / 1.60 × scale`, so **`posture_extension` stretches or compresses the whole spine** (1.60 = normal standing length).
- `paws[0..4]` are `[LF, RF, LH, RH]`. Each one is planted at a world position and has its own swing state.
- `body_angle` is the heading in radians, and `head_yaw` turns the head relative to it.

### Carrier spine vs. displayed spine

There are two copies of the body chain:

- **`carrier_nodes`** is the smooth path the body follows: each segment trails the previous one, with bending limited to 0.35 rad per joint (0.52 while chasing its tail).
- **`nodes`** is the carrier plus *temporary* offsets: the lateral S-wave of the gait, a sideways head bob, and the hip wiggle.

Keeping the two separate means a temporary wobble never feeds back into the path the body follows, so the cat can't slowly drift off course or twist its spine.

### Window anchor

`window_anchor` (returned by `window_center()`) is a stable point on the torso, and the overlay window is centred on it. While the cat moves, it follows the carrier waist (`carrier_nodes[3]`). While resting, the spine is rebuilt **outward from the waist** in both directions, so getting up, crouching, curling or breathing never slides the cat across the screen. During Eating, the anchor creeps toward the placed bowl rather than the bowl moving toward the cat. See commit `9274373` for the jitter this fixed.

## Posture

Each activity has a target `(posture_extension, curl_amount)`. Both values are approached with exponential smoothing, so changing poses never snaps.

| Activity | extension | curl |
|---|---|---|
| Sleeping | 0.98 | 0.56 × curl_side (tight ball) |
| Sitting / Grooming / PawBatting / AloofNotice | 1.16 | 0.09 × side |
| Eating | 1.20 ± nibble bob (pauses at 1.14 for 2.8 s of every 16 s) | 0.05 × side |
| Lying | 1.26 | 0.07 × side |
| BellyUp | 1.30 | roll twist + slow wiggle |
| DramaticFlop | 1.38 | −0.10 × side |
| StalkWiggle | 1.50 (low crouch: the legs fold under, so from above the body stays long) | 0 |
| Stretching | 1.52 + 0.32·sin(πp) | 0 |
| Walking / Standing | 1.60, or 1.58 ± 0.34 while galloping (see gallop flex) | 0 |
| Pouncing | 1.50 + 0.42·`pounce_stretch(p)`: extends during push-off, stays long in the air, gathers on landing | 0 |
| Zoomies | gallop flex wave (zoomies are a gallop) | 0 |
| Circling | 1.45 | 0.22 × turn direction (curved around the circle) |

Values blended in alongside posture:
- `belly_roll` (0–1, roll-on at 2.4/s and roll-off at 2.8/s): once it passes 0.35 the paws are drawn **on top of** the torso, and the belly is shown.
- `sleep_blend` (0–1): shortens the tail segments by up to 48% and tucks the tail under the chin.
- `tail_puff`: 1.0 normally, 1.65 during zoomies, up to 1.25 while galloping.
- `breath_phase`: a continuous phase that sets the breathing rate (sleeping 1.9 ≈ 18 breaths/min, belly-up 1.85, otherwise 2.3 rad/s ≈ 22/min) and widens the waist and shoulders.

> **The phase pattern.** `breath_phase`, `tail_phase` and `gait_cycle` are kept as running phases instead of being computed from `time × freq`. If the frequency changed while the phase was computed from time, the phase would jump. After hours of uptime, `time × freq` also loses `f32` precision. `tail_phase` wraps at 1024 and blends two noise samples half a period apart, so the wrap is invisible. Follow this pattern for any new oscillator. Tests cover it: `high_uptime_tail_motion…` and `repeated_tail_chasing_and_phase_wraps…`.

## Locomotion

### Steering (Walking)

- Turn rate is limited to 5.6 rad/s (10.5 rad/s when sprinting) and gets up to 65% faster as the cat nears its target. Speed is scaled by `(1 + cos(err))/2`, so sharp turns slow the cat down. Together these keep the turning radius `v/ω` small, so the cat doesn't swing in wide loops (commit `8632ca6`).
- Cruise speed is **104 px/s walking**, ramping up to **152 px/s trotting** when the target is 220–360 px away. Sprinting runs at **290–365 px/s**. All three are multiplied by the temperament's speed multiplier, except that the **Realistic** temperament uses its own real-cat walk and trot speeds (66 and 135 px/s; see [behavior.md](behavior.md#temperaments)).
- Braking: speed eases off as `(d / slow_radius)^k` within 95 px (68 px when sprinting).

### Gaits

`gait_cycle` ∈ [0,1) advances in proportion to the distance covered: `speed / stride_length()`. Like a real cat, the stride lengthens with speed instead of only stepping faster: `30 + 0.155·v` px when walking or trotting (46 px at 104 px/s, 54 px at 152 px/s) and `40 + 0.16·v` px when galloping (98 px at 365 px/s), blended by `gallop_weight`. Cadence is never clamped. Above 4.2 strides/s (`MAX_CADENCE`) the stride grows instead, so the distance per cycle always matches the distance covered. `gait_weight` (0–1) fades the stride animation in and out as speed changes.

| Gait | Used when | Paw lift-off phases | Swing fraction |
|---|---|---|---|
| **4-beat lateral walk** | default | LH 0.00 → LF 0.25 → RH 0.50 → RF 0.75 | 0.34 |
| **2-beat diagonal trot** | speed > 128 px/s (back to walk below 114) | LF+RH 0.375, RF+LH 0.875 | 0.42 |
| **Rotary gallop** (right lead) | `gallop_weight` > 0.5 (sprinting; leaves below 0.35) | RH 0.41 → LH 0.51 → LF 0.73 → RF 0.83 (touchdowns RH 0.03 → LH 0.13 → LF 0.35 → RF 0.45) | 0.62 |

### Gait changes

`CatSim::gait` (`Gait::Walk/Trot/Gallop`) switches with the hysteresis shown above. Each pattern's overall phase is chosen so moving between neighbouring gaits shifts every leg as little as possible (walk → trot moves each leg 0.125 of a cycle). The pattern is never swapped in one go: `update_gait_pattern` slides each leg's lift-off phase (`leg_offsets`) and the `swing_fraction` toward the new pattern, by at most half of how far `gait_cycle` advanced that frame. Each leg's own phase therefore keeps moving forward, and a change takes about one stride. A paw starts a swing only when its own cycle wraps past lift-off, and keeps swinging until touchdown, so a gait change can't pop a planted paw into a half-finished swing or cut a swing short.

When the cat starts stepping after standing, the gathered paws sit under their sockets, which is mid-stance. A leg that's due to swing lifts off immediately for a short first step. A leg in early stance is treated as mid-stance. Steps are also shorter while `gait_weight` ramps up or down (down to half length), and the gait cycle uses the same length, so starting and stopping don't drag the paws. `gait_changes_never_cut_swings_short…` drives walk → trot → walk → gallop → trot → walk and checks all of this.

Spine sway, the sideways head bob and the tail's gait wave are timed to the left hind paw's step cycle (`gait_cycle − leg_offsets[LH]`), so they stay in step with the footfalls in every gait.

Each paw cycle has two parts:
- **Swing:** the paw lifts and follows a smoothstep curve to a target ahead of its shoulder or hip socket. The start point is kept relative to the moving socket (`step_start` is an offset from it): the leg folds as it lifts, so the paw comes forward with the body at once instead of trailing further behind, and it still settles onto its landing spot with zero speed.
- **Stance:** the paw stays fixed in world space while the body moves over it.

While a paw is planted, the body travels `(1 − swing fraction) × stride` over it. A forepaw lands half that sweep ahead of its socket and lifts off half of it behind. A hind paw's sweep is weighted forward (`HIND_LEAD_SHARE` = 0.62 ahead, 0.38 behind), as in a real cat: it lands well forward under the belly and pushes off only a little behind the haunch. Either way the paw stays put until lift-off instead of being dragged (`planted_paws_stay_put…` tests this). The sockets sit 4 px ahead of the shoulder node and 1 px behind the hip node. The hind paws track only 3.4 px either side of the midline (`HIND_TRACK`), because cats walk with a narrow track, hind feet tucked under the haunches. That puts the fore paw's touchdown under the chin and keeps the hind paw at most ~8–9 px behind the tail base at walk and trot speeds and ~10 px in a gallop (`hind_paws_stay_close…` tests this). The cost is that the hind paw lands 8–16 px ahead of the fore paw's print, an overstep that grows with walking speed. A safety clamp, sized from the walk's stance (the longest of any gait), keeps paws from reaching farther than that from the socket. While galloping, the fore sockets move up to 7 px further forward (`GALLOP_FORE_REACH`), so the forelegs reach out past the chin at touchdown.

The gallop's duty factor is about 0.35. All four paws are off the ground from RF lift-off (0.83) until RH touches down again, which is the *gathered flight* of a real cat's rotary gallop. When the cat is nearly stopped, the paws gather back under the body.

While spinning on the spot (TailChasing, Circling), each paw lands half a stance ahead along its **own** socket's smoothed velocity, since the shoulders and hips each move at their own speed and in their own direction, and the stride is shortened so the body turns at most 1.6 rad (`MAX_SPIN_PER_STRIDE`) per stride. A fast tail chase therefore takes quick little steps instead of sweeping planted paws round with it.

### Spine sway and gallop flex

- **Side-to-side S-wave** (`gait_spine_sway`): each segment gets a lateral bend based on the left hind paw's step phase. The bend is small at the shoulders (0.055 rad) and larger at the lumbar and pelvis (0.195 and 0.23 rad), each a bit later in the phase than the one before. This shifts the hips over whichever hind leg is planted. The wave is smaller at low speeds and while trotting, and mostly switched off while galloping (commit `7df46c1`).
- **Gallop flex:** while galloping, `posture_extension` follows `1.58 − 0.34·cos(2π·(gait_cycle + 0.05))`. The spine is **most bunched** as the hind paws land under the belly (RH touchdown at 0.03). It **stretches out** through hind stance to full extension at hind lift-off and fore touchdown (~0.48), then bunches up again through fore stance and the gathered flight. The 0.05 phase lead (`GALLOP_FLEX_LEAD`) makes up for the lag from posture smoothing. The head also gets an extra step forward proportional to the change in extension. That makes the body move like an accordion instead of sliding as a rigid shape (commit `cc879e4`).

### Tail

Each tail segment chases a target angle. The follow rate is 16/s at the base and gets slower toward the tip, so motion ripples down the tail. The target angle combines:
- the direction of the rump,
- fbm noise (a per-activity amplitude and speed, delayed along the tail),
- while walking, a wave that trails the gait phase,
- a per-activity **wrap bias**: curled toward the side the body curls to when sitting or sleeping, and leaning *against* the turn when walking or pouncing (`-angular_vel × 0.065`, at most 0.28 rad),
- extra noise and a hook at the tip,
- while Sitting, Grooming or AloofNotice, a **sitting wrap** (`sit_tail_wrap`, eases in over ~1 s): extra bend per segment (`SIT_TAIL_WRAP`) turns the tail sideways off the rump, runs it forward along the flank and hooks it in toward the front paws. Only the tip keeps flicking. It unwraps quickly when the cat moves or turns.

The tail is always drawn **under** the body and head, so it never covers the face. It shares the body's outline, so no line separates it from the rump.

### Head & ears

`update_head_and_ears` sets `head_yaw` per activity. Normally the head follows the cursor up to `MAX_HEAD_YAW` (1.4 rad, ~80°) to either side. It stays within about ±0.6 rad during PawBatting, StalkWiggle and Pouncing. If the cursor goes more than ~100° behind a sitting cat, the cat shuffles round on the spot to face it (`SIT_TURN_RATE`, 1.8 rad/s, pivoting around its torso) instead of standing up. It looks back toward the tail while chasing it, tucks around (2.38 rad × curl side) when sleeping, and bobs into the bowl while eating. The ears are separate angles. Negative is flattened back ("airplane ears": stalking, zoomies) and positive is pricked forward.

## Rendering (`render.rs`)

`render_cat(cat, pixmap, clip_mask, window_origin)` draws into a premultiplied-RGBA tiny-skia `Pixmap` the same size as the overlay window. World positions are converted to window space by subtracting `window_origin`. Each node gets a tangent and a normal vector, and outlines are built by offsetting along the normals.

Draw order:

1. Soft ground shadow
2. Food bowl and kibble (while eating; it empties as `feeding_fill_ratio` drops)
3. **Outline pass** (`Pass::Outline`): the outlines of the legs, tail, body and head, in that order. The fills drawn after it cover the outlines wherever parts overlap, so the cat gets **one shared silhouette** with no seams where the tail, legs and neck join the body. Body and head outlines are stroked at twice their visible width, because the fill covers the inner half
4. Paws, with toe beans (`Pass::Fill`). When `belly_roll > 0.35` the paws are drawn after the body instead, with their own outline and fill, because they lie on top of the belly. Each leg is a tapered shape that narrows from the shoulder or hip to a slim wrist and ends in an oval paw, wider than the wrist, centred on the paw position; two toe notches mark the top of the foot (left off when the soles face up). While walking, trotting and galloping, each leg is drawn from its shoulder or hip anchor to the simulated `paws[i]` position. Walking and trotting draw only legs that reach past the chest or haunches. A hind leg is drawn from the back of the haunch (4.5 px behind the hip, 3.2 px out), so only the lower leg shows beyond the rump instead of a long stick running along its side. Galloping draws all four and lets the body cover what's underneath, and a swinging hind leg flares up to 4 px outward, because in a rotary gallop the hind feet pass outside the forefeet
5. Tail: a band along the tail curve (`tail_band`) that is about 1.45× thicker where it leaves the rump, thins towards the tip and ends in a round cap. It has a darker shade layer and a lighter centre; calico rings are short bands of the same shape
6. Body: an outer flank-shade fill, then the inner base fill. The outer contour is also written into `clip_mask`, so **coat patches, the exposed belly and muscle creases are clipped to the body silhouette**
7. Head, snout and nose, head patches, and ears. The head's darker rim is drawn only outside the torso (the still-set body mask, inverted); over the shoulders it takes the base colour, so the neck blends into the body. When curled up asleep (`|curl_amount|` above ~0.42), a faint outline is drawn where the head lies over the torso so it stays readable. The mask is then cleared and refilled with the head outline, so head patches are clipped to the head

How wide the body is at each node comes from `body_half_widths`. The base widths are neck 5.8, shoulders 8.9, waist 7.8, hips 8.5 and rump 5.0. On top of that, breathing adds width, a compact posture plumps the waist and hips, a stretched posture slims the shoulders and waist, and crouching widens the hips.

Coat colours come from `Palette::for_coat`. Each `CoatStyle` sets a base, flank shade, outline, crease, outer and inner ear, and nose colour, plus optional primary and secondary patch colours. Calico is cream with orange and dark patches, and Tuxedo is light with dark patches. Brown, Charcoal and Silver are solid colours with no patches. `coat_swatch_colors` gives the menu's coat swatches the same colours.

### Tray icon

`render_tray_icon` draws the cat's head from the front (`build_tray_head_path`) in the current coat colours, so it fits in a 16–24 px tray slot. The head shape droops when the cat is sleeping or hidden. If the tray can't take an ARGB icon (`opaque_bg`), the icon is drawn on a solid dark background. A test (`tray_icon_never_clips_at_edges…`) checks every coat and state combination for clipping at the edges.

## Visual checking

```sh
cargo run -- --preview-sheet /tmp/sheet.png --scale 1.5
```

This renders all five coats twice: walking on the top row, and a rest or play pose on the bottom row. It doesn't need a display, which makes it the quickest way to check a rendering change.
