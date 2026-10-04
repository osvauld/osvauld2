# Lua app tests — app-shipped tests, run beside the app

Status: **implementation in progress, 2026-10-01.** Built so far: the bridge discovers
`tests/*.lua`, opens a non-persisting temporary app tab, runs each file in a separate test VM with
`t.expect`, `t.step`, `t.world`, `t.rects`, `t.centre_of`, `t.click_at` and `t.text`, and returns
structured pass/fail results. Keyboard and text-input helpers remain unbuilt.

## 1. Why

Today app tests live outside the app: Python smokes drive the shell over the bridge, probe pixels,
and read app text. That remains necessary for rendering and hit-testing, but it is the wrong place
to express app behaviour. Once `DumpTree` exposed `world.entities` as data, a smoke caught a chest
sleeping half inside a wall — a bug pixels did not show.

The next step is assertions written in Lua, shipped with the app source, and run by the host. The
agent that writes `main.lua` can write `tests/*.lua` in the same language and folder.

## 2. Boundary

Tests are **inside the app package**, not inside the app VM.

- `tests/*.lua` is part of the app source doc and uploads/syncs with it.
- Each test file runs in a separate sandboxed test VM.
- The app under test runs normally in its own VM and is driven through the same input/frame paths a
  player uses.
- The test VM cannot read app locals, call `ui`/`gfx`, or mutate app state directly. It only sends
  player inputs, steps time, reads host snapshots, and asserts.

This keeps the test behavioural: it can check what a player can cause plus what the host can inspect
(`DumpTree`, world snapshots, console), but it cannot pass by peeking at `local carrying`.

## 3. Freshness

Once a test needs app interaction (`step`, `world`, input), `RunTests` never uses the currently open
tab. Each such test gets a temporary app instance built from the same source revision as the item
under test. Slice 1 tests that only call `t.expect` do not instantiate the app yet.

Default state is empty documents and fresh per-viewer state under a distinct test item/run id
namespace, never the live tab's item id. Persisted user docs are not copied into tests unless a
later fixture mechanism asks for them explicitly. This makes tests order-independent and prevents a
manual session from changing the result. Fixtures are deferred; the first shape should be explicit
in the test file, for example `setup(t)` plus `test(t)`.

## 4. Test API, first shape

A test file returns one function:

```lua
return function(t)
	t.hold("KeyS", 1.0)
	t.tap("KeyE")
	local w = t.until(function(w) return w.room.chest.body == "fixed" end, 600)
	t.expect(w.room.chest.pos[2] + 80 <= 388, "the chest rests above the south wall")
end
```

- **Act:** `t.tap(code)`, `t.hold(code, seconds)`, and `t.click_at(x, y)` for hit-tested logical
  screen coordinates. `t.rects()` / `t.centre_of(id)` provide those coordinates; raw numbers are
  only for intentional world/canvas points, never guessed. Direct handler dispatch by id is not in
  the first test API because it can pass when the UI is not touchable. Keyboard helpers name
  physical key codes first; text input remains a separate later helper.
- **Step:** `t.step(frames)` advances the virtual clock; `t.until(pred, max_frames)` reads a world
  snapshot, calls `pred(w, frame)`, then steps one frame until the predicate returns truthy or the
  budget fails; it returns the successful snapshot.
- **Look:** `t.world()` returns the current world snapshot as read-only plain data;
  `t.text(id)` and `t.console()` read non-world app evidence.
- **Check:** `t.expect(cond, message)` fails the test immediately.

Snapshot tables are read-only, or copies whose mutation is discarded before the next read. Watching
must never change what it watches.

## 5. Running

Bridge request: `RunTests { item_id, filter? }`.

**Built:** it loads matching `tests/*.lua` from the item's source doc, opens a non-persisting
temporary app tab with empty docs and a distinct `test:<item>:<run>` retained-id namespace, runs each
file in an isolated source-only test VM with `t.expect`, `t.step`, `t.world`, `t.rects`,
`t.centre_of`, `t.click_at` and `t.text`, then removes the tab and returns `[{ name, ok, frames, failure? }]`.
`t.step(n)` and pointer helpers cross to the runtime Runner through the same deferred driver path as
the bridge's `Frame`/pointer ops; `t.world()` reads the temporary app's current world inspection as
Lua tables. Failures carry the Lua error string only.

**Future behavioural runner:** once `step`, snapshots or input land, each test runs against a fresh
temporary app instance and failures also carry frame number, console tail and the latest snapshot. A
Python smoke calls `RunTests` once so `scripts/smoke.py` can run app-shipped tests without writing
per-test Python.

A faster non-shell runner is deferred. It must still use the runtime/headless driver around the app,
not `app_host` alone, because layout, input routing, world ticking and screenshots are runtime
concerns.

## 6. Slices

1. **Built:** discover `tests/*.lua`, open a non-persisting temporary app tab, run each test in a
   separate source-only test VM, expose `t.expect`, Runner-driven `t.step`, `t.rects`, `t.centre_of`,
   `t.click_at`, `t.text`, and `t.world`, and return structured pass/fail results over the bridge.
   The test VM runs on a worker; each step/click is a deferred driver op.
2. Add console snapshots and keyboard/text-input helpers.
3. Add keyboard/click helpers and port carry/drop/wall checks from `scripts/smoke_world.py`.
4. If needed, add explicit fixture-only world moves (`push`/`place`) through physics, before the
   behavioural phase of a test, not as assertions' hidden cause.
5. Add the faster runtime/headless runner for `cargo test`.

## 7. Open

- Fixture shape and whether a test can opt into copying an existing document.
- The exact result wire schema and frame/time budgets.
- How much non-world API (`t.text`, selectors, console matching) ordinary apps need.
