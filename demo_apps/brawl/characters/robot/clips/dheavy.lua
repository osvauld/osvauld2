-- The down heavy (dheavy, 70 ticks): blade lit and both arms raised through the startup, then a
-- crouching slam on the active frames (move ticks 21-30): the blade sweeps low in front, the near fist
-- hammers the floor behind. Back up in recovery. Feet planted.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 70 / 120
local function slam(a, b) return { { 0, 0 }, { 9 * F, a, e }, { 10 * F, b, e }, { 15 * F, b }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		["slot:sword_forward"] = { show = { { 0, false }, { 2 * F, true }, { 31 * F, false } } },
		["slot:sword"] = { show = { { 0, true }, { 2 * F, false }, { 31 * F, true } } },
		bone = { y = slam(-2, 10) },
		body = { y = slam(-4, 15), rot = slam(-2, 4) },
		head_neck = { rot = slam(-2, 4) },
	},
	ik = {
		arm_back = { { 0, 251, 184, nil, 0 }, { 9 * F, 234, 82, e, -70 }, { 10 * F, 262, 196, e, 26 }, { 14 * F, 266, 198, nil, 28 },
			{ 22 * F, 258, 190, e, 20 }, { L, 251, 184, e, 0 } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 9 * F, 172, 92, e }, { 10 * F, 140, 206, e }, { 14 * F, 136, 208 }, { L, 149, 189, e } },
		leg_back = { { 0, 217.9, 229.3 }, { 10 * F, 228, 229.3, e }, { 15 * F, 228, 229.3 }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 10 * F, 166, 229.5, e }, { 15 * F, 166, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
