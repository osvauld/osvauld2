# 2D tank game — Lua authoring and performance proof

Status: **WIP on branch `2d-tank-game`: keyboard routing, a Lua tank with movement, pointer
tracking, continuous bounded firing, spawned chasing enemies, relative swept projectile hits
and score are implemented and smoke-tested; enemy contact damage, walls, pause/reset and GPU
benchmarking are unbuilt.** This is a small playable app and a
measurement probe, not a public game-engine API. The current 2D Frame, pointer routing, offscreen
driver and experimental `on_frame` are the starting point.

## Goal and scope

A top-down tank arena authored in Luau: one player tank moves with WASD/arrow keys, its turret
tracks the pointer in field-local coordinates, and a click fires one projectile. Walls and a
bounded set of stationary targets give projectiles something to hit; hits update a visible score.
Start/pause/reset are ordinary UI controls. The first game uses vector Frame visuals, not imported
sprites, audio, network peers, physics libraries or 3D meshes. Targets may be multiplied for the
load test without introducing autonomous enemy behavior.

Game state (tank pose, aim, projectiles, targets, score and held keys) is per-viewer Lua state. Do
not persist positions or write a CRDT on every frame. Static paths/brushes are compiled once;
placement changes are assembled into a Frame for the current view. This knowingly rebuilds
instance/transform tables and the Frame container each tick; measure it before adding retained
sprite or dynamic-buffer machinery. Lua owns movement and collision rules. The runtime owns
validated drawing, layout, input routing, the clock and resource budgets.

**Revised 2026-09-23:** The first proof used click-to-fire and three stationary targets. The
requested game is now closer to an arena survival shooter: the tank fires automatically along its
last valid turret aim, and a bounded population of enemies spawns periodically and pursues it.
The original goal above is retained as the earlier testable slice, not the current gameplay
contract. Enemy damage and sophisticated AI remain separate work.

## Input contract to build

The game needs a stable-id key down/up handler on its active surface, carrying physical and
logical key names, pressed/released state, repeat and modifiers as plain data. Key repeats must not create extra motion;
movement derives from the set of keys held during the next tick. Text inputs and shell shortcuts
retain priority. Losing window focus, switching tabs, removing the game surface or reloading must
clear held input so a tank cannot drive forever on a missing release. The native path delivers
`cancelled = true` without a key identity to clear all held keys. Runtime and Lua-host tests exist;
the driver and live-Lua smoke cover key routing, with gameplay tests still to come. The existing bridge `Key` action only invokes Enter/Esc on an input; it is not a held-key driver.
The new offscreen `Keyboard` request drives down/up through the live game-key eligibility path,
and refuses reserved F5/F12. Its smoke uploads a real Lua app, holds a key across frames, releases
it and verifies movement stops. This is not arbitrary text/IME input.

`ui.frame` receives pointer aim through `on_hover(e)` in field-local coordinates. Its id is
stable; aiming when the pointer leaves the field keeps the last valid angle. The click-to-fire
`on_click(e)` from the first slice was removed per the 2026-09-23 revision above.
Use bridge `Rects` to aim test pointers; never guess screen coordinates. Mouse motion and held keys
are viewer-local, not document state.

## Prototype time contract

For this game proof, one declared `on_frame(e)` on the active game root advances Lua state once
after each painted frame. Use `e.dt`, bounded by the runtime to 0.1 s, for motion in units/second;
no per-tick document writes. Pause removes the callback and clears held input. Gameplay is not
claimed deterministic across refresh rates: variable steps may affect collision at high speeds,
and a stall discards excess time. Bound projectile speeds and use a swept segment against walls
and targets (rather than endpoint overlap) so a bullet cannot tunnel through a thin target.

An offscreen `Frame(n)` advances a virtual 1/60 s per requested frame, but inspection, pointer
operations and screenshots can paint and dispatch `on_frame` too. Tests must assert state around
those operations, not equate callback count with simulation steps. Hidden-tab resume, screenshot
observation, callback error quarantine and a fixed-step simulation clock remain separate product
gates. We will revisit scheduling only if this small proof demonstrates a concrete failure; do
not claim it is already a game-loop contract.

## Build slices and acceptance

1. **Input seam:** key down/up, focus and tab cancellation, text/shell priority; headless Runner
   tests plus a bridge driver exercised against a real Lua app. Keep runtime messages plain data,
   handler IDs stable, and each write around 100 changed lines of code (tests ride free).
2. **Playable core:** bounded arena, tank, turret, walls, click-to-fire, swept projectile hits,
   targets, score and pause/reset. Keep model/math separate from Frame assembly. Exercise normal
   pointer/key routing and a clean-console windowless screenshot. No host-side game rules.
3. **Load scenarios:** the same game with controlled target and projectile counts, reporting
   rendered count, visual composition time and interaction correctness; include a baseline
   scene with no moving entities. No silent drop of objects at the visual resource cap.
4. **Measurement seam:** release build, fixed viewport and scenario, warmup then repeated runs,
   median and high-percentile wall-time/frame with hardware/backend/build recorded. Separate Lua
   simulation, Lua/Frame assembly, layout/scene building and GPU work where instrumentation
   supports it. Do not call offscreen `Frame(n)` a full rendered frame: without a surface,
   `Render::present` returns before GPU rasterization. A GPU benchmark must render to the same
   offscreen target without PNG readback on every frame, synchronize at measured boundaries,
   and report readback separately. Screenshots verify pixels, not frame-rate claims.

The first exit gate is a player who can move, aim, fire, hit a target, pause and restart inside a
shell-hosted Lua app, with deterministic offscreen *input routing* tests and an honest record of
what time/paint was measured. Image sprites, retained bulk transforms, collision libraries and
fixed-step worlds are follow-on decisions informed by this game's evidence, not prerequisites.
