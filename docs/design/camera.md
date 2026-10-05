# Camera — a world bigger than its box, drawing only what is on screen

**Status (2026-10-05):** built, all four steps: `world/src/camera.rs` (C1–C4, C6), the Lua
surface in `app_host` (C5, C7–C9, `app_host/src/tests/camera.rs`), both B7 benches, the dump's
`camera`, `smoke_camera.py` (C10) with `demo_apps/camera`, and the guide's Camera section. The
contract with the 3D session (follow, `ease`,
`set_camera`, lost targets, reload) was agreed 2026-10-04; §2 notes one revision.

Today a world draws its whole `width × height`, and its frame holds every entity: 20000
entities cost 3.4 ms to build (`world_frame_20000`, [perf-benchmarks.md](perf-benchmarks.md)).
An RPG map is many screens wide. The camera picks which part of the world the box shows, follows
an entity there, and the frame holds only what the box can show — so a big map costs about one
screen.

## 1. Tests — what proves it done

World tests (`world/src/tests.rs`) unless marked; app tests drive a real Lua VM; the smoke drives
the shell over the bridge.

| # | Test | Proves |
|---|---|---|
| C1 | A camera following the hero on a 4000×3000 map shows the hero at the box's centre from the first frame; the frame's items sit at world position − camera | following, and no ease in from (0, 0) |
| C2 | `ease = 8`: one step closes `1 − exp(−8/120)` of the gap; the same second ends at the same camera at 30, 60, 144 and 240 fps | time-based easing on the fixed step |
| C3 | `bounds = { 0, 0, 4000, 3000 }`: at the map's corner the box stops at the edge; a map smaller than the box is centred | bounds |
| C4 | The followed entity despawns: the camera holds, `lost = true`; it comes back: the camera eases to it again | a lost target is not an error |
| C5 | (app) `world("map"):set_camera({ at = { x, y } })` clears `follow`; `{ follow = "boss" }` follows again; a changed `camera` in the description applies, an unchanged one does not undo a `set_camera`; refused in `view` | commands, last change wins |
| C6 | 20000 entities, a 1280×720 box: the frame holds only those whose box meets the view (plus margin); one half on screen is drawn; one off screen is neither posed nor drawn | culling |
| C7 | (app) a click at a screen point reports the entity under it; `to_world` and `to_screen` round-trip; `at(to_world(p))` finds what was clicked | hits and questions through the camera |
| C8 | (app) hot reload keeps the camera where it is and takes `follow`, `ease`, `bounds` from the new description | reload |
| C9 | (app) strict: an unknown `camera` field, `ease <= 0`, a malformed `bounds`, `follow` with `at` are errors naming the field | the Lua surface is strict |
| C10 | (smoke, `smoke_camera.py`) offscreen shell: walk the hero right with keys; pixels show it staying centred while the ground moves; the dump's `camera.at` advances and `drawn` < entity count; a click via `rects()` hits the entity drawn there | the real path |
| B7 | `world_frame_20000_camera` and `app_walk_20000_camera` in `bench.py` | the gain, with budgets |

## 2. The Lua surface

```lua
ui.world({
	id = "map", width = 1280, height = 720,           -- the box: what is shown
	camera = { follow = "hero", ease = 8, bounds = { 0, 0, 4000, 3000 } },
	...
})
world("map"):set_camera({ at = { 2000, 1500 } })      -- look here (clears follow)
world("map"):set_camera({ follow = "boss" })
local wx, wy = table.unpack(world("map"):to_world({ e.x, e.y }))
local sx, sy = table.unpack(world("map"):to_screen({ wx, wy }))  -- a speech bubble over an NPC
```

- `width`/`height` stay the box, as today; a world without `camera` is unchanged (it shows
  `0, 0` to `width, height`).
- The camera's `at` is the world point at the box's centre. `follow` aims at the target's
  drawing-box centre; without `ease` it is exact.
- **Revision of the 3D agreement:** `bounds = true` ("the world's rectangle") has nothing to mean
  once the box is not the world, so bounds are an explicit `{ x, y, w, h }`, as 3D already
  required. Without bounds the camera goes anywhere.
- `to_world` and `to_screen` are questions: allowed in `view` and handlers.
- Not now: zoom, rotation, shake, several cameras on one world.

## 3. In Rust

- Camera state lives in `World2d` beside the timers: `at`, `follow`, `ease`, `bounds`, `lost`,
  and the last described spec (so an unchanged description does not undo a `set_camera`).
- Follow moves on the fixed step (`alpha = 1 − exp(−ease · STEP)`), clamped to bounds; the frame
  reads it. Dumps and screenshots never move it. No interpolation between steps, as entities.
- `frame(width, height)` wraps its items in one `Item::group` translated by the camera and
  skips any entity whose placed box (`transform_rect_bbox` of its drawing box) misses the view
  grown by a margin — before posing it, which is the cost culling saves. Margin: half the
  entity's own size each side, so a swung sword is not cut off; a pose reaching further may pop
  at the edge.
- Hits need nothing new: the frame's ids are still entity ids, and the group's transform is
  undone by the runtime's hit path.
- Inspection gains `camera = { at, follow, lost, ease, bounds }` and `drawn` (entities in the
  last frame).

## 4. Steps

1. World: camera state, follow on the step, bounds, lost (C1–C4). ✅
2. World: the frame through the camera, and culling (C6, B7's world bench). ✅
3. app_host: `camera` in the description, `set_camera`, `to_world`, `to_screen`, strictness,
   reload (C5, C7–C9, B7's app bench). ✅
4. The dump, `smoke_camera.py` (C10), the guide's camera section, status. ✅

### Notes from steps 1–2 (2026-10-05)

- Culling applies to every world, not only one with a camera: a world without one draws only
  what meets its own box. So `world_frame_5000`/`_20000` now draw a box the size of the whole
  map, to stay the "everything drawn" reference; `app_walk_*` (a 400 × 300 box over 2000 × 1000
  of coins) gets cheaper and its baselines need re-recording.
- Measured, release: 20000 entities, all drawn, 4.3 ms; through a 1280 × 720 camera (~6%
  drawn), 0.9 ms. Most of that 0.9 ms is checking every entity against the view (place and
  bounding box). A spatial grid would make it proportional to what is on screen; not yet needed.
- The test that an off-screen entity is not posed is the bench, not an assertion: posing is
  internal and has no count to read.
- Breaking the easing on purpose fails C2: the easing tests catch it.
- Step 3: a frame of walking past 20000 grouped coins through a 1280 × 720 camera is 1.8–2.0 ms
  (`app_walk_20000_camera`), at the 2 ms target with no room; the per-entity view check is the
  lever. (A first reading of 2.8 ms was taken on a loaded machine.)
- A question answers as of the last step, so `at(to_world(p))` finds a just-spawned collider
  only after a tick — as the guide already says for spawns.

## 5. Decisions (confirmed 2026-10-05)

1. Bounds are an explicit rectangle, not `true` (§2) — tell the 3D session.
2. `to_world`/`to_screen` as methods, rather than adding world coordinates to pointer events.
3. The cull margin: half an entity's size each side.
