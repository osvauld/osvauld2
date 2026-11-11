-- Every attack's frame data (docs/design/brawl-fighting.md §1, "Starting frame data"). Ticks are
-- 1/120 s and 1-based within the move: startup ticks 1..startup, then active, then recovery; a
-- window's `from`/`to` are move ticks. Angles are degrees counter-clockwise from the attacker's
-- facing, y up (300 is down and forward). `base` and `growth` (per percent) are on the 1/64 grid.
-- A circle rides `part`: `at` is a point of the robot's setup drawing on that part (drawing units,
-- the robot facing right), `r` its radius in world units. scripts/bake_brawl_moves.lua carries it
-- through the move's clip into moves_baked.lua, which the rules read; rebake after any edit to this
-- file or a move clip (the hitbox test fails until you do). Heavies ride the blade, `sword_forward`.
-- Dodges are not moves (combat.lua, tune.lua); the jet's tank is jet.lua's, after its ignite here.
return {
	nlight = { clip = "nlight", startup = 10, active = 6, recovery = 20,
		windows = { { from = 11, to = 16, dmg = 8, base = 5, growth = 1 / 16, angle = 30,
			circles = { { part = "palm_back", r = 20, at = { 252.5, 189 } } } } } },
	slight = { clip = "slight", startup = 12, active = 6, recovery = 24, lunge = { from = 1, to = 12, speed = 6 },
		windows = { { from = 13, to = 18, dmg = 10, base = 5.5, growth = 1 / 16, angle = 25,
			circles = { { part = "palm_back", r = 22, at = { 252.5, 189 } } } } } },
	dlight = { clip = "dlight", startup = 10, active = 6, recovery = 22, lunge = { from = 1, to = 8, speed = 4 },
		windows = { { from = 11, to = 16, dmg = 8, base = 4.5, growth = 1 / 16, angle = 15,
			circles = { { part = "hoof-back", r = 20, at = { 232, 231 } } } } } },
	nheavy = { clip = "nheavy", startup = 22, active = 8, recovery = 36,
		windows = { { from = 23, to = 30, dmg = 18, base = 7, growth = 11 / 64, angle = 80,
			circles = { { part = "sword_forward", r = 30, at = { 369.7, 147 } }, { part = "sword_forward", r = 26, at = { 318.4, 167 } } } } } },
	sheavy = { clip = "sheavy", startup = 24, active = 8, recovery = 40, lunge = { from = 12, to = 30, speed = 8 },
		windows = { { from = 25, to = 32, dmg = 20, base = 7, growth = 11 / 64, angle = 40,
			circles = { { part = "sword_forward", r = 30, at = { 295.1, 176.1 } }, { part = "sword_forward", r = 24, at = { 365, 148.8 } } } } } },
	dheavy = { clip = "dheavy", startup = 20, active = 10, recovery = 40,
		windows = { { from = 21, to = 30, dmg = 16, base = 6.5, growth = 9 / 64, angle = 60,
			circles = { { part = "sword_forward", r = 26, at = { 318.4, 167 } }, { part = "palm_front", r = 26, at = { 149.5, 190 } } } } } },
	nair = { clip = "nair", air = true, startup = 8, active = 12, recovery = 16, land_lag = 6,
		windows = {
			{ from = 9, to = 12, dmg = 4, base = 4.5, growth = 0, angle = 70,
				circles = { { part = "palm_back", r = 26, at = { 252.5, 189 } }, { part = "palm_front", r = 26, at = { 149.5, 190 } } } },
			{ from = 17, to = 20, dmg = 5, base = 4.5, growth = 1 / 16, angle = 50,
				circles = { { part = "palm_back", r = 26, at = { 252.5, 189 } }, { part = "palm_front", r = 26, at = { 149.5, 190 } } } },
		} },
	sair = { clip = "sair", air = true, startup = 10, active = 6, recovery = 20, land_lag = 8,
		windows = { { from = 11, to = 16, dmg = 9, base = 5, growth = 7 / 64, angle = 35,
			circles = { { part = "hoof-back", r = 22, at = { 232, 231 } } } } } },
	dair = { clip = "dair", air = true, startup = 10, active = 8, recovery = 20, land_lag = 10,
		windows = { { from = 11, to = 18, dmg = 8, base = 4, growth = 1 / 16, angle = 300,
			circles = { { part = "hoof_front", r = 22, at = { 192, 231 } }, { part = "hoof-back", r = 22, at = { 232, 231 } } } } } },
	shair = { clip = "shair", air = true, startup = 14, active = 8, recovery = 30, land_lag = 14, lunge = { from = 10, to = 22, speed = 9 },
		windows = { { from = 15, to = 22, dmg = 15, base = 6.5, growth = 9 / 64, angle = 35,
			circles = { { part = "sword_forward", r = 30, at = { 295.1, 176.1 } }, { part = "sword_forward", r = 24, at = { 365, 148.8 } } } } } },
	-- Stalls in the air for its startup (vy = 0), then falls at `fall` until it lands (at most its
	-- active ticks); landing gives land_lag.
	dhair = { clip = "dhair", air = true, startup = 14, active = 60, recovery = 0, land_lag = 18, stall = true, fall = 16, until_land = true,
		windows = { { from = 15, to = 74, dmg = 14, base = 6, growth = 3 / 32, angle = 300,
			circles = { { part = "hoof_front", r = 24, at = { 192, 231 } }, { part = "hoof-back", r = 24, at = { 232, 231 } } } } } },
	-- The jet's ignite (heavy in the air, neutral or Up): vy = -jet_kick on the press (jet.lua), a rising
	-- uppercut with the near fist.
	jet = { clip = "jet", air = true, startup = 4, active = 8, recovery = 0, land_lag = 6,
		windows = { { from = 5, to = 12, dmg = 9, base = 6, growth = 5 / 64, angle = 85,
			circles = { { part = "palm_front", r = 24, at = { 149.5, 190 } } } } } },
}
