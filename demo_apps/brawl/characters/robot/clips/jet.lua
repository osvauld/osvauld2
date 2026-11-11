-- The jet's ignite (the move `jet`, 12 ticks): a rising uppercut with the near fist, from low in front
-- (startup, move ticks 1-4) up past the visor (active 5-12), boots tucked, the flame bursting out of
-- the pack. The chassis stays upright, so the flame points down.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 12 / 120
local function hold(v) return { { 0, v } } end
return A.clip({
	length = L,
	keys = {
		body = { y = { { 0, 4 }, { 2 * F, -2, e }, { L, -3, e } }, rot = { { 0, 3 }, { 2 * F, -2, e }, { L, -1, e } } },
		bone = { y = { { 0, -4 }, { 2 * F, -8, e }, { L, -9, e } } },
		leg_front = { rot = { { 0, 6 }, { 3 * F, 14, e }, { L, 12, e } } },
		hoof_front = { rot = hold(-6) },
		leg_back = { rot = { { 0, 10 }, { 3 * F, 18, e }, { L, 16, e } } },
		["hoof-back"] = { rot = hold(-8) },
		shoulder_back = { rot = { { 0, -40 }, { 3 * F, -60, e }, { L, -55, e } } },
		arm_back = { rot = { { 0, -20 }, { L, -30, e } } },
		flame = { scale_y = { { 0, 3 }, { F, 3.8 }, { 2 * F, 3.4 }, { 3 * F, 4 }, { 4 * F, 3.5 }, { 5 * F, 3.9 }, { L, 3.6 } } },
	},
	ik = {
		arm_front = { bend = -1, { 0, 176, 182 }, { 2 * F, 200, 130, e }, { 4 * F, 214, 84, e }, { L, 208, 74, e } },
	},
})
