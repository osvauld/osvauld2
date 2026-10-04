# Math-authored 3D — toward a procedural marble machine

Status: **2026-10-01, branch `3d-math`: bounded immutable mesh resources, native/Lua scene
mesh selection, identity-based GPU cache, triangle picking and raw mesh inspection implemented,
pending user acceptance. `demo_apps/mesh_math` and its registered smoke prove Lua-calculated
wave geometry and shared instances. The committed 2D world foundation is integrated and a
native Rapier3D drop/reset backend, retained body IDs, bounded fixed-step clock and immutable
pose-to-render snapshots are implemented. Runner/shell/host dispatch, reload-safe `gfx.world3d`,
input-handler reset, omission/tab pause and raw `worlds3d` inspection now drive the Lua-generated
sphere/platform in `demo_apps/marble_gates`; its registered smoke checks physics and captures.
Authored/reset rotation, zones, atomic pose/motion commands and paused resolved snapshots now
drive a complete Lua Marble Gates level with adjustable tilt, release, win/loss and retry in the
[continuation checkpoint](marble-gates.md). Higher-level curves/tubes, spatial gradients,
configurable physics, queries and independent agent-as-maker acceptance remain unbuilt.**
Lua-authored geometry and raw agent feedback precede Blender import. API names below are
provisional. This revises the next-step order in [model viewer](3d-model-viewer.md), not the
owned-WGPU decision. The broader [Environment](environment-runtime.md) gates still apply.

## 1. Goal and baseline

Build a procedural marble machine in stages: curved rails, a revolved funnel, supports,
then simulated marbles and gates. Geometry, materials, inspection and simulation remain
separate capabilities; the first milestone is static and has no physics or asset import.

Main has a strict Lua cube scene, shared-device WGPU depth/MSAA pass, cube ray picking,
camera/object inspection and final-composite screenshots. It has one rendered viewport
and incorrect foreground Vello composition; neither limitation is solved by arbitrary meshes.
The mesh lighting shader needs inverse-transpose normal transformation for nonuniform scales.

`3d-mesh-data` commit `c1d4eb7` adds internal validated indexed geometry and local bounds,
not Lua mesh handles or general resource caching. At audit time its worktree also had
uncommitted multi-built-in-mesh rendering and triangle picking: volatile coordination information,
not a dependency contract. Review committed work separately with its owner; never copy or merge
its uncommitted work without approval.

## 2. First vertical proof

**2026-10-01 revision:** after the mesh proof, the next visible milestone is **Marble Gates**:
one marble drops onto a platform, bounces, settles and resets through Lua. A ramp and goal follow.
This moves milestone F ahead of C–E; curves and spatial gradients are useful later, not a game
prerequisite. The earlier sculpture proof below is preserved as the deferred geometry direction.

A Lua app describes a cubic 3D curve and a constant-radius capped tube, assigns an
object-space height gradient, and displays repeated instances beside ordinary controls.
Controls change geometry parameters on interaction, not every view or animation frame.
An agent can inspect the recipe, bounds and costs, sample the curve, ray-pick the generated
triangles and edit the recipe. Screenshots independently check appearance and depth.

Provisional resource vocabulary: `gfx.curve3d`, `gfx.tube`, `gfx.material`, and
`gfx.linear_gradient3d`; existing `gfx.scene3d` objects accept immutable mesh/material handles.
Do not add a public World/ECS API, arbitrary shaders or Lua surface callbacks in this proof.

## 3. Contracts to resolve before their implementation slices

- Right-handed, Y-up local geometry; document winding, camera clipping and rotation conventions.
  Coordinates are abstract scene units until physics establishes a unit contract.
- Curves: move/line/quadratic/cubic, initially one open continuous path. Evaluation returns
  position and tangent with an explicit parameter convention. No implied arc-length timing.
- Tube: positive radius, bounded radial segments, adaptive centerline sampling, optional caps.
  Define tolerance units/error guarantees; fail when limits prevent meeting them. Stable frame
  transport must handle or explicitly reject zero tangents, reversals and sharp joins.
  Closed loops, self-intersection repair, variable radius and arbitrary profiles are later tiers.
- Meshes: validated finite vertices/normals, indices, bounds and immutable construction recipe.
  Retain explicit geometry as an escape hatch, but do not force agents to author triangle arrays.
  Define per-resource and scene-wide CPU/GPU budgets before exposing arbitrary mesh handles.
- GPU resources: renderer-owned cache keyed by native resource identity, not caller strings;
  explicit eviction and device recreation behavior. Repeated instances share uploads. Handles
  are app-scoped and generation checked; reject stale/cross-app use. Define successful reload
  reattachment or replacement and tab teardown; failed staged reload leaves old resources usable.
  Native resources never retain Lua callbacks.
- Materials: opaque solid/linear gradients first. Define stop order, duplicates, interpolation
  color space and extend mode. Evaluate local position in the fragment shader. Do not claim
  physically based rendering; 2D Frame brushes stay distinct, sharing stop rules where suitable.
- Picking: use the rendered mesh, culling policy and near/far clipping; return stable object ID,
  triangle index, barycentric coordinates, world point and geometric normal. Collision shapes
  are separate; a render mesh does not automatically imply a suitable dynamic collider.

## 4. Agent feedback from the first geometry slice

Keep authored recipe and resolved facts separate. Extend existing scene inspection with
resource identity, recipe summary, local/world bounds, vertex/triangle counts and material
parameters. Resource graphs use bounded references, not repeated expanded mesh dumps.

Curve sampling and ray queries return named coordinate spaces and the snapshot/revision they
examined. Structural bounds are conservative and frustum inclusion is not occlusion visibility.
Large buffers require explicit bounded/pageable reads if added later. A retained simulation
will additionally identify its tick and distinguish authored poses from simulated poses.

## 5. Libraries and execution boundary

Reuse `glam` for vectors/transforms, `kurbo` for 2D profiles, WGPU/Naga for GPU execution.
Evaluate focused curve/frame implementations and `parry3d` queries before writing algorithms;
`lyon_tessellation` becomes relevant for filled profiles/extrusion, not the initial round tube.
No new CAD/game-engine dependency is selected. Check compatibility and maintenance first.
CPU compiles bounded static geometry once; GPU transforms, lights and evaluates gradients.
GPU compute for deformable geometry is a later measured need, not an initial requirement.

## 6. Sequence and acceptance gates

Each row is a milestone, split into reviewed ~100-line code slices; tests ride with each slice.

| milestone | evidence required |
|---|---|
| A: mesh foundation | review/adopt existing validation; bounded shared meshes render; CPU/GPU budgets, inspection caps and cache lifetime tested before exposing handles |
| B: Lua meshes and picking | strict declarations; triangle hits agree with depth/culling/clipping; failed reload preserves old scene |
| C: curve and tube | deterministic bounded generation; tolerance/frame edge-case tests; recipe and curve samples inspectable |
| D: gradient sculpture | spatial gradient stays attached under transforms; nonuniform-scale lighting correct; shared instances and parameter edits |
| E: funnel and supports | revolve profile, named assembly and shared resources; inspect dimensions and spatial relationships |
| F: one marble | choose physics backend, fixed-step clock and render/collider agreement; observational captures; body/contact feedback |
| G: machine | multiple marbles, sensors and gates; deterministic driven-time smoke can diagnose a stuck marble |

Add a real Lua demo and registered smoke for the geometry proof, exercising pointer eligibility,
raw inspection, source editing and clean console plus live/custom composite captures. Run workspace
check/tests and `python3 scripts/smoke.py` before landing runtime/host/shell work. Foreground
composition remains an explicit blocker for demos with UI overlays over the viewport.

## 7. Marble drop/reset slices — 2026-10-01

Extend the existing `world` crate, not a second engine or renderer. Import only committed
`world-engine` work (`e23660b`); other worktrees' uncommitted work is not a dependency.

1. Integrate the 2D foundation while preserving the pending mesh work and both sets of smokes.
2. Native Rapier3D solver: centred sphere/box colliders, metres, right-handed Y-up, gravity
   `{0, -9.81, 0}`, fixed 1/120-second steps. Box dimensions are full dimensions. Visual meshes
   are independent. Inspect position, quaternion, linear/angular velocity, sleeping and tick;
   reads do not step. Reset restores position and identity rotation, wakes the body, and clears
   velocity and forces.
3. **Native model/clock, Runner hook and host dispatch implemented:** retained author-ID bodies (max 256)
   validate batches before changing membership. Existing IDs keep simulated poses; authored
   positions update reset targets only. Collider/type changes require remove/recreate. Clock
   planning runs at 120 Hz with at most eight catch-up steps; excess time is reported and not
   retained as a backlog. Pausing clears fractional carry; resume establishes a new baseline.
   Quiet worlds suspend their clock. Sorted inspection separates recipes from resolved state;
   immutable scene snapshots share meshes and apply native poses to matching object IDs.
   `App::advance_simulation` now runs before normal-frame view construction and skips live/custom
   captures; the app supplies fixed-step planning and redraw demand. Headless tests exercise a
   real World3d; a GPU test verifies capture exclusion with actual readback. Legacy Lua `on_frame`
   behavior is unchanged. The original gate required shell/host dispatch, reload, view omission
   and teardown wiring before exposing the host API; failed reload must not mutate accepted
   worlds and captures must remain observational.
   **2026-10-01 implementation update:** module-scope `gfx.world3d` collects recipes in the staged
   VM. App-local batches preflight all worlds before acceptance; existing poses/clock state remain
   native. Walked scene handles resolve current poses; omission and inactive tabs pause without
   deletion or catch-up. Dropped source declarations and app teardown release worlds. Input
   handlers can reset; frame/hover handlers cannot. Ordinary offscreen RPCs repaint without
   advancing time, preventing capture/inspection from creating native catch-up debt.
4. **Visible proof implemented:** Lua-generated sphere, platform, orbit/zoom, visibility toggle
   and Reset button, with raw authored/resolved body inspection and a registered deterministic
   driven-time smoke. Falling and settled screenshots have been inspected. Authored/reset
   rotation and a visible tilted ramp were the first step of [Marble Gates](marble-gates.md).
   Its continuation checkpoint now includes live adjustment, sensors and Lua game rules;
   independent agent-as-maker acceptance remains a separate gate.
