# Frame hits — touching the shapes inside a visual (design, 2026-09-18)

Status: **§7 fully landed 2026-09-18.** Items carry an optional `Id`, `gfx` accepts it,
`Frame::hit` answers with the topmost named shape, the point in its own space *and the transform
that put it there*, and `on_click`, `on_hover` and `on_drag` all deliver `shape, sx, sy` to Lua —
a drag reporting the shape it grabbed, from press to release. The bridge's `Action::Click` and
`Action::Hover` fire with no shape, having no layout to hit-test against.

**Revision 2026-09-18, on building slice 4:** `FrameHit` carries `into: Affine`, the frame-to-shape
transform, not just the point. §3 said a drag's `sx, sy` "track the pointer in that shape's space",
which is only possible if the shape's transform outlives the hit — the pointer leaves the shape
almost immediately in a real drag. The walk accumulates it anyway on the way down, so keeping it
costs nothing.

This is item (1) of the interaction direction — "anything an agent can describe mathematically is
touchable" — and the oldest thing still unbuilt in it.

Companions:
- [Frame implementation plan](frame-implementation-plan.md): the resource model this extends; §1
  already names "a hit region" as part of the first milestone.
- [View + interaction](view-and-interaction.md): element-level pointer contract and z order.
- [Lua guide](../lua-apps.md): the shipped authoring contract these props join.

## 0. Verified against source — don't re-diagnose

- **A frame is one leaf.** `paint.rs:103-104` places the whole visual with a single
  `frame.draw(scene, t * p.transform * origin, p.alpha)`. Layout sees one box.
- **Items carry no identity.** `ItemKind` in `frame.rs` is `Fill`/`Stroke`/`Group`/`Instance`,
  and `draw_items` (`frame.rs:346`) composes `transform * *local` as it descends. There is a
  transform tree already; there is nothing to name a node in it.
- **Element events are already local.** Dispatch undoes zoom, scroll and offset through
  `geometry.node_point(ScreenPoint)`, which is why `on_click`/`on_hover`/`on_drag` all report the
  element's own units. Frame hits are the same trick one level deeper.
- **Painter's order exists inside a frame.** `draw_items` iterates forward, so later items cover
  earlier ones. Elements hit-test with `.iter().rev().find(...)` (e.g. `lib.rs:708`); inside a
  frame the same reversal is available and means the same thing.
- **Frames are compiled once and immutable**, with recursive budgets (`item_stats`,
  `MAX_FRAME_DEPTH`, `FrameStats.expanded_items`). Anything we can precompute at build time is
  free at hit time, forever.
- **Apps invert their own drawing today.** `demo_apps/dashboard/chart.lua:19` turns a pointer x
  into a sample index; node_graph picks edges with `graph.edge_at`. Both work. Both are a second
  copy of the geometry.

## 1. Why now

Math picking is genuinely fine for continuous, invertible mappings — a line chart's "which x am I
on" should stay arithmetic, and agents are good at arithmetic. It fails for everything else: pie
slices, treemap cells, force-layout nodes, overlapping or rotated shapes. And even when it works
it is **two sources of truth** — the draw math and the pick math drift apart with nothing to
catch it.

A vector runtime whose units are shapes should let you touch shapes.

## 2. Rejected alternatives

| option | why not |
|---|---|
| Math picking only (status quo) | Doesn't generalize past invertible mappings; duplicates geometry. Stays available and stays correct where it fits. |
| Separate invisible hit declarations | Reintroduces the drift it was meant to fix — except as a *special case* of §3, where an id'd item with no brush is a hit region. |
| GPU picking buffer (render ids, read the pixel) | Exact, but a readback per pointer move: async, headless-hostile, untestable. Not our style. |
| Hit-testing descendants of the handler's element | Requires a subtree search per event to find frames. §3 puts the handler on the frame instead. |

## 3. The Lua contract

**Ids go on items.**

```lua
gfx.fill({ path = wedge, brush = c, id = "slice:" .. i })
gfx.stroke({ path = spoke, brush = g, width = 2, id = "spoke:" .. i })
gfx.group({ transform = t, id = "arm", … })     -- the whole group is one target
gfx.fill({ path = pad, id = "grab" })           -- no brush: invisible, still hittable
```

An id is optional. Only id'd items are hit candidates, so an app pays for exactly the granularity
it asks for.

**No new handler names.** The frame element's existing pointer handlers gain trailing arguments:

```lua
ui.frame({
    visual = v,
    id = "plot",
    on_hover = function(phase, x, y, shape, sx, sy) … end,
    on_click = function(x, y, shape, sx, sy) … end,
    on_drag  = function(phase, x, y, dx, dy, scale, ox, oy, shape, sx, sy) … end,
})
```

Lua ignores extra arguments, so every existing handler keeps working unchanged.

**Revision 2026-09-19:** the trailing arguments were the mistake this note didn't see coming.
"Lua ignores extra arguments" cuts both ways — it also ignores a handler that reads them in the
wrong order, silently, and `on_drag` had grown to eleven positions. Handlers carrying more than one
value now take a single event table, `function(e)` with `e.phase`, `e.x`, `e.shape`, `e.sx` and so
on; `on_enter`/`on_esc`/`on_faded_out` (nothing) and `on_input` (one string) keep their plain form.
The keys below are otherwise exactly as described. See `docs/lua-apps.md` for the current shape.

- `x, y` — **frame-local**, exactly as today, defined whether or not a shape was hit. `index_at(x)`
  must not change meaning.
- `shape` — the id of the topmost id'd item containing the point, or `nil`.
- `sx, sy` — **shape-local**: the same point with that item's accumulated `group`/`instance`
  transforms undone. Equal to `x, y` for an item drawn straight into the frame; different exactly
  when it is rotated, scaled or instanced. `nil` when `shape` is `nil`.

**The handler goes on the `ui.frame` itself.** Frames claim their own size, so the wrapper `col`
the demo charts use is habit, not necessity. One element, one visual, no descendant search.

**Hover does not bookkeep per shape.** Moving from `slice:1` to `slice:2` without leaving the
frame is a `"move"` with a different `shape`; the app compares. Enter/leave stay per element,
where `Hovered::step` already computes them.

**A drag grabs the shape it pressed.** `shape` is captured at `"start"` and reported unchanged for
the whole gesture, and `sx, sy` track the pointer in *that* shape's space. Grabbing a body at an
offset is the physics case, and re-picking mid-drag would mean something else entirely.

## 4. Semantics, decided

| question | decision | why |
|---|---|---|
| hit order inside a frame | topmost wins — reverse item order | Items have a real painter's z. Between elements we chose containment with no occlusion, because they don't. Different rules, both correct, documented as different. |
| strokes | within half the width of the line (see the revision below) | `contains` on a hairline never hits. |
| groups | an id'd group is one target; descend into it only if it has no id | Lets an author choose the granularity without a second mechanism. |
| instances | an id'd instance is one target; ids **inside** a reused visual are not reported | Fifty instances of one resource would report the same inner ids. Per-instance namespacing is a later question (§6). |
| fill rule | hit-test with the item's own `nonzero`/`evenodd` | The hit should agree with the pixels. |
| no shape under the pointer | `shape = nil`, `x, y` still reported | An empty region of a plot is still a position. |

**Revision 2026-09-18, on building slice 2:** strokes are **not** expanded to outlines. Expansion
allocates a `BezPath` per stroke every time a visual is built — and a visual is rebuilt on every
pointer move — to answer one point per event. The distance test is `path.bounds()` inflated by
half the width, then `PathSeg::nearest` against each segment: no allocation, no build-time cost,
and exact to half the width. What it gives up is caps, joins and dashes, which it does not model:
a dashed line is still one line to the pointer, and a round cap's bulge is a butt cap's square.
Name the shape you actually want if that distinction ever matters.

## 5. Runtime shape

1. `Item` becomes a struct carrying `kind: ItemKind` and `id: Option<Id>` — the name belongs to
   the item, not to each of four variants. *(Landed.)*
2. `Frame::hit(&self, p: Point) -> Option<FrameHit>` walks items in reverse, inverts each
   container's transform, tests the geometry, and returns `{ id, local: Point }`. *(Landed.)*
   Non-invertible transforms (a zero scale) are skipped rather than erroring — they draw nothing.
3. `FrameStats` counts id'd items so the existing budget covers hit cost too.
4. Dispatch already has the frame-local point; it asks the frame for the rest and appends the
   arguments. `Action::*` in `el.rs` grows the same fields so the MCP bridge can fire them.

## 6. Open questions

- **Instance namespacing.** `gfx.instance({ visual = pin, id = "pin:7" })` could prefix the inner
  ids (`"pin:7/dot"`) instead of masking them. Deferred until something needs it.
- **Shape-local for `on_drop`.** Drop reports normalized coordinates today; a shape there is
  plausible but no app wants it yet.
- **Contact normals.** Derivable later from the path plus the local point — an addition, not a
  redesign. Explicitly not in this note.
- **Text items.** When `gfx.text` lands it is just another shape with a box, hittable by the same
  walk. Doing hits first is what makes that free.

## 7. Slices

Each lands under the ~100-line rule in `CONVENTIONS.md`.

1. ~~**Identity.**~~ *(landed)* `id` on fill/stroke/group/instance, through `gfx.rs` validation into `ItemKind`,
   plus `FrameStats`. No hit-testing. Tests: ids survive compilation, budgets count them.
2. ~~**The walk.**~~ *(landed)* `Frame::hit` with transform inversion and stroke outlines. Pure geometry, unit
   tested against rotated groups and instances — no Lua, no dispatch.
3. **Delivery.** Trailing handler arguments, docs. Tests: a headless app receives
   `shape`/`sx`/`sy`; an existing 3-argument handler still works. *(Landed for `on_click` and
   `on_hover`.)*
4. ~~**The grab.**~~ *(landed)* `on_drag` carries the shape captured at `"start"` for the whole
   gesture, with `sx, sy` tracked in that shape's space.

## 8. Proof app

A **pie or bar chart**, not a line chart — a line chart is exactly the case math picking already
handles. A pie slice's inverse is `atan2` plus a radius test written twice, once to draw and once
to pick, which is the drift this removes. Built by an agent from the docs, like
`demo_apps/dashboard`, and its friction log is part of the deliverable.

## 9. What this still doesn't give

Text inside a frame (no `gfx.text` exists at all — every label is a sibling `ui.text` element),
wrapping or alignment inside a frame, per-shape enter/leave, and hit-testing across frames in
different elements. None of them are blocked by this; all of them are easier after it.

**Revision 2026-09-19:** per-shape enter/leave is now derivable in Lua, though still not delivered
as phases. Hover is sampled once per frame as well as on pointer events, and reports a `"move"`
whenever the hit under the pointer changes — including when the shape slides rather than the
pointer — so an app that remembers the last `e.shape` gets enter and leave by comparing. The reason
this works without flooding is that the runtime compares the whole hit before speaking: still
geometry under a still pointer recomputes the same point and says nothing. What is still missing is
the runtime naming those transitions itself, which matters for a frame with many shapes where every
app will otherwise write the same three lines.
