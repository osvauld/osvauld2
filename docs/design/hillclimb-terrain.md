# Hillclimb terrain and detail — ground that drives differently, a world with depth

Status: **plan, 2026-11-04; nothing built.** Follows [Hillclimb](hillclimb.md) (steps 1–9 built,
branch `hillclimb`). Worktree `.claude/worktrees/hillclimb`, same branch. Scope (user,
2026-11-04): **better terrain and detailing, both how the ground drives and how it looks; engine
features allowed.** Still one car, one stage, desktop first.

Why: the first stage is one line of grass hills with one grip. The art pass (`5c9024f`) put as
much detail into the bodies as today's drawings allow. What's left needs the engine: ground that
grips differently, depth behind the road, dust off the tyres. The rule stays: **Rust holds the
vocabulary, Lua writes the content.** No Rust line names a biome, a bridge or a tyre.

What we have, checked (not grepped): a loose thing's `grip` is its Rapier friction and pairs combine
with **Max**, so a fixed surface cannot make anything slipperier. Gradients exist only inside
drawings. There is no parallax and no particle system. A 2D world has **no body cap** (the
256 in `lua-apps.md` is `gfx.world3d`'s, corrected 2026-11-05), so describing only what's near
the car is a speed question, settled by W-B1.

## 0. End-to-end tests (written first, red until their step lands)

Same letters as the first plan: `W` = `world` headless, `L` = a Luau VM through `app_host`,
`P` = pure Lua (`scripts/test_hillclimb_*.lua`), `S` = the offscreen smoke.

### Engine (Rust, generic)

| id | proof | step |
|---|---|---|
| W-G1 | `surface = { grip = g }` on a fixed entity (rect or line): a loose block with `grip = 1` slid at 800 u/s stops in `v²/(2·μ·g)` to 5 %, with μ = loose grip × surface grip. `g = 0.1` slides ~10× further than `g = 1` | 1 |
| W-G2 | without `surface`, every existing world is unchanged: the step-5 stopping-distance and traction tests pass as they are, and gravity_lab's numbers don't move | 1 |
| W-G3 | a car on a line whose `surface.grip` is 0.15 spins its wheels under full torque (wheel speed ≫ ground speed) and accelerates at most 0.2 of what it does on grip 1 | 1 |
| W-G4 | bad surfaces are refused whole: grip < 0 or NaN, `surface` on a loose entity, an unknown field | 1 |
| L-K1 | a world's `sky = { stops = … }` fills its box with a vertical gradient behind everything: offscreen pixels at the top and bottom of the world match the first and last stop | 2 |
| L-P1 | `parallax = f` (0 to 1) on an entity with no collider: when the camera moves 1000 u, the entity moves `1000·f` on screen (from `rects`/dump), `f = 0` stays put, `f = 1` is the default. A `parallax` entity with a collider, a sensor or `loose` is refused | 3 |
| L-E1 | `emit = { rate, life, speed, spread, size, colour, at, gravity }` on an entity: Rust spawns, moves and fades particles with no Lua per frame. After `t` s the live count is `rate·min(t, life)` ± 1, they leave from `at` in the entity's frame, and they stop when `emit` is gone (the live ones finish) | 4 |
| W-E2 | particles are seeded from the world's tick: the same run twice paints the same pixels; particles never touch the physics (a world with and without `emit` gives bit-identical bodies) | 4 |
| W-B1 | the full stage (~600 bodies: coins, rocks, planks) described at once vs only what's within ±30 m: step and reconcile time for both (bench, `--ignored`). Windowing is built only if the full stage misses 1 ms a frame | 6 |

### Game (Lua + smoke)

| id | proof | step |
|---|---|---|
| P-T1 | the stage is sections in order (meadow → desert → rock → snow → …). Each has a length, a grip and a palette. Together they cover the km with no gaps, the ground is continuous at every join, and slopes stay within `max_slope` | 5 |
| P-T2 | each section is its own line entity with its section's `surface.grip`; together the lines are exactly the old one-line ground (same points), so a car crosses a join without a bump | 5 |
| P-T3 | features land on legal ground: a bridge spans a gap with both banks within ±20 u of level, a ramp ends at a gap no wider than a flat-out jump clears (from the car's top speed), rocks sit on the surface | 7 |
| L-T1 | the real car (the 4 modules, as in `the_hillclimb_car_drives_on_a_bare_world`) on a snow line accelerates slower than on a meadow line, from rest over 3 s | 5 |
| L-T2 | the car crosses a plank bridge (planks on `pin` joints, ends to the banks): the middle plank sags under it and comes back, nothing breaks, the car reaches the far bank | 7 |
| L-T3 | the car drives into a loose rock: the rock moves on, the car isn't stopped dead | 7 |
| P-W1 | if W-B1 calls for it: `items_near(x)` describes only what lies within ±30 m of the car, and an item once taken stays gone when it scrolls back in | 6 |
| S-7 | the smoke drives into each section in turn (or jumps the car there by seed), and checks the section's name in the dump, a screenshot per section, and a clean console | 8 |
| S-8 | depth: across 300 frames of driving, a far layer moves less on screen than the ground. The sky's top pixel matches its first stop | 8 |
| S-9 | dust: with the gas held on dirt, the rear wheel's emitter has live particles; at rest, none | 8 |
| S-6′ | determinism still holds (S-6), and so does the wall test | all |

The flat-out distance (288 m today) is not a test. It will change with the terrain. "Gets over
the first hills (> 150 m)" stays.

## 1. Engine additions — the Lua surface (strict, refused whole)

- **`surface = { grip }`** on a fixed entity. Contact friction becomes `loose grip × surface grip`
  for that pair. Without `surface`, nothing changes (Max, as now). Rapier picks one combine rule
  per pair by priority, and Max beats Multiply, so this is a contact-modification hook (or a
  per-pair rule), not a collider setting. Step 1 decides which after a spike.
- **`sky = { stops = { {0, colour}, …, {1, colour} } }`** on `ui.world`: a screen-space vertical
  gradient under the entities. `fill` stays as it is.
- **`parallax = f`** on a no-collider entity: drawn at `pos − camera·(1 − f)`. Hills at 0.3,
  mountains at 0.1, foreground grass at 1.1 (above 1 is allowed, for things in front).
  Draw order stays list order.
- **`emit = {…}`**: a particle stream, visual only. It is owned by the world and seeded from the
  tick, so lockstep peers (jetpack-mp) paint the same. Particles are kept in Rust and dropped
  from the dump except `emit_live` (a count), so the dump stays small.

## 2. Game layout

- `stage.lua` grows sections: `{ kind, from_x, to_x, grip }`. The points are generated as now,
  then cut into one line per section (shared end points). Features (bridges, ramps, rocks) are
  placed by section rules.
- `biomes.lua` (new) is data only: per section kind, the grip, the palette, the backdrop
  layers, and the dust colour. Art reads it; `stage.lua` reads only the grip.
- `art.lua` draws per section: ground strata, surface edge (grass, sand, scree, snow), props
  (trees, cacti, boulders, pines) on parallax layers. The car keeps its art. Dust comes off the
  driven wheel, coloured by the section it's on.
- `main.lua` describes the whole stage unless W-B1 says otherwise.

## 3. Steps

1. **Surface grip** (W-G1–G4): spike the combine rule, then build it, with the Lua surface and
   docs.
2. **Sky gradient** (L-K1).
3. **Parallax** (L-P1).
4. **Particles** (L-E1, W-E2).
5. **Sections** (P-T1, P-T2, L-T1): meadow, desert, rock, snow, with grips about 1.0 / 0.8 / 0.9
   / 0.35. Tuned by play.
6. **Body count** (W-B1, then P-W1 only if the bench says so).
7. **Features** (P-T3, L-T2, L-T3): plank bridges, ramps with gaps, loose rocks.
8. **The look** (S-7–S-9): per-biome art, parallax layers, dust and snow spray, sky per
   section. Screenshots go to you after each section.
9. **Car detail, if wanted after play:** e.g. a strut drawn between chassis and wheel (needs an
   `attach` to two bodies, or Lua posing a decoration from `get`).

Every step leaves `cargo test` for `world`/`app_host` and the hillclimb smoke green. Steps 1–4
touch `world/`/`app_host/`, so `smoke.py` runs in full before landing.

## Coordination

jetpack-mp is building save/restore in `world/` (its step 6). Our new state needs a place in
its snapshot: surface grip is description, but particles are live state. Tell them at step 1,
and again when `emit` lands.

## Open questions

- Should a section's end be visible on the stage (a sign, a colour band) or seamless?
- Weather (snow falling) is just an `emit` on a camera-following entity. Is it in scope?
