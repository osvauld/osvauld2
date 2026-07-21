# Runtime rebuild — 13-week execution plan (2026-07-21)

Companion to `runtime-rebuild-plan.md` (plan of record). That doc is the *what/why*;
this is the *when*. Scope for the 3-month window: **M0 (finish) + M1 + M2** — end at
demoable agent-built dashboards. **M3 doc editor and M4 canvas are out of this window**,
and so are presence/permits/iroh sync.

## Assumptions & velocity

- **Capacity: solo, agent-guiding.** Claude drives design and reviews; you write the
  code to learn it ([[explain-with-edits]]). Deep understanding, not pure-agent speed.
  Budget ~one milestone-seam per week; heavier seams (overlay, rich text, grid island)
  get a full week each.
- **Start:** week of 2026-07-21. **Target close:** ~2026-10-17 (W13).
- **No slack weeks are built in.** The buffer is *scope inside a week*: the stub-grade
  seams (dirty-gate scaffold, a11y stub, images, chart polish) are the release valves —
  drop them to stubs first when a week runs hot. See Risk & cut lines.
- **Where the code is today (verified):** ids (`id.rs`), keyed state store
  (`state.rs`), scroll containers (`scroll.rs` + wheel), drag v1 (`drag.rs`, ~20 LOC),
  Field/Focus (`editor.rs`), single-style text (`text.rs`). Roughly a third into M0.

---

## M0 — finish runtime plumbing (W1–W4)

Remaining seams from §2. Exit test: shell2 login rebuilt with zero `custom()` hacks
(except identicon/wordmark) + a scrollable, overlay-using demo screen (dropdown + modal
+ image + rich text) driven by a headless snapshot test.

### W1 — Event backbone + scroll correctness (§2.2, §2.3)
- Land drag **phases** (Start/Move/End, element-local coords + mods, pointer capture
  press→release) as `on_drag(fn(DragEvent) -> M)`. Column-resize as the smoke consumer.
- `on_key` for focused element/island (app-level fallback stays in `App`);
  `on_hover_enter/exit` as messages (hover paint already exists).
- Fix scroll **hit-testing to respect clip** — a scrolled-out button must not catch
  clicks (the app_engine sharp edge). Draggable scrollbar thumb swallows the click.
- **Done when:** demo with resizable column + nested scroll + hover states; clicks
  correct under clip; drag captured across the whole gesture.

### W2 — Overlay layer (§2.4)
- `.overlay(anchor)` subtrees laid out against the anchor's computed rect (flip/clamp at
  window edges), painted last, hit-tested first, click-away dismiss contract.
- Prove it with a dropdown + a modal on the one mechanism.
- **Done when:** dropdown positions/flips/dismisses; modal traps click-away; both
  assertable from a rect/tree dump.

### W3 — Rich text leaf (§2.5)
- `rich(runs)` over parley `RangedBuilder`: family/size/weight/style/color/underline/
  strike/bg/mono per run. Real `FontWeight` bold (no fake-bold). Serves table cells,
  labels, code spans, doc reader later.
- **Done when:** a multi-style line renders correctly; layout runs snapshot green.

### W4 — Focus/clipboard/shortcuts + IME + wakeup/timers + M0 close-out (§2.6–2.11)
- Tab/Shift-Tab traversal + focus ring; `arboard` clipboard into the editor driver
  (Cmd/Ctrl C/X/V); one modifier-aware shortcut router; `set_ime_cursor_area` at caret.
- `EventLoopProxy<UserEvent>` wakeup + timer wheel (caret blink), replacing
  all-or-nothing `animating()`. **Stub-grade** this week: dirty-gate `SceneKey` scaffold
  (§2.10), a11y role/label seam behind a flag (§2.11), minimal `img(src)` (§2.9).
- **M0 exit test:** rebuild shell2 login clean + build the demo screen; headless
  snapshot test passes.
- **Overloaded week — see cut lines.** Images/a11y/dirty-gate are seams, not features;
  keep them stubs if the week runs hot.

---

## M1 — Lua app layer + DX + store + bridge (W5–W9)

The centerpiece (§3). Exit test: the agent (over MCP) creates an app, validate gates it,
hot reload shows it, screenshot + dump_tree verify it, a broken write shows an error
card with the right line — without restarting the shell.

### W5 — Luau VM + sandbox (§3.1)
- mlua `luau` feature; `Lua::sandbox(true)` + allocator memory cap + `set_interrupt`
  instruction/wall budget. String ids everywhere (numbers are doubles). **Run source,
  not bytecode** (line numbers in tracebacks).
- Load `main.luau` returning `{ view, actions, page }`; bare-function sugar for tiny apps.
- **Done when:** a trivial app loads and `view()` runs; an infinite loop is killed by the
  interrupt budget.

### W6 — ui.* binding + style vocab (§3.2)
- `ui.*` maps 1:1 onto `El` builders (one validation checkpoint, no parallel node type).
  Port the CSS-ish style vocab (`padding = "10 20"`, `border = "1 #333"`,
  `csscolorparser`, CSS key names). Handlers → per-view callback table; tree carries
  `u32` ids.
- **Done when:** a Lua counter app renders the same pixels as its Rust `El` twin; click
  callback ids dispatch.

### W7 — doc:* CRDT binding + hot reload + error cards (§3.3, §3.4.2, §3.4.5)
- `doc:list/map/text` handles, metamethod sugar, re-resolved by name each access, stable
  row ids stamped at `list:add`.
- Hot reload as an engine feature (watcher + MCP write → reload, preserving CRDT +
  retained islands, sub-second). Everything pcall'd; broken view keeps last good frame +
  inline error card (`file:line` + traceback); handler errors → console ring buffer.
- **Done when:** editing a running app's view reloads sub-second with CRDT intact; a
  syntax error shows an error card at the right line, data survives.

### W8 — Types gate + harness (§3.4.1, §3.4.4)
- Generate `ui.d.luau` / `data.d.luau` / `osv.d.luau` stubs from the **one** Rust binding
  registry (source of truth for binding + stub + doc). `.luaurc`; `validate_app` runs
  `luau-lsp analyze` + parse **before** any hot-swap.
- `lua_app::Harness`: load app, `view()`, click, assert tree/state — headless, no GPU.
  Golden-snapshot the serialized tree; bundle example apps as regression tests.
- **Done when:** a type error is caught by validate before swap; a harness test asserts
  tree + state across a click.

### W9 — spaces/explorer + bridge2 + MCP senses (§3.5, §3.4.3)
- Minimal store: workspace = index LoroDoc, file = own LoroDoc (`.app = {files: block
  docs, state: CRDT}`), vault persistence. Shell2 explorer (spaces → files) + tabbed
  hosting.
- `bridge2` UDS/rpc (port `osvauld-rpc` + `bridge.rs`: single authority, mutate on owning
  thread, seed on create, repaint on write, `Refresh` fan-out). MCP tools: `screenshot`,
  `dump_tree`, `click`, `read_state`, `read_console`.
- **M1 exit test** (above).

---

## M2 — table + data plane + charts (W10–W13)

§4. Exit test: Superstore/HR dashboards rebuilt as Lua apps — 10k-row grid smooth at
trackpad speed, KPIs + joined SQL + charts live-update on an MCP row write.

### W10 — Spikes + table_core port (T1) (§4.1, §4.2 T1)
- Run the spikes first: Loro memory/load at 10k/100k/1M cells (calibrates shard size);
  cold-shard deferred import; parquet+overlay window fetch at 1M rows. (mlua-Luau sandbox
  parity already exercised in M1 — confirm.)
- Phase **T1**: single-doc table (manifest + per-row LoroMap, cells LWW); port
  `table_core` read/write/typed-cells/row-id stamping mostly as-is.
- **Done when:** create/read/edit a table by stable row id; spike numbers recorded to set
  the T2/T3 thresholds.

### W11 — Grid island (§4.3)
- Retained grid owning: viewport row window (uniform-height fast path + prefix-sum
  variable path), pinned header, per-axis scroll, column drag-resize, cell editing via
  runtime fields `(table, row_id, col)`, select = combobox on the overlay layer with
  `transitions` gating, `on_edit` → Lua. Scroll never runs Lua; `view()` re-runs on
  frontier/query/viewport-window change (dirty gate).
- **Done when:** 10k-row grid scrolls smooth at trackpad speed; a cell edit fires
  `on_edit`.

### W12 — Columnar mirror (T2) + data.* (§4.2 T2, §3.3 data.*)
- Phase **T2**: Polars mirror maintained by subscription, version-tagged; all reads
  (windows, sort/filter perms, dashboards, joins) hit the mirror. Port `table_query`
  (SQL, pivot — **fix the numeric-`on` pivot bug**), `table_import`.
- `data.*`: `data.use(alias)`, `data.sql(q, params)`, `:value()`, `:pivot{}` — Polars off
  the render path, only scalars/windows cross into Lua; callable in `view()`/handlers.
- **Done when:** import Superstore xlsx; SQL + pivot return correct values; a 100k-row
  read stays smooth through the mirror.

### W13 — Chart island + dashboard rebuild (§4.4, M2 exit test)
- `chart` crate: renderer-agnostic modules (scales + vendored nice-numbers ticks,
  `colorous`/`colorgrad`, series prep, spatial index for hover) + one isolated vello
  paint step. `ui.chart { data, type, x, y }`. egui_plot lessons: no rotated labels,
  stagger categories, always a tooltip formatter, clip to the plot rect. LTTB
  downsampling for big series.
- **M2 exit test** (above) — the demo.

---

## Risk & cut lines

Two weeks carry most of the schedule risk. Name the cut before you hit it.

1. **W4 is overloaded** (focus + clipboard + shortcuts + IME + wakeup/timers + 3 stubs +
   exit test). If it slips: land focus/clipboard/shortcuts + wakeup/timers (load-bearing
   for M1) and keep images / a11y / dirty-gate as the flagged stubs. Do **not** let it eat
   W5 — the VM start is on the critical path.
2. **W13 chart + dashboard** is the last-mile squeeze. If charts run long, use the
   `charts-rs → vello_svg` stopgap (§9) for the demo and ship the native `chart` crate in
   the following week. The grid + `data.*` live-update (W11–W12) is the real product
   proof; charts are the garnish.

**Global cut order if the 13 weeks compress:** chart polish → a11y stub → dirty-gate
scaffold → images. Never cut: overlay (W2), rich text (W3), the VM + validate gate
(W5/W8), hot reload + error cards (W7), the grid island (W11). Those are the load-bearing
seams everything downstream stands on.

**If W1–W9 hold but M2 doesn't finish:** the honest 3-month deliverable is M0 + M1 +
grid + `data.*` (agent authors a working table app over MCP) with charts trailing by a
week. That still clears the "agent-built dashboards" bar for the demo.

## Out of scope (post-window)

M3 doc editor (§5), M4 canvas (§6), presence, permits, iroh sync. Sequenced after per the
plan of record; the M0/M1 seams built here (overlay, rich text, islands, hot reload, data
plane) are exactly what those milestones land on.
