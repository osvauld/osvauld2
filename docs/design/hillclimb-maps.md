# Hillclimb maps — courses an agent writes, art from any tool

**Integration revision 2026-11-10:** The implemented Lua map support is integrated into dev. Combined verification is recorded in [the integration plan](game-branch-integration.md).

Status: **plan agreed 2026-11-05; steps 1–3 built 2026-11-05, not landed.** Found while building
step 3: worlds outlive hot reload, so the world id carries a fingerprint of the course (§1.7). Worktree `.claude/worktrees/hillclimb`,
branch `hillclimb`. Follows [Hillclimb](hillclimb.md) and [SVG drawings](svg-drawings.md).

Scope (user, 2026-11-05): **agents draw the map themselves; Lua is the way.** Assets come in
from other tools (SVG today). Tiled was considered and dropped: it is a tool for people, and a
converter to this format can come later if wanted.

Today the course is code: `Stage.build(seed)` makes one stage from a seed. After this, a course is
a **map file**, `maps/<name>.lua`. It is plain data that an agent writes and edits over the
bridge: the ground's shape, the start and finish, where the fuel cans and coins go, and which
SVG pieces of scenery stand where. The seeded generator stays, as one way to *produce* a map.

No Rust changes. Everything here is app Lua and app files.

## 0. End-to-end tests (written first, red until their step lands)

`P` = pure Lua under stock `lua` (`scripts/test_hillclimb_maps.lua`); `S` = bridge smoke over
an offscreen shell (`scripts/smoke_hillclimb_maps.py`, added to `SMOKES` once green).

| id | proof | step |
|---|---|---|
| P-M1 | `Stage.check(map)` passes a good map. It refuses each bad one with an error naming the field and, for ground, the point: an unknown field; fewer than 2 ground points or an odd count; x not increasing; a first x other than 0 or an x off the half metre; a slope over 40° after smoothing (names the metre); start or finish off the ground; a fuel can or coin row off the ground; scenery whose art isn't a `.svg` name | 1 |
| P-M2 | `Stage.from_map(map)` gives a line with a point every 0.5 m that passes through every control point exactly and never overshoots (between two control points, y stays within their range). Items sit at their x, the given height above the ground. `finish_x` is the map's finish | 1 |
| P-M3 | the seeded stage survives as a map: `Stage.from_map(Stage.generate(7))` has the same points (to 1e-6 units), start, finish and items as the old `Stage.build(7)`; the old P-S1 checks run on it | 1 |
| S-M1 | upload Hillclimb and open it: the console is clean. The default map is the hand-written `maps/green_hills.lua`. The car rests on its start, the dump has its scenery entities, and the screenshot shows them. `Map: …` switches to the seeded map and back | 2 |
| S-M2 | live edit: `read_file_versioned` + one exact `edit_file` raises the map's highest control point by 5 m. After the reload the console is clean and the ground entity's `pos` y is exactly 500 units higher (it sits at the line's top) | 3 |
| S-M3 | a broken edit (one ground x moved behind its neighbour) puts `Stage.check`'s error, naming the point, on the console. Fixing it returns a clean console and the car | 3 |

`smoke_hillclimb.py` (S-1..S-6) keeps passing unchanged in its asserts. It clicks `Map: …` once
at the start to drive the seeded map it was written for.

## 1. The map format (decisions)

```lua
-- maps/green_hills.lua
return {
	name = "Green Hills",
	-- Control points, metres: x along the road, height up. Smoothed into the road (below).
	ground = { 0, 0,  15, 0,  30, 2,  45, -1,  70, 6,  … },
	start = 3,                         -- metres along; the car spawns just above the ground here
	finish = 480,                      -- metres along
	fuel = { 60, 180, 320 },           -- a can at each x, floating 0.6 m up
	coins = { { 20, 5 }, { 90, 8 } },  -- rows: first x, count (0.6 m apart, 0.7 m up)
	scenery = {                        -- SVG pieces standing on the ground, bottom centre at x
		{ "art/tree.svg", 25 },
		{ "art/rock.svg", 52, front = true },  -- in front of the car; default behind the road
	},
}
```

1. **Metres, height up.** That's how an agent or a person thinks about a hill. The loader turns
   it into world units (100 to the metre, −y up). Nothing outside `stage.lua` changes:
   `from_map` returns the same stage table `Stage.build` did (`points`, `step`, `start`,
   `finish_x`, `items`, `items_by_id`), plus `scenery`.
2. **Control points, smoothed.** The agent writes a point every 5–30 m, not every 0.5 m. The
   loader resamples them to the 0.5 m line with a monotone cubic (Fritsch–Carlson). It goes
   through every point and never overshoots, so a hill is never higher than the agent wrote.
   `smooth = false` keeps straight lines between points (the generator uses it, because its
   points are already 0.5 m apart). Ground x starts at 0 and every x is on the half metre, so
   the 0.5 m line holds each control point exactly *(added while building step 1)*.
3. **Strict.** Unknown fields and bad values are errors that say which field and which point,
   in metres. The console is the agent's feedback loop (S-M3).
4. **A map file is Lua.** It can compute: `return require("stage").generate(7)` is the seeded
   map (`maps/seeded.lua`). Agents can hand-place points or write a loop.
5. **Scenery is SVG**, through `gfx.svg`. Each piece is drawn at its SVG size with its bottom
   centre on the ground at x. Each is a scenery entity with no body: listed behind the ground
   (so the grass covers its foot), or after the car with `front = true`. The art comes from
   any tool or pack, or an agent writes the SVG itself (the first pieces are agent-drawn).
6. **Picking.** `main.lua` lists the maps (`MAPS = { "maps/green_hills", "maps/seeded" }`) and a
   `Map: <name>` button cycles them, like `Car:`. A Lua app can't list its own files, and an
   explicit list is clearer anyway.
7. **An edited map is a new world** *(added 2026-11-05, step 3)*. A `ui.world` outlives hot
   reload and never moves an entity that already exists, so after a live edit the old ground
   stayed put whenever the reloaded app happened to describe the same world id. S-M3 caught it.
   The world id now carries a fingerprint of everything the map spawns: the ground line, the
   pickups and the scenery positions.

## 2. Steps

1. **Map → stage.** `stage.lua` gains `check`, `from_map` and `generate` (the old generator,
   returning a map). `Stage.build` goes away. `maps/seeded.lua` exists, `main.lua` loads it,
   and the pure tests P-M1..P-M3 pass. `smoke_hillclimb.py` stays green and deterministic.
2. **A hand-made map with scenery.** `maps/green_hills.lua`, written by an agent: about 500 m,
   gentle at first, with a few jumps and one real climb. Agent-drawn `art/tree.svg`,
   `art/rock.svg` and `art/flag.svg` (start and finish). Scenery entities and the `Map:`
   button. S-M1.
3. **Live editing.** `smoke_hillclimb_maps.py` S-M2, S-M3; added to `SMOKES`.

When it lands: `docs/status.md` gets a "Hillclimb maps" entry. This doc's status line says
what's real.
