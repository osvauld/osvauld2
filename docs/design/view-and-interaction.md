# View + interaction layer — press feel, z, camera, 2.5D (draft, 2026-08-31)

Makes containers **reactive** (press, lift, ripple, glow), gives the runtime a **camera**
(zoom/pan/touch), and adds a **z order**. Apps target **2.5D**. Real 3D is scoped here as a
deferred *producer*, not a rendering stack.

**Prerequisite:** the Loro↔Lua binding (`app_host/src/crdt.rs`, currently `todo!()` at :49).
None of this starts before an app's state round-trips through a LoroDoc.

Companions: `animation.md` (rev 3 — the timeline/binding layer this extends),
`visual-substrate.md` (Frame, math, buffers, physics — the substrate this feeds),
`w3.md` (the current week; this is not W3 work).

Status: **design notes from a pairing session. Nothing built.**

---

## 0. Verified against source — don't rebuild, don't re-diagnose

**Ordering is already correct.** Paint walks `placed` in emit order; hit-testing uses
`.iter().rev().find(...)` (`lib.rs:462`). So **paint order == reverse hit order**, both from
tree order. Every change below must preserve that invariant.

**Visuals and hits share one rect.** `emit()` accumulates `(ox, oy)` down the tree, folding in
`behaviour.offset` and the slide transition (`layout.rs:109-127`); `hit_rect` derives from that
same `p.rect` (`lib.rs:210`). This is *why* `animation.md` §8a got hit-testing free for
`.offset`.

**A global transform seam exists.** `render.transform()` = `Affine::scale(scale * SUPERSAMPLE)`
(`render.rs:193`), passed as `t` into every paint call (`lib.rs:193`). `viewport()` stays
logical (`render.rs:185`), so layout is already DPI-independent.

**A two-tier z already exists.** Overlays are collected during emit, re-solved, and appended
(`layout.rs:236-293`) — so they paint last and hit first. That's the mechanism, hardcoded to
two levels.

**Gaps:**

- Press is a **hard swap** — `paint.rs:69`. No transition, no slot. `animation.md` §13 Phase B
  lists "press feedback (stretch)", unchecked.
- Press identity compares **geometry**: `pressed.is_some_and(|pr| pr == p.rect)` (`paint.rs:69`,
  captured `lib.rs:464`). Survives a paint-time transform, but drops silently if layout shifts
  between press and release.
- **No per-element scale.** `animation.md` §8 defers scale *and* rotate together. §2 below
  argues that was one item too many.
- **No `Path`** — paint knows `RoundedRect` + glyphs only (`paint.rs:79`).
- **`Easing` is 3 hardcoded variants** (`anim.rs:33`).
- **`lerp_color` interpolates sRGB** (`el.rs:82`) → muddy midpoints.
- **`now()` is whole seconds** (`lib.rs:176`); the real frame clock is `lib.rs:189`.
- **`repaint` isn't in the Lua props table** (`props.rs:195-215`) though `El::repaint()` exists
  (`el.rs:585`) and is honored (`lib.rs:217`).
- **No touch.** `CursorMoved` + `MouseWheel` only (`lib.rs:927`); no `WindowEvent::Touch`.
- Click carries no position; `on_right_click` already does (`el.rs:243`).

---

## 1. Press feel — the cheap bundle

Three changes, ~40 lines, and buttons feel real.

- **`Driver::Press`** beside `Driver::Hover`. Hover already resolves through `resolve_t`
  (`el.rs:73`); press should take the same path instead of the hard swap.
- **`Spring { value, velocity, stiffness, damping, target }`** replacing linear `Transition`
  (`anim.rs:1`). Semi-implicit Euler, ~10 lines, same `tick(dt)` shape, same `Store`/`Slot` home.

  Why it matters: `Transition::tick` walks toward the target at constant speed, so flipping the
  target mid-flight gives a visible velocity discontinuity. Tap twice fast and a tween stutters.
  A spring carries velocity through. There's also no duration to pick — tune stiffness once and
  the whole app coheres.

  **The spring is needed three times over** — press/release, drag-and-throw, and pan inertia.
  Build it properly once.
- **Scale**, per §2.

---

## 2. Scale belongs in `emit`'s accumulator

Generalize the accumulator from a translation to **scale + translate**:

```
(ox, oy)  →  (ox, oy, sx, sy)
```

`rect` is then computed with the accumulated scale, and **hit-testing needs zero changes** —
same trick `.offset` already uses.

### 2.1 Narrowing `animation.md` §8

§8 deferred "scale / rotate (full Affine + hit inversion)" as one item. They are not the same
cost:

| | rects stay rects? | needs inverse hit-test? |
|---|---|---|
| translate | yes | no — built |
| **scale** | **yes** — axis-aligned | **no** |
| rotate | no — a rotated rect is a quad | yes |

Only rotation forces inverse hit-testing. **Scale was deferred by association.** Rotation stays
deferred, and that's fine — 2.5D doesn't need it.

### 2.2 Why the accumulator, not paint-only

A paint-only scale (hits ignore it) works for a leaf button but breaks a **container that scales
with interactive children** — a card that lifts on hover with buttons inside. The children's
visuals move; their hit rects don't.

Folding into the accumulator preserves the invariant the codebase already leans on — visuals and
hits are the same rect — and gets nested content right for free. At 0.97 the hit boundary moves
~1px on a 40px button; not worth a second code path.

### 2.3 `transform-origin`

Scale applies **about a point**, defaulting to the element's centre. Without it, scaling drags
the element toward its top-left. This is a hard requirement, not a refinement.

---

## 3. `z` — a paint-order key

Add `z: i32` to `Placed`, stable-sort by `(z, emit_index)` before painting, and push hits in
that same order. `.rev().find()` still picks the topmost. **One sort.**

Scope: this is a *sort key*, not a third dimension.

**`z` and scale are coupled.** A card that scales up on hover is broken without `z` — it grows,
and the next sibling paints over the grown edge. Neither is useful alone; build them together.

The existing overlay tier (`layout.rs:236-293`) is the two-level version of exactly this and
should collapse into it.

---

## 4. Event-driven decorations — the third animation kind

Ripple, glow, squiggle, toast, burst. These **do not fit the current state model**.

| kind | retained state | example | status |
|---|---|---|---|
| property transition | one scalar → target | hover tint | built (`animation.md` §14.1) |
| parametric | none — `f(now)` | math animation | needs clock + `repaint` |
| **event-driven** | `[(event, t0)]` | ripple, glow, burst | **no home** |

`Store` holds one scalar per `(Id, Slot)`. A ripple is a **list of events with timestamps** —
click fast and they overlap, each with its own origin and clock.

`animation.md` §9 already named the primitive: *"Loops (Tier 4) — `f(now)`, no stored progress."*
Ripple is that plus a start time. Retain `Vec<(origin, t0)>`; radius and alpha are pure functions
of `now - t0` — on **separate curves**, usually separate durations (radius lands first, alpha
lingers).

Needs: click position on `on_click`, sub-second clock, `repaint`, a circle `Path`, and a clip.

### 4.1 The clip question — the one real decision here

A glow should escape its **parent's** bounds but still be clipped by an enclosing **scroll
container**, or a glowing row bleeds outside the scroller. So "ignore clip" isn't a boolean.

Rule: **clip to the nearest scroll ancestor, not the immediate parent.** `layout.rs` already
threads `child_clip` down the tree, so the information is present — it's a question of which
ancestor's clip applies, not of new plumbing.

### 4.2 The test

**Ripple should be Lua-authored.** Every primitive it needs is already on the list for other
reasons. If ripple has to be a Rust feature, the primitives are wrong — same test as the Lua
defaults library in `visual-substrate.md` §7.

---

## 5. The camera — zoom, pan, touch

### 5.1 Separate three things

| | what | reflows? | have it? |
|---|---|---|---|
| **DPI / device scale** | OS HiDPI ratio | no | yes — `render.rs:127`, `:193` |
| **UI scale** | "everything at 125%" | **yes** — re-measure, re-layout | no |
| **viewport zoom + pan** | a camera over the scene | **no** — magnify | no |

This section is the third. It never touches layout: `viewport()` stays logical, taffy runs
unchanged, text measures the same. UI scale is a *different feature* — it multiplies sizes
before layout so `text_engine.measure` (`layout.rs:62`) returns different extents and everything
reflows. Don't build them as one thing.

### 5.2 Why the camera is easier than element scale

`animation.md` §8 deferred per-element transforms over hit inversion. A **global** camera doesn't
have that problem: one inverse, applied to the *pointer*, once — where physical→logical
conversion already happens (`lib.rs:653`). Hit rects stay in world coords and
`hit_rect.contains(...)` (`lib.rs:216`) never changes.

`transform()` becomes `Affine::scale(dpi) * camera`. That's the integration point.

Vello earns its keep here: it re-rasterizes **outlines** every frame at target resolution, so
400% zoom is genuinely crisp text, not upscaled pixels.

### 5.3 What stays in screen space — the actual design work

| | zooms? |
|---|---|
| content, text, shapes | yes |
| 1px borders, focus rings | no — 4px borders at 400% look broken |
| scrollbars, tooltips, overlays | no — chrome |
| caret + selection | yes, and must route through the camera |

So elements need a `screen_space` flag, and the camera has to reach `PlainEditor`'s cursor
geometry (`editor.rs`) or the caret drifts off the text at any zoom ≠ 1.

Settle wheel semantics early — wheel = scroll, ctrl+wheel = zoom, pinch = zoom. Today wheel is
unconditionally scroll (`lib.rs:927`).

### 5.4 Touch

Not present at all. Needs `WindowEvent::Touch`, a gesture recogniser (pinch = 2-finger distance
ratio, pan = centroid delta), and **momentum on release** — the spring again.

---

## 6. Interpolation — steal CSS's rules, not its model

CSS's contribution here isn't the transform (that's matrices). It's the **interpolation rules**.

- **Never lerp matrices.** If two transforms share a list structure, interpolate component-wise
  (`rotate 0°→90°` interpolates the *angle*). Only when structures differ do you decompose into
  translate/rotate/scale/skew and interpolate those. Naive matrix lerp collapses a rotation
  through zero scale. **This is what Frame morphing needs** (`visual-substrate.md` §4.3).
- **`cubic-bezier(x1,y1,x2,y2)`** replacing the 3-variant `Easing` enum (`anim.rs:33`) — ~20
  lines with Newton-Raphson to solve t from x. Same closed-enum→open move as `Slot::Anim`.
- **Interpolate colour in Oklab**, not sRGB. `color` 0.3.3 is already in the tree with
  `interpolate(…, ColorSpaceTag::Oklab, HueDirection)` (`dynamic.rs:369`, `tag.rs:40-42`).
- **Derive state colours in Oklch instead of declaring a second hex:**

  ```lua
  fill  = "#3b82f6",
  hover = { l = +0.05, c = +0.02 },
  press = { l = -0.08 },              -- hue and chroma untouched
  ```

  L, C, H are independent perceptual axes — darkening via sRGB multiplication drifts saturated
  blues toward purple; dropping Oklch lightness holds the hue exactly. This matters most because
  **the agent authors the UI** and should never have to pick a harmonising second colour.

**Skip:** `calc()` and percentage resolution (taffy owns layout math), 3D/perspective, and CSS's
`transition` model itself — we're choosing springs, which CSS doesn't have.

### 6.1 Open: one driver or two?

Springs have velocity but no curve; beziers have a curve but no velocity. Current read: different
jobs — **springs for anything a finger touches** (press, drag, throw, pan inertia), **beziers for
scripted timelines** (a morph over 800ms, a page transition). That means `Binding` carries
either, which changes `animation.md` §14.1. **Not decided.**

---

## 7. Motion — three ways to produce a position

| source | shape | character |
|---|---|---|
| **path** | `pos = curve(t)` | authored, exact, repeatable |
| **parametric** | `pos = f(now)` | arbitrary math — orbit, sine, Lissajous |
| **physics** | emerges from forces | reactive, never exact |

**They compose, they don't compete.** Path or parametric produces a **target**; the spring (§1)
chases it. Pick one alone and you get either rigid motion or motion you can't direct — layered,
you get a card that follows a curve *and* still feels alive when grabbed mid-flight.

### 7.1 kurbo already has the machinery

Present via vello — kurbo 0.13.1, `src/param_curve.rs`:

- `ParamCurve::eval(t)` — the point
- `ParamCurveDeriv::deriv()` — the **tangent**, so an element can orient along its path (an arrow
  following a curve actually points the right way)
- `ParamCurveArclen::inv_arclen(distance, accuracy)` (`:98`) — distance → `t`
- `ParamCurveNearest` — nearest point on the curve; what dragging *along* a path needs

**The gotcha: bezier `t` is not distance.** Animate `t` linearly along a curved path and the
element races through the straight parts and crawls through the bends. It reads as broken and
most people can't say why. Always drive motion through `inv_arclen`, never raw `t`.

### 7.2 The Lua ladder, again

| how the app declares it | crossings | good for |
|---|---|---|
| control points, curve type, spring constants | O(1) — Rust evaluates | any count |
| a Lua **closure** as the path function | one call per element per frame | ~tens of elements |

A closure is genuinely fine for motion design — 10 elements × ~300ns is nothing. It is fatal at
10k particles. Authored motion → closures; simulated motion → buffers (`visual-substrate.md` §3).

### 7.3 Motion constants belong at workspace scope

Same argument as the colour palette (`visual-substrate.md` §7). If every agent-built app picks
its own stiffness and damping, one dashboard has four different bounces and reads as sloppy
though nothing is wrong. One workspace-level vocabulary — `snappy`, `gentle`, `heavy` — and apps
reference it by name. Motion becomes part of the design system rather than a per-app accident.

**Gating:** *drawing* a path needs the `Path` arm in `paint.rs`. *Evaluating* along one needs
nothing new.

---

## 8. Coroutines — `tick(dt)` and the app contract

mlua 0.10.5 has the full API: `create_thread` (`state.rs:1261`), `Thread::resume`
(`thread.rs:139`), `Thread::status` (`:191`).

### 8.1 What it buys

Sequenced motion stops being a state machine:

```lua
ui.run("intro", function(seq)
  seq.move(card, path, 0.6)
  seq.wait(0.1)
  seq.fade(label, 1, 0.3)
end)
```

versus three timelines with staggered delays chained through `on_done`. For an **agent** authoring
animation that difference is large — the coroutine version is the one it writes correctly first
try.

### 8.2 A scheduler above §7, not a fourth source

The coroutine should **set targets over time** and let paths and springs interpolate. If it steps
position directly each frame, you're doing per-frame math in Lua — slower, and jerky at frame
granularity.

### 8.3 Where "elsewhere" is — a third entry point

**Not another thread.** The Luau VM is single-threaded per state; a coroutine cannot be resumed
off the frame loop.

| | when | what |
|---|---|---|
| `view()` | every frame | describe the tree — pure |
| `update(msg)` | on event | handle input |
| **`tick(dt)`** | **before `view()`** | **resume due coroutines** |

A small addition to `LuaApp`, and it keeps `view()` pure: motion advances outside the description
pass.

### 8.4 State fits the existing machinery

A coroutine is retained *execution* state — a suspended stack. It lives in `_state` like anything
else, so it survives frames, and `_sweep()` drops it when its element leaves the tree. That's the
correct lifecycle: **the animation dies with the thing it was animating.** Nothing new to build.

### 8.5 Three costs

- **Hot reload.** Suspended coroutines hold closures from the old `main.lua`. This is now the
  **third** retained-state thing with the same problem — physics worlds, Frame caches, coroutines.
  It's one pattern, not three bugs. Solve it generically.
- **A coroutine that never yields hangs the frame.** `fires` (`lib.rs:185`) catches it, but the
  error will be confusing unless it's special-cased.
- **Scaling.** One coroutine per animated element is fine for hundreds, wrong for 10k. Same ladder
  as §7.2.

### 8.6 The wider win

This is also the answer for anything sequenced that isn't animation — multi-step onboarding, a
guided walkthrough, a chained data load. Painful as state machines, natural as scripts.

---

## 9. Layout — taffy is enough, with one gap

Taffy 0.12.2 gives block, flexbox, grid, **and float** (`src/compute/`), plus
`AlignItems::Baseline`. That covers dashboards, forms, and app chrome — everything the el tree
does and everything M2 needs.

Three things it won't do, all correctly *not* its job — a graph/canvas (positions come from a
simulation: one taffy leaf, fixed size, place nodes inside), math (TeX box rules), and charts.
Those are Frame producers.

**The real gap is inline layout.** A paragraph with inline math — or an inline chip or image —
needs that box to participate in **line breaking** and sit on the text baseline. Taffy has no
inline formatting context (`AlignItems::Baseline` aligns flex items, not inline content in a
wrapped line), and parley breaks lines of text but knows nothing about arbitrary boxes.

Neither serves it, and M3's doc editor needs it. The fix is a small inline pass: parley gives
line breaks and baselines, and Frames interleave as atoms — each needing exactly a **width** and
a **baseline**, which `Frame` already carries.

---

## 10. 2.5D is the target

Apps target 2.5D. It is not a consolation prize — with `z` + scale + camera you get:

| | how |
|---|---|
| **correct occlusion** | painter's algorithm — for non-intersecting objects this *is* real 3D occlusion |
| **isometric scenes** | a fixed 2D affine (shear + scale) |
| **parallax depth** | layers at different rates under one camera |
| **card flip** | animate horizontal scale 1 → 0 → −1 (what CSS did pre-transforms) |
| **lift / elevation** | scale + shadow + z |

---

## 11. 3D — a deferred *producer*, not a rendering stack

**The hard boundary:** vello takes a kurbo `Affine` (2×3). Perspective is a **projective**
transform — 3×3 homogeneous with a non-affine bottom row. An affine mathematically cannot express
it, so no vello transform can tilt a rect in perspective.

**The way through: do the projection in software and emit projected polygons as filled paths.**

| step | rides existing machinery? |
|---|---|
| model → world, world → view (look-at), view → clip (perspective) | pure math |
| **perspective divide (÷w)** | **the step `Affine` can't do — so we do it** |
| NDC → screen | pure math |
| backface cull (projected cross-product sign) | pure math |
| depth sort (centroid z) | **§3's `z`** |
| emit faces → `BezPath` + fill | **the `Path` primitive** |

~400 lines total, plus `glam` 0.33.6. Smaller than the math typesetting engine, and **no GPU work
at all**.

**The payoff:** 3D becomes one more Frame producer — `scene3d → Frame`, alongside `str → Frame`
(math) and `data → Frame` (charts). Same output type, so a 3D chart composites with 2D UI,
exports to PDF, caches, and hit-tests through machinery built anyway. Picking is *easier* than
real 3D: keep the projected polygons, walk front-to-back, point-in-polygon. No ray casting.
Vertex transforms are the tier-2 bulk-op case (`visual-substrate.md` §3.2).

**What breaks:**

- **Painter's algorithm has no valid sort for cyclic overlaps**, and intersecting geometry is
  always wrong. Fixes are a BSP tree (correct, precomputed, expensive) or subdivision. *The*
  limitation.
- No per-pixel depth → no interpenetration, shadow maps, or depth of field.
- Texture mapping is bad — perspective-correct texturing needs a projective warp per triangle;
  vello gives affine image draws. Subdivide and approximate.
- CPU-bound: ~10–20k triangles/frame.

**Good for:** 3D charts, molecule viewers, terrain patches, wireframes, exploded diagrams,
network graphs in 3D space, architectural scenes with true perspective.
**Not for:** textured meshes, characters, >50k triangles, real lighting — that needs a separate
wgpu pass rendering to a texture, composited as an image. We own the device (`render.rs`), so the
door stays open. Different project.

**Strictly last.** It needs `Path`, `z`, the camera, and `Frame` first. It falls out; it isn't
planned around.

---

## 12. Build order

**Bundle A — reactive containers** (do first; forces the generalisation everything else needs)

- [ ] fix press identity: key by `Id`, not rect equality (`paint.rs:69`)
- [ ] `Driver::Press` beside `Driver::Hover` — closes `animation.md` §13 Phase B
- [ ] `Transition` → `Spring` (needed 3× over: press, throw, pan inertia)
- [ ] scale + `transform-origin` in `emit`'s accumulator (§2)
- [ ] `z: i32` on `Placed` + stable sort; collapse the overlay tier into it (§3)
- [ ] Oklab interpolation + Oklch-derived state colours (§6)
- [ ] **Done when:** a card lifts on hover — scales up, shadows, paints above its siblings, and
      its child buttons stay clickable.

**Bundle B — the three unblockers** (small; gate ripple, rope, charts, math alike)

- [ ] `Path(BezPath)` arm in `paint.rs` — vello already does it, just unplumbed
- [ ] sub-second monotonic `now()` from the frame clock (`lib.rs:189`), never `SystemTime`
- [ ] `repaint` in the Lua props table

**Bundle C — events + camera**

- [ ] click position on `on_click`
- [ ] `[(event, t0)]` retained state + the scroll-ancestor clip rule (§4)
- [ ] ripple, authored in Lua — **the test of whether the primitives are right**
- [ ] camera into `render.transform()`; one inverse on the pointer; `screen_space` flag
- [ ] wheel semantics; `WindowEvent::Touch` + pinch/pan recogniser + momentum

**Bundle D — motion + sequencing** (§7, §8)

- [ ] path following via `inv_arclen` — **never raw `t`** — plus `deriv()` for orientation
- [ ] paths/parametric set a *target*; the spring interpolates. Not a separate motion path.
- [ ] `tick(dt)` as a third app entry point, resumed before `view()`
- [ ] `ui.run(id, fn)` — coroutines in `_state`, swept with their element
- [ ] generic **hot-reload survival for retained state** — physics worlds, Frame caches, and
      coroutines are one problem (§8.5), not three
- [ ] workspace-scope motion constants (`snappy` / `gentle` / `heavy`)

**Later:** `cubic-bezier` easing, UI scale (§5.1 row 2), inline layout pass (§9), software 3D (§11).

---

## 13. Open questions

- **One driver or two** (§6.1) — springs vs beziers in `Binding`. Blocking the spring work.
- **Does `z` subsume the overlay tier, or sit beside it?** They're the same idea at different
  granularity; keeping both means two orderings to reason about.
- **Where does `screen_space` live** — a flag per element, or a separate layer the camera skips?
- **UI scale (§5.1 row 2)** — wanted at all? It's the accessibility story, and it's a re-layout,
  not a transform.
- **Inline layout (§9)** — is it this doc's problem or M3's? It's needed by inline math, which is
  M3, but the primitive is a layout pass, which is here.

---

## 14. Decisions log

- **Scale goes in `emit`'s accumulator**, not paint-only — preserves visuals==hits, and gets
  scaled containers with interactive children right for free.
- **Scale is not rotate.** Only rotation forces inverse hit-testing; `animation.md` §8 deferred them together.
- **`z` is a sort key**, not a dimension.
- **Springs over duration tweens** — interruption with velocity is the point, and it's needed
  three times over.
- **Event-driven animation is a third kind**, with its own retained shape `[(event, t0)]`.
- **The camera is 2D** — pan/zoom/rotate. It is not "the start of 3D".
- **Apps target 2.5D.** Painter's-algorithm occlusion is real occlusion for non-intersecting
  objects.
- **3D, if ever, is a software projection producer emitting Frames** — not a parallel rendering
  stack, and strictly last.
- **Oklch for derived colours, Oklab for interpolation** — the agent should never have to pick a
  harmonising second hex.
- **Taffy is enough**; the gap is inline layout, which is neither taffy's nor parley's job.
- **Path, parametric and physics compose** — the first two set a target, the spring chases it.
  They are not competing motion systems.
- **Always drive path motion by arc length**, never raw bezier `t`.
- **Coroutines are a scheduler above motion, not a fourth motion source** — they set targets;
  springs and paths interpolate.
- **`tick(dt)` is a third app entry point**, so `view()` stays pure.
- **Retained state that outlives a reload is one problem** — physics worlds, Frame caches and
  coroutines share it; solve it generically.
- **Motion constants live at workspace scope**, like the palette — otherwise every agent-built
  app bounces differently.
