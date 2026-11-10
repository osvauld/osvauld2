# Composable 3D physics

**Integration revision 2026-11-10:** The generic body/wheel/motor API and Lua composition are integrated into dev; the earlier uncommitted wording below describes the original checkpoint. Combined verification is recorded in [the integration plan](game-branch-integration.md).

Status: **implemented and verified, 2026-11-05**, in the uncommitted `moon-rover` worktree. User-directed revision
of the first rover implementation. The earlier vehicle design remains as dated history.

## End-to-end acceptance

Write failing checks first, then implement through the real bridge, shell, Lua VM and vault:

- `smoke_physics_components.py` opens an independent educational wheel bench, assembles an
  ordinary dynamic body and wheel components from Lua, drives a bounded physical motor,
  freewheels and brakes. No vehicle recipe or normalized game controls enter Rust. Inspect
  body mass, independent wheel commands, suspension contacts and matching rendered poses.
- Reuse `smoke_rover_drive.py` and the crater mission round-trip to preserve driving,
  obstacle collisions, input release/cancel, steering direction, recovery and durable results.
  Brake priority and throttle mapping are exercised through the actual Lua recipe/controller.
- Failed recipe and command batches preserve every live body/wheel, including a bad final
  element, cross-world staged reload, missing or fixed parents, duplicate component IDs,
  changed retained setup and invalid physical numbers. Allowed hover changes wheel actuation
  only; view, load, frame and animation cannot command the solver.
- Pause, hidden worlds and both capture paths are observational. Reopening reconstructs the
  educational apparatus from sealed Lua source. No per-tick document writes or Lua callbacks.
- Native tests pin SI mass/sag, independent wheel commands, velocity motor torque bounds,
  reset, sleeping/waking, deterministic stepping, coarse/subdivided ground and low-block contact.
  A single wheel is a valid component; stable arrangements are the author's responsibility.

Build/check/tests use LoadTable `--peer nixos`, at most six build jobs. Run focused tests first,
then workspace checks/tests and the registered smoke sweep. Fetch a release for laptop preview
and performance measurement; no laptop build. The user's `pi` review exception remains active.

## Boundary

Rust owns physical components, solver state, contacts, constraints, actuation and fixed-step
scheduling. Lua owns apparatus assembly, game or lesson meaning, input mapping, drive/reverse,
parking, brake priority, steering limits, motor targets, missions and UI. A wheel, spring,
rotary motor, hinge or pulley can be a native primitive. A car, rover, elevator or educational
experiment is Lua composition. Runtime stays Lua-free; the world crate keeps Rapier private.

This step extracts existing ray wheel/suspension mechanics and general body properties. General
joints, standalone springs and ideal pulley/rope constraints are subsequent primitives; they are
not implemented by renaming the existing vehicle controller or faking a pulley in app code.
The educational proof for this step is a wheel/motor bench.

## Component contract

`gfx.world3d` declares ordinary `bodies` plus optional flat `wheels`. No wheel allocates a
parent body implicitly. Each wheel names an existing dynamic `body`, local mount, radius,
width, suspension rest/travel, stiffness, damping and traction tuning. One to eight wheels per
parent and at most 128 per world. Component IDs are unique across bodies and wheels. Body
mass and local centre of mass are general body properties, usable without wheels; materials
retain their existing defaults. Positions/orientations remain authored reset targets.

`world:set_wheels({{id, torque, brake, steering, motor}, ...})` is an atomic batch of held
physical commands. Torque/brake are N·m, steering radians about the local up axis. `motor`,
if present, is `{velocity, max_torque}`: angular velocity in rad/s and a torque bound. The
native velocity servo converts speed error to a bounded contact force; it has no throttle,
vehicle speed limit, front/rear knowledge, key mapping or game brake-priority rule. Lua
chooses mutually exclusive motor/brake commands for the rover. Direct torque and motor are
mutually exclusive actuator modes; brake is an independent physical control. Missing command
fields reset to zero/absent. Unmentioned wheels retain commands. Resetting a parent clears its
wheel actuation and solver state. Retained physical setup changes require remove/recreate.

The current wheel backend remains Rapier's ray support model, Y-up with +Z forward and X axles.
Its internal chassis grouping is solver bookkeeping, not a vehicle API. Existing massless
cylindrical obstacle shapes follow suspension; independent unsprung mass and airborne motor
reaction torque remain unbuilt. Do not present this wheel model as a general rigid pulley.
Native inspection exposes flat `wheels` with physical commands and contact/pose information,
and body properties/mass. Old unmerged `vehicles`/`drive` surfaces reject rather than silently
retaining the former policy. App code and tests migrate together.

## Verification — 2026-11-05

Job 2561 shows the new real-path educational test failing on the former API (`unknown field
wheels`). Native extraction and Lua migration then pass the five focused bench, terrain,
driving, FPS and full crater-mission smokes in job 2566. Later native checks pass 57 3D world
tests (job 2569) and 15 3D host tests (job 2568), including the actual rover Lua source with a
changed speed setting, half throttle, moving reverse and brake priority. Native tests separately
prove physical motor/brake competition, torque bounds and waking an existing sleeping parent
when wheel components are added. The full 44-script sweep (job 2576) passes the other 43 scripts, including the rover mission
with the combined physical brake model; two optional Android checks skip without opt-in.
The bench's new pixel helper initially assumed every scene had a hit rect. Noninteractive
scene leaves intentionally do not. The corrected test changes only the isolated carriage's
visual color through versioned edits and compares actual screenshot bytes, while asserting
native state is identical. Focused rerun 2579 passes that pixel gate, native/render pose
agreement, physical controls, pause/captures and reopening from the same sealed source store.
Every registered smoke therefore passes across the full sweep and focused test repair.

The component API intentionally remains a bounded ray wheel model. Standalone spring, joint
and ideal pulley primitives are unbuilt; this refactor establishes their boundary without
claiming those solvers exist. No Lua physics-tick callbacks, renderer changes or 2D Hillclimb
API changes are introduced.

Workspace check passes in job 2571; job 2572 passes 1,132 workspace tests (23 ignored). Builds
retain existing warnings in untouched runtime/auth/budget code. The release built/fetched in
job 2575 passes all 14 laptop performance checks (job 2577): controlled foreground Iris Xe /
Vulkan at 1920×1080, 63.56 median recent presentation FPS, 8.88 ms median rolling-p95 CPU+wait
work, 120.18 Hz physics and zero dropped time across 16 snapshots over 7.56 seconds. Geometry
budgets remain unchanged. This is a short CPU/cadence sample, not GPU timing or a soak result.

The live preview (job 2578) contains the migrated rover and an independent Mechanics wheel
bench in the same test account. Existing rover source was updated with versioned exact edits
while closed; the bench was initially uploaded as a new app. Both activate without console
errors. The bench is selected and paused; Run starts it. Capture: `shots/wheel-bench-live.png`.
All work remains uncommitted/unmerged. `pi` review remains skipped at the user's request.
