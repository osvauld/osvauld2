# Hillclimb — a side-on driving game on Rapier

Status: **planned 2026-11-04; nothing built.** Worktree `.claude/worktrees/hillclimb`, branch
`hillclimb` (from `dev` `9b1e08d`). Scope (user, 2026-11-04): **Rapier, desktop first, one car,
one stage.** A side-scrolling drive over hills: gas and brake, fuel to pick up, coins, and a
crash when the driver's head touches the ground. Art, name and tuning are our own.

Why: it tests detailed physics the way Jetpack does not. Jetpack's physics is 64 lines of Lua
boxes. This game needs suspension, driven wheels, traction, tipping and long terrain, all
through `ui.world` and Rapier. A 3D Moon rover on `rapier3d` is being considered next.
Hillclimb doesn't build anything for it, but it records its vehicle setup and tuning lessons
(§6) in a form the rover can reuse.

Grows out of [2D gravity](2d-gravity.md) (gravity, `pin`/`rope`/`spring` joints, `density`,
`push`, `blast`) and [2D world](2d-world.md) (fixed 120 Hz step, `on_hit`, `on_zone`, camera
`follow`, `ray`). The rule stays the same: **Rust holds the vocabulary, Lua writes the content.**
No Rust line names a car, a wheel or a pedal.

## 0. End-to-end tests (written first, red until their step lands)

`W` = `world` crate headless (`World2d` ticked at `STEP` = 1/120 s), `L` = a real sandboxed Luau
VM through `app_host`, `P` = pure Lua under stock `lua` (`scripts/test_hillclimb_*.lua`),
`S` = bridge smoke over an offscreen shell (`scripts/smoke_hillclimb.py`, added to `SMOKES`
only once green). World units are 100 to the metre. The game runs gravity `{ 0, 981 }`.

### Engine (Rust, generic)

| id | proof | step |
|---|---|---|
| W-L1 | `collider = { line = { x1,y1, x2,y2, … } }` on a fixed entity: a loose ball dropped onto a 30° slope of it rolls downhill and never passes through, including at 3000 u/s | 1 |
| W-L2 | a 2000-point line 200 000 u long builds within budget and steps at < 0.2 ms with a car on it (bench, `--ignored`) | 4 |
| W-L3 | bad lines are refused whole and keep the last good world: < 2 points, odd count, NaN, a line on a loose entity | 1 |
| L-B1 | `world(id):get("car")` returns `{ pos, rot, velocity, spin, tick }` matching the dump's entity; unknown id → `nil`; allowed in `view` and handlers; any argument shape but a string is an error | 2 |
| W-M1 | `loose.mass = m` sets the body's mass whatever its area (a 1×1 box and a 100×100 box of mass 5 respond the same to the same push); `mass` and `density` together is an error; ≤ 0 or NaN refused | 3 |
| W-M2 | `loose.center = { x, y }` (drawing units) moves the centre of mass: a plank with its centre at one end, set on a point, tips toward that end | 3 |
| W-S1 | `wheel` joint, at rest under gravity: the chassis sags `m·g/k` (to 5 %) and settles. Stiffness is in kg/s² = N/m | 4 |
| W-S2 | dropped 50 u onto flat ground, the suspension's overshoot shrinks each cycle; at critical damping `c = 2√(k·m)` it does not overshoot by more than 5 % | 4 |
| W-S3 | travel limits hold: compression never exceeds `travel` + 1 u after a 500 u drop; the wheel never drifts sideways off its axis (more than 0.5 u) under hard cornering loads | 4 |
| W-D1 | `motor = { speed, torque }` on a `pin` or `wheel` joint: an unloaded wheel spins up to `speed` (deg/s) and holds it; a torque-limited motor under load spins slower than `speed`, never faster | 5 |
| W-D2 | traction: a car on flat ground with wheel `grip = 1` accelerates; with `grip = 0` the wheels spin at `speed` and the car stays within 5 u | 5 |
| W-D3 | braking = `motor = { speed = 0, torque = brake }`: from 1000 u/s on flat ground the car stops, and higher `brake` stops it in a shorter distance until the wheels lock (then it slides) | 5 |
| W-D4 | reaction torque: in the air, spinning the rear wheel up turns the chassis the opposite way (sign check), without a separate tilt control | 5 |
| W-T1 | `loose.torque = t` is a standing torque applied every step while described: an airborne body turns faster each step in the direction of `t`; removing it stops the push (spin decays only through grip and contact) | 6 |
| W-X1 | determinism: the same described car and the same scripted motor schedule over 20 s give bit-identical `pos/rot` twice in one process | 4–6 |
| L-J1 | `wheel`, `motor`, `mass`, `center`, `torque`, `line` through `ui.world` with strict Lua errors that name the entity or joint and the field | 1–6 |

### Game (Lua + smoke)

| id | proof | step |
|---|---|---|
| P-V1 | `vehicle.lua` validates: every parameter in §3 present, in range, with its SI unit; an unknown field is an error | 7 |
| P-V2 | `units.lua` round-trips: kg, m, N/m, N·s/m, N·m, rad/s to world units and back, within 1e-9 | 7 |
| P-V3 | `setup.lua` builds the world entities and joints from a vehicle table alone: no controls, art or stage in its inputs. The joint `stiffness` equals the vehicle's N/m, and the anchors are at the wheel positions | 7 |
| P-G1 | `model.lua` rules on a virtual clock: fuel burns at `burn` L/s while the gas is held, plus `idle` L/s at rest; at 0 the motor is cut; a fuel can refills it; distance = the furthest chassis x reached | 8 |
| P-G2 | crash: a head `on_hit` with the ground ends the run; a body hitting the ground does not; a car resting upside down for 2 s ends the run | 8 |
| P-S1 | the stage generator for seed `n` gives the same polyline every time, slopes stay within `max_slope`, and fuel cans are spaced so a full tank can reach the next can on flat driving | 8 |
| S-1 | upload `demo_apps/hillclimb`, open it: the console is clean, the car is at rest on the start, and its sag matches the vehicle's `m·g/k` (from the dump) | 9 |
| S-2 | a real pointer holds the gas pedal (position from `rects`) for 5 s of virtual time: the chassis x increases, the speed text matches `get("car")`, and the camera keeps the car in its band | 9 |
| S-3 | on the stage's first climb, held gas gets over the top; held brake on the way down keeps the speed under a bound | 9 |
| S-4 | the scripted run passes a fuel can and a coin: fuel rises, the coin count rises (from labels) | 9 |
| S-5 | a scripted wheelie past the tipping point flips the car: the head hits, and the run-over panel shows the distance | 9 |
| S-6 | the same scripted run twice gives the same final dump (`pos`, `rot` per entity) | 9 |

## 1. Engine additions — the Lua surface (all strict, all refused whole)

1. **Line ground.** `collider = { line = { x1, y1, x2, y2, … } }`, in drawing units, only on a
   fixed entity. It's a Rapier polyline. One entity can be the whole stage.
2. **Read-back.** `world(id):get(entity)` → `{ pos, rot, velocity, spin, tick }`. These are the
   dump's names and values (the dump already has `rot` and `spin`). It's a question like `ray`,
   so it's allowed in `view`. It reads as of the last step.
3. **Mass and centre.** `loose = { mass = kg, center = { x, y } }`. `mass` replaces `density`
   (both together is an error). `center` is the centre of mass in drawing units, by default
   the collider's centroid.
4. **Wheel joint.** `{ id, kind = "wheel", a = chassis, a_at = { x, y }, b = wheel,
   b_at = { x, y }, axis = { 0, 1 }, stiffness = k, damping = c, travel = { lo, hi } }`.
   `b` turns freely about `b_at`. Along `axis` (in `a`'s frame) it slides within `travel`,
   sprung at `stiffness` and damped at `damping`. It is held rigid across the axis.
   Rapier: one `GenericJoint`, with linear X locked, linear Y limited and spring-motored
   (position target 0), and angle free.
5. **Motors.** `motor = { speed = deg_per_s, torque = max }` on a `pin` or `wheel` joint drives
   its angle. A Rapier velocity motor whose factor is capped at `torque`. It's part of the
   description, so a pedal is state and the next view drives the wheels. `speed = 0` with a
   large `torque` is a brake.
6. **Standing torque.** `loose = { torque = t }` is applied every step while described. It's for
   air control. W-D4 checks first how much reaction torque already gives for free.

Units: mass is kg. Since stiffness (kg/s²) and damping (kg/s) don't depend on length, they pass
through as N/m and N·s/m with no conversion. Forces scale ×100 and torques ×10⁴ (m → u, m² → u²).
The guide says so next to the fields.

Not in scope: Rapier's enhanced determinism, step-on-demand, save/restore (the lockstep pieces,
see [jetpack-mp](jetpack-mp.md)). One player on one machine only needs W-X1 and S-6.

## 2. App layout — vehicle setup apart from controls and rendering

```
demo_apps/hillclimb/
  vehicle.lua   the car as data, SI units, validated (§3). No world, no art, no input
  units.lua     SI ↔ world units, the only place the 100 u/m scale lives
  setup.lua     vehicle → world entities + joints (colliders, mass, centre, wheel joints)
  drive.lua     pedal state + vehicle → motor/torque fields (throttle, brake, air control)
  stage.lua     seeded terrain polyline, fuel cans, coins, finish
  model.lua     run rules: fuel, distance, coins, crash, run over (pure, virtual clock)
  controls.lua  on-screen pedals (pointer down/up) and keys → pedal state
  art.lua       chassis, wheel, driver and terrain drawings (our own)
  view.lua      ui.world description + HUD; reads get("car")
  main.lua
```

`setup.lua` and `drive.lua` take only a `vehicle` table, so the same car can be tested in a
bare world with no stage, art or controls (W/P tests). The rover would need these two files'
*concepts*, not their code. No shared 2D/3D abstraction is introduced.

## 3. Vehicle parameters (in `vehicle.lua`, SI, all required)

| field | unit | meaning |
|---|---|---|
| `chassis.mass` | kg | chassis body mass (driver included unless `driver.mass` given) |
| `chassis.size` | m `{w, h}` | collider box |
| `chassis.center` | m `{x, y}` | centre of mass from the box's top-left; low and forward = harder to flip |
| `driver.mass`, `driver.head_radius` | kg, m | head is its own body on a `pin` with limits; its contact ends the run |
| `wheels[i].at` | m `{x, y}` | suspension mount on the chassis |
| `wheels[i].radius` | m | |
| `wheels[i].mass` | kg | |
| `wheels[i].grip` | — | Coulomb friction coefficient (Rapier `friction`, Max combine) |
| `wheels[i].driven` | bool | whether the motor drives it |
| `suspension.stiffness` | N/m | per wheel |
| `suspension.damping` | N·s/m | per wheel; `2√(k·m)` is critical |
| `suspension.travel` | m `{up, down}` | compression and extension limits |
| `drive.torque` | N·m | max motor torque per driven wheel |
| `drive.top_speed` | km/h | converted to wheel deg/s through the radius |
| `brake.torque` | N·m | max brake torque per wheel |
| `air.torque` | N·m | standing tilt torque while airborne and a pedal is held (0 = off) |
| `fuel.capacity`, `fuel.burn`, `fuel.idle` | L, L/s, L/s | |

The guide and `vehicle.lua` comments give these units. The validator rejects a missing field
or one outside a sane range (P-V1).

## 4. How suspension and traction work (the model the tests check)

- **Suspension** is a spring-damper along a fixed axis in the chassis frame, with travel stops.
  Sag at rest is `m_corner·g/k`. Ride frequency is `√(k/m)/2π`, and the damping ratio is
  `c / 2√(k·m)`. The stage is tuned for ~0.3–0.5. The axis constraint keeps wheels from swinging.
- **Traction** isn't scripted. The motor applies up to `torque` to the wheel, and the contact
  friction (`grip` × normal load) decides whether that becomes thrust or wheelspin. So load
  transfer onto the rear wheel on a climb really does add grip there, and a light front end
  really lifts.
- **Tilt** comes first from the motor's reaction on the chassis (W-D4), then from
  `air.torque` if the game needs more.

## 5. Steps

Each step lands its tests green plus `cargo test -p world -p app_host` and the touched smokes.
Then a fresh review (`expert-runtime` for `world/`, `expert-app-host` for `app_host/`,
`expert-lua-app` for the app) runs before the user judges.

1. Line collider (W-L1..3, L-J1 part).
2. `get` read-back (L-B1).
3. `mass` and `center` (W-M1, W-M2).
4. Wheel joint (W-S1..3, W-X1 start).
5. Joint motors (W-D1..4).
6. Standing torque (W-T1), only if W-D4 shows the reaction torque alone isn't enough to tilt
   the car usefully. Otherwise, record that and drop the step.
7. `vehicle.lua`, `units.lua`, `setup.lua`, `drive.lua` (P-V1..3) and a bare-world L test that
   drives the vehicle over a flat line.
8. `stage.lua`, `model.lua` (P-G1, P-G2, P-S1).
9. View, art, controls, `smoke_hillclimb.py` (S-1..6); guide section in `lua-apps.md`;
   `status.md`.

## 6. Hand-off for the 3D rover (filled in when step 9 lands)

Lessons so far:
- **Step 1, joints in the ground:** an unoriented polyline snags at its joints. A rolling ball
  lost 20% of its distance over 160 *collinear* joints. Parry's `PolylineFlags::ORIENTED`
  (one-sided, pseudo-normal clamped) removes it, and the ball then matches a single segment
  within 2%. The 3D twin is `TriMeshFlags::FIX_INTERNAL_EDGES` / `HeightFieldFlags::FIX_INTERNAL_EDGES`.
  Test for it the same way: one face against the same surface cut into many.
- **Step 1, tunnelling:** Rapier's speculative contacts held a radius-4 ball at 30 000 u/s
  against a line without CCD.
- **Step 1, check the physics against a formula:** a 2D circle is a disc (I = ½mr²), so it rolls
  down a slope at (2/3)·g·sin θ. Rapier matched that within 1%.
- **Step 3, mass in kg:** Rapier's springs are `MotorModel::ForceBased`, so once mass is in kg
  their stiffness is in N/m with no conversion. Sag = m·g/k matched within 3%. The centre of
  mass is set through `MassProperties` (the shape's inertia scaled to the stated mass); the tip
  of a plank off a peg matched ½·α·t² within 1°.
- **Step 3, sleep vs suspension:** an overdamped spring (ζ ≈ 1.4) fell asleep ±10% short of its
  rest, on whichever side it came from. Suspension sag tests and tuning must stay near ζ ≤ 1,
  or the car's sleep thresholds must change. Watch for this in W-S1.
- **Step 4, the wheel joint:** one `GenericJoint`: `LIN_Y` locked, `LinX` along the suspension
  axis (`local_axis1` = `local_axis2` = the axis) with limits and a force-based position motor
  as the spring, angle free. Body 1 is the chassis. Measured: sag m·g/k within 5% at ζ 0.5. The
  wheel stays within 0.05 u of its axis landing at 8 m/s sideways (so linear constraints follow
  the chassis frame, not the spinning wheel's). The bump stop gives 2.1 u at a 10 m/s landing
  (soft limit). Same run twice: bit-identical.
- **Step 5, motors:** a force-based velocity motor on `AngX`, gain `torque × 10` per rad/s,
  capped at `torque`. Spin-up matched τ/I within 5% (2D disc I = ½mr²). Braking past wheel-lock
  matched v²/(2μg) within 15% and stopped improving. With grip 0 the wheels spin at speed and
  the cart stays put. A motor change is updated in place (`impulse_joints.get_mut(_, wake)`):
  the joint keeps its handle. Reaction torque turns the chassis against a wheel spun up in the
  air, so step 6 (standing torque) waits for tuning in step 9.
- **Step 5, cost:** one 120 Hz step of a driven cart on a 2000-point, 200 000-unit line is 7 µs
  (release, nixos worker). Polyline cost does not show at this size.
- **Step 4, testing damping:** a landing starts the spring with speed, so even critical damping
  overshoots once. Compare ζ levels instead (0.1 → 9.7, 0.4 → 5.6, 1.0 → 0.1 u). Keep the swing
  inside the travel, or the bump stop decides, not the damper.

- Where the vehicle setup lives, and which fields mapped cleanly to Rapier versus needed
  conversion.
- The suspension and traction numbers that felt right, and the ones that went unstable (step
  rate, mass ratios chassis:wheel, stiffness ceilings at 120 Hz).
- Which driving tests (§0 W-S*, W-D*, P-V*) carry over as-is to 3D, and which are 2D-only.

## Coordination with jetpack-mp (2026-11-04)

The jetpack-mp worktree also changes `world`, adding lockstep, save/restore and
enhanced-determinism. Agreed between the two sessions:
- Hillclimb lands `world(id):get` first, with `{ pos, rot, velocity, spin, tick }`; jetpack-mp
  adds `grounded` later.
- jetpack-mp adds `material = { grip, bounce }` on fixed entities (ground materials), for ice or
  mud here.
- Their restore rebuilds the world's maps outside Rapier, so the joints and motors here need
  nothing special as long as all state is in Rapier's sets or in the description.
- Message that session before each world-crate landing. Landing on `dev` still needs the user's nod.

## Open questions

- Keyboard held pedals: whether world `actions` report a release, or the pedals must be pointer
  elements. Checked at step 9 by a probe, not assumed. On-screen pedals are the default either way.
- Head-on-ground versus body-on-ground in `on_hit`: `who` names the ground entity. If the
  chassis and head are both loose and hit the same line, `e.id` tells them apart (P-G2 covers it).
