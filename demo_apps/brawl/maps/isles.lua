-- Twin Isles: two cliffs and a bridge over a pit, golden hour. Same format as the yard: it only
-- places library assets (maps/assets/*.lua), and collision and art come from the same placements.
-- +y is down; the islands' tops are y = 0 and the pit between them (x -260..260) is open to the
-- blast zone, with a one-way bridge across it. `pieces` order is the order of the stage's solids and
-- platforms (the boulder's solid comes after the islands').
-- The boulder is the pack's grass-topped block (boulder_5) at 1.5: its picture is square, like its 156 x 156
-- solid. The round boulders read as tyres or holes beside a square box.
return {
	name = "isles", sky = "golden", home = { x = 0, y = -100 },
	back = { -- the yard's recipe in warm light: each haze lowers the contrast of what is behind it
		{ asset = "ridge_dusk", y = -520, f = 0.04 },
		{ asset = "cloud_soft_2", x = -700, y = -560, scale = 0.8, f = 0.03, drift = 4, span = 5000, opacity = 0.85 },
		{ asset = "cloud_soft_1", x = 900, y = -640, scale = 0.7, f = 0.04, drift = 6, span = 5000, opacity = 0.8 },
		{ asset = "mountain_far", y = -440, scale = 1.8, f = 0.1 },
		{ asset = "haze_gold_far", y = -640, f = 0.12 },
		{ asset = "mountain_near", y = -250, scale = 1.6, f = 0.25 },
		{ asset = "background_fog_2", y = -230, scale = 1.4, f = 0.3, opacity = 0.35 },
		{ asset = "foreground_basic_3", y = -200, scale = 1.2, f = 0.45 },
		{ asset = "haze_gold", y = -150, f = 0.47, opacity = 0.5 },
		{ asset = "hedge", y = -240, scale = 1.1, f = 0.75 },
		{ asset = "haze_abyss_gold", y = 40, f = 0.75 },
	},
	pieces = {
		{ asset = "grass_block", id = "isle_l", x = -1100, y = 0, w = 840, h = 600 },
		{ asset = "grass_block", id = "isle_r", x = 260, y = 0, w = 840, h = 600 },
		{ asset = "bush_2", x = -1030, y = 0, scale = 0.7 },
		{ asset = "tree_1", x = -940, y = 0, scale = 0.8 },
		{ asset = "sign", x = -820, y = 0 },
		{ asset = "boulder_5", id = "boulder", x = -480, y = 0, scale = 1.5 },
		{ asset = "bush_3", x = 340, y = 0, scale = 0.8 },
		{ asset = "rock_1", x = 560, y = 0 },
		{ asset = "tree", x = 960, y = 0, scale = 0.85 },
		{ asset = "grass_1", x = -620, y = 8, scale = 0.9 },
		{ asset = "grass_4", x = 700, y = 8, scale = 0.9 },
		{ asset = "bridge", id = "bridge", x = -260, y = 0, w = 520 },
		{ asset = "wood_ledge", id = "ledge_l", x = -900, y = -280, w = 280, scale = 1.25 },
		{ asset = "wood_ledge", id = "ledge_r", x = 620, y = -280, w = 280, scale = 1.25 },
		{ asset = "platform_1", id = "top", x = -160, y = -420, w = 320, scale = 1.25 },
	},
	spawns = { { x = -700, y = 0 }, { x = 700, y = 0 } }, respawn = { x = 0, y = -700 },
	blast = { x = -2200, y = -1700, w = 4400, h = 2800 }, camera = { x = -1500, y = -1100, w = 3000, h = 1800 },
}
