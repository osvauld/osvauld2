# Runtime rebuild — 13-week execution plan (2026-07-21, reordered MVP-first 2026-07-26)

Companion to `runtime-rebuild-plan.md` (plan of record). That doc is the *what/why*; this is
the *when*. Scope for the 3-month window: **M0 (finish) + M1 + M2** — end at demoable
agent-built dashboards. **M3 doc editor and M4 canvas are out of this window**, and so are
presence/permits/iroh sync.

> **Reordered MVP-first (2026-07-26).** The old plan finished all of M0 (rich text, focus,
> IME, stubs) *before* touching Lua at W5. But the MVP *is* the app layer — a Lua app that
> renders, mutates its state, and can be agent-authored. So we pull the app layer to **W2**
> and defer M0's back half to where each piece is actually needed (rich text → the grid;
> focus/clipboard → when serious text editing lands). Two things make this safe: the old
> `app_engine` is a pre-solved decision list, and the W2 research settled the architecture
> (the 3-layer split), so W2 can compress the old VM+binding+doc weeks into one. Net effect:
> **the working MVP moves from ~W9 to ~W5**, freeing weeks for M2 + a real end buffer.

## Assumptions & velocity

- **Capacity: solo, agent-guiding.** Claude drives design and reviews; you write the code to
  learn it ([[explain-with-edits]]). Deep understanding, not pure-agent speed. ~one
  milestone-seam per week; heavier seams (the app layer, grid island) get a full week.
- **Start:** week of 2026-07-21. **Target close:** ~2026-10-17 (W13).
- **Slack now exists, at the end.** Pulling the MVP forward buys **W11–W13 as buffer** —
  they absorb W2's aggressive compression and the M2 spike unknowns, and hold the deferred
  M0 polish (IME, a11y, dirty-gate, images) that no app blocks on. The old "no slack" plan
  had none; this is the single biggest health improvement of the reorder.
- **Where the code is today (verified):** ids, keyed state store, scroll containers, drag
  *gesture* (phases + capture, W1), Field/Focus (click + autofocus), single-style text,
  overlay (anchored, catcher+dismiss, flip/clamp, Point anchor, W1), animation/transition
  (timer wheel + retained progress, W1).
- **Progress (W1, done):** §2.4 overlay finished and the animation system stood up
  (`w1.md`). M0's *core* seams are in; its *polish* seams are deferred by the reorder, not
  dropped. W1 detail in `w1.md`; **W2 detail in `w2.md`.**

---

## M0 core — done (W1)

Exit was originally "shell2 login clean + a demo screen." The reorder narrows M0's blocking
scope to what an app layer stands on — overlay, animation, drag gesture, scroll, single-style
text, Field/Focus — all **done in W1**. The rest of old-§2 (rich text, focus/clipboard/
shortcuts, IME, images, dirty-gate, a11y) is **deferred to where it's needed** (see the
"Deferred M0" note under M2). Nothing is cut; it's resequenced.

### W1 — Overlay finish + animation system (§2.4, §2.7 timer) — *done, detail in `w1.md`*
- Drag phases + capture unification (§2.2), scroll hit-clip (§2.3): done.
- Overlay: catcher + click-away dismiss, flip/clamp, Point anchor: done.
- Animation: `EventLoopProxy` wakeup + timer wheel; retained transition progress in the
  `Store` keyed by id; paint interpolates `Look`; button hover-fade proves it: done.

---

## M1 — Lua app layer + DX + store + bridge (W2–W5)

The centerpiece (§3), pulled forward. Exit test: the agent (over MCP) creates an app,
validate gates it, hot reload shows it, screenshot + dump_tree verify it, a broken write
shows an error card with the right line — without restarting the shell. **This is the
working MVP, now landing ~W5.**

### W2 — Lua app layer v1 + drop targets (§3.1, §3.2, §3.3 doc) — *current, detail in `w2.md`*
Compresses the old W5+W6+W7-doc into one week (reference + settled architecture make it
possible; `w2.md` scopes the risk).
- Sandboxed Luau VM: `sandbox(true)`, memory cap, `set_interrupt` budget, **source-only**
  (no bytecode), `require` shim, `now()`.
- Prelude + walk → `El<LuaMsg>` directly (no parallel node type); CSS-ish style vocab →
  taffy/peniko; retained state keyed by **doc id, never index**.
- `doc:list/map/text` as a **local single-writer store** (Loro; CRDT-ness/sync deferred;
  persistence free); re-resolve handles, never cache.
- Input via MVU string round-trip (the honest cheat); host mounts `todo` + `kanban`.
- **Drop targets**: the one new runtime primitive (hit-test during drag + `on_drop`);
  preview + placeholder compose from W1's overlay + Point anchor.
- **Done when:** `todo.lua` renders/scrolls/mutates; `kanban.lua` drags a card across
  columns with a live preview; an infinite loop is killed with a line number; state survives
  restart.

### W3 — Hot reload + error cards (§3.4.2, §3.4.5)
- Hot reload as an engine feature: watcher + write → reload, preserving the doc + retained
  islands, sub-second. Everything pcall'd; broken view keeps last good frame + inline error
  card (`file:line` + traceback); handler errors → console ring buffer.
- **Done when:** editing a running app's view reloads sub-second with state intact; a syntax
  error shows an error card at the right line, data survives.

### W4 — Types gate + harness (§3.4.1, §3.4.4)
- Generate `ui.d.luau` / `doc.d.luau` stubs from the **one** Rust binding registry (source of
  truth for binding + stub + doc). `.luaurc`; `validate_app` runs `luau-lsp analyze` + parse
  **before** any hot-swap. (The research's constrained-surface finding: good stubs + itemized
  errors are what make the surface agent-authorable, not the sandbox alone.)
- `lua_app::Harness`: load app, `view()`, click, assert tree/state — headless, no GPU;
  golden-snapshot the serialized tree; example apps become regression tests.
- **Done when:** a type error is caught by validate before swap; a harness test asserts tree
  + state across a click.

### W5 — spaces/explorer + bridge2 + MCP senses (§3.5, §3.4.3) — **M1 exit / working MVP**
- Minimal store: workspace = index LoroDoc, file = own LoroDoc, vault persistence. Shell2
  explorer (spaces → files) + tabbed hosting.
- `bridge2` UDS/rpc (single authority, mutate on owning thread, seed on create, repaint on
  write, `Refresh` fan-out). MCP tools: `screenshot`, `dump_tree`, `click`, `read_state`,
  `read_console`.
- **M1 exit test** (above) — **the agent authors a working app over MCP.**

---

## M2 — table + data plane + charts (W7–W10, buffer W11–W13)

§4. Exit test: Superstore/HR dashboards rebuilt as Lua apps — 10k-row grid smooth at
trackpad speed, KPIs + joined SQL + charts live-update on an MCP row write.

### W6 — Seam week: catch-up / backfill (float)
The joint between M1 and M2, deliberately soft. **Its first job is to absorb any W2–W5
slip** (W2's compression is the likeliest to run long). If M1 landed clean, it backfills the
deferred M0 that real apps now want — **focus/clipboard/shortcuts** (§2.6) and rich-text
*prep* — or starts the M2 Loro spikes early. Do not pre-commit it; spend it on wherever the
schedule actually is.

### W7 — Spikes + table_core port (T1) (§4.1, §4.2 T1)
- Loro memory/load spikes at 10k/100k/1M cells (calibrates shard size); cold-shard deferred
  import; parquet+overlay window at 1M rows.
- Phase **T1**: single-doc table (manifest + per-row LoroMap, cells LWW); port `table_core`
  read/write/typed-cells/row-id stamping mostly as-is.
- **Done when:** create/read/edit a table by stable row id; spike numbers recorded.

### W8 — Grid island + **rich text leaf** (§4.3, §2.5)
- Retained grid: viewport row window, pinned header, per-axis scroll, column drag-resize,
  cell editing via runtime fields, select = combobox on the overlay layer, `on_edit` → Lua.
  Scroll never runs Lua; `view()` re-runs on frontier/query/viewport change.
- **Rich text leaf** lands here — its original home. `rich(runs)` over parley `RangedBuilder`
  (family/size/weight/style/color/underline/strike/bg/mono per run, real `FontWeight` bold):
  the grid's own cells + labels are its first consumer. (Note: `text.rs` already wires
  `ranged_builder` — this is ~half-built.)
- **Done when:** 10k-row grid scrolls smooth; a cell edit fires `on_edit`; a multi-style cell
  renders correctly.

### W9 — Columnar mirror (T2) + data.* (§4.2 T2, §3.3 data.*)
- Phase **T2**: Polars mirror maintained by subscription, version-tagged; all reads hit the
  mirror. Port `table_query` (SQL, pivot — **fix the numeric-`on` pivot bug**), `table_import`.
- `data.*`: `data.use(alias)`, `data.sql(q, params)`, `:value()`, `:pivot{}` — Polars off the
  render path, only scalars/windows cross into Lua.
- **Done when:** import Superstore xlsx; SQL + pivot correct; a 100k-row read stays smooth.

### W10 — Chart island + dashboard rebuild (§4.4) — **M2 exit test**
- `chart` crate: renderer-agnostic modules (scales + nice-numbers ticks, series prep, spatial
  index for hover) + one isolated vello paint step. `ui.chart { data, type, x, y }`. LTTB
  downsampling for big series.
- **M2 exit test** — the demo.

### W11–W13 — Buffer / hardening / deferred M0
Real slack, earned by the pull-forward. In priority order: **(1)** absorb any M2 slip
(spikes/charts are the likeliest); **(2)** harden the app layer under real agent use;
**(3)** land the remaining deferred M0 — IME (§2.8), a11y stub (§2.11), dirty-gate `SceneKey`
(§2.10), images (§2.9) — none of which any app blocks on. If W2–W10 held, this is where the
demo gets polished instead of rushed.

---

## Deferred M0 — where each piece went (nothing cut)

| Old-§2 seam | Old week | New home | Why |
|---|---|---|---|
| Rich text leaf (§2.5) | W2 | **W8** (grid) | Its real consumer is table cells |
| Focus/clipboard/shortcuts (§2.6) | W3 | **W6** float | No MVP app needs it; the input cheat covers todo/kanban |
| IME (§2.8) | W3 | **W11–13** | Only serious text editing needs it |
| Wakeup/timers remainder (§2.7) | W4 | **W3/W5** | External wakeup folds into hot reload + bridge |
| Images (§2.9) | W4 | **W11–13** | gallery/deck are not MVP apps |
| Dirty-gate (§2.10) | W4 | **W11–13** | Pure perf; grid does its own windowing |
| a11y stub (§2.11) | W4 | **W11–13** | Seam behind a flag, no consumer yet |

---

## Risk & cut lines

The reorder moved the risk. Name the cut before you hit it.

1. **W2 is the new overloaded week** (VM + walk + style + doc + host + drop targets, one
   week). It compresses the old W5–W7 on the strength of the reference engine + settled
   architecture. If it slips: `w2.md`'s never-cut floor is "todo renders and mutates + a card
   drops into another column" — that alone unlocks the MVP; hot reload (W3) can start against
   it. **W6 (float) and the W11–13 buffer exist to absorb this.**
2. **M2 spikes (W7) are unknowns** — Loro at 1M cells calibrates the whole shard design. If
   numbers are bad, T2/T3 thresholds shift and the grid/data weeks stretch. The W11–13 buffer
   is the release valve; if it's fully consumed, the honest deliverable is M0 + M1 + grid +
   `data.*` (agent authors a working table app) with charts trailing.
3. **W13 chart squeeze** (unchanged in kind, but now cushioned): if charts run long, the
   `charts-rs → vello_svg` stopgap (§9) ships the demo and the native `chart` crate follows.

**Global cut order if the 13 weeks compress:** deferred M0 polish (W11–13) → chart polish →
a11y stub → dirty-gate → images. **Never cut:** the app layer (W2), hot reload + error cards
(W3), the VM + validate gate (W2/W4), the grid island (W8), drop targets (W2, §8 of `w2.md`).
Those are the load-bearing seams everything downstream stands on.

**If W2–W5 hold, the MVP is real by ~W5** — an agent-authored, hot-reloading Lua app over
MCP — with 8 weeks left for M2 + buffer. That is a far more comfortable clock than the
original "MVP at W9, no slack."

## Out of scope (post-window)

M3 doc editor (§5), M4 canvas (§6), presence, permits, iroh **sync**. Sequenced after per the
plan of record; the M0/M1 seams built here (overlay, app layer, islands, hot reload, data
plane) are exactly what those milestones land on.
