# UI reuse — signals in the UI tree, and layout that is not redone when nothing changed

Status: **plan, 2026-10-05; revised the same day (§2a: text shaping is the cost, so a text cache
comes first).** Built: the step 1 benchmark and step 1a (text cache). Grows out of [signals](signals.md) steps 3–4
(`ui.group` in the UI tree, `view` as the root group), which that doc left open, and out of a
measurement of the workspace screen (§2) that says describing is not where the frame goes —
layout is. `docs/status.md` says what is real.

Companions:
- [Signals](signals.md) — the invalidation mechanism this builds on (T5, T8 live there).
- [Perf benchmarks](perf-benchmarks.md) — where the budgets for §1 live.
- [Architecture](../architecture.md) — "immediate description + retained islands".

## 1. End-to-end tests — done means these pass

Through the real path: Lua VM in `app_host`, `Headless` in `runtime`, the shell over the bridge,
`OSVAULD_FRAME_TIMINGS` for stage costs. Written first, watched failing, then built until green.

| # | test | proves | step |
|---|---|---|---|
| C1 | `TextEngine`: measuring the same (text, font, size, width) twice shapes once; drawing it after measuring it shapes zero more times (counter on the engine) | the double shaping per label per frame goes | ✅ 1a |
| C2 | A different width, size, family or string is a miss; a rich string with different runs is a miss | the key is complete — a wrong hit is stale text on screen | ✅ 1a |
| C3 | An entry unused for a whole frame is dropped (two-generation sweep); one used each frame survives indefinitely | the cache does not grow without bound for apps that change their text | ✅ 1a |
| C4 | A scene drawn from a cached layout is identical to one from a fresh engine (`Scene` encodings equal), plain and rich, wrapped and not | the cache changes cost, never pixels | ✅ 1a |
| U1 | `bench_workspace_list_idle_frame` (app-level, release): the workspace list with 8 workspaces × 15 items, 60 forced frames, nothing changing. Reports p50 of view, layout, total; budget in `scripts/bench_budgets.json` | the number this plan exists to move; fails first against a budget set below today's 2.7 ms (8 cards) and 13.8 ms (40 cards); moves with 1a | 1, 1a |
| U2 | A UI `ui.group` of rows reads signal `rows`; a handler changes a signal it does not read; the next view calls only the groups that changed (per-group run counter in Lua) | only dirty groups re-run, in the UI tree | ✅ 2 |
| U3 | A clean group's nodes are reused: a click on a button inside it still lands on the right handler, before and after another group re-ran | reuse is safe for interaction (signals T5) — handlers are rebuilt every view today, so this is the risk | ✅ 2 |
| U4 | A group that reads no signal re-runs every view; an app with no groups behaves exactly as today (the existing app_host suite, unchanged) | opt-in, nothing breaks | ✅ 2 |
| U5 | `view` reads only signals and none changed: `view` is not called (counter), the frame still paints the same pixels as the last (screenshot equal) | `view` as root group (signals T8) | 3 |
| U6 | Nothing changed since the last frame — same viewport, same scroll/zoom/animation state, view clean: `layout::solve` is not run (counter in `runtime`), and the placed list from last frame is used | whole-frame layout reuse | 4 |
| U7 | Same as U6 but a scroll offset changes, or a spring is mid-flight, or the viewport resizes: layout runs | the reuse key is complete — the traps are stale pixels, so they are tests | 4 |
| U8 | One row group goes dirty in a list of 120: only that group's subtree is laid out again; the pixels equal a full layout (compare a `Headless` capture against a forced full solve) | subtree reuse is exact, not approximate | 5, if needed |
| U9 | `DumpTree` lists the groups with signals read, runs, last tick (signals T10); the smoke drives the workspace list over the bridge: hover and scroll cost no view runs and no layouts | an agent can see why a frame was or was not redone; the whole path, real socket | 6 |

`cargo test` and the smokes stay green at every step.

## 2. What was measured

2026-10-05, release build, offscreen 1000×800, `OSVAULD_FRAME_TIMINGS=1`; the workspace list
with 8 workspaces of 15 items; 197 forced frames with nothing changing
(`scripts/summarize_frame_timings.py`):

| stage | p50 | p95 |
|---|---|---|
| view (the Lua walk) | 0.30 ms | 0.39 ms |
| layout | 2.37 ms | 2.89 ms (max 6.9) |
| paint | 0.58 ms | 0.67 ms |
| total | 3.29 ms | 4.0 ms |

Layout is ~72% of a frame; skipping `view` alone saves ~0.3 ms. The earlier viewer/kanban
diagnostic (status.md, 2026-10-04) says the same: release layout 0.8/3.4 ms against view
0.1/0.5; debug layout 15/54 ms.

**Caveats, so the numbers are not oversold.** These frames are *forced* (`rpc.frame`); the live
loop is `ControlFlow::Wait` and renders only when something asked, so a genuinely idle window
costs nothing. What the plan saves is the frames something *did* ask for — a hover, a scroll, a
keystroke, a remote edit — where most of the screen is unchanged and all of it is laid out
again. It is CPU only: no windowed run, no GPU, no input-to-display latency yet. Debug builds
are 5–10× worse and are what a developer feels.

Why layout is redone from scratch (`runtime/src/layout.rs`, `solve`): the `El<M>` tree is
consumed; a new `TaffyTree` is built, computed (text leaves measured by parley inside the solve)
and emitted into a flat `Vec<Placed<M>>` every frame, and `lib.rs` re-walks that for hit-testing
and paint. Nothing is kept between frames but the `Store` (scroll, springs, zoom, editor state).

## 2a. Revision, 2026-10-05: it is text shaping

*Disproves the §2 reading that layout structure is the cost.* On a quiet machine pinned to one
core (`taskset -c 2`; an earlier run on a loaded machine read 7–50 ms and was discarded), the
benchmark gives 2.7 ms for 8 cards and 13.8 ms for 40 — linear, ~0.3 ms a card. A probe with the
same 40 cards, same flex/padding/fill/radius, **without their 120 text labels: 0.5 ms; with them:
10.2 ms** — ~80 µs a label, ~95% of the frame. `runtime/src/text.rs` keeps nothing between
frames: every `measure`, `content_widths` and `draw` builds a fresh parley layout, so a label is
shaped at least twice a frame (measured, then painted) whether or not anything changed.

**Landed 2026-10-05 (step 1a):** `TextEngine` keeps layouts for the frame and the one before
(`Shaped`, swept from the frame loop); tests in `runtime/src/text/cache_tests.rs`. U1, quiet
machine, one core: **8 cards 2.7 → 0.52 ms; 40 cards 13.8 → 2.4 ms** (5–6×), both inside their
budgets. **With real amounts of text the difference is far larger** (`runtime/src/text/bench_tests.rs`,
same method, cache switched off for the "before"): a page of 100 wrapping chat messages plus a
50 × 8 table, unscrolled and unculled, **186 ms → 3.7 ms** a frame; one cell edited each frame
3.6 ms (a miss is invisible); 60 wrapped 400-character paragraphs 14.2 → 1.1 ms warm, 7.1 ms
cold (the first frame, or a screen of new text — what cold costs is why tables must virtualize).
Layout/paint structure was never the cost; what remains per card (~0.05 ms) is the
next thing to measure before steps 2–5 are justified.

So the order is: **cache the shaping first** (cheap, helps every frame, dirty ones too), then
signals/groups, then layout reuse only for what the cache leaves. Signals still matter — they
bound what Lua re-describes — but they could not have fixed this frame.

## 3. Design

Two layers, each useful alone, in this order because the second is only safe once the first can
say what is clean:

**Layer 0 — shaping is remembered (step 1a).** `TextEngine` keeps the parley `Layout` it built,
keyed by everything that shapes it: string, family, size, wrap width (and, for a rich string,
every run's range, face, size, weight, slant and colour). `measure`, `content_widths` and `draw`
of the same label share one entry. Entries live two frame-generations: used this frame or last,
kept; otherwise dropped — the same sweep idea as the `Store`, with a `sweep` call from the
frame loop. This is what egui does for its galleys and what browsers do per word. Plain layouts
are aligned `Start` when built, which changes neither width nor height, so measuring and
painting can share. **It does not bound the work of a screen with 100,000 labels** — the first
frame still shapes them all and the cache holds them all; tables must virtualize (below).

**Layer 1 — Lua describes less (signals steps 3–4).** `ui.group(id, fn)` in the UI tree, with the
same rules as in worlds (§4 of signals.md): re-runs only when a signal it read changed; a group
that read no signal re-runs every view. A clean group's *built* output is kept: the `El`
subtree and its handler registrations. Handlers are cleared and rebuilt per view
(`app_host/src/lib.rs`, `body`), so a kept subtree must carry its handler entries across or a
click lands on nothing — U3 is this risk. `view` is the root group: nothing it read changed →
not called (U5).

**Layer 2 — the runtime lays out less.** Two rungs, taking the first and measuring before the
second:

1. *Whole-frame reuse (U6, U7).* If the app says the view is unchanged (a new `App` hook, e.g.
   `view_unchanged()`, default `false` so every Rust screen keeps today's behaviour) and the
   layout's other inputs are equal to last frame's, keep last frame's `placed` and skip `view`
   and `solve`. Inputs besides the tree: viewport, the `Store` entries layout reads (scroll,
   zoom, running springs/transitions), tile sizes, text-engine state (fonts loaded). Anything
   we cannot compare cheaply counts as changed. Paint and hit-testing still run on the kept
   list: they read pointer and focus, which change without the view changing.
2. *Subtree reuse (U8), only if rung 1 leaves a measured cost.* A dirty group in a clean tree
   still pays a full solve. Cache the solved subtree of each clean group keyed by group path,
   available width, and the same store inputs, and splice its `Placed` run (offset by the
   group's position) into the new list. Taffy's cache cannot do this for us: the tree is
   rebuilt each frame, so there is no node to hold a cache. This is the larger change and the one
   with the correctness risk (a group's height can depend on siblings in a flex column), so it
   waits for evidence.

**Invariants this must not break** (architecture.md): paint order == reverse hit order (kept
`placed` is in tree order, spliced runs go in place); messages stay plain data (nothing in the
runtime learns about Lua or signals — the hook is a bool); retained ids stay namespaced per item;
a reload stages a whole new VM, so every group is dirty after one (as in signals step 2).

**The failure to design against is stale pixels**, silently. So: the reuse key is conservative,
U7 enumerates what must invalidate it, and signals step 5's dev check (re-run a clean group on a
sampled frame, compare, note a difference) extends here to also compare a kept layout against a
fresh solve on a sampled frame.

## 4. Steps

1. **Baseline.** U1: the bench and its budget, failing. No code beyond the bench. Lands first so
   every later step has a number.
1a. **Text shaping cache** (C1–C4). `runtime/src/text.rs` and the frame loop's sweep. Re-run U1;
   the 40-card frame should drop from ~13.8 ms toward ~3 ms. The rest of this plan is then
   re-measured and may shrink.
2. **`ui.group` in the UI tree with handler reuse** (U2, U3, U4). **Landed 2026-10-05.** Needed
   `El` to be cloneable first (step 2a: handler closures `Box` → `Rc`, `#[derive(Clone)]`;
   `runtime/src/tests/el_clone.rs`), because a kept group hands back the `El` it built. Then
   `app_host/src/ui_group.rs`: a group's `El`, its handler entries and the groups below it are
   kept per VM; a nested group's reads count for the group around it; a group that read no
   signal, holds a world or a retained 3D scene, or had an element error is never kept, nor is
   any group around one. Tests: `app_host/src/tests/ui_groups.rs`. Not measured yet — the next
   number to take is a chat-style list of groups, one message dirtied at a time.
3. **`view` as root group** (U5). Lua + `app_host` only.
4. **Whole-frame layout reuse** (U6, U7). `runtime` gains the hook and the reuse key; `app_host`
   implements the hook from step 3's root-clean state. Re-measure U1; decide on step 5.
5. **Subtree layout reuse** (U8) — only if step 4's re-measure still shows layout over budget on
   a frame where one group is dirty.
6. **Inspection, the dev check and the smoke** (U9; signals T9–T11), then docs as signals
   (signals step 6), which is what makes a remote chat message dirty one group.

Reviews: `expert-app-host` for steps 2–3, `expert-runtime` for 4–5, `expert-architect` across,
each by a fresh instance after the step lands. `docs/status.md` and `signals.md` move as steps
land.

## 5. Open questions

- Does the workspace list, as the launcher Lua app builds it today, have group boundaries that
  make rung 1 enough, or is it one big `view` where any signal change dirties everything? Read
  `shell2`'s launcher and `workspace_create.lua` at step 2 and split it if so.
- Which `Store` reads does layout make (scroll offset into `emit`, springs in `build`)? Step 4
  starts by listing them from the code, not from this doc.
- Is a hover enough to dirty a group? Hover/focus state lives in the runtime, not signals; the
  intent is that it does not (paint and hit run every frame anyway), but a hover style that
  changes size would need to — check with the first app that has one.
- Should the hook be a bool, or hand the runtime a cheap version counter it can compare itself
  (also useful to tiles)?
- **Tables and shared data sets will hold thousands of text cells.** The cache makes unchanged
  cells cheap; it does not make 100,000 cells cheap. Tables must **virtualize** — lay out and
  shape only visible rows plus a small buffer, with a fixed row height so off-screen rows need no
  measurement, and single-line cells (ellipsis) by default so a cell never wraps. That belongs in
  the tables design (pure typed DB; dashboards do the compute), not here — noted so the cache is
  not mistaken for the answer. Others do the same: AG Grid, Sheets and Excel shape only what is
  visible.
- Cache cap: two generations bound it by what a screen shows, but a huge unvirtualized list still
  holds it all for a frame. Add a hard entry cap only if a measurement asks for one.
