# Multiple cats (design)

Status: **all five phases are implemented** (multiple cats, slots, shared bowl, per-cat tray pages; awareness, kitten tuning, the leash, fetching and bonds; greetings, grooming each other, sleeping together and nursing; play and friction; coat inheritance, growing up and carrying by the scruff; see `behavior.md` → "Several cats", "Sharing the bowl", "Kittens", "Doing things together", "Playing and squabbling" and "Bonds"). What's left here is the original proposal, with notes where the result differs. When a phase lands, move what it settles into `behavior.md` / `animation.md` / `platform-notes.md` and trim it from here.

Catsor today runs exactly one `CatSim` in one overlay window. This design covers two things: running several cats at once, and making them aware of each other: greeting, grooming, playing, squabbling, and mother cats raising kittens.

## Decisions

| Question | Decision |
|---|---|
| Who follows the cursor? | **Every cat**, each to its own slot around the cursor |
| Feeding | **One shared bowl**; cats take turns |
| Friction | **Yes, mild**: mostly in play (kittens especially) and around the bowl. It always ends with someone walking off or grooming, never real fighting |
| Relationships | **Persisted** across restarts |
| Kittens | **Yes**, with explicit **mother → kitten** relations |
| Tray | **A page per cat**: clicking a cat's row drills into its page inside the same popup, with a Back row |
| Follow Cursor / Playful Hunting | **One global switch each**, applying to every cat; they stay top-level rows |

## Cats as data

Each cat has a stable **name**. It's the key for prefs, bonds and the tray.

```toml
# config.toml
scale = 1.0            # global UI size, as today

[[cat]]
name = "Luna"
coat = "calico"
temperament = "balanced"

[[cat]]
name = "Pip"
coat = "random:00c0ffee"
temperament = "playful"
kitten = true
mother = "Luna"        # must name another, non-kitten cat
```

- `size` (optional, per cat) multiplies the global scale. Kittens default to `0.6`, adults to `1.0`. Every `CatSim` distance is already `× self.scale`, so a kitten is mostly a smaller cat plus different tuning.
- Migration: the old top-level `coat` / `temperament` keys become `cats[0]`, named "Luna". An invalid `mother` (missing, a kitten, or itself) is ignored, like other invalid entries.
- CLI `--coat/--temperament` apply to the first cat. A cap of **6 cats** keeps the per-cat window and render cost small.
- **Coat inheritance:** "Add kitten" rolls a coat from the mother's (`Coat::inherit(&mother, seed)`). Kittens share her base colours, pattern and markings with random variation; a calico mother can have calico, tortie-ish or solid kittens. This is optional and only applies when the tray creates the kitten; a hand-written `coat` always wins. *(Done. The kitten's coat is stored as `litter:<seed>:<mother's coat id>`, so it follows a later change to a custom mother coat. Dilute (greyed) kittens turn up now and then.)*
- Newly added cats walk in from the nearest screen edge rather than appearing in place.

## Kittens

Kitten tuning (on top of temperament, which they still have; default Playful):

- Higher play drive, shorter attention span, frequent short naps (sleeps more often but wakes sooner).
- Clumsier: pounces sometimes overshoot or end in a tumble (a brief BellyUp); faster cadence and a bouncier trot.
- Tail held up more often; more curious about the other cats than adults are.
- **Growing up** (opt-in, `grows_up = true`): size and tuning ease toward adult over real days (persisted birth date), and `kitten` turns off at the end. The mother bond stays. *(Done over 30 days, toggled from the kitten's tray page. A kitten grows to the size of a grown cat, or stays at its `size` if that is already bigger.)*

## Following the cursor

All cats follow. Instead of one `rest_target` per cat at `stop_radius` from the cursor, the `Clowder` (see Architecture) hands out **rest slots** on an arc around the cursor:

- Slots are spaced by body size and assigned by approach angle, so cats don't cross paths to reach them. Assignments are sticky, so they don't swap every frame.
- A kitten's slot is next to its mother's. *(Not done: kittens take their own slots after the grown cats; the leash keeps them within reach of their mother.)*
- Temperament keeps working per cat: an Aloof cat can still hang back or take its AloofNotice pause, and a Lazy cat may stay asleep while the others move.
- **Spacing:** separation steering while walking, plus a personal-space radius by temperament (Aloof large, Playful small, kitten↔mother almost zero). Cats only overlap during joint activities.
- **Laser wiggle:** every cat in range gets excited, but only one gets to pounce on the cursor. The losers sit and flick their tails, or pounce on the winner's tail. *(Done. Only kittens and Playful cats go for the winner's tail; the winner doesn't notice.)*
- **Shake over a cat** starts roaming for that cat only.
- **Follow Cursor off** makes every cat roam. Kittens then stay within the leash of their roaming mother instead of wandering alone, and friends sometimes wander together. **Playful Hunting** is likewise one switch for everyone.

## Shared bowl

- One bowl; the Clowder runs a **queue**. One cat eats; the others sit near the bowl watching, tails twitching.
- Queue order depends on bonds and temperament: a **mother always lets her kittens eat first** and waits beside them, and Aloof cats wait their turn.
- Friction: a pushy cat (Playful, or a kitten with a weak bond to the eater) sometimes head-butts in. The eater either gives way, or flattens its ears and gives a short paw swat, and the pushy cat backs off. *(Done; the push is walking up beside the eater's face rather than a drawn head-butt.)*
- The eating session ends when every queued cat has eaten or the feed preset ends. Keep-awake stays as it is.

## Social behaviours

These are in roughly ascending cost. Each is a new `Activity` variant (or a role inside one) driven by the Clowder.

### Awareness (always on)
- **Head tracking:** a resting cat turns its head toward another cat moving nearby, as it does for the cursor.
- **Tail-up** when approaching a cat it likes; low or still tail toward one it doesn't.
- **Copying:** when one sits or grooms, a nearby friend is more likely to do the same.
- **Startle:** a cat galloping past a sleeper can wake it partway (Sleeping → Lying).

### Greetings (after time apart)
- **Nose boop:** walk up to each other, touch noses ~1 s, carry on.
- **Head bunt / cheek rub:** one rubs along the other's flank, tail hooking over its back. *(Done with the tail held up and hooked; it isn't drawn over the other cat's back.)*
- **Sniff circle:** half a turn around each other.

### Bonding
- **Grooming each other:** one licks the other's head and neck while both sit or lie. The groomed cat leans in, then may get up and leave in a huff. *(Done with a "huff" that is grooming itself afterwards rather than leaving.)*
- **Sleeping together:** curl up together, nested head to tail. Several kittens make a pile. *(Done as lying down alongside a sleeping friend, side by side, one pair at a time: a cat already curled up against another can't be joined by a third, so there are no piles yet.)*

### Play
- **Tail stalk:** StalkWiggle → Pouncing aimed at another cat's tail tip instead of the cursor. Mostly reuses existing code.
- **Chase:** one gallops off, the other chases; they sometimes swap roles. *(Done: the runner circles the cursor, so a chase never leaves the screen area the user is at.)*
- **Wrestle:** a short tangle, both rolling BellyUp and PawBatting, then both groom ("we weren't fighting"). *(Done face to face, taking turns; the two bodies aren't drawn tangled together.)*
- **Zoomies spread:** a Playful friend or kitten joins a cat that has the zoomies.

### Friction
- **Play escalation** (mostly kitten↔kitten): a wrestle or chase gets too rough, one **puffs up** (thicker tail outline, ears flattened back, body arched a little), there's a short standoff, they break apart, and both groom. *(Done; from above, the arch shows as a slightly shorter, bowed body standing side-on, with its fur fluffed out.)*
- **Pestering:** a kitten keeps pouncing on a non-mother adult's tail; the adult tolerates it for a while, then gives a paw swat or walks off with its tail held still.
- **Crowding an Aloof cat:** it turns away and leaves; repeated crowding means a standoff. *(Not done: Aloof cats only keep more space and squabble more readily.)*
- Friction costs a little affection (see Bonds); greetings and grooming each other win it back.

## Mother and kitten

- **Leash:** the kitten follows the cursor but stays within a leash radius of its mother. If she's too far, it trots over to her first, tail up.
- **Watching over:** the mother tracks her kitten with her head while resting. If it strays past a larger radius, she gets up and fetches it (walks to it, nose boop, walks back with it following). *(Done without the nose boop, which comes with the greetings in phase 3.)*
- **Carrying by the scruff** (stretch goal): if the kitten is very far, or asleep away from her, she picks it up by the scruff and carries it back. The kitten hangs limp from her mouth, with its chain anchored to her head node. This needs special handling for the render order and windows. *(Done for a kitten asleep more than 1.3 leashes away, and for 35% of fetches of an awake roaming kitten. The kitten is anchored at her mouth rather than her head node. It is drawn in her window, under her, instead of restacking windows. She puts it down when she stops at her spot, or when a sprint, food, play or being shaken off interrupts.)*
- **Grooming:** the mother grooms her kitten much more often than adults groom each other. The kitten squirms and escapes after a while (mild friction that costs no affection). *(The squirming isn't done.)*
- **Nursing / naps:** when the mother lies on her side (DramaticFlop pose), her kittens curl into the curve of her belly. Awake kittens there knead (front paws alternating). *(Done for the mother lying down in any pose or asleep: the kitten kneads 6–12 s against her side, then naps there.)*
- **Tail lure:** the mother flicks her tail on purpose to get the kitten to play. She tolerates tail pounces far longer than other adults do, and ends it with a gentle head boop. *(Done, the boop as a nose boop.)*
- **Food:** her kittens eat first (see Shared bowl).
- **Siblings** (same mother) start with a high bond: more wrestling, sleeping in piles, and more play escalation.

## Bonds (persisted)

Every pair of cats has a bond:

- `familiarity` (0–1): grows slowly while both are running, and faster while they're near each other. Decays very slowly when one is removed and re-added.
- `affection` (−1–1): goes up with positive interactions (greeting, grooming each other, sleeping together, play that ends well) and down with friction. Drifts back toward a baseline over time.
- Baseline comes from temperaments (Playful+Playful high, Aloof+anyone low). **Mother↔kitten and siblings have a high fixed floor.** A newly added adult starts unfamiliar and a little wary: more sniffing, fewer greetings, more personal space.

Bonds scale how likely each interaction is and how much personal space a pair keeps. They're stored in `bonds.toml` next to `config.toml`, keyed by cat names, and written atomically (like prefs) every few minutes and on exit. Removing a cat keeps its rows so re-adding the same name restores them. *(Changed: removing a cat from the tray drops its rows, since Add Cat reuses names. The rows are `a`/`b`/`familiarity`/`affection` tables. There's no renaming in the tray yet; when there is, it should carry the bonds over.)*

The Realistic temperament keeps the rule from `behavior.md`: social interactions there are rarer and slower, the other temperaments show them more often.

## Architecture

- **`CatSim` stays a single cat and never touches the OS.** The Clowder sets read-only snapshots of the other cats from **last frame** before each `update` (phase 1: `CatSim::neighbors`, holding head, body point and personal-space radius; later phases extend it with name, heading, speed, activity kind and tail tip), so results don't depend on update order and stay deterministic.
- **`Clowder` (new `src/clowder.rs` or `src/cat/clowder.rs`)** owns `Vec<CatSim>`, the bond table, rest slot assignment and the bowl queue. It's also the **director** for joint activities: it picks an available pair, rolls with bond-weighted odds and calls `cat.begin_social(SocialRole { partner, kind, role })` on both. Each cat runs its half as a normal `Activity`, and either can cancel (cursor moved away, sprint), which releases the partner. This avoids cats negotiating with each other and keeps everything on the Clowder's deterministic RNG.
- **Feeding API** moves from `CatSim` to `Clowder` (`place_bowl`, queue) with `CatSim` keeping the per-cat Eating activity.
- **Render:** `render_cat` is unchanged. New poses: puffed/flattened-ear state, kneading, carried-kitten.
- **Backends:** `cat_win: OverlayWindow` becomes one window per cat. Stacking matters during contact (sleeping together, carrying, grooming each other), so the window drawn on top is decided by the Clowder (the kitten on top of the mother, for example). If per-window stacking proves flaky under picom, the fallback is a combined window covering both cats while they're in contact.
- **Tray (done):** the main page lists one row per cat (coat swatch + name), each opening that cat's **page** in the same popup (`MenuPage::Cat`): Back, Coat, Random Coat, Design Coat..., Temperament, Add Kitten (adults only) and Remove. Size stays global on the main page, with Add Cat and the global rows (Follow Cursor, Playful Hunting, Feed, Show Cats, Start on Login, About, Quit). Drilling in rather than opening a second popup keeps one window, one grab and one hit-test per backend.
- **Tests** stay behavioural: two or more cats driven by scripted cursor paths. For example: they never overlap outside joint activities; each reaches a distinct slot; a greeting happens within N s of meeting; the tail stalk ends on the partner's tail; a mother's kitten eats first; bonds round-trip through `bonds.toml`.
- **Preview / demo:** the preview sheet gets a row of social poses; the demo GIF gets a second scene with a mother and kitten.

## Phases

1. **Multiple cats (done):** `[[cat]]` config with names, `kitten`, `mother` and migration; `Clowder` with slots and spacing; a window per cat on X11 and Win32; tray Cats section; shared bowl queue (with mother-first). Kittens use adult behaviour at a smaller size for now.
2. **Awareness and kittens (done):** head tracking, tail-up, copying, startle; kitten tuning; the leash and the mother watching over her kitten; bonds with persistence.
3. **Paired interactions (done):** nose boop, head bunt, sniff circle, grooming each other, sleeping together, nursing/kneading.
4. **Play and friction (done):** tail stalk, chase, wrestle, zoomies spreading, laser competition, play escalation and puffing up, pestering, bowl friction, tail lure.
5. **Stretch (done):** carrying by the scruff, growing up, coat inheritance.
