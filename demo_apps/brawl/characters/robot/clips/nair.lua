-- The neutral air (nair, 36 ticks): tucked, both fists flung out to the sides on the first window
-- (move ticks 9-12), pulled in, flung out again on the second (17-20), then the apex pose.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 36 / 120
local function hold(v) return { { 0, v } } end
return A.clip({
	length = L,
	keys = {
		body = { rot = { { 0, 2 }, { 4 * F, -4, e }, { 6 * F, 4, e }, { 8 * F, -4, e }, { L, 2, e } } },
		bone = { y = hold(-6) },
		leg_front = { rot = hold(10) },
		hoof_front = { rot = hold(-6) },
		leg_back = { rot = hold(14) },
		["hoof-back"] = { rot = hold(-8) },
	},
	ik = {
		arm_back = { { 0, 236, 160 }, { 3 * F, 232, 150, e }, { 4 * F, 264, 140, e }, { 5 * F, 265, 138 }, { 7 * F, 236, 152, e },
			{ 8 * F, 266, 134, e }, { 9 * F, 266, 134 }, { L, 252, 150, e } },
		arm_front = { bend = -1, { 0, 172, 156 }, { 3 * F, 176, 150, e }, { 4 * F, 132, 140, e }, { 5 * F, 131, 138 }, { 7 * F, 168, 152, e },
			{ 8 * F, 130, 134, e }, { 9 * F, 130, 134 }, { L, 142, 150, e } },
	},
})
