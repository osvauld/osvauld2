-- The yard's collision data as it was before maps (demo_apps/brawl/stages/yard.lua, art left out):
-- P-K2 pins the compiled yard to this, field by field and in order. Do not edit it to follow the map.
return {
	name = "yard",
	solids = {
		{ id = "ground", x = -900, y = 0, w = 1800, h = 500 },
	},
	platforms = {
		{ id = "left", x = -620, y = -260, w = 320 },
		{ id = "right", x = 300, y = -260, w = 320 },
		{ id = "top", x = -160, y = -520, w = 320 },
	},
	-- spawn 2 added 2026-11-11 with the map's (brawl-fighting.md §2); spawn 1 is as it was.
	spawns = { { x = 0, y = 0 }, { x = 450, y = 0 } },
	respawn = { x = 0, y = -700 },
	blast = { x = -2100, y = -1700, w = 4200, h = 2800 },
	camera = { x = -1500, y = -1100, w = 3000, h = 1800 },
}
