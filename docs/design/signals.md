# Signals — re-describe only what changed

Status: **design draft, 2026-10-04; nothing built.** Talked through in conversation; the two
probes behind it (§8) ran in a release build on the apps' Luau VM. `docs/status.md` says what is
real.

Companions:
- [Architecture](../architecture.md) — "immediate description + retained islands", which this
  extends rather than replaces.
- [2D world](2d-world.md) — the world `ui.group` first serves: thousands of small entities.
- [Workspace permissions](workspace-permissions-sync.md) — docs, whose edits (local or remote)
  become signal changes in §6.

## 1. End-to-end tests — done means these pass

Through the real path: Lua VM in `app_host`, the shell over the bridge, `DumpTree` for what an
agent sees. Written first, watched failing, then built until green.

| # | test | proves |
|---|---|---|
| T1 | `signal(v)` freezes `v` deeply; `s()` returns it; `s()[1].x = 5`, `table.insert(s(), …)`, `table.sort(s(), …)` each error with the signal's name in the message | a missed write is loud, never silent |
| T2 | `s:update(fn)` hands `fn` a writable shallow copy, freezes the result and stores it; `s:set(v)` replaces it; an unchanged `set` (same table) is a no-op | the two ways to write |
| T3 | Two groups read different signals; a handler updates one; the next view calls only that group's function (a counter per group) | only dirty groups re-run |
| T4 | A group that reads no signal re-runs every view | today's apps keep working unchanged |
| T5 | A clean group's output is reused: same UI nodes and handlers, clicks inside it still land | reuse is safe for interaction |
| T6 | A world with `ui.group("coins", …)` of 5000 entities and a hero: moving the hero re-runs neither the coins function nor their reconcile; picking a coin re-runs the coins group only | the world case, measured: describe cost with one coin picked < 1 ms in release |
| T7 | Writing a signal inside `view` or a group function is an error naming it | describing never changes state |
| T8 | Nothing a view reads changed (a hover elsewhere): `view` is not called at all | `view` is the root group |
| T9 | Dev check: a clean group is re-run on a sampled frame; if its output differs, the console says which group and which plain value it seems to depend on | state outside signals is caught, not silently stale |
| T10 | `DumpTree` lists groups: id, the signals each read, runs, last run tick | an agent can see why something did or did not update |
| T11 | Smoke: a world app over the bridge with 5000 coins; walking costs no coin re-describes (dump's run counters); a pickup costs one | the whole path, real socket and pixels |
| T12 | A doc read inside a group makes the group depend on that doc; a remote edit (second peer) re-runs it | docs and signals are one mechanism (§6) |

## 2. Why

Measured 2026-10-04, release build, 5000 small things in a world (`world/src/tests.rs`,
`five_thousand_small_things_cost_this_much`; `app_host/tests/probe_reactive.rs`):

- Lua building 5000 entity tables: **3.5 ms**. Rust diffing them (`reconcile`): **3.2 ms**.
  Physics for the same world: 0.76 ms a frame. Re-describing is the cost, not simulating.
- A world-wide re-describe happens on every `view`, and `view` runs after every handler — a coin
  pickup re-describes 4000 blades of grass that did not move.

Smarter diffing cannot help: by the time Rust compares, Lua has already built every table. The
skip must happen **before** Lua builds a part, which means knowing what that part depends on.

## 3. The model — Vue's, not SolidJS's

`view = f(state)` stays. What changes is how much of `f` re-runs:

- **A group is the unit of re-running.** `ui.group(id, fn)` marks a subtree; its function runs
  only when a signal it read last time has changed. Inside a dirty group the existing diff runs
  as now (the UI tree diff; the world's `reconcile` by id).
- **`view` is the root group.** If nothing it read changed, it is not called (T8).
- **Plain state still works.** A group that read no signal is dirty every view (T4) — exactly
  today's behaviour. Signals are opt-in where re-describing costs.

Rejected: SolidJS-style fine-grained bindings (no re-run, no diff; every changing prop its own
closure, `Show`/`For` for control flow). Harder to write — especially for an agent — and it
scatters the description through closures, so a frame stops being one inspectable description.
At our sizes (UI trees of hundreds of nodes) the diff is not the cost; building is.

Still immediate mode: Lua never holds widget objects or calls `setX`. It describes; Rust
retains what it already retained (worlds, frames, layout).

## 4. The Lua surface

```lua
local coins = signal({ { id = "c1", x = 40, y = 80 }, ... }, "coins")   -- name: for errors and the dump
local hero  = signal({ hp = 10 }, "hero")

-- read: one tracked call, then a plain, frozen table at full speed
for _, c in coins() do ... end

-- write, in handlers only
coins:update(function(list)               -- a writable shallow copy; items stay frozen
  table.insert(list, { id = uuid(), x = 5, y = 9 })
  list[3] = { id = list[3].id, x = 0, y = list[3].y }   -- change an item by replacing it
end)
hero:set({ hp = hero().hp - 1 })

-- describe
ui.world({ id = "map", width = 720, height = 400,
  player(hero()),                          -- small: part of the root group
  ui.group("coins", function()             -- re-runs only when `coins` changes
    local out = {}
    for _, c in coins() do out[#out + 1] = coin(c) end
    return out
  end),
})
```

Rules:
- **Values are frozen, deeply, on `signal` and `set`;** `update`'s result is frozen shallowly
  (its unchanged items already are). Errors name the signal: `signal "coins" is read-only here:
  change it with coins:update(fn) in a handler`.
- **Tracking is per signal**, not per field: one record per read call. A group that needs
  finer grain is split into smaller groups (e.g. coins by map chunk).
- **Reads are tracked only while a group (or `view`) runs.** Handlers read freely, untracked.
- **Writes happen in handlers.** Writing during `view` or a group function is an error (T7).
- **`ui.group` works in both trees:** among `ui.*` children it returns UI nodes; among a world's
  entities it returns entity tables. The id is unique among its siblings, like any keyed child.
- **A signal may hold any Lua value;** `set` with the identical table is a no-op, as an
  unchanged doc write is.

## 5. The Rust side

- **Tracking:** a stack of running groups. A signal read (`__call`) adds the signal to the top
  group's read set. A write bumps the signal's version and marks each group that read it.
- **Reuse:** a clean group's last output is kept — for the UI tree the built nodes and their
  handler registrations (handlers are keyed per view today, so reuse must carry them over: the
  main risk, T5); for a world the group's `EntitySpec`s, which `reconcile` skips as a block.
- **Group identity** is its path of ids from the root, so the same `"coins"` in two worlds are
  two groups.
- **The dev check (T9):** on a sampled frame (dev builds, or a bridge flag) one clean group
  re-runs and its output is compared with the kept one; a difference is a console note, not an
  error. This is what catches state kept outside signals.
- **Inspection (T10):** each group's id, the signal names it read, run count and last tick go in
  `DumpTree`, beside the world's `tick`.

## 6. Docs are signals

A doc mirror is already a signal in all but name: reads are plain tables, writes are explicit
(`board:set`, `:insert`), and Rust knows each change, local or remote. So:

- reading a doc mirror inside a group records the doc as read (per doc, matching per-signal
  grain); a write or a synced remote edit marks those groups dirty (T12);
- mirrors become frozen too, so `board.cards[1].text = "x"` errors. Today a mirror is a plain
  table Rust patches in place (`app_host/src/crdt.rs`, `patch_into`), so a stray write sticks in
  the mirror, is never saved, and can be overwritten. Freezing it means Rust lifts the read-only
  flag while it patches (`Table::set_readonly`, already used in `app_host/src/index.rs`).

One mechanism serves local state, saved state and other people's edits.

## 7. Steps

1. `signal`: freeze, read, `set`, `update`, errors (T1, T2, T7). Lua-only change in `app_host`.
2. `ui.group` in worlds: tracking, dirty sets, spec reuse in `reconcile` (T3, T4, T6). The
   measured case first.
3. `ui.group` in the UI tree with handler reuse (T5).
4. `view` as the root group (T8).
5. Inspection and the dev check (T9, T10), then the smoke (T11).
6. Docs as signals, frozen mirrors (T12).

Each step leaves `cargo test` and the smokes green.

## 8. Probes — what was tried, 2026-10-04

**Transparent tables (rejected).** A `reactive(t)` stand-in with `__index`/`__newindex`/`__len`/
`__iter` tracked nested writes, `#` and the generalised `for` — but `ipairs` silently looped
zero times, and `table.insert`/`remove`/`sort`/`concat`, `pairs` and `rawset` silently acted on
the empty stand-in (an insert was lost into it). Tracked reads cost 22 ms for 5000 coins against
0.24 ms plain. Silent wrong behaviour is the worst case for agent-written code.

**Rust userdata (loud, slow).** State kept in Rust: `ipairs`, `pairs`, `table.insert` and writes
each **error** — loud, good — but reading 5000 coins cost 3.8–4.8 ms.

**Frozen tables (chosen).** `table.freeze`d values: reads at plain speed (0.14 ms for 5000),
and `table.insert`, `remove`, `sort`, `rawset` and field writes all error with "attempt to
modify a readonly table". `table.clone` gives a writable copy whose items stay frozen; copying
5000, inserting one and freezing costs 0.07 ms.

## 9. Open questions

- Per-item signals for very large lists, or chunked groups? Start with chunked groups.
- Does `ui.group` need a `key` escape hatch for state that cannot be a signal (a host value)?
- Should frozen mirrors (§6) wait for step 6, or land with step 1 as a safety fix of its own?
