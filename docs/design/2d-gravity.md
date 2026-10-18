# 2D gravity, forces, grabbing and joints

Status: **plan, 2026-10-18; nothing built.** Branch/worktree: `world-gravity`,
`.claude/worktrees/world-gravity`. Grows out of [2D world](2d-world.md) slice 5 (Rapier): bodies
that Rapier moves already exist (`loose`), but the world has no gravity, nothing pushes a body
except its own `velocity`, Lua cannot take hold of a body, and nothing connects two.

Why: every physics demo so far is top-down (hockey, the playground's table). The showcase this
unlocks is a side-on siege that tells a cause-and-effect story in one take — a cannon, a castle
of mixed materials that collapses, a domino run, a ball into a goal — and is also a toy.

## 0. End-to-end tests (written first, red until the step lands)

Every test runs the real path: `W` = `world` crate headless (`World2d` ticked at the 1/60 fixed
step), `L` = a real sandboxed Luau VM through `app_host` (`ui.world` description + handlers),
`S` = the bridge smoke over an offscreen shell (`scripts/smoke_gravity_lab.py`, registered in
`SMOKES` only once green). Units are the world's (about 100 to the metre).

| id | proof | step |
|---|---|---|
| W-G1 | gravity `(0, 900)`: a loose ball dropped 400 above a fixed floor falls about `½·900·t²`, lands, and rests asleep on it (`body == "loose"`, velocity 0) | 1 |
| W-G2 | gravity unset keeps every old behaviour: a loose ball launched sideways keeps its speed minus friction, no drift down | 1 |
| W-G3 | a loose body spawned in a gravity world starts awake and falls (the "spawn asleep" rule holds only without gravity) | 1 |
| W-G4 | changing gravity wakes sleepers: a ball at rest on the floor rises when gravity flips to `(0, -900)` | 1 |
| W-G5 | bad gravity (`nil`-less shapes: a number, 1 or 3 numbers, NaN, huge) is an error and keeps the last good world | 1 |
| L-G1 | `ui.world({ gravity = { 0, 900 } })` reaches the world; a view with `gravity = "up"` errors `world gravity must be { x, y }` | 1 |
| W-D1 | `loose.density`: a light ball struck head-on by a 10× denser one is flung faster than the heavy one slows; density ≤ 0 or NaN is refused | 2 |
| W-P1 | `push(id, v)` adds `v` to a loose body's velocity and wakes it; a fixed or unknown id is an error | 3 |
| W-P2 | `blast(at, speed, radius)`: bodies within `radius` get speed `speed·(1 − d/radius)` directed away from `at`; one outside is untouched; a body at `at` itself is nudged up, not NaN | 3 |
| W-P3 | a `wells` entry pulls loose bodies inside its radius toward its centre (acceleration `pull·(1 − d/radius)`), none outside; wells wake sleepers; `pull < 0` repels | 3 |
| L-P1 | `world(id):push/blast/grab/release` are handler-only (`only in a handler; view describes`), strict about shapes, and `wells` is strict about unknown fields | 3,4 |
| W-R1 | `grab(id, target)`: a loose body is brought to `target` (within 3 units after 1 s) even under gravity; `release(id)` leaves it its velocity — a throw continues the motion | 4 |
| W-R2 | grabs of a removed entity or of a non-loose one are refused or dropped; two grabs do not fight (last target wins) | 4 |
| W-J1 | `pin` joint: a plank pinned at one end to a world point, under gravity, swings down (rotates ≥ 60°) while its pinned end stays within 1 unit of the point | 5 |
| W-J2 | `rope` joint: a ball on a rope of length `L` never gets farther than `L + 1` from the anchor under gravity, however it is thrown, and hangs below it at rest | 5 |
| W-J3 | `spring` joint: a ball on a spring oscillates about its rest length (≥ 2 sign changes of displacement in 3 s) and settles | 5 |
| W-J4 | a joint description is kept across views (state does not reset); removing a joint frees the bodies; removing an entity drops its joints | 5 |
| W-J5 | bad joints (unknown entity, pin on a loose body without `grip`, non-positive length, duplicate joint id, unknown field) refuse and keep the last good world | 5 |
| L-J1 | the same joint rules through `ui.world({ joints = {...} })` with Lua-level errors that name the joint | 5 |
| S-1 | `smoke_gravity_lab.py`: upload `demo_apps/gravity_lab`, read the dump — the tower stands at rest; a real pointer drag on the cannon and release topples it (block positions change), the domino run falls, the goal sensor lights and the score text rises; the console stays empty | 6 |
| S-2 | determinism: the same scripted pointer run twice gives the same final dump | 6 |

## 1. The Lua surface (strict; unknown fields are errors)

Description props on `ui.world` (declarative, so `view = f(state)` holds):

- `gravity = { gx, gy }` — units/s², default `{ 0, 0 }`. A change wakes every loose body.
- `wells = { { at = { x, y }, radius = r, pull = a }, … }` — a radial field: a loose body whose
  centre is `d < r` from `at` accelerates toward it at `a·(1 − d/r)`; `a < 0` repels. A body
  beyond `r` feels nothing. For gravity wells, magnets, fans.
- `joints = { { id, kind = "pin" | "rope" | "spring", a = entity, a_at = { x, y }, b = entity | nil,
  b_at = { x, y }, … }, … }` — see §3. Entity anchors are in the entity's own drawing box; with no
  `b`, `b_at` is a fixed point of the world.

`loose = { bounce, friction, grip, density }` gains `density` (default 1, > 0): mass is
density × area, so a stone ball outweighs a wooden one of the same size.

Commands (handler-only like `set`, atomic, plain data):

- `world:push(id, { vx, vy })` — adds to a loose body's velocity (a kick, mass-independent).
- `world:blast({ x, y }, { speed = s, radius = r })` — every loose body within `r` of the point
  gets `s·(1 − d/r)` away from it (an explosion). Returns how many it moved.
- `world:grab(id, { x, y })` — takes hold of a loose body: each step it is driven toward the
  point (velocity `= (target − centre)·STIFF`, capped), so gravity and walls still apply to what
  it hits. Calling it again moves the target. `world:release(id)` lets go; the body keeps the
  velocity it had, so dragging then releasing is a throw.

Design choices, with reasons:
- **`push` is a velocity change, not an impulse.** An app author thinks "kick this ball", not in
  kg·units/s; mass-independent kicks are predictable. Mass still matters for what the body then
  hits, through `density`.
- **Gravity is world-level, not per body.** A body that ignores it is a fixed one or a held one.
- **`friction` is Rapier linear damping** in this engine (speed lost per second, see
  `Material`): a falling body with `friction = 6` has terminal velocity `g/6` — slow. Gravity
  worlds use small `friction` (0 to 0.1) and let `grip` (Coulomb surface friction) stop
  sliding. The guide says so next to `gravity`.
- **Wells and joints are description, blasts/pushes/grabs are commands**: standing state is
  described, moments are commanded — the split `after`/`on_timer` already makes.

## 2. Engine changes (steps 1–4)

1. **Gravity** — `Physics` takes the vector; `World2d::set_gravity` (validated, finite, `|g| ≤
   1e5`) wakes all bodies on change; `put` does not `sleep_if_clear` a new loose body when
   gravity is non-zero; `needs_ticks` is already true while any loose body is awake.
2. **Density** — `Material::density`, checked, applied with `ColliderBuilder::density`. Part of
   `Material`'s equality, so changing it rebuilds the body.
3. **Forces** — `World2d::push`, `blast`, and `set_wells`; wells apply to loose bodies once per
   fixed step before Rapier steps (a velocity change `a·(1−d/r)·dt`), waking those in range.
4. **Grab** — `grabs: HashMap<Entity, (f64, f64)>`; applied once per fixed step before the
   step; dropped when the entity is removed or stops being loose.

## 3. Joints (step 5, its own step, cut only if it blows the budget)

Rapier's `PhysicsWorld` already owns `impulse_joints`; the plan is a small, well-tested
surface over three: **pin** (revolute: a hinge; optional `limits = { lo, hi }` degrees), **rope**
(max distance `length`: a chain link or a pendulum string — slack when closer), **spring**
(`length`, `stiffness`, `damping`).

- Anchors are in an entity's drawing box, converted to the body's centre frame; a joint with no
  `b` is attached to a fixed ground body at `b_at`.
- A `pin` needs both sides able to turn: a loose body must have `grip` (nothing else turns), a
  fixed one is fine. Refused otherwise, naming the joint.
- Joints are reconciled by `id`: kept while spec and both bodies' handles are unchanged (so
  per-frame description does not reset a swinging pendulum); rebuilt when a body was rebuilt;
  dropped with their entities (Rapier removes a body's joints with it).
- `contacts = true` lets jointed bodies collide with each other; default off.

## 4. The showcase — `demo_apps/gravity_lab` ("Siege")

One continuous, playable cause-and-effect chain, ~15–20 s scripted, deterministic (fixed RNG,
scripted pointer):

1. **Cannon / slingshot**: drag back on the launcher; a dotted trajectory arc (computed in Lua
   from the launch velocity and the world's gravity) previews the shot; release fires a stone.
2. **Castle**: a tower and wall of mixed blocks — heavy stone (`density 4`, low bounce), light
   wood (`density 0.6`, bouncier), small glass-like blocks — that collapses realistically; a
   **pendulum wrecking ball** (rope/pin) can be released to join in.
3. **Domino run** on a ramp the collapse topples.
4. The last domino rolls a **ball down a slope** into a **bucket goal**: a sensor that lights up
   and counts score.
5. **Targets** (little crates/creatures on the platform) score when they fall off or enter a
   sensor zone.
6. Toys: **bomb** (`blast`), **gravity flip** and a **gravity well** button, **drag-to-throw** of
   any body (`grab`/`release`), **reset**.

Visuals: vector drawings with gradients and shadows, per-material colour, a flash/dust puff on a
heavy `on_hit`, a motion trail on fast bodies where `gfx` allows. Read well at 700 px wide.

## 5. Order and process

Each step: tests for it written and seen red → implement → `cargo test -p world` / `-p app_host`
→ commit. Step 6 adds `scripts/smoke_gravity_lab.py` to `SMOKES` once green and the final full
offscreen `python3 scripts/smoke.py` runs before reporting. `docs/lua-apps.md` and
`docs/status.md` are updated as each step lands. The reel clip is
`gravity_lab` in `scripts/demo_reel.py`.

Out of scope (noted, not built): per-body gravity scale, continuous force APIs on single bodies,
Lua read-back of body positions (the showcase needs none: scoring comes from `on_zone`/`on_hit`),
joint motors, 3D.
