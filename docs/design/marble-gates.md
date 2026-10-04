# Marble Gates — from drop proof to a playable Lua level

Status: **steps 1–4 implemented in the 3d-math checkpoint: rotation, zones, atomic pose/motion commands,
paused resolved snapshots and a complete Lua-authored level with tilt, release, win/loss and
retry. Workspace check, 104 world tests, 229 host tests (two ignored) and all 13 individual
registered smokes pass; the full sweep hit its time cap after 12 and the final smoke passed
separately.**
Camera follow is owned by the 2D session first.

## End-to-end proofs (write failing tests before each step)

All proofs use the real offscreen shell, bridge, Lua VM and raw `worlds3d` inspection;
`smoke_marble_gates.py` is the registered driver. Screenshots supplement state assertions.

1. **Rotated ramp:** surgically edit the Lua platform body to a tilted quaternion; accepted
   reload preserves its current pose until reset. Reset applies the latest authored orientation;
   the rendered platform matches physics and gravity carries the marble down the slope.
   Reject zero/nonfinite/sparse quaternions without changing any accepted world. Inspection
   and both capture paths remain observational. Native tests pin reset, validation and determinism.
2. **Zones:** a nonblocking goal reports one enter, then one leave as the marble passes.
   `on_zone` uses `{phase="enter"/"leave", id, who}`; raw bodies expose sorted `zones`.
   Pause/captures emit no new events; sensor removal and body removal semantics mirror 2D.
   Use `sensor = true` on a body's existing sphere/box shape (fixed-only initially).
   Bind `on_zone` on the `ui.scene3d` leaf displaying its retained world. Events include the
   physics tick; missing handlers discard events. A 4096-event queue discards newest overflow
   and reports `dropped_zone_events`; raw sorted memberships remain authoritative.
   Sensor removal emits leave for surviving bodies; body removal silently clears its memberships.
3. **Commands:** Lua input handlers change a ramp's pose without body recreation and launch or
   stop a dynamic body. Invalid multi-field commands apply nothing. Native inspection and the
   render snapshot agree before the next tick. View/capture callbacks cannot issue commands.
4. **Game:** Lua describes the level and implements win on goal entry, loss below its authored
   fall threshold, and retry. Drive both winning and losing attempts through real controls;
   capture screenshots and verify state/console. No ramp, marble, goal or win concept enters Rust.

## Boundaries and compatibility

Rust owns generic transforms, rigid bodies, fixed-step scheduling, overlap detection/events,
validated commands, snapshots and lifetime/reload invariants. Lua owns geometry, level layout,
UI, camera choices and all gameplay rules. ECS/Rapier types stay private.

Step 1 adds optional `rotation = {x,y,z,w}` to body recipes, default identity. Finite bounded
quaternions of length at least 0.001 are normalized for physics. Like position, retained recipe edits change
reset targets, not current placement; reset clears motion/forces and restores both pose fields.
Bound visual authored rotation remains identity: body recipes are the single source of initial
physical orientation and resolved snapshots supply the rendered orientation.

Commands use the retained handle: `game:set(id, {pos, rotation, velocity, spin})`.
`pos`/`velocity` are three-component metre/metre-per-second vectors; `rotation` is x/y/z/w;
`spin` is a three-component degrees-per-second vector, matching 2D's degree convention.
All supplied fields validate before any mutation; velocity/spin require a dynamic body.
Commands never alter authored reset targets. Recipe/dump `position` stays unchanged and native
`angular_velocity` stays radians/second.

`game:scene(camera, {running=false})` renders actual resolved poses while pausing the world,
without catch-up on resume. Lua uses this generic snapshot option for Ready/Won/Lost states;
Rust never knows those states. The third demo mode is a Lua-only level module with a goal gate,
fall-zone threshold, Release, tilt controls and Retry. Existing drop/ramp proofs remain available.
The bridge proof changes tilt, drives a win and a loss, verifies pause/capture stability, retries,
and confirms a subsequent win. The real-path command proof failed before the setter existed;
the pause proof failed before the running option worked; the playable proof failed on missing
controls before the level module was written. Targeted proofs now pass. Ready/won/lost pictures
were inspected. This demonstrates one authored game, not independent agent-as-maker acceptance.

Ownership: this session owns `world3d*`, `physics3d*`, `gfx/world3d*` and Marble Gates. The other
session owns 2D world/physics/tests, 2D graphics and hockey. Shared host/shell/runtime wiring is
changed narrowly and reconciled by whichever session lands second. No camera-follow state or
foreground-composition work is bundled into these physics steps.
