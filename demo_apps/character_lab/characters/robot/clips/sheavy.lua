-- The side heavy (sheavy, 72 ticks): the blade lights in the far hand and is drawn back behind the
-- hip, pointing back, through the startup; the lunge (move ticks 12-30) carries a straight thrust, blade level, through
-- the active frames (25-32); then it settles and the blade goes out.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 72 / 120
-- the blade's turn from its drawn angle (-21.3 degrees): 21.3 is level, forward
local function lean(a, b, c) return { { 0, 0 }, { 6 * F, a, e }, { 11 * F, b, e }, { 12 * F, c, e }, { 16 * F, c }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		["slot:sword_forward"] = { show = { { 0, false }, { 3 * F, true }, { 30 * F, false } } },
		["slot:sword"] = { show = { { 0, true }, { 3 * F, false }, { 30 * F, true } } },
		body = { x = lean(-4, -8, 16), rot = lean(-6, -10, 14), y = lean(2, 4, 4) },
		bone = { x = lean(-2, -4, 8), y = lean(1, 2, 3) },
		head_neck = { rot = lean(2, 4, -8) },
	},
	ik = {
		arm_back = { { 0, 251, 184, nil, 0 }, { 6 * F, 214, 172, e, -120 }, { 11 * F, 206, 166, e, -150 }, { 12 * F, 266, 150, e, 21 },
			{ 15 * F, 270, 148, nil, 21 }, { 22 * F, 262, 168, e, 40 }, { L, 251, 184, e, 0 } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 11 * F, 190, 150, e }, { 12 * F, 158, 168, e }, { 16 * F, 156, 170 }, { L, 149, 189, e } },
		leg_back = { { 0, 217.9, 229.3 }, { 11 * F, 214, 229.3, e }, { 12 * F, 240, 229.3, e }, { 16 * F, 240, 229.3 }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 11 * F, 180, 229.5, e }, { 12 * F, 160, 229.5, e }, { 16 * F, 160, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
