-- The neutral heavy (nheavy, 66 ticks): the blade lights and is raised back over the shoulder through
-- the startup, swept overhead from straight up to forward on the active frames (move ticks 23-30),
-- then lowered and put out. Feet planted.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 66 / 120
local function sway(a, b, c) return { { 0, 0 }, { 10 * F, a, e }, { 11 * F, b, e }, { 14 * F, c, e }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		["slot:sword_forward"] = { show = { { 0, false }, { 2 * F, true }, { 30 * F, false } } },
		["slot:sword"] = { show = { { 0, true }, { 2 * F, false }, { 30 * F, true } } },
		body = { rot = sway(-10, -2, 10), x = sway(-4, 0, 8), y = sway(0, 0, 3) },
		bone = { x = sway(-2, 0, 3) },
		head_neck = { rot = sway(4, 0, -6) },
	},
	ik = {
		-- the fifth value turns the hand from its drawn angle; the blade is drawn at -21.3 degrees
		arm_back = { { 0, 251, 184, nil, 0 }, { 10 * F, 220, 82, e, -125 }, { 11 * F, 240, 74, e, -55 }, { 12 * F, 252, 82, e, -30 },
			{ 14 * F, 266, 104, e, 0 }, { 22 * F, 262, 160, e, 30 }, { L, 251, 184, e, 0 } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 10 * F, 140, 150, e }, { 14 * F, 150, 176, e }, { L, 149, 189, e } },
		leg_back = { { 0, 217.9, 229.3 }, { 10 * F, 220, 229.3, e }, { 14 * F, 226, 229.3, e }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 10 * F, 172, 229.5, e }, { 14 * F, 172, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
