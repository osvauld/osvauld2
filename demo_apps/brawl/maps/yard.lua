-- The yard: plain data that only places assets (docs/design/brawl-maps.md decision 3). No pictures
-- and no computed numbers here: collision and art both come from these placements (map.lua).
-- +y is down; the ground's top is y = 0. `back` goes back to front (x is the centre, 0 when left out;
-- a `layer` or `haze` asset's y is its top, a prop's or cloud's y its anchor; f its depth); tiles, widths
-- and body depths are computed (decision 7). `pieces` is the map plane, in draw order, and its order is
-- also the order of the stage's solids and platforms (a fighter's `on` is an index into that list).
-- Platforms come after the props, as the yard drew them before maps.
return {
	name = "yard", sky = "day", home = { x = 0, y = -100 },
	back = { -- decision 8's recipe: each haze lowers the contrast of what is behind it
		{ asset = "ridge_pale", y = -520, f = 0.04 },
		{ asset = "cloud_soft_1", x = -560, y = -600, scale = 0.9, f = 0.03, drift = 5, span = 5000, opacity = 0.9 },
		{ asset = "cloud_soft_3", x = 1300, y = -680, scale = 0.6, f = 0.02, drift = 3, span = 5000, opacity = 0.8 },
		{ asset = "cloud_soft_2", x = 520, y = -520, scale = 0.75, f = 0.05, drift = 7, span = 5000 },
		{ asset = "mountain_far", y = -440, scale = 1.8, f = 0.1 },
		{ asset = "haze_far", y = -640, f = 0.12 },
		{ asset = "mountain_near", y = -250, scale = 1.6, f = 0.25 },
		{ asset = "background_fog_2", y = -230, scale = 1.4, f = 0.3, opacity = 0.35 },
		{ asset = "foreground_basic_3", y = -200, scale = 1.2, f = 0.45 },
		{ asset = "haze_day", y = -150, f = 0.47, opacity = 0.5 },
		{ asset = "hedge", y = -240, scale = 1.1, f = 0.75 },
		{ asset = "haze_abyss", y = 40, f = 0.75 },
	},
	pieces = {
		{ asset = "grass_block", id = "ground", x = -900, y = 0, w = 1800, h = 500 },
		{ asset = "tree", x = -790, y = 0, scale = 0.95 },
		{ asset = "tree_2", x = 770, y = 0, scale = 0.9 },
		{ asset = "bush", x = -330, y = 0, scale = 0.8 },
		{ asset = "rock", x = 400, y = 0 },
		{ asset = "grass_3", x = -200, y = 8, scale = 0.9 },
		{ asset = "grass_3", x = 540, y = 8, scale = 0.8 },
		{ asset = "wood_ledge", id = "left", x = -620, y = -260, w = 320, scale = 1.25 },
		{ asset = "wood_ledge", id = "right", x = 300, y = -260, w = 320, scale = 1.25 },
		{ asset = "wood_ledge", id = "top", x = -160, y = -520, w = 320, scale = 1.25 },
	},
	-- spawn 2 (2026-11-11, brawl-fighting.md): the dummy's and the second fighter's; spawn 1 is untouched.
	spawns = { { x = 0, y = 0 }, { x = 450, y = 0 } }, respawn = { x = 0, y = -700 },
	blast = { x = -2100, y = -1700, w = 4200, h = 2800 }, camera = { x = -1500, y = -1100, w = 3000, h = 1800 },
}
