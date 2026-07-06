# Runtime rebuild — apps + Lua DX (plan of record, 2026-07-03)

The vello/parley `runtime` is the substrate now. Everything else in the workspace is
reference: we port its *primitives and lessons*, we do not extend its code. This doc is
the technical design for what gets built on the runtime — the core plumbing, the Lua
app layer (and its DX, which is the centerpiece), and the three big apps: table, doc
editor, canvas — plus charts. Library choices are from a fresh 2026-07 landscape scan
(§9).

## 0. Ground truth

Live code: `runtime` (~1.5k LOC: winit 0.30 / wgpu 29 / vello 0.9 / parley 0.10 /
taffy 0.11; `App` trait = Elm shape; `El<M>` tree → Taffy solve → `Placed` list →
vello paint; retained islands = parley `PlainEditor` fields + scroll offsets keyed by
`&'static str`; IME preedit/commit done; `ControlFlow::Wait` on-demand paint) and
`shell2` (login screen + theme).

What runtime does NOT have yet: dynamic ids, a general keyed state store, scroll
containers (`Scrolls::by` is `todo!()`), wheel input, clipboard, focus traversal,
overlays/popups, rich (multi-style) text leaves, images, timers/wakeup, dirty-gating,
virtualization, Lua, accessibility.

Reference crates that stay as *code we port from*: `app_engine` (the egui Lua engine —
its style vocab, sandbox, Loro binding, table host, hot reload, MCP loop),
`doc_editor`/`block_doc` (block model + editing engine), `code_editor` (Lua block
splitter), `table_core`/`table_query`/`table_import` (egui-free: port nearly as-is),
`pdf_paint` (egui-Galley-coupled: re-target), `text_edit`/`rich_text` (egui: lessons
only), `sthalam` (bridge/vault wiring patterns), `osvauld-rpc`/`osvauld-mcp`
(transport reusable as-is). `cryptography`/`identity`/`storage`/`vault` are live
backend crates, unchanged.

## 1. Invariants (settled, don't relitigate)

- **One node vocabulary, two front-ends.** Lua `ui.*` and Rust `El` build the *same*
  tree through the same layout/paint/state machinery. Built-ins author in Rust, apps
  in Lua; same pixels, same pipeline. `El<M>` is generic over the message type — Lua
  apps use callback ids (`El<u32>`), Rust screens use typed enums.
- **Immediate description + retained islands.** `view = f(state)` rebuilds the tree per
  interaction; heavy widgets (editor, table, canvas, chart) are retained islands that
  own their hot state (caret, scroll, camera) and never run Lua on scroll. Disciplines:
  on-demand repaint (have), virtualize collections, memoize by version. No VDOM differ,
  no reactive-signal framework — re-validated for LLM-authored code by the 2026 scan
  (reactive dependency graphs are a known LLM failure mode; flat rebuild is not).
- **CRDT is the document truth** (Loro). Ephemeral view state (scroll, filters, combobox
  query, drag sizes) never enters the CRDT. Derived data is recomputed, not stored;
  reads at scale go through a columnar mirror, never the CRDT (§4).
- **App boundary = actor-shaped protocol, threads later.** App = VM + Loro handles +
  layout, producing a scene fragment + hit regions (callback ids, never closures). The
  protocol stays serializable; v1 runs in-process on the main thread, lifted to
  app-per-thread actors when multiple simultaneous apps or heavy queries demand it.
  No OS IPC.
- **Moat discipline.** Own the triad — Lua authoring, sandbox/capabilities, CRDT
  binding. Rent rendering primitives (vello/parley/taffy) and language intelligence
  (tree-sitter). Keep the ui vocab minimal and composable; widget kits are userland Lua.

## 2. M0 — runtime core plumbing

Order chosen so every later milestone lands on ready seams. All of this is `runtime`
crate work.

**2.1 Ids + keyed state store.** Replace `&'static str` with an owned `Id` (interned
`Arc<str>` or `smol_str`) — Lua apps mint dynamic ids (`"cell:orders:r7:qty"`).
Generalize `Editors` + `Scrolls` into one store: `state.get_or::<T: Default>(id)`,
entries registered by the frame's live keys, swept (unmount) when a key disappears.
Field, Scroll, combobox query, drag state, canvas camera all become entries. This is
the retained-island backbone.

**2.2 Event vocabulary.** Today: `on_click(Msg)` + implicit text editing. Add:
- `on_drag(fn(DragEvent) -> M)` — phases Start/Move/End with element-local coords +
  modifiers; pointer capture from press to release. (Column resize, block drag, canvas
  tools, scrollbar thumbs all ride this.)
- `on_key` for the focused element/island; app-level fallback stays in `App`.
- `on_hover_enter/exit` derived (already have hover paint; expose as message).
- Wheel events are consumed by the scroll system (below), not exposed to apps v1.

**2.3 Scroll containers.** `.scroll(Axis)` on any `El`: Taffy overflow-hidden +
children flex-shrink-0 on scrolled axes (the app_engine lesson), offset in the state
store, clip rect carried in `Placed`, wheel routed to the innermost region that can
move on that axis, always-on draggable scrollbars (thumb drag swallows the click).
**Hit-testing must respect clip from day one** — a scrolled-out button must not catch
clicks (known app_engine sharp edge).

**2.4 Overlay layer.** `.overlay(anchor)` subtrees are laid out against their anchor's
computed rect (flip/clamp at window edges), painted last, hit-tested first, with a
click-away dismiss contract. Needed by: combobox, slash menu, context menus, tooltips,
modals, toolbar. One mechanism, all of them.

**2.5 Rich text leaf.** `rich(runs)` where a run = text + family/size/weight/style/
color/underline/strike/bg/mono. Built on parley `RangedBuilder` (per-range styles are
a layout feature parley fully supports; only *editing* is single-style — §9). Serves:
table cells, markdown-ish labels, code blocks (highlight spans → runs), doc reader.
Bold = real `FontWeight` (OS sans has weights; no egui fake-bold hacks).

**2.6 Focus + clipboard + shortcuts.** Tab/Shift-Tab traversal over input-bearing
elements in tree order; focus ring; `arboard` clipboard wired into the editor driver
(Cmd+C/X/V — parley gives selection, not clipboard); one modifier-aware shortcut
router so islands and app-level bindings don't fight.

**2.7 Wakeup + timers.** `EventLoopProxy<UserEvent>`: external threads (MCP bridge,
iroh sync, query workers) can deliver messages + request repaint; a timer wheel drives
caret blink and transitions. Replaces the all-or-nothing `animating()`.

**2.8 IME positioning.** We handle preedit/commit already; add
`set_ime_cursor_area` at the focused caret so candidate windows land next to the text.

**2.9 Images.** `image` crate decode → `peniko::Image` cache keyed by content hash;
`img(src)` element. SVG via `vello_svg` later.

**2.10 Dirty gate (scaffold).** `SceneKey`-style memo (frontiers, viewport, scroll,
focus) skipping view+layout when unchanged — port the app_engine scene-cache pattern.
Not urgent while idle-repaint is zero, but the seam goes in now; it becomes load-bearing
with tables.

**2.11 AccessKit seam (stub).** Give `El` optional role/label; mirror the `Placed` list
into an AccessKit tree behind a flag. Known friction: parley's `LayoutAccessibility`
assumes Masonry-style `TreeUpdate` ownership (parley#310) — budget a shim, don't block
on it. Cheap seam now, painful retrofit later.

**Exit test:** shell2 login rebuilt with zero `custom()` scene hacks except the
identicon/wordmark, plus a scrollable, overlay-using demo screen (dropdown + modal +
image + rich text) driven headlessly by a snapshot test.

## 3. M1 — the Lua app layer (`lua_app` crate) and its DX

This is the centerpiece. An app is a directory of `.luau` files; `main.luau` returns:

```lua
return {
  view = function()
    return ui.col { style = { padding = 16, gap = 8 },
      ui.text { "Orders", style = { font_size = 20 } },
      ui.table { rows = doc:list("orders"), columns = ..., on_edit = ... },
    }
  end,
  actions = {   -- the agent/MCP/button-facing API of the app (future permit boundary)
    add_order = { params = { sku = "string" }, run = function(p) ... end },
  },
  page = nil,   -- or { size = "A4" } for print apps
}
```

(Bare `return function() ... end` stays as sugar for tiny apps.)

**3.1 VM: Luau via mlua** (`luau` feature). Confirmed mature in the 2026 scan.
- Sandbox: `Lua::sandbox(true)` (readonly builtins, no io/package, per-script global
  tables) + our allocator memory cap + `set_interrupt` instruction/wall budget. This
  replaces most of app_engine's hand-rolled vm.rs.
- Numbers are doubles → **string ids everywhere** (row ids, block ids, handles).
- **Do not precompile to bytecode in dev/hot-reload** — Luau strips line numbers from
  errors; agent-facing tracebacks need them.
- Watch `mluau` (fork with clean yield-through-Rust) if async data queries need it.

**3.2 The `ui.*` binding.** Maps 1:1 onto `El` builders — one conversion/validation
checkpoint, no parallel node type. Style tables keep the proven CSS vocabulary from
app_engine (`padding = "10 20"`, `border = "1 #333"`, full CSS colors via
`csscolorparser`, CSS-standard key names — the LLM-fluency lever). Handlers register
into a per-view callback table; the tree carries `u32` ids. Islands are exposed as
leaves: `ui.table`, `ui.editor` (one rich field), `ui.doc` (block editor), `ui.chart`,
`ui.canvas`, `ui.code`, `ui.image`. Widget kits (buttons, badges, forms) remain
userland Lua modules (`require`), not Rust.

**3.3 Data bindings.** Port the two-world shape:
- `doc:*` — the app's own CRDT: `doc:list/map/text` handles, metamethod sugar, handles
  re-resolved by name each access (never stale), stable row ids stamped at `list:add`.
- `data.*` — big/imported tables, host-side: `data.use(alias)`, `data.sql(q, params)`,
  `:value()`, `:pivot{}`, writes by stable row id only. Polars stays off the render
  path; only scalars and windows cross into Lua. Callable only inside `view()`/handlers.

**3.4 DX pillars (the reason this milestone exists).** The agent is the primary
author; the loop is **plan → apply → verify**, all over MCP:

1. **Types as the gate.** Ship generated `ui.d.luau` / `data.d.luau` /`osv.d.luau`
   stubs with Moonwave-style doc comments; `.luaurc` points at them; `validate_app`
   runs **`luau-lsp analyze`** + parse before any hot-swap. (2026 finding: ~94% of
   LLM code errors are type errors — this gate is nearly a free bug filter.) Stubs are
   generated from the same declarative Rust table that registers the bindings — one
   source of truth for binding + stub + reference doc. (`mlua-extras` is prior art;
   fork or imitate.)
2. **Errors that teach.** Everything pcall'd; a broken view keeps the last good frame
   and shows an inline error card with `file:line` + traceback; handler errors stream
   to a console ring buffer readable over MCP (`read_console`). Hot reload of a broken
   file never wipes app data (CRDT survives; VM is rebuilt).
3. **Senses.** MCP tools: `screenshot` (offscreen wgpu render → PNG),
   `dump_tree` (the resolved `El` tree + computed rects as JSON — cheaper and more
   assertable than pixels), `click(x,y | selector)` synthetic input, `read_state`
   (CRDT as JSON), `table_sql` profiling. Act → observe → assert.
4. **Harness.** `lua_app::Harness` for Rust-side tests: load app, `view()`, click,
   assert tree/state — headless, no GPU. Bundled example apps run as regression tests
   (the proven app_engine pattern). Golden-snapshot the serialized tree, not pixels.
5. **Hot reload** as a first-class engine feature (file watcher + MCP write → reload,
   preserving CRDT + retained islands; sub-second). The scan's LÖVE/Defold lesson:
   bolted-on reload rots, engine-owned reload works.
6. **Per-block code edits.** `.luau` sources are stored as block docs (port the
   code_editor splitter; see §7) so the agent edits by block id, merging cleanly with
   concurrent human edits — carry over the read_file_blocks/set_file_block_text tools.

**3.5 Store + shell.** Minimal workspace/file model so apps are real files: workspace =
index LoroDoc, each file = its own LoroDoc (`.app` = `{files: block docs, state: CRDT}`),
vault persistence. Shell2 grows an explorer (spaces → typed files) + tabbed hosting.
The MCP bridge is rebuilt on shell2 with the same UDS/rpc shape as sthalam's (port
`osvauld-rpc`, `bridge.rs` patterns: single authority, mutate on owning thread, seed
on create, repaint on write, `Refresh` fan-out).

**Exit test:** the agent (over MCP) creates an app, validate gates it, hot reload shows
it, screenshot + dump_tree verify it, a broken write shows an error card with the right
line — without restarting the shell.

## 4. M2 — table + data plane

Product decisions carried forward: tables are a **pure typed DB** (no formulas, no
schema relations — compute lives in dashboard apps); edit by stable row id, never
display index; declarative `where`/`order_by` + Lua escape hatch; `on_edit` hook in,
data-change subscriptions out (automation layer's job); filters are per-viewer view
state, options prefilled from data.

**4.1 Spikes first** (from the CRDT-scale design): Loro memory/load at 10k/100k/1M
cells (calibrates shard size); cold-shard deferred import; mlua-Luau sandbox parity;
parquet+overlay window fetch at 10⁶ rows.

**4.2 Storage architecture** (design locked, phased):
- Phase T1: single-doc table (manifest + rows as LoroMap per row, cells LWW), port
  `table_core` read/write/typed-cells/row-id stamping mostly as-is.
- Phase T2: columnar mirror (Polars) maintained by subscription, version-tagged; all
  reads (windows, sort/filter permutations, dashboards, joins) hit the mirror. Port
  `table_query` (SQL, pivot — fix the numeric-`on` pivot bug), `table_import`.
- Phase T3: sharded CRDT (manifest + ~2k–10k-row shard docs, hash/seq partition,
  shard-prefixed row ids) + hot/warm/cold lifecycle + parquet-base+CRDT-overlay for
  imported analytics data. Only when a real table pushes past T1 limits.

**4.3 The grid island.** Retained widget owning: viewport row window (uniform-height
fast path, prefix-sum variable path), pinned header, per-axis scroll, column drag-
resize + per-row heights (view state), cell editing via runtime fields addressed
`(table, row_id, col)`, select = combobox on the overlay layer with `transitions`
state-machine gating, `on_edit` to Lua. Scroll never runs Lua; `view()` re-runs on
frontier/query/viewport-window change only (dirty gate from §2.10).

**4.4 Charts (`chart` crate).** Rust island painting into vello, declarative spec from
Lua (`ui.chart { data = q, type = "line", x = ..., y = ... }`). Scan verdict (§9):
nobody has shipped a vello chart backend — roll our own, structured as gpui-d3rs does
it: renderer-agnostic modules (scales + ticks, color, series prep, spatial index for
hover) with one isolated vello paint step. Rent `colorous`/`colorgrad` for palettes,
vendor the ~40-line d3 nice-numbers tick algorithm, hand-roll LTTB/M4 downsampling for
big series. Interaction (tooltip/crosshair/zoom) is always application code: invert the
draw scales + binary-search sorted x (or rstar for scatter). egui_plot lessons baked
in: never rotate label text, stagger category labels, always set a tooltip formatter,
clip to the plot rect. Long-tail types (candlestick, treemap, gauge) can stopgap
through charts-rs SVG → `vello_svg` on data-version change.

**Exit test:** the Superstore/HR dashboards rebuilt as Lua apps on runtime: 10k-row
grid smooth at trackpad speed, KPIs + joined SQL + charts live-update on an MCP row
write.

## 5. M3 — doc editor (`.doc`)

**5.1 Model ports as-is** (renderer-independent): `block_doc` LoroTree body, nested
blocks, meta kinds, marks on LoroText (`config_text_style` with the full key set,
`ExpandType::None` — the typing-after-bold rule), eager-create content containers
(the undo gotcha), child-promoting delete, `UndoManager` post-seed with the
no-commit-around-undo discipline + `group_start/end` for paste/multi-op steps.

**5.2 The editing widget is new, on parley.** Facts (scan-confirmed): parley 0.11 has
no rich editor and none is roadmapped; `PlainEditor` is single-style but its
cursor/selection/IME machinery is solid; `RangedBuilder` renders multi-style fine.
Design:
- Per-block **layout cache**: Loro runs (delta) → styled `RangedBuilder` → parley
  `Layout`, keyed by (content fingerprint, width, block kind). Virtualize by viewport.
- **Caret/selection = our own model over parley geometry**: position = (block id,
  byte offset); selection = anchor/head pair (Helix's `Range` shape), cross-block
  aware; parley `Cursor`/`Selection` APIs give hit-testing + geometry per block
  layout; multi-caret is a `Vec<Range>` from day one (cheap now, retrofit later).
- **Editing ops write to LoroText directly** (insert/delete/mark), layout cache
  invalidated by fingerprint; undo = Loro UndoManager, no local stack.
- IME per focused block via the runtime's existing plumbing + caret-area positioning.
- Slash menu / floating toolbar / language picker ride the §2.4 overlay layer; block
  drag rides `on_drag`; gutter stays pinned-left (deliberate deviation from Notion).
- **Scene→PDF discipline carries over**: content composes to a backend-neutral display
  list; screen and PDF are siblings. `pdf_paint` re-targets from egui Galley walking to
  parley Layout glyph runs (same printpdf gotchas apply: DrawPolygon not DrawRectangle,
  SetTextMatrix not SetTextCursor, text_layout feature).

**5.3 `ui.doc`** — the same widget embedded in Lua apps over a named tree in the app's
own CRDT (the workbook pattern), read_only flag = the reader/permit mode.

**Exit test:** the old doc_editor test suite green against the new widget (ported
headless tests: selection, marks, undo/redo incl. the nested-container regression,
markdown rules, paste semantics), plus Malayalam text editing correctly in a block
(the Indic edge, now with real shaping).

## 6. M4 — canvas (`.canvas`)

New app, no legacy. Whiteboard first (shapes/ink/text/arrows/select), the dataflow-
canvas idea stays parked until the substrate exists.

- **Doc model:** `LoroTree` of shapes (native fractional-index ordering + reparenting
  gives z-order, groups and frames in one container; `LoroMovableList<LoroMap>` is the
  fallback if tree overhead annoys) — each shape node's map holds {type, x, y, w, h,
  rotation, props…}; freehand `points` stored as one atomic array value per shape
  (whole-field LWW — strokes aren't co-edited point-by-point; the loro-excalidraw
  lesson); text labels = child LoroText (collab editing inside shapes). Presence
  cursors later ride the awareness channel, not the doc.
- **Island:** owns camera (pan/zoom `Affine` — vello scales vectors + glyphs cleanly),
  tool state machine (select/hand/rect/ellipse/line/arrow/draw/text), marquee +
  handles (resize/rotate) as chrome, snapping later. Input = the §2.2 drag events with
  camera-inverse to world coords.
- **Hit-testing:** broad phase = R-tree over shape AABBs (`rstar`), narrow = kurbo
  contains/stroke-distance. Cull painting by viewport AABB query.
- **Ink:** pointer samples → `geo::Simplify` (RDP/VW) → `perfect_freehand` crate for
  the pressure-scaled outline (tldraw's algorithm, Rust port). winit 0.30's pointer
  overhaul carries pen pressure (Wayland/Windows/Web; tilt deferred upstream — verify
  per platform).
- **Arrows/connectors:** bound to per-shape normalized anchors (tldraw model);
  straight/arc v1, elbow = A* over a visibility grid built from shape bboxes
  (the Excalidraw recipe — their greedy version failed; don't repeat it). No usable
  routing crate exists; hand-roll.
- **Text at zoom:** vello glyphs are vector paths — crisp at any zoom; render canvas
  text *unhinted* (hinting shimmers under continuous zoom, vello#204).
- Boolean ops (eraser/shape merge) when needed: `i_overlay` on flattened paths;
  vendor Graphite's `path-bool` if curve-preserving results matter (kurbo has no
  booleans, #277; linesweeper is archived). Graphite itself = the same-stack
  (kurbo/vello/parley) flagship to study.

**Exit test:** two windows on one doc: draw/move/edit shapes concurrently, converge;
1k shapes pan at full frame rate (culling + R-tree working).

## 7. Crate map

New (working names):
- `lua_app` — Luau VM, sandbox, `ui.*`/`doc:*`/`data.*` bindings, style parser, hot
  reload, harness, stub generator.
- `grid` — table island (uses `table_core`, `table_query`).
- `doc_ed2`… final name TBD — block editor island (uses `block_doc`).
- `canvas` — canvas island.
- `chart` — chart island.
- `spaces` — workspace/file model over `vault`.
- `bridge2` — UDS bridge + MCP tool surface on shell2 (ports `osvauld-rpc`).

Ported (egui-free, keep): `block_doc`, `table_core`, `table_query`, `table_import`,
`cryptography`, `identity`, `storage`, `vault`, `osvauld-mcp` shim. The code_editor
Lua splitter module moves into `lua_app` or `block_doc` (tree-sitter grammar now via
tree-house bindings, since inkjet is archived — §9). `code_highlight` is rebuilt on
tree-house with the same `HlKind` span API.

Reference-only (delete when their lessons are absorbed): `app_engine`, `doc_editor`,
`code_editor` widget layers, `text_edit`, `rich_text`, `pdf_paint` (re-target),
`sthalam`.

## 8. Build order

```
M0 runtime plumbing  ── ids/state store → events/drag → scroll → overlay →
                        rich text → focus/clipboard → wakeup/timers → IME area →
                        images → dirty-gate seam → a11y stub
M1 lua_app + store   ── VM/sandbox → ui.* + style → doc binding → hot reload +
                        error cards → stubs + validate gate → harness →
                        spaces/explorer → bridge2 + MCP senses
M2 table + charts    ── spikes → table_core port + grid island → mirror + data.* →
                        chart island → dashboard rebuild
M3 doc editor        ── block_doc port → block layout cache → caret/selection →
                        marks/undo → overlays (slash/toolbar) → PDF re-target → ui.doc
M4 canvas            ── doc model → camera+tools → hit-test/cull → ink → arrows
```

Rationale for table-before-editor: the table exercises every M0/M1 seam (islands,
virtualization, overlays, data plane, dirty gate) and is the near-term product edge
(agent-built dashboards); the editor is deeper, benefits from those seams being firm.
The code editor (block-based Lua editing) rides after M3 on the same block/editing
infra. Then: presence, permits, iroh sync — per the standing direction (git loop last).

## 9. Library verdicts (scan 2026-07-03)

**Lua/DX** (settled):
- **Luau via mlua**, sandbox mode on; `mluau` fork on watch for yield-through-Rust.
- **luau-lsp `analyze`** as the validate gate; `.luaurc` + generated `.d.luau` stubs
  with Moonwave doc comments (single source: the Rust binding registry; `mlua-extras`
  is the prior art to imitate). piccolo still immature — pass.
- Keep rebuild-every-view; no Fusion/Roact-style layer (LLM-reliability evidence).
- Dev path runs source, not precompiled bytecode (line numbers in errors).

**Text editing** (settled):
- parley 0.11 now; **no RichEditor exists or is planned** — keep `PlainEditor` as the
  single-field kernel (IME/selection solid), build the multi-run styled editing layer
  ourselves per §5.2. NLnet-funded parley work through 2026 = safe bet.
- **tree-house 0.4 + tree-sitter-lua** for highlighting (helix's layer; actively
  released; optional ropey integration). **inkjet is archived (2025-09) — migrate.**
- ropey 1.6 stable if an ephemeral buffer view is needed (not 2.0-beta); Loro
  `UndoManager` (group_start/end, local-only undo semantics) instead of any local
  undo stack; Helix `Selection`/`Range{anchor,head}` as the multi-caret model.
- AccessKit 0.24 + accesskit_winit current; parley↔AccessKit bridging outside Masonry
  is a known friction point (parley#310) — budget a shim.

**Charts** (settled):
- **Roll `chart` on vello** — no vello backend exists for plotters/charming/plotlars/
  plotkit; a plotters-vello backend would import plotters' output model, not give a
  vello-native scene. Copy gpui-d3rs's module split (renderer-agnostic scale/color/
  spatial + isolated paint) and iced_plot's retained-geometry + picking pattern (our
  version-gated scene cache is the same idea).
- Rent: `colorous` (d3 palettes, dtolnay) + `colorgrad` (custom gradients); vendor
  nice-numbers ticks (escalate to Talbot-Lin-Hanrahan only if label collisions bite);
  `contour` crate later for density charts; LTTB/M4 downsampling hand-rolled (~20 lines).
- Stopgap for long-tail chart types: charts-rs (16 types, active) SVG → `vello_svg`
  (linebender, active) rebuilt on data-version change.
- Watch, don't adopt: charton (GoG/Polars-native spec design, 7 months old), avenger
  (Vega scenegraph on wgpu, dormant), plotkit (day-one 1.0).

**Canvas** (settled):
- kurbo does hit-testing (`Shape::winding/contains`), offset, simplify, stroke —
  but **no boolean ops** (#277; linesweeper archived 2026-01). Booleans: `i_overlay`
  (very active, polygon-only, flatten first) now; vendor Graphite's `path-bool`
  (in-repo lib, same kurbo/vello/parley stack) for curve-native later.
- `rstar` (active) for spatial index — no update op: wrap with id→AABB
  remove/reinsert per move, batched per drag frame; doubles as viewport culling
  (tldraw pattern: 10k shapes → ~50 painted).
- `perfect_freehand` 0.1 (Rust port of tldraw's), `geo::Simplify`, `splines`
  (Catmull-Rom) for ink; winit 0.30 pen pressure (tilt pending upstream).
- Connectors + camera: hand-rolled (no crates exist) — Excalidraw A*-elbow recipe,
  tldraw anchor snapping, `kurbo::Affine` camera.
- CRDT schema: per-shape LoroMap in a movable container; atomic points arrays;
  Loro-native ordering (don't roll fractional indexing; `fractional_index` crate
  exists if ever needed outside Loro).
