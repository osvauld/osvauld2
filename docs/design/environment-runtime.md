# Composable Environment runtime — design and handover

Status: **planning baseline, 2026-09-12, revised 2026-09-20; the first bounded built-in-mesh 3D
rendering slice is now built, but no retained World, ECS, physics binding, cloth, world picking,
GLB or perspective UI is built.** The current runtime is a Lua-authored 2D interface/vector
substrate with one experimental 3D viewport. This document records the product direction
and experimental gates. The owned-WGPU substrate decision is recorded in §10; proposed names and
example Lua below are illustrative, not shipped contracts.

Companions:
- [Architecture](../architecture.md) — current live crate boundaries and invariants.
- [Frame implementation](frame-implementation-plan.md) — the shipped 2D visual-resource seam.
- [Visual substrate](visual-substrate.md) — prior Frame/math/physics research and history.
- [View and interaction](view-and-interaction.md) — current 2D interaction model and history.
- [Animation](animation.md) — retained UI transitions and the experimental Lua frame callback.

## 1. Product intent

Osvauld is growing toward a Lua-authored spatial runtime for interfaces, simulations and games.
The target is not a native graph widget and not a workspace-specific physics view. Any app may
host one or more environments: a knowledge universe, puzzle, deformable control, canvas, map,
particle system, world-space application panel, or game.

The product surface remains Lua. Rust supplies validated resources, retained hot state, bounded
bulk operations, rendering, device input and optional simulation backends. Domain meanings such
as documents, apps and graph edges remain app data; the Environment sees opaque stable entity IDs
and explicit components/relationships. A domain relation may project into a visual connection or
physical constraint, but is not inherently either one.

3D is a requirement. The first useful tier may use planar physics with perspective visual
transforms (2.5D), but the architecture must support real cameras, depth, X/Y rotation, ray
picking and a later true-3D rigid-body world without redefining identity or hierarchy.

## 2. Verified built baseline

The workspace currently resolves wgpu 29.0.4, winit 0.30.13, Vello 0.9.0, Parley 0.11.0,
Taffy 0.12.2, Kurbo 0.13.1, Euclid 0.22.14 and Peniko 0.6.1. No ECS or Rapier dependency is
present. **2026-09-20 implementation note:** `glam`/`bytemuck` and the first owned WGPU mesh/depth
pass have landed; it is the deliberately narrow model-viewer proof described below, not a World.

Built today:
- `El<M>` from Rust or strict `ui.*` Lua tables; Taffy layout; Parley text/editor support;
- shared 2D Geometry for layout transforms, clipping and inverse pointer mapping;
- Vello painting into one full-window RGBA texture, then a WGPU surface blit;
- 2D Frame resources: Path, solid/linear Brush, Fill, Stroke, Group and shared Instance;
- strict Lua `gfx.*` construction and `ui.frame` placement;
- scrolling, overlays, mouse click/drag, camera pan/zoom and animation primitives;
- experimental `on_frame(dt, elapsed)` Lua callbacks;
- live/custom screenshots and the animated `demo_apps/frame_orbits` proof;
- one experimental Lua `ui.scene3d` viewport with validated built-in cubes, perspective camera,
  instanced GPU buffers, real depth, final-target capture and bounded `DumpTree` inspection.

Hard current limits:
- Frame and Geometry transforms are 2D Kurbo `Affine`; there is no projective transform;
- the experimental 3D proof has a depth attachment, camera matrix, 4× MSAA and a lit cube pass,
  plus transformed-cube click raycasts, but no general mesh/material resources, GLB, hover/world
  query surface or correct foreground Vello pass;
- transformed clip eligibility uses screen-space bounding boxes while Vello paints actual shapes;
- drag capture retains press-time Geometry; deforming/current-geometry capture is unbuilt;
- Frame has no text resource, image, internal clip, identity/hit item or dynamic buffer;
- no retained world, fixed-step scheduler, collision/picking index, touch pipeline or physics;
- there is no per-subtree render target for a perspective/deformable UI surface.

Therefore the orbital app proves Lua-driven 2D visuals, transforms and aggregate declarations. It
does not prove a simulation world, world input, stable reload, depth or perspective.

## 3. Layer and authority model

The runtime should compose distinct authorities rather than ask one solver to own everything:

| layer | owns | does not own |
|---|---|---|
| El | app/shell flow layout, ordinary controls, outer placement and overlays | world entity placement or physics |
| UI surface | Taffy/Parley layout in logical material coordinates | camera projection or rigid-body motion |
| Frame | immutable reusable 2D vector visual data | mutable entities, time or physics |
| World | retained entities, hierarchy, cameras, views and typed relationships | domain truth or Lua VM identity |
| simulation system | forces, constraints, collision and integration for opted-in entities | every interface element by default |
| renderer/compositor | depth, mesh/surface composition and final pixels | app meaning |

A subtree has one external geometry authority:
1. **flow** — Taffy owns placement;
2. **transformed flow** — an explicit transformed footprint participates in layout;
3. **world** — an Environment owns entity placement; Taffy may still lay out an entity's local
   surface.

Visual-only transforms must remain distinct from layout-affecting transforms. Continuously
feeding projected screen bounds back into Taffy would cause reflow loops and text popping. A
rigid panel's text is laid out at its logical width and then projected; camera foreshortening does
not rewrap it. Authored/rest material-size changes may trigger re-layout.

## 4. World, views and lifetime

A World is mutable retained state, scoped to a running app by default and independent of whether a
particular view is currently painted. It must not live only in runtime's frame-swept `Store`: an
inactive tab or temporarily absent view must not silently destroy it.

A World may have multiple views. A view owns camera, viewport, hover, selection and capture. Adding
a second view must not step simulation a second time. Simulation clocks belong to worlds, not
windows or render leaves.

Recommended ownership seam to test:
- runtime defines generic native World/resources without knowing Lua;
- `LuaApp` owns an app-scoped, generation-checked registry of handles to those resources;
- successful staged reload reattaches compatible worlds by stable world ID and schema version;
- failed reload leaves the old VM and worlds untouched;
- incompatible schemas require explicit migration or reset, never reinterpretation.

Canonical CRDT data stores authored intent, relationships, seeds and explicit user decisions.
Velocity, contacts, temporary grabs, solver caches and cooling state are viewer-local. Persisting a
position is an app decision, not an automatic per-tick document write.

## 5. Entities and typed relationships

An ECS-like internal layout is reasonable for bulk iteration, but raw untyped component bags are
not the desired Lua contract. Entities need stable, generation-checked identity. A visual, body,
collider, semantic payload and script are optional capabilities rather than one giant object type.

Relationship kinds remain distinct:
- **transform hierarchy** — acyclic parent/local transform composition;
- **rigid constraints** — joints, distance, hinge, slider, weld and motors;
- **soft constraints** — cloth pins, stretch/shear/bend and material attachments;
- **graph/domain edges** — app-owned semantic relations;
- **visual bindings** — Frame/UI surface attached to a body, bone or deformable material;
- **transient interaction** — pointer capture, grab, hover, selection and contacts.

Every retained relationship needs stable identity, typed endpoint roles, validated parameters and
explicit deletion behavior. Do not encode all relationships as parenthood, array positions or an
ambiguous `{from,to}` record.

Illustrative only:

```lua
world:entity({
    id = "piece:7",
    transform = { position = {0, 0, 0}, rotation = {0, 0, 0, 1} },
    visual = piece_frame,
    body = physics.dynamic({ mass = 2 }),
    collider = physics.box({ 80, 50, 12 }),
})
```

The engine does not know whether `piece:7` is a puzzle piece, control, document proxy or game
object. App metadata maps it to domain meaning without granting host authority.

## 6. Lua/native boundary

Lua owns intent and composition:
- entities, stable IDs and app metadata;
- visual assembly and style;
- custom graph/layout/interaction policy;
- system ordering and parameters;
- semantic responses to bounded event batches.

Native code owns hot storage and bounded bulk work:
- transforms, meshes, colliders and solver state;
- rigid-body/PBD integration and broad phase when selected;
- depth rendering, picking and render extraction;
- shaping, textures and resource validation;
- fixed-step scheduling, sleeping and limits.

Avoid one FFI call or transform read per entity per tick. Lua systems may be appropriate for small
proofs and custom algorithms, but scalable worlds need aggregate queries/commands or generic native
systems. Native worlds must never retain VM callbacks. Events cross as bounded plain data; reload
re-registers Lua behavior against stable native identities.

## 7. Simulation systems

No single physics backend fits every interface/world behavior:

| need | candidate |
|---|---|
| ordinary hover/press/transition | existing retained timelines/springs |
| graph forces and authored procedural motion | Lua or bounded bulk force systems |
| rope, cloth, hair, elastic borders, soft bodies | PBD/XPBD |
| planar rigid UI/puzzles, friction, stacking, joints, CCD | Rapier2D |
| real 3D rigid bodies and scene queries | Rapier3D |

Rapier's documented scope is rigid bodies, collisions, forces, joints, CCD and scene queries. It
does not provide a general cloth/deformable-body solver; contact softness is not a soft body.
Rapier is optional per world, not the definition of Environment physics. Mixed rigid/cloth worlds
need an explicit one-way or two-way coupling policy.

A real scheduler is per-world fixed-step state: monotonic accumulator, bounded substeps, explicit
excess-time policy, sleep/awake state and render interpolation. Hidden views pause by default unless
a declared reason keeps their world active.

The shipped `on_frame` must not be treated as that scheduler:
- its `dt` is Runner-global, so "first tick is zero" is not true for a newly added subscription;
- its required ID is discarded when dispatching a positional callback index;
- an in-range stale callback index may identify a different current closure;
- a failing handler remains declared and can retry/log forever;
- custom screenshots dispatch callbacks despite not presenting, then request restoration;
- tests inject a callback message directly and do not pin these scheduling semantics.

Keep `on_frame` as an experimental visual/prototyping mechanism until those claims are corrected.
Simulation screenshots should be observational unless a request explicitly asks to advance time.

## 8. Rendering and compositing

Frame stays explicitly 2D. Do not force perspective matrices or Z into Kurbo `Affine`, Taffy or
the Frame tree. A separate 3D scene vocabulary owns cameras, meshes, materials, depth and instances
of 2D surfaces.

Provisional rendering recommendation: keep Taffy/Parley/Vello and test a narrow WGPU compositor.
Render a logical UI/Frame surface into a GPU texture, then draw that texture on perspective quads
or deformable meshes in a depth-tested pass on the same device and event loop:

```text
Lua El/Frame → Taffy + Parley + Vello → GPU surface texture
Lua scene → camera + mesh + material → WGPU depth pass → window
```

There must be no CPU texture readback between these passes. Surface textures need explicit size,
memory, dirty/rerasterization and lifetime budgets. Existing live/custom screenshots must capture
the final composite rather than only the Vello target.

### Text on rigid and deformable surfaces

A rigid panel lays out and shapes text in local logical coordinates, then projects the complete
surface. Perspective does not reflow lines.

For cloth, the shortest credible path is a Vello-rendered texture mapped onto a PBD mesh. It
preserves shaping and naturally deforms but is no longer resolution-independent. Quality depends
on texel density, mipmaps, anisotropic filtering, viewing angle and camera distance. Start with a
bounded adaptive-resolution surface; if it cannot meet an agreed readability gate within a 2048²
surface, separately investigate MSDF glyphs or tessellated glyph outlines. Do not claim Vello's
current final-resolution sharpness for pre-rasterized perspective textures.

## 9. Input and picking

Outer ordinary UI remains first in input order: overlays and controls must not ray-click through to
a world. Once a World view wins outer hit eligibility:
1. map screen position through outer Geometry to its surface;
2. construct a camera ray;
3. resolve the nearest depth-visible collider/triangle;
4. return stable world/entity identity plus named coordinate data;
5. capture by world ID + entity generation + pointer ID;
6. queue commands at a defined simulation-step boundary.

A rigid UI panel maps a ray-plane hit into surface-local coordinates, then routes through its
ordinary hit tree. A deformable surface uses ray-triangle intersection and barycentric UV/material
coordinates. Capture must cancel on entity removal, view disappearance, incompatible reload or
device cancellation; it cannot retain press-time geometry while the surface deforms.

Events should name spaces rather than return ambiguous tuples: screen, view/surface, ray, world
point/normal, material UV and target-local where available. Mouse, pen and touch enter one pointer
model; multi-touch camera arbitration is a later tested slice, not implied by basic picking.

## 10. Alternatives evaluated

### Native WGPU compositor — provisional first choice

Advantages: same device/window/event loop, preserves current Lua and UI architecture, smallest
falsifiable seam. Cost: we own mesh/depth pipelines, cameras, surface caching, picking and resource
budgets.

### Bevy 0.19 — not the runtime substrate

Verified upstream facts at review time: Bevy 0.19.1 uses WGPU 29.0.3 and winit 0.30, supports
perspective cameras, render-to-texture, custom meshes and render resources. `bevy_vello` 0.14
supports Bevy 0.19 with Vello 0.9. Bevy's built-in UI transforms remain 2D, its UI pass has no depth
attachment, and built-in text uses a glyph atlas. Bevy does not automatically solve perspective
Osvauld UI, deformable text, Lua authorship, reload or UV-to-nested-hit routing.

The original recommendation was to run a Bevy host challenger before selecting a renderer.
**Revised 2026-09-20:** Osvauld will retain its existing application, device and composition
pipelines and use focused libraries beneath an owned WGPU renderer; Bevy and its tightly coupled
render/PBR/glTF crates are references and feature benchmarks, not runtime dependencies. See
[the model viewer plan](3d-model-viewer.md) for the selected substrate. Reconsideration requires a
dated decision revision, not an implementation spike assumed by default.

### Fyrox / three-d — not preferred

Both are credible in their intended domains, but their OpenGL-oriented/current integration and
retained UI/text stacks conflict more strongly with the existing WGPU/Vello/Parley runtime.

Primary references used in the 2026-09-12 review:
- Vello 0.9 README: https://github.com/linebender/vello/blob/v0.9.0/README.md
- Bevy 0.19.1 source: https://github.com/bevyengine/bevy/tree/v0.19.1
- bevy_vello 0.14: https://github.com/linebender/bevy_vello/tree/v0.14.0
- Rapier overview: https://rapier.rs/docs/
- Rapier rigid bodies: https://rapier.rs/docs/user_guides/rust/rigid_bodies
- Rapier joints: https://rapier.rs/docs/user_guides/rust/joints
- Rapier scene queries: https://rapier.rs/docs/user_guides/rust/scene_queries

## 11. Falsifiable prototype gates

**2026-09-20 revision:** the immediate rendering experiment is now the
[Lua-first model viewer proof](3d-model-viewer.md): built-in mesh geometry, depth, camera
control and picking inside a real shell-hosted Lua app, followed separately by GLB import.
The original gates below are retained as broader Environment acceptance targets. Passing
that narrower proof does not satisfy Gate 1's projected text/nested controls. The owned-WGPU
substrate decision supersedes the Bevy challenger. Gate 0 still applies before continuous
animation is used as evidence.

These are experiments, not production APIs or performance promises.

### Gate 0 — scheduling harness

Dynamically add/remove an experimental tick, hide/show a tab, inject a handler error, reload and
take live/custom screenshots. Pin first-subscription `dt`, pause/resume, stale-message generation,
error quarantine and observational-capture behavior before using Lua frame callbacks as evidence.

### Gate 1 — native perspective panels

On one WGPU device, render two intersecting Y-rotated panels with real depth. Each contains Taffy-
laid-out multilingual text and a nested button rendered by Vello. A bridge-synthesized ray click
must always reach the nearest visible panel and correct button. Capture the final composite with no
CPU texture readback. This decides whether the narrow native compositor is viable.

### Gate 2 — deformable surface

Animate a bounded cloth mesh carrying a Vello-rendered multilingual label and controls. Implement
ray-triangle → barycentric UV → logical surface input. Pass only if sampled hit error is below one
logical point and text is acceptable at an agreed minimum angle/distance with at most a 2048²
surface. Failure triggers the glyph/mesh rendering investigation, not silent quality reduction.

### Gate 3 — Bevy challenger (superseded 2026-09-20)

The original gate would reproduce Gate 1 with Bevy 0.19/Vello 0.9 while preserving current app
`view/update`, `ControlFlow::Wait`, screenshots, hit routing and one device/window. It is no longer
on the implementation path: the selected substrate is the owned WGPU pipeline plus focused
libraries. Bevy remains a comparison implementation; reopening this gate requires revising the
recorded decision first.

### Gate 4 — retained world lifecycle

One app owns a retained world shown through two views. It steps once, sleeps, pauses hidden, keeps
state through compatible successful reload, survives failed reload unchanged, rejects stale/cross-
app handles and resets/migrates incompatible schemas explicitly. Screenshots do not advance it.

### Gate 5 — physics proofs

Build separate Lua-authored proofs rather than one misleading mega-demo:
- a small custom force graph;
- a Rapier2D physical control/puzzle with stacking, hinge, sensor, CCD and pointer grab;
- a Rapier3D tabletop with camera/depth picking inside surrounding El UI;
- PBD/XPBD cloth pinned to and colliding with a rigid frame.

Only after these gates should a public World/Entity/Relationship API be frozen.

## 12. Safety, inspection and budgets

Every native resource is app-scoped and generation checked. Validate finite numeric ranges,
entity/relationship counts, mesh vertices/indices, texture dimensions/bytes, solver substeps,
event batch sizes and traversal depth. A malformed declaration must fail staging and leave the live
world untouched.

Inspection must expose bounded summaries—stable IDs, kinds, transforms, bounds, relationship
endpoints, sleep state and resource costs—without dumping textures, secrets or arbitrary Lua
objects. Bridge gesture synthesis must enter the normal input pipeline. Screenshot tests complement
structural, picking, reload and scheduling assertions rather than replacing them.

Worlds are capabilities. Creating a world does not grant arbitrary workspace, filesystem, network
or GPU shader access. Domain references remain app data until the shell explicitly authorizes an
action.

## 13. Open decisions

Do not silently settle these during implementation:
- exact owned-WGPU pass/resource architecture; the Bevy-host alternative was closed 2026-09-20;
- 3D math representation/library and typed coordinate-space wrappers;
- ECS/storage library versus focused packed stores;
- surface texture cache keys, adaptive resolution and memory budget;
- exact world registry owner and compatible-reload migration protocol;
- fixed-step defaults, hidden-world policy and excess-time behavior;
- Lua query/command batching and custom-system execution model;
- Rapier version/features and 2D/3D world coexistence;
- PBD/XPBD solver and rigid/cloth coupling direction;
- accessibility/semantics for projected or deformed UI;
- layout-affecting transforms versus world-owned placement contract;
- nested live app surfaces and their focus/authority boundary.

## 14. Handover order

A new agent should:
1. read `AGENTS.md`, architecture, status, conventions and this document;
2. inspect the current Render target/device ownership and screenshot path;
3. reproduce the `on_frame` audit before relying on it;
4. write the revised Lua-first model viewer proof plan in ≤100-line implementation slices
   and get user approval (see the 2026-09-20 revision in §11);
5. load `expert-runtime` for runtime changes and `expert-architect` for the cross-cutting seam;
6. keep Frame 2D and preserve the current app/Lua pipeline during the spike;
7. keep renderer dependencies focused and recheck compatibility before each capability tier;
8. update status honestly after each landed slice and keep rejected alternatives as dated notes.

Do not begin with a public ECS API, graph widget, complete physics binding or Frame perspective
extension. The next question is narrower: can one shared-device compositor preserve Osvauld's
vector/interface strengths while adding correct depth, projected input and deformable surfaces?
