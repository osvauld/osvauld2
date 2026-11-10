# Moon rover

**Integration revision 2026-11-10:** The committed rover, mission, generic wheel-component and diagnostics checkpoints are integrated into dev. Combined verification is recorded in [the integration plan](game-branch-integration.md).

Status: **2026-11-05: steps 1a, 1b and the first vehicle in step 1c are implemented in
`moon-rover` from `dev` `9b1e08d`, uncommitted and awaiting user acceptance.** Workspace check
and 1,124 workspace tests pass (23 ignored). The real terrain and driving smokes and the full
41-script sweep pass (two optional Android checks skip without opt-in). The later quantitative
control regression and final 162-test native world suite pass (6 ignored), as do the repeated
terrain/driving smokes after the direction fixes. Detailed suspension geometry and agent navigation remain unbuilt.

**2026-11-05 boundary revision:** the user requires reusable native physics components with
Lua apparatus/game composition. The first native `vehicles`/`drive` API below is superseded
by [composable physics](physics-components.md). Ordinary bodies now own mass/centre/material;
flat native wheel components own physical suspension/contact/motor state. Lua assembles the
rover and defines throttle, brake priority, steering and motor targets. Earlier sections remain
historical design/verification records, not the current API.

**Earlier 2026-11-04 terrain checkpoint:** steps 1a and 1b supplied gravity and bounded fixed
mesh surfaces. Workspace check and 1,116 workspace tests passed (23 ignored), along with the
terrain/control smoke, contour regression and full 40-script sweep. The two optional Android
smokes skipped without emulator opt-in. Vehicle control and the detailed rover were then unbuilt.

**Earlier 2026-11-04 checkpoint:** gravity alone passed workspace check, 1,112 workspace tests
(23 ignored) and the full 39-script smoke sweep. Terrain and vehicle control were then unbuilt.
The two Android smokes require emulator opt-in.
Desktop first: a detailed four-wheel rover exploring a bounded generated lunar landscape.
The first playable milestone is a simple rover on a physics test course. The user accepts
each reviewable step before it lands. Hillclimb's separate 2D implementation is not a dependency.

## 0. End-to-end acceptance tests

Tests drive a real offscreen shell, sandboxed Lua VM, bridge and isolated vault. Source updates
use revision-checked edits. `frame(n)` drives simulation; screenshots and inspection spend no time.
Write each step's test first and observe its failure before implementation.

| step | proof through the real app path |
|---|---|
| 1a: gravity | Drop a body for the same driven time under default Earth, explicit lunar and zero gravity. Velocity matches acceleration; omitted gravity preserves existing behavior. A rejected reload changes no live world; a successful reload preserves pose/tick and changes subsequent acceleration. Capture and pause are observational. Restart restores the saved source's gravity. Native tests also prove independent worlds use independent gravity vectors. |
| 1b: terrain | A body drops onto a generated sloping surface, contacts the visible ground, and rolls downhill. Invalid geometry rejects before mutating any live world. |
| 1c: vehicle | Pointer/keyboard controls drive a four-wheel chassis over flat ground, a ramp, bumps and a drop. Throttle accelerates, steering changes heading, brakes stop, suspension stays within travel, and reset clears chassis/wheel motion. Raw wheel contacts and poses match rendered wheel positions. |
| 2: landscape | Seeded terrain is repeatable. A scripted drive reaches a crater sample site and returns without tunneling through the rendered surface. |
| 3: detailed rover | Close and chase captures show articulated wheels, steering, suspension arms and equipment. A changed cosmetic mesh does not change collision behavior. |
| 4: presentation | Sunlight, surface appearance and camera controls work together at a measured frame budget on the target device; screenshots verify actual pixels. |
| 5: agent partner | An agent inspects bounded terrain/vehicle facts and marks a reachable sample destination. Saved mission changes survive reopening; transient physics is not written per tick to documents. |

Lower-level tests pin physics behavior, strict binding validation, same-process repeatability,
sleep/wake, paused clocks, staged reload atomicity and app isolation. Real-path tests remain
the acceptance gate; unit tests cannot substitute for them.

## 1. Boundaries

`world` owns Rapier3D bodies, terrain collision, fixed-step vehicle control and inspection.
`runtime` owns mesh resources, camera/rendering and input vocabulary. `app_host` validates
Lua recipes and commands and preserves transactional staged reload. Lua owns vehicle parameters,
terrain recipes, controls, missions and presentation. Rapier/Bevy types never enter Lua or runtime.

Reuse the existing 120 Hz clock, hidden-tab pause, capture exclusion, resource budgets and
app-local lifetime. Evaluate Rapier's ray-cast vehicle controller before implementing suspension.
The first terrain and renderer must derive from the same bounded sampled height data.

## 2. First implementation step: gravity

Add optional `gravity = {x,y,z}` to module-initialized `gfx.world3d` recipes, in m/s²,
right-handed Y-up. Default remains `{0,-9.81,0}`. Components must be finite numbers bounded
to ±10,000; zero is valid. Inspect gravity with the world's tick and bodies.

Gravity is part of a world's staged recipe, not a per-frame command. Validate the entire
world batch before applying it. A successful reload updates gravity without resetting poses
or clocks and wakes dynamic bodies only when gravity actually changes. A failed load, view,
or recipe leaves running gravity and all native state unchanged.

## 3. Vehicle and world milestones

1. **Physics course:** gravity (1a), static terrain collision (1b), then the native vehicle
   controller and strict Lua bindings (1c). A primitive visible chassis is enough for this gate.
2. **Lunar area:** approximately 500 × 500 metres, a landing station, a crater and rocks.
   Generated lunar-like terrain is not claimed to reproduce a surveyed site.
3. **Detailed model:** body, equipment and articulated wheels/suspension. Prefer existing
   bounded mesh vocabulary for the first assembly; assess import formats against actual needs.
4. **Presentation:** surface materials, lighting/shadows, cameras and input polish, with measured
   performance. Streaming and planetary coordinates are unnecessary for this bounded area.
5. **Agent navigator:** inspection and destinations first; autonomous driving follows reliable
   manual control. Multiplayer and agent construction remain later milestones.

Hillclimb's parameters and test scenarios may inform tuning. Keep SI units explicit and
do not introduce a shared 2D/3D vehicle abstraction before both implementations justify it.

## 4. Verification and review

### Step 1c — 2026-11-05, Hillclimb findings and rover acceptance

Hillclimb's committed implementation now includes SI vehicle data, separate setup/drive/input,
explicit mass/centre of mass, force-based suspension and torque-limited wheel motors. Its
terrain tests found internal-edge snagging; test the 3D equivalent with coarse and subdivided
meshes. Reuse these conventions and tests, without merging its 2D implementation or introducing
a shared 2D/3D vehicle abstraction.

Use Rapier's native ray-cast vehicle controller for the first 3D vehicle. A recipe's `vehicles`
list declares a dynamic box chassis (id, position/rotation, box size, mass in kg, local centre
of mass) and 2–8 ray wheels. Wheels declare visual IDs, local mounts, radius/rest/travel in metres,
spring stiffness N/m, damping N·s/m, traction tuning, steering limit in degrees, drive and
brake force limits in N. A vehicle speed limit is m/s. Lua converts its torque settings to wheel
forces using radius. This controller does not simulate unsprung wheel mass or motor reaction
torque in the air; do not claim Hillclimb's full rigid-wheel behavior.

`world3d:drive(id, {throttle, steering, brake})` retains normalized controls, applied at 120 Hz.
Commands validate atomically and run in input/zone handlers; drive alone also accepts hover
input for held mouse pedals. Pose/reset commands retain their existing stricter permission.
`world3d:get(id)` copies the current body pose/velocity/tick for camera/UI reads. Vehicle and wheel
inspection reports real contacts, suspension length/force and wheel poses; rendering follows
those poses. Inspection/capture never calls the controller. Reset clears inputs and wheel state.
Configuration changes require remove/recreate; position/rotation remain reset targets on reload.
Whole-world/batch preflight rejects invalid references, limits and changes before any mutation.

Write tests first:

- Native flat-course tests: four contacts, mass/sag, acceleration, steering, braking/reverse,
  wheel travel/pose, reset, repeatability, invalid controls/configuration and reload atomicity.
- Lua/host tests: strict recipe and drive options, get read-back, handler permissions and
  failed reload preservation.
- `smoke_rover_drive.py`: real pointer/keyboard throttle, steer, brake, reset and ground-level
  chase camera over the course; rendered wheel poses agree with native inspection. Mouse pedals
  release on leaving, keyboard cancel clears controls, paused captures preserve state.
- Keep the existing terrain smoke green. The Lua vehicle parameters/setup remain separate
  from art, camera and input, following Hillclimb's useful split.

### Step 1b acceptance detail — 2026-11-04

Reuse a validated `gfx.mesh` as a fixed body's `mesh` shape. Vertices are local metres;
body position/rotation place it, while visual scale remains independent. Mesh colliders cannot
be dynamic bodies or sensors. Bound geometry to ±10,000 metres and each world's instanced
collision triangles to 131,072, checked before native allocation/mutation. Reload compares
positions and triangle indices, not process-local resource IDs or cosmetic vertex normals.
Changed collision geometry follows the existing remove-before-recreate rule.

`smoke_rover_terrain.py` uploads `demo_apps/moon_rover` into an isolated vault. Before the
binding exists it must fail opening the mesh body. Then it must prove:

- Actual course pixels, and the same mesh resource/counts in rendering and collision inspection.
- A box settles on the flat section; a sphere lands on the ramp, rolls downhill and crosses
  its join with the flat section without sinking below the sampled surface.
- Real pointer Release/Pause/Reset controls; paused/captured/inspected worlds do not advance.
- Invalid mesh geometry and edited collision topology fail activation without changing the
  running world. Exact repair succeeds with a new handle for equivalent geometry.
- Reload preserves native poses; reopening from sealed source reconstructs the course.

Native and Lua-host tests cover fixed-only validation, transformed terrain, high-speed drop,
per-world collision budgets, and equivalent geometry on reload. The course includes flat ground,
a slope, bumps and a shallow crater; its mesh and test sampling use the same triangle split.

**2026-11-04 play feedback:** the course now uses half-metre samples, grid/contour guides,
click-to-focus, close zoom and inspection presets. Ground view is the default: WASD walks the
camera at a constant sampled height above the same surface; it is a viewing tool, not a physical
character controller or rover chase camera. Mouse drag/arrows look, scroll moves closer, Q/E
adjusts eye height, and F returns to overview. Space pauses/resumes probes; R resets them.
The smoke drives actual pointer and keyboard paths, verifies ground height while descending
the ramp, and proves paused physics remains unchanged during camera navigation.

`scripts/test_rover_course.lua` reproduces contours tracing arbitrary triangle edges at a flat
plateau's height. Contour levels are offset from the course's plateaus, retaining 0.2 m spacing.
Guides are decorative; this fix changes no collision geometry. Run the pure Lua regression
through LoadTable as well as the real-shell smoke.

Run builds/tests only through LoadTable, with `--who moon-rover` and at most `-j 6`.
Use focused targets while iterating; workspace check/tests and `python3 scripts/smoke.py`
must be green before landing changes to the runtime/shell path. Run smokes with an explicitly
built `OSVAULD_SHELL_BINARY` so their helper never invokes a bare Cargo build.
Register new smokes in `SMOKES`. Inspect captured pixels. Obtain a fresh `pi -p` review using
the architect/app-host/runtime/Lua checklists relevant to the diff. Update current contract
docs with implementation and record verified progress; no merge/commit without the user's nod.

**2026-11-04 review exception:** the user explicitly requested skipping `pi` review for this
work. Apply the expert checklists directly and retain the test/smoke gates.

### Hillclimb cross-check — 2026-11-05

Inspected the separate `hillclimb` worktree's playable app, vehicle parameters and physics tests.
Keep its SI parameter/setup/drive/control separation and corner sag check `m*g/k`. Its renderer
uses 100 drawing units/metre, while this 3D world uses metres directly; do not import its force
or torque scaling. Its wheel joints model separate wheel bodies. This first 3D controller uses
Rapier's ray vehicle, with no wheel rigid-body mass or airborne motor reaction torque. Spring
and damping coefficients are divided by chassis mass inside the adapter; brake newtons become
per-step impulses. The native subdivided-slope rolling test passes without a terrain flag change.

The real driving smoke additionally uses a revision-checked reset-orientation change to drive
across the ramp and bumps, checking wheel travel and chassis height. Rendering resolves wheel
poses without allocating full inspection/configuration snapshots each frame. Only `drive` gains
hover input permission; the existing pose/reset handler guard stays narrower.

Hillclimb's later terrain commits were also checked: fixed-surface grip, a screen-space sky
gradient, parallax layers and bounded native particles. These currently belong to `World2d`.
The particle positions/emitter directions are XY and its renderer emits vector discs; 3D dust
needs XYZ motion and mesh/billboard rendering. A 3D perspective camera supplies geometric parallax
from actual depth. Surface friction on a 2D rigid wheel is not the ray vehicle's grip model.
Retain the bounded, seeded, visual-only particle policy for later; do not expose those 2D props
on `gfx.world3d` or claim they already work there.

### Low obstacle contact revision — 2026-11-05

Play feedback exposed a low mint block passing through the wheels beneath the chassis. The
native regression first reproduces this without moving the block. Add massless cylindrical
contact shapes attached to the chassis, following suspension length and steering each tick and
reset. Optional wheel `width` defaults to 0.26 m and is bounded to 0.02–2 m. Radius is inset to
94% of the ray tyre radius so rigid ground contact cannot replace spring support. These are
obstacle contact proxies, not independent sprung wheel bodies. Preserve mass, flat sag, steering,
brake, reset and world-reload tests. Extend the real driving smoke to hit the actual low block.

### Control dynamics check — 2026-11-05

The additional native quantitative regression measures flat-ground acceleration, lift-off coast,
brake-over-throttle and reversing while still moving. With the demo's 400 kg chassis and four
120 N drive limits, speeds are 1.199 m/s at one second and 2.398 at two. One second coasting
leaves 2.400 m/s: passive rolling resistance is not built. Full brake reaches 1.464 m/s after
0.5 s and zero at the three-second sample. Four 300 N brake limits cap nominal deceleration
at 3 m/s²; actual contact loads/traction reduce it during pitch. Opposite throttle decelerates
before reversing, reaching -2.369 m/s after four seconds from 2.4 m/s forward. These are one
same-process flat-course measurements, not guarantees for slopes or obstacle contacts.

### Direction controls revision — 2026-11-05

Play feedback exposed direction tests checking only that something turned. Positive drive
steering had turned left with +Z forward; the adapter now converts positive driver-right input
to negative Rapier Y steering. Native and real D-key tests assert the actual forward direction.
The chase camera also used the overview camera's yaw sign despite facing the opposite direction;
Right arrow had orbited left on screen. Reverse the follow-camera yaw input and assert actual
screen-right camera displacement while the native world stays paused. Overview and ground view
retain their existing directions. Mouse drag remains a separate grab/orbit gesture.
