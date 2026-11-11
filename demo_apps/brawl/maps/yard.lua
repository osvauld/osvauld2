-- The yard: plain data that only places assets (docs/design/brawl-maps.md decision 3). No pictures
-- and no computed numbers here: collision and art both come from these placements (map.lua).
-- +y is down; the ground's top is y = 0. `back` goes back to front (x is the centre; a `layer` asset's y is
-- its top, a prop's y its anchor); `pieces` is the map plane, in draw order, and its order is also the
-- order of the stage's solids and platforms (a fighter's `on` is an index into that list).
-- Platforms come after the props, as the yard drew them before maps. `bottom` is where a back layer's
-- body colour stops (step 4 replaces it with mist).
return {
	name = "yard", sky = "day", home = { x = 0, y = -100 }, bottom = 40,
	back = {
		{ asset = "cloud_2", x = -300, y = -495.4, scale = 0.6, f = 0.05 },
		{ asset = "cloud", x = 420, y = -468.15, scale = 0.55, f = 0.08 },
		{ asset = "cloud", x = -1000, y = -436.5, scale = 0.5, f = 0.07 },
		{ asset = "mountain_far", x = 0, y = -460, scale = 1.8, f = 0.12 },
		{ asset = "mountain_near", x = 0, y = -250, scale = 1.6, f = 0.4 },
		{ asset = "hedge", x = 0, y = -240, scale = 1.1, f = 0.6 },
	},
	pieces = {
		{ asset = "grass_ground", id = "ground", x = -900, y = 0, w = 1800, h = 500 },
		{ asset = "tree", x = -790, y = 0, scale = 0.95 },
		{ asset = "tree_2", x = 770, y = 0, scale = 0.9 },
		{ asset = "bush", x = -330, y = 0, scale = 0.8 },
		{ asset = "rock", x = 400, y = 0 },
		{ asset = "grass_3", x = -200, y = 8, scale = 0.9 },
		{ asset = "grass_3", x = 540, y = 8, scale = 0.8 },
		{ asset = "wood_ledge", id = "left", x = -620, y = -260, w = 320 },
		{ asset = "wood_ledge", id = "right", x = 300, y = -260, w = 320 },
		{ asset = "wood_ledge", id = "top", x = -160, y = -520, w = 320 },
	},
	spawns = { { x = 0, y = 0 } }, respawn = { x = 0, y = -700 },
	blast = { x = -2100, y = -1700, w = 4200, h = 2800 }, camera = { x = -1500, y = -1100, w = 3000, h = 1800 },
}
