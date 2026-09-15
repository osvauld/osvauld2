# Frame — Lua-programmable visual substrate implementation plan

Status: **implementation in progress, 2026-09-11.** Runtime and Lua now share immutable measured
Frame, Path, solid/linear Brush, Fill, Stroke, Group, and Instance resources with recursive work
budgets and Vello rendering. `ui.frame` places the payload through normal El geometry, while
`demo_apps/frame_orbits/` and its bridge screenshot script are the live proof. Arcs, radial/sweep
brushes, shaped text, internal clips, animation, hits, simulation and export remain unbuilt. This
document records agreed scope and phased contracts; unresolved later-phase spellings are not
shipped API. No production implementation is authorized merely by listing a phase here.

Companions:
- [Visual substrate](visual-substrate.md): original vision and research, retained as history.
- [Viewport geometry rebuild](viewport-geometry-rebuild.md): placement, inverse input and clips.
- [Animation](animation.md): retained drivers, scheduling and compositing design.
- [Architecture](../architecture.md): crate boundaries and invariants.
- [Lua guide](../lua-apps.md): the actual shipped authoring contract.
- [Resolved UI senses](app-discovery-and-invocation.md): future Runner inspection and gestures.

## 1. Scope and success

Frame is a composable, measured visual scene in local coordinates. At the app surface it is
an embeddable kind of El; internally it is visual data carried by El, not another layout or
application tree. It must also be usable as background and foreground decoration.

**Lua authors must be able to invent visual types without adding a Rust widget enum.** Charts,
diagrams, animated illustrations, math composition and simulation output are consumers of the
same primitives. Rust supplies rendering, shaping, resources and bulk computation; Lua supplies
application algorithms, composition and visual policy.

A Rust-only renderer is an internal step, not a completed milestone. The first milestone is a
Lua-authored visual with paths, nested transforms, a shaped label, animation and a hit region,
embedded in a zoomable viewport. Camera arithmetic must not appear in the app.

All capabilities in §3 remain in scope. Later phases mean deferred implementation, not removal.
This is not a commitment to old line-count, timing or performance estimates in the research doc.

## 2. Current foundation and boundaries

The live pipeline remains immediate El descriptions, Taffy layout, placed geometry and Vello
paint. Runtime already has custom paint closures, rich text runs, retained press feedback,
zoom/pan, typed coordinate foundations and transformed portal anchors. Lua already has app-local
multi-file require. These supersede the corresponding historical observations in the original
visual-substrate doc; they do not establish that Frame exists.

Overlay anchoring and press-settle stability were live-verified. Cross-gesture overlay occlusion
and stable-ID capture re-resolution are explicitly deferred, not fixed. They do not block static
Frame work. Advanced Frame capture must revisit them rather than inherit stale geometry silently.

| owner | responsibility |
|---|---|
| `runtime` | Frame values, measurement integration, rendering, resolved hit mapping, resource/animation substrate; no Lua types or closures |
| `app_host` | bounded Lua declarations and handles → runtime values; validation, callback registration, lifecycle bridging |
| Lua libraries | charts, diagram composition, palettes, ticks, simulation orchestration and other product policy |
| `shell2` | host wiring and demos, not an alternative renderer or chart engine |
| Geometry | external placement, content/screen inverse mapping and ancestor/viewport clips |
| Frame | ordered local visuals, internal group transforms and local clips |

No separate PaintItem hierarchy, VDOM, second Flexbox tree or parallel Lua-only renderer.
Reference-only crates are research sources; none are extended. New crate extraction needs a
separate boundary decision, not an automatic dependency on legacy math/PDF/chart code.

## 3. Complete capability ledger

| required capability | deliverable / proof | phase |
|---|---|---|
| vector paths | lines, quadratic/cubic curves, closed paths, fill rules, stroke caps/joins/dashes | A–B |
| brushes | solid colors, gradients and reusable patterns with explicit brush coordinates | A, D |
| nested composition | ordered groups, affine transforms, shared subframes | A–B |
| text | shaping, fallback, measured text and positioned glyph resources | B |
| math | baseline-aware composition, MATH font metrics, unencoded glyph variants/assemblies | G |
| images | managed resources, crop/fit, bounded decoding | D |
| embedding | content, background, foreground, overlay content and zoom/scroll integration | B, D |
| local effects | shape clips, group opacity, blend modes and masks | D |
| render-pass effects | arbitrary-content blur, color transforms and glow | H |
| interaction | stable internal hit identities, pointer coordinates, capture/cancellation and semantics | C, F |
| animation | monotonic time, scheduling, retained springs/timelines and Lua parameterization | C, E |
| simulation | Lua algorithms, retained state, bulk buffers/instances | E |
| reuse and caching | shared resources, bounded lifecycle, versioning and measurable reuse | A, E |
| morphing | explicit matching/interpolation rules; unmatched items and incompatible paths handled | E |
| drag previews | reuse visual data at root placement, without copying callbacks/source clipping | F |
| inspection | bounded Frame structure and resolved hit data through Runner | F |
| export | SVG/PDF adapters with explicit unsupported-effect policy | H |

Frame is a 2D substrate. A producer may project 3D into it; this plan does not promise a 3D
renderer. Editable rich text is a separate editor problem, not solved by displaying glyphs.

## 4. Data and resource contract

Proposed conceptual model, not a final Rust enum:

```text
Frame
  intrinsic size: width, height
  optional baseline
  ordered items

Item
  path + fill/stroke brushes
  shaped/positioned glyph run + font resource
  image resource + source/destination geometry
  group: local transform + optional clip/compositing + subframe
  optional local hit/semantic identity
```

Separate three lifetimes:
1. **Resources:** paths, fonts, images and reusable subframes.
2. **Visual snapshot:** ordered instances, placement, styling and references to resources.
3. **Application state:** selection, simulation, spring targets and animation progress.

Snapshots should be immutable after publication. Sharing must not require copying geometry on
every view. Exact representation (`Arc`, arena handles, copy-on-write) remains an implementation
decision; it must support bounded resource accounting and prevent cycles or dangling references.
No VM values, app callbacks, raw GPU handles or filesystem paths belong in portable Frame data.

IDs identify logical instances, not shared resource allocations. Reusing one subframe twice must
not collide: event identity includes the owning El and instance path. Retained IDs are namespaced
per app item. Content hashes/cache keys are not interaction identities.

Caching is not automatically correct because data is immutable: keys must include resource
versions, font/shaping inputs and relevant rendering conditions. Measure benefits before adding
raster caches that compromise zoom quality.

## 5. Measurement and coordinate contract

The chain is `item/group local → Frame local → El content → viewport → logical screen`;
physical-device scaling happens at the rendering boundary. Each coordinate has an owner.
The viewport step is schematic: nested cameras contribute multiple mappings; ordinary UI without
an enclosing camera maps directly through root placement to logical screen. A portal starts a new
root placement rather than inheriting its anchor's camera transform.
Internal group spaces are dynamic; do not pretend one static Rust type identifies every group.

- Frame positions its items; Taffy positions the complete measured island.
- Intrinsic size is a layout claim, distinct from painted bounds (strokes/shadows can overflow).
- Baseline is measured from the Frame's top edge. Absence is explicit, not a fabricated zero.
- Padding places the visual origin at the El content origin, not the border origin.
- Proposed default: explicit El size overrides allocation, not the Frame's coordinates. Scaling
  to fit must be explicit; no accidental stretch or reflow when camera zoom changes.
- Define contain/cover/stretch and alignment as explicit fit policies before shipping them.
- Frame bounds do not implicitly clip. Local clips and outer El/viewport clips are explicit.
- External clip boundaries and internal clips must be applied identically for paint and input.
- Singular transforms may paint a degenerate result but cannot be inverse-hit-tested; reject
  non-finite inputs and specify non-invertible hit behavior without panicking.
- Group opacity composites the group once; multiplying each child's alpha is not equivalent.

Responsive Frame production needs a separate decision: fixed intrinsic visuals come first.
A later constraint-aware producer must avoid running arbitrary Lua recursively inside Taffy
measurement or creating layout/Frame-size feedback loops.

## 6. Lua authoring contract (proposed)

Use the same runtime vocabulary as Rust. Favor batch declarations for ordinary scenes and
opaque managed resources for reuse. Do not require one FFI call per path segment or glyph.
Strict unknown-field/type checking applies inside Frame declarations just as it does to ui props.

Illustrative syntax only; **not available in apps today**:

```lua
local visuals = require("visuals") -- app-owned composition library
local icon = visuals.make_icon() -- reusable local visual resource

return function()
	local picture = frame.scene({
		width = 240, height = 120,
		frame.group({
			id = "marker", transform = { 1, 0, 0, 1, 40, 30 },
			frame.instance(icon),
		}),
		frame.label({ text = "Lua owns this visual", x = 8, y = 90 }),
	})
	return ui.frame({ id = "picture", visual = picture })
end
```

The final API must demonstrate these operations, regardless of names:
- batch path commands, styles and nested composition;
- shape text and obtain size/baseline, then compose it without reshaping at paint;
- construct small scenes in view or reuse prebuilt resources;
- instantiate resources multiple times with independent transforms/IDs;
- declare local hit regions and receive plain event tables at the containing El;
- animate from a frame clock/state without document writes or camera calculations;
- bind resource-backed bulk instances without thousands of scalar boundary crossings.

Ordinary Lua text authors should not need glyph IDs. Specialized math producers need access to
measured glyph composition, including glyphs without Unicode codepoints. Font access remains
managed and sandboxed. Resource creation/measurement may memoize internally but must not mutate
application state during view. Simulation steps run in update/tick, not in view.

Before implementation, pin positional-child versus data-table semantics, color/transform formats,
error locations and resource-handle validity in app_host tests. No silently ignored fields.
The defaults libraries should be Lua. Workspace-shared library resolution is separate, unbuilt
infrastructure; initial demos use the existing app-local require.

### 6.1 Geometry and path authoring

Frame must expose geometry capability, not Rust crate names. Runtime uses Euclid to distinguish
owned boundary spaces and Kurbo `BezPath`/f64 geometry for paths and Vello. Lua declarations are
Frame-local; apps do not perform camera conversion. Event tables name `screen`, `frame`, and
`target_local` positions rather than returning an ambiguous pair.

The initial path grammar supports batched move, line, quadratic Bézier, cubic Bézier, elliptical
SVG-style arc, and close commands. Paths are reusable geometry resources; fill/stroke are ordered
items so one path can serve different brushes, local clips and hit policy. Stroke includes width,
cap, join, miter limit, dash pattern and offset; fill rule is explicit. A pure app-local Lua helper
may accumulate friendly commands and call `gfx.path(commands)` once. Do not make every `line_to`
a host call.

The complete geometry toolkit includes:
- f64 points, vectors, sizes and rectangles; arithmetic, dot/cross, length, normalization,
  perpendiculars, projection, distance, interpolation, containment, union and intersection;
- affine translate/rotate/scale/skew, ordered composition, inversion and application to points,
  vectors, rectangles, paths and group instances;
- curve/path bounds, segment evaluation, derivatives/tangents/normals, split/subsegment,
  arc length/inverse arc length, nearest point, flattening, winding/containment and reversal;
- later stroke-to-outline, trimming, offsets, intersections, boolean operations, simplification
  and explicitly matched morph normalization.

Cheap scalar algorithms and ergonomic helpers belong in Lua; aggregate validation/compilation,
shaping and large queries cross into Rust once. Occasional scalar queries are allowed—the rule is
to avoid an FFI call inside a hot element/sample loop. Large samples and instances use bounded
batch calls or buffers. Luau numbers and Kurbo paths remain f64. Luau native vectors/f32 or packed
GPU data are allowed only after rebasing where precision requirements permit.

### 6.2 Feasibility against pinned dependencies (verified 2026-09-11)

The workspace currently resolves Euclid 0.22.14, Kurbo 0.13.1, Vello 0.9.0, and mlua 0.10.5.
This is a source check, not a frozen dependency promise.

| capability | current support / required work |
|---|---|
| path storage and M/L/Q/C/Z | direct: Kurbo `BezPath` / `PathEl` |
| SVG elliptical arcs | wrapper: Kurbo `SvgArc` → `Arc` → cubic path approximation with a bounded tolerance |
| render fill/stroke/dashes | direct: Vello scene and Kurbo `Stroke`; app validation still required |
| bounds and fill containment | direct: Kurbo `Shape::bounding_box` / `winding` |
| segment eval/split/tangent/length/nearest | direct per `PathSeg` traits; whole-path distance sampling needs a cumulative-length wrapper |
| flatten/reverse/stroke outline | direct Kurbo facilities, exposed only with output/work budgets |
| offset | partial: Kurbo exposes cubic offset machinery, not a complete arbitrary-path policy |
| intersections/boolean operations/morphing | not supplied as the required general contract; separate algorithms/dependency decisions |
| typed points/transforms | direct in Euclid at Frame/El/content/screen boundaries; dynamic group ownership remains runtime-checked |
| group opacity/blend/clip | direct Vello layers, but Vello documents clip/blend interaction limitations that need fixtures |
| glyph rendering | direct Vello support; shaping/resource/baseline API still needs design |
| Luau buffers | available, but mlua 0.10.5's safe public host API copies via `to_vec`; do not claim zero-copy host reads |

Therefore phase A needs no new geometry dependency. General booleans, robust offsets, render-pass
effects, and zero-copy mutable bulk storage are not smuggled into that phase. Each gets a workload,
bounds and implementation decision before exposure.

### 6.3 First evolving proof: Lua orbital diagram

Use one demo across phases rather than disposable rectangles. Phase A draws cubic/quadratic orbit
paths, an arc, reusable planet subframes, nested affine groups, fill/stroke variation and intrinsic
layout. It must remain sharp and correctly placed inside the retained zoom viewport. Phase B adds
reused shaped labels/baselines. Phase C animates nested orbit transforms, samples a path for marker
placement/tangent, and returns a stable clicked body ID with screen/Frame/target-local positions.
Later phases add background/foreground layers, clips/compositing and a buffered particle ring.

The proof is successful only when its visual algorithm and composition are Lua-authored. Rust may
compile declarations and provide aggregate curve/text operations; a Rust `orbital_diagram()` or
closed visual enum fails the goal. Stress tiers compare readable declarations, shared instances,
and packed bulk data before choosing performance thresholds.

### 6.4 Phase A candidate declaration schema

This is the concrete review checkpoint before code. `gfx.path` and `gfx.frame` are aggregate host
calls returning immutable userdata backed by shared runtime values. `gfx.fill`, `gfx.stroke`,
`gfx.group`, and `gfx.instance` are pure Lua helpers that only construct tagged declaration tables;
`gfx.frame` recursively validates and compiles the whole item tree in one crossing. Compiling takes
a snapshot, so later mutation of a declaration table has no effect.

```lua
local orbit = gfx.path({
	{ "move", 40, 120 },
	{ "cubic", 40, 30, 280, 30, 280, 120 },
	{ "cubic", 280, 210, 40, 210, 40, 120 },
	{ "close" },
})

local disc = gfx.path(geom.circle_path(12, 12, 10))
local planet = gfx.frame({
	width = 24, height = 24, baseline = 18,
	gfx.fill({ path = disc, color = "#67a8ff" }),
	gfx.stroke({ path = disc, color = "#ffffff", width = 2 }),
})

local picture = gfx.frame({
	width = 320, height = 240,
	gfx.stroke({ path = orbit, color = "#73809b", width = 2 }),
	gfx.group({
		transform = geom.compose({
			geom.translate(160, 120), geom.rotate(angle), geom.translate(-160, -120),
		}),
		gfx.instance({ visual = planet, transform = geom.translate(268, 108) }),
	}),
})

return ui.frame({ id = "orbits", visual = picture })
```

Path commands are positional arrays with exact arity:

| command | fields after command name |
|---|---|
| `move` | x, y |
| `line` | x, y |
| `quad` | control-x, control-y, x, y |
| `cubic` | control1-x, control1-y, control2-x, control2-y, x, y |
| `arc` | radius-x, radius-y, rotation-radians, large-arc-bool, sweep-bool, x, y |
| `close` | none |

A drawable subpath begins with `move`; an empty path is valid. Arc zero radii degrade to a line,
equal endpoints add nothing, radii are made positive, and conversion follows SVG endpoint-arc
semantics at a documented bounded tolerance. Unknown commands, named command fields, wrong arity,
non-finite values and invalid sequencing are errors. Shape helpers such as `circle_path` are Lua
algorithms producing the same command grammar, not privileged renderer primitives.

An affine is six f64 values with one documented convention:
`{xx, yx, xy, yy, dx, dy}` maps `(x,y)` to
`(xx*x + xy*y + dx, yx*x + yy*y + dy)`. `geom` is initially an app-local pure Lua module, so
scalar composition does not cross FFI. Runtime converts the validated matrix at the boundary;
Frame declarations never contain screen/camera transforms.

Phase A runtime values are conceptually:

```text
Path: immutable BezPath + local bounds + command count
Frame: width + height + optional baseline + ordered items + expanded statistics
Item: Fill(path, solid, rule) | Stroke(path, style) |
      Group(transform, ordered items) | Instance(transform, shared Frame)
```

Frame/Path fields and item construction stay private behind validating constructors. A Group owns
local ordered items; Instance reuses a measured Frame as a visual subtree without applying layout.
A repeated instance is counted repeatedly for expanded rendering budgets. Immutable publication
prevents reference cycles because an existing value cannot later acquire an ancestor reference.

Initial named limits are deliberately conservative and revisable from measurements: group depth
32, 4,096 expanded items, 65,536 expanded path commands, and finite dimensions/coordinates/matrix
coefficients within named runtime constants. Empty zero-sized Frames are valid; width/height are
non-negative; a present baseline is within `[0, height]`. Limits apply both to each compiled value
and to expansion across every Frame El in one app view, so many individually valid values cannot
bypass the frame budget. The first Path slice provisionally caps coordinate magnitude at 10,000,000
logical units; Vello stress tests may tighten or raise it. Stroke ceilings land with Stroke rather
than being asserted before that type exists.

The Phase A API intentionally omits mutation, retained numeric arenas, internal IDs/hits, text,
images, brushes beyond solid color, fit modes, local clips, opacity and effects. Their places in the
full contract are preserved by later phases; absence from the first enum is not a claim that the
capability was cut.

## 7. Interaction, layers and semantics

The outer El owns normal behavior and callback registration. Frame hit metadata is plain data;
app_host maps resolved IDs/events into the current callback table. Shared visuals carry no closures.

Proposed routing: find the eligible topmost El, map through Geometry, traverse Frame groups in
reverse paint order with their inverse transforms and clips, return the topmost eligible hit.
Specify fill/stroke/custom-region hit semantics and decorative pass-through before adding hits.
A paint item is not automatically a target. Backgrounds/foregrounds are decorative by default.

Events must name screen position, Frame-local position and target-local position separately.
Movement deltas must state their owner space. Capture identity is owner El + instance path + hit
ID, not vector index or old closure. Define cancel/removal/reload behavior before multi-phase
Frame gestures ship. Deferred outer capture and occlusion work are dependencies where applicable.

Background/content/foreground require explicit ordering relative to normal El decoration, text,
children and overlays; decide that ordering before implementing the slots. Do not use coincidental
child order to simulate a foreground slot.

Visuals do not imply semantics. Decorative frames are ignored by accessibility; meaningful charts
need descriptions, and interactive marks need names, roles and a keyboard/focus strategy. Complete
platform accessibility depends on broader runtime support; Frame must not erase semantic metadata.

## 8. Animation, simulation and reload

Frame is a snapshot, not a clock or simulation world. Lua can derive visuals from time and state;
retained runtime machinery supplies efficient drivers where useful.

- Add an explicit monotonic animation clock; preserve existing wall-clock `now()` semantics.
- Define a tick/update seam, bounded dt/catch-up and redraw subscriptions. Idle apps must sleep;
  hidden tabs must not accidentally run unbounded animation work.
- Springs preserve velocity on retargeting; test bounded integration and settling independently
  of the button-specific prototype. Timelines and named property bindings need explicit lifecycle.
- Small simulations use Luau math. Larger ones use validated buffers/bulk operations or instances;
  do not expose scalar Rust math merely to cross the boundary for every value.
- Simulation state is per-viewer; persist user intent/parameters, not every integrator step.
- Resource and simulation state may outlive a staged VM swap only through explicit host-owned,
  namespaced handles with compatible schemas. Never retain pointers into the old VM.
- Failed reload leaves the live VM/resources untouched. Successful reload reconciles ownership,
  releases stale registrations and defines reset behavior for incompatible state.
- Morphing requires stable matching, interpolation rules and enter/exit policy. Arbitrary path
  topology changes and mathematical equivalence are not solved by a Group node.

A physics engine binding is optional, not required for Lua-programmable simulation. Bulk APIs
must justify themselves with a workload and measured crossing/allocation costs.

## 9. Safety, rendering and observability

Lua's instruction budget does not bound native shaping, path parsing, image decoding or GPU work.
Before each feature ships, choose and test concrete per-app limits for:
- tree depth, expanded instances, items and path commands;
- finite coordinates, stroke widths, gradient stops and numeric ranges;
- text length, glyph count and shaping/cache work;
- decoded image dimensions/bytes and total resource bytes;
- offscreen effect dimensions, nesting and pixel budgets;
- bulk buffer types, lengths, strides and operations;
- per-frame work, retained resources and inspection output size.

Reject malformed/cyclic declarations with useful bounded errors. Reject cross-app/stale handles;
closing an app or failed staged reload must release owned resources. Sandboxed resource loading
must not become arbitrary file/network access. New storage access needs the existing authority
boundary, not paths accepted by the renderer.

Map effects to verified Vello capabilities. Rounded-rectangle shadow helpers are not arbitrary
blur; masks and offscreen effects need explicit bounds and cost tests. Export must report unsupported
features or an explicit raster fallback, never silently omit them. Portable scene data enables
export adapters but does not implement them automatically.

Inspection should expose bounded item IDs, kinds, local bounds, transforms, clips and resource
summaries. Resolved hit data belongs to Runner's frame snapshot, not a second bridge layout pass.
Do not dump image/font payloads or arbitrary Lua tables. Screenshot tests complement structural,
geometry and hit assertions rather than replacing them.

## 10. Delivery phases and acceptance gates

Each phase is a work package, **not one write**. Before coding a package, split it into roughly
100-production-line slices; explain the immediate slice, run matching expert checklists, obtain
approval, write/test, and request a fresh diff review. User acceptance decides completion.

### A — settle the seam and build the smallest vertical path

1. Resolve minimum schema, resource lifetime, dimensions and limits from §11.
2. Add local Frame/path/group values and structural tests in runtime. **Path, the first
   Frame/Fill/Group/Instance model, and solid/linear Brush slices landed 2026-09-11;
   radial/sweep Brush is next.**
3. Add recursive Vello rendering using existing outer transforms/clips. **Fill rendering for solid
   and linear brushes through Group/Instance transforms landed 2026-09-11.**
4. Integrate one measured El leaf and intrinsic/explicit-size tests. **Landed 2026-09-11:** Frame
   measures as content, padding offsets its local origin, and explicit El dimensions override
   allocation without scaling the visual.
5. Expose bounded batch declarations through app_host and a tiny Lua visual. **In progress:**
   strict batched `gfx.path` for M/L/Q/C/close and the first complete Fill/Group/Instance +
   `ui.frame` vertical slice landed 2026-09-11. `demo_apps/frame_orbits/` is its live bridge-
   screenshot proof.

Gate: the same scene renders through Rust and Lua, nested transforms compose correctly, malformed
input fails safely, and no screen coordinates enter Frame. No Rust-only milestone declaration.

### B — useful measured graphics

Add stroke semantics and brushes in tested slices. **Stroke landed 2026-09-11:** validated width,
caps, joins, miter limit and bounded dashes; budgeted recursive Vello rendering; strict Lua
construction; and live solid/dashed use in `frame_orbits`. Add managed shaped text/glyph resources and
baseline composition; shared subframes and instance identity. Prove labels and visuals align and
stay sharp under pan/zoom. Test Unicode shaping/fallback, empty frames, overflow and resource reuse.

Gate: a Lua library produces a labeled vector diagram without custom Rust drawing code.

### C — first interactive animated milestone

Add monotonic frame time and bounded scheduling, then simple tagged hit regions and plain events.
A Lua demo animates a group and responds to a mark click inside a zoom viewport. It also appears
in a normal row and fixed-size overlay. Verify clipped-out marks cannot fire and idle animation
stops scheduling. Avoid implying multi-phase capture is solved by this click milestone.

Gate: paths + groups + text + animation + local hits work end-to-end from Lua. Live-check alongside
kanban so press feedback, portal anchors, scrolling and camera layout remain correct.

### D — complete embedding and local compositing

Specify layer ordering and fit, then implement background/foreground slots, image resources,
gradients/patterns, group opacity, clipping, blending and masks in separate slices.

Gate: overlapping children fade as a group; internal and viewport clips agree for paint/hit;
images and effects obey resource limits; decoration does not steal interaction.

### E — scalable animation and simulation

Generalize tested springs/drivers, add resource reuse/versioning and measured cache policy, then
bulk buffers/instances for a concrete Lua simulation. Define reload preservation and morph matching.

Gate: a Lua rope/particle/chart demo animates without per-item FFI reads; allocations and frame
costs are measured at named workloads, settling sleeps, and failed reload preserves the live app.
Numeric performance targets are set from a baseline, not inherited from historical estimates.

### F — robust interaction and tooling

Revisit deferred outer gesture occlusion and capture re-resolution; add internal capture/cancel,
keyboard/semantic metadata, reusable root drag previews and bounded Runner inspection.

Gate: removal, reordering, camera changes and reload during gestures cannot dispatch to the wrong
item; preview reuse does not duplicate handlers; resolved inspection matches rendered hit order.
Bridge gesture synthesis remains its own slice through normal runtime input, not Frame-only input.

### G — math and Lua visual libraries

Validate a bundled MATH font and metrics integration, then implement measured math constructs with
headless baseline/placement tests. Decide parser/layout ownership from the required font APIs;
Lua must be able to compose the resulting Frames and author libraries around them. Build chart
layout/ticks/palettes and diagram helpers in Lua rather than a Rust chart-kind enum.

Gate: nested fractions/scripts/delimiters align with text; a Lua-authored chart type requires no
Rust change. This does not promise arbitrary formula morphing or rich-text editing.

### H — expensive effects and export

Design bounded render-to-texture effects and SVG/PDF adapters separately. Verify backend resource
and font handling, color/compositing semantics and fallback policy.

Gate: supported effects/export are pinned by fixtures; unsupported behavior is explicit and costly
work cannot bypass per-app budgets. No extension of reference-only PDF crates.

## 11. Decisions to close before the relevant slice

| question | proposed direction | gate |
|---|---|---|
| public Lua syntax | tagged/batched declarations + opaque reusable resources; names not final | A |
| ownership representation | immutable published snapshot; app-scoped managed sharing | A |
| intrinsic size/baseline | explicit size, optional baseline; distinguish painted bounds | A–B |
| layout overrides/fit | allocation does not implicitly scale visuals | A, D |
| resource cleanup/reload | staged ownership, bounded retention, no old-VM references | A, E |
| numeric/depth/work limits | select concrete defaults with adversarial tests | each feature |
| shaping API | return measured reusable text; raw glyph path for specialized producers | B |
| event identity | owner + instance path + local hit ID; plain named fields | C |
| scheduling API | monotonic clock separate from now(); update/tick separate from view | C |
| decoration ordering | explicit background/content/foreground ordering and pass-through | D |
| responsive production | no Lua execution inside recursive Taffy measurement | before support |
| math ownership/parser | verify metrics and font access; do not copy old estimates as a plan | G |
| export/effect fallback | explicit error or opt-in rasterization, never silent omission | H |

## 12. Validation and completion bookkeeping

Tests live in `src/<module>/tests.rs`; app_host tests cover the Lua boundary and existing kanban
round-trip stays green. Run `cargo check --workspace`, `cargo test --workspace`, and
`git diff --check` for implementation milestones; record pre-existing warnings separately.
Use small `demo_apps/` consumers for live checks and preserve unrelated working-tree changes.

Each accepted slice updates `docs/status.md`; shipped Lua surface updates `docs/lua-apps.md`.
Change this plan's phase status only when its gate is demonstrated. Preserve revised decisions
with dated notes. The first milestone does not mean the complete capability ledger is done.
