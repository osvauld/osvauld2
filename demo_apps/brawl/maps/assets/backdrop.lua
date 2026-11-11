-- Wide layers behind the stage. A `layer` is tiled across and its `body` colour continues it down;
-- `src` crops the silhouette's solid lower rows (they read as a wall past the stage edge). Files are the pack's names (decision 10).
return {
	mountain_far = { kind = "layer", image = "art/background_mountain_1.png", size = { 1025, 405 }, src = { 0, 0, 1025, 300 },
		body = "#0d949b", layer = "far" },
	mountain_near = { kind = "layer", image = "art/background_mountain_2.png", size = { 1025, 241 }, src = { 0, 0, 1025, 200 },
		body = "#0d949b", layer = "mid" },
	hedge = { kind = "layer", image = "art/foreground_basic_1.png", size = { 1025, 593 }, src = { 0, 0, 1025, 360 },
		body = "#337830", layer = "near" },
	-- The rest of the depth elements (step 3), cropped to their opaque top in the file. Their `body`
	-- continues the silhouette downward. The fogs are the pack's white masks (they read as a white wall
	-- at full opacity: step 4 draws them with element opacity); the greens are darker the nearer.
	foreground_basic_2 = { kind = "layer", image = "art/foreground_basic_2.png", size = { 1025, 377 }, body = "#337830", layer = "mid" },
	foreground_basic_3 = { kind = "layer", image = "art/foreground_basic_3.png", size = { 1025, 443 }, body = "#3e933b", layer = "mid" },
	background_fog_1 = { kind = "layer", image = "art/background_fog_1.png", size = { 1025, 518 }, body = "#ffffff", layer = "mid" },
	background_fog_2 = { kind = "layer", image = "art/background_fog_2.png", size = { 1025, 378 }, body = "#ffffff", layer = "mid" },
	background_fog_3 = { kind = "layer", image = "art/background_fog_3.png", size = { 1025, 442 }, body = "#ffffff", layer = "mid" },
	-- Translucent sky colour over the layers behind it: the depth cue, since pictures have no tint.
	haze_day = { kind = "haze", color = "#d6ecec", from = 0, to = 0.55, height = 320 },
	-- Over the far range: from clear above its peaks (no edge in the sky) to well toward the sky at its foot.
	haze_far = { kind = "haze", color = "#cde7ee", from = 0, to = 0.62, height = 300, layer = "far" },
	-- Below the stage's top: mist from clear to opaque, so past the stage's edge reads as depth, not sky.
	haze_abyss = { kind = "haze", color = "#dcebe6", from = 0, to = 1, height = 300, layer = "near" },
	-- Far hills the pack lacks: rounded, pale, in the sky's own blue (vector, depth.lua's ridge).
	ridge_pale = { kind = "ridge", color = "#9fcbd8", h = 240, step = 260 },
	-- Golden hour (Twin Isles): the same three jobs as the day hazes, in warm colours over the same greens.
	haze_gold_far = { kind = "haze", color = "#eda98a", from = 0, to = 0.8, height = 300, layer = "far" },
	haze_gold = { kind = "haze", color = "#f2c69a", from = 0, to = 0.55, height = 320 },
	haze_abyss_gold = { kind = "haze", color = "#efc79f", from = 0, to = 1, height = 300, layer = "near" },
	ridge_dusk = { kind = "ridge", color = "#d9948a", h = 240, step = 260 },
}
