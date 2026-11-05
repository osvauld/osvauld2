# Crater sample mission

Status: **implemented and verified, 2026-11-05**, in `moon-rover`, uncommitted.
Builds on the verified rover and FPS checkpoint. Lua owns terrain, scenery, mission rules and UI; reuse native 3D physics.
The user has authorized this continuation and retains the `pi` review exception.

## End-to-end acceptance

Write the tests first and watch them fail:

- `smoke_rover_mission.py` enters the mission through a real pointer, drives the actual rover
  to the crater with keyboard throttle/steering/brake, collects a sample while stopped, returns
  to the lander and delivers it. Early/far/fast interactions cannot complete the objective.
- Native inspection and screenshots prove the larger surface, lander, rocks, sample beacon and
  rover reach the renderer. Rendering and fixed terrain/rock collision reuse the same meshes.
- A source edit adds an allowed input-handler fall injection to the isolated test app. A real
  sensor event recovers the rover upright at its authored spawn, clears motion/held controls,
  loses carried cargo and pauses. Recovery is automatic; no reset from view or frame callbacks.
- Paused navigation/captures preserve native state. Switching back to the test course pauses
  the mission with no hidden-time debt. Closing/reopening keeps the completed-run count, starts
  a fresh empty-cargo run, and preserves source. Repair after a failed edit keeps native worlds.
- `test_lunar_terrain.lua` proves seed repeatability, different-seed variation, exact mesh/sample
  agreement, bounded slopes and a clear drivable approach. Keep existing course and controls
  smokes green. Re-run the live release performance audit against the mission.

Tests/builds run through LoadTable `--peer nixos`; only the release preview/measurement runs
locally. No Rust change is expected. Register the mission smoke in `scripts/smoke.py`.

## Level and rules

A 96×96 m seeded fictional lunar height field at 1.5 m samples (8192 collision triangles),
with rolling terrain, a broad primary crater, secondary craters, scattered fixed rocks and a
lander. Heights and the render/collision triangles share one source; this is not surveyed lunar
terrain. Keep the existing 32×24 m physics course as a separate selectable world.

The rover starts at the lander facing the primary sample site. Route markers, a visible beacon
and a distance/objective readout provide direction. E or the interaction button collects at the
sample site and delivers at home, within 3 m, below 0.6 m/s and upright. Cargo, held controls,
pause and current-run state are local. Only a completed-run count is written to an app document,
once per successful delivery, in `crater-mission.stats.completed_runs`. Root document entries
are containers; a scalar root entry is invalid. Delivery clears the carried sample. Reload/reopen
starts an empty-cargo run, so saved cargo cannot teleport home when transient physics is reconstructed. Reset starts a new run; successful-return
history remains. No per-frame document writes.

A wide nonblocking volume below every legitimate terrain point catches falling rovers through
`on_zone`. Recovery resets pose/controller state, clears cargo and input, parks and pauses at the
lander. R remains manual reset for a rollover on solid ground. World visibility/pause use the
existing native clock policy, and physics never runs in Lua.

Camera follow accepts the active terrain sampler, so it remains above the mission ground rather
than sampling the old course. A map view covers the larger area. Controls retain their corrected
left/right directions and held-mouse release behavior. The native FPS overlay remains available.

Target 60 FPS in a controlled foreground release run, 120 Hz physics and zero steady catch-up
drops. Existing hard limits remain 256 bodies/objects, 16 MiB unique scene geometry and 131072
instanced visual/collision triangles. Record actual counts/performance; do not infer the new
scene's performance from the smaller course.

## Verification — 2026-11-05

The first mission smoke failed before implementation; subsequent runs exposed an overly steep
crater approach and an invalid scalar document root on delivery. The crater now uses a broad
smoothstep bowl with a flat collection area, and the return count lives in a guarded named map.
The 14 m primary crater radius keeps the tested neighboring-sample slope below 0.45.

LoadTable job 2551 passes the deterministic stock-Lua mesh/sampling/slope checks on NixOS.
Job 2553 passes the existing terrain, driving and performance smokes. Job 2555 passes the
actual keyboard-driven outbound/return mission, collection, delivery, restart persistence,
real fall sensor recovery, pause/capture and shared-geometry assertions. The full 43-script sweep,
job 2556, passes all other 42 scripts (including two optional Android skips); the mission's new
intentional failed-edit test initially expected an empty console after deliberately generating a
validation error. The corrected assertion accepts exactly that rejection and rejects any added
error. Focused NixOS rerun 2559 passes the final mission smoke, including map/keyboard camera
navigation while paused, both fast-interaction rejection checks, delivered cargo clearing,
recovery clearing held throttle before resume, failed-edit/repair preservation of both worlds,
and durable history across reopening. Thus every registered smoke passes across the sweep and
focused repair run. Job 2558 was superseded/cancelled. No Rust was added for this mission step;
the earlier native checkpoint remains workspace check plus 1,127 tests passing (23 ignored).

Controlled foreground release measurement on the laptop, job 2554: Intel Iris Xe / Vulkan,
1920×1080 at scale 1, three-second warmup and 16 snapshots over 7.56 seconds, held 10% native
throttle. Median recent presentation-call FPS 63.52, lowest sampled 59.18; median rolling-p95
CPU+wait work 10.85 ms; physics 119.99 Hz, dropped time 0 s. All 14 audit checks pass. The scene
has 37 rigid bodies, one four-wheel vehicle, 53 objects, 8,336 collision triangles, 10,332 visual
triangles and 194,688 unique mesh bytes. GPU timestamps remain unavailable; this short sample
does not establish a long-duration or GPU-time bound. Raw artifacts are generated under
`shots/crater-mission-performance.{json,png}`. The later cargo/save fix and Earth placement
change do not alter these geometry/body counts or the native driving path.

The persistent laptop preview was updated through versioned exact source edits and initial
creation of the four new mission files, using the existing Nix-built release and test account.
It opens the mission paused at the lander with the native FPS overlay; Drive / Resume starts
it. The map overview capture is `shots/crater-mission-map.png`. The separate test course remains
available. Changes remain uncommitted and unmerged for user acceptance.
