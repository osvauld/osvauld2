-- The jet cutting out (10 ticks; the flame is out, look.lua hides it): a jolt down, arms flung up,
-- boots kicking, then the start of a fall.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 10 / 120
return A.clip({
	length = L,
	keys = {
		body = { y = { { 0, 3 }, { 2 * F, -1, e }, { L, 0, e } }, rot = { { 0, -6 }, { L, -4, e } } },
		bone = { y = { { 0, -2 }, { L, -4, e } } },
		head_neck = { rot = { { 0, 6 }, { L, 3, e } } },
		shoulder_back = { rot = { { 0, -80 }, { 3 * F, -112, e }, { L, -108, e } } },
		arm_back = { rot = { { 0, -10 }, { L, -26, e } } },
		shoulder_front = { rot = { { 0, 80 }, { 3 * F, 112, e }, { L, 108, e } } },
		arm_front = { rot = { { 0, 10 }, { L, 26, e } } },
		leg_front = { rot = { { 0, 6 }, { 3 * F, -8, e }, { L, -6, e } } },
		hoof_front = { rot = { { 0, -4 }, { L, 6, e } } },
		leg_back = { rot = { { 0, 4 }, { 3 * F, 10, e }, { L, 6, e } } },
		["hoof-back"] = { rot = { { 0, -2 }, { L, -4, e } } },
	},
})
