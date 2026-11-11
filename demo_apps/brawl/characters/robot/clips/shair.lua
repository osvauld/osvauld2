-- The side heavy air (shair, 52 ticks): the blade lights and is drawn back over the shoulder, then a
-- level thrust through the lunge on the active frames (move ticks 15-22), legs split behind it.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 52 / 120
local function lean(a, b) return { { 0, 0 }, { 6 * F, a, e }, { 7 * F, b, e }, { 11 * F, b }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		["slot:sword_forward"] = { show = { { 0, false }, { 2 * F, true }, { 23 * F, false } } },
		["slot:sword"] = { show = { { 0, true }, { 2 * F, false }, { 23 * F, true } } },
		body = { rot = lean(-8, 14), x = lean(-4, 12) },
		bone = { x = lean(-2, 6), y = lean(-4, -2) },
		head_neck = { rot = lean(2, -8) },
	},
	ik = {
		arm_back = { { 0, 251, 184, nil, 0 }, { 6 * F, 210, 166, e, -150 }, { 7 * F, 268, 148, e, 21 }, { 10 * F, 272, 146, nil, 21 },
			{ 18 * F, 260, 166, e, 40 }, { L, 251, 184, e, 0 } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 6 * F, 186, 150, e }, { 7 * F, 150, 164, e }, { 11 * F, 148, 166 }, { L, 149, 189, e } },
		leg_back = { { 0, 216, 222 }, { 6 * F, 214, 214, e }, { 7 * F, 240, 220, e }, { 11 * F, 240, 220 }, { L, 216, 222, e } },
		leg_front = { { 0, 180, 222 }, { 6 * F, 182, 214, e }, { 7 * F, 160, 226, e }, { 11 * F, 160, 226 }, { L, 180, 222, e } },
	},
})
