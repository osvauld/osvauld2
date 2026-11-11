-- The down heavy air (dhair, 74 ticks): stalled, the blade lit and swept back overhead (move ticks
-- 1-14); then the dive, both boots driven straight down below the feet line until it lands (15-74).
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 74 / 120
return A.clip({
	length = L,
	keys = {
		["slot:sword_forward"] = { show = { { 0, false }, { 2 * F, true } } },
		["slot:sword"] = { show = { { 0, true }, { 2 * F, false } } },
		body = { y = { { 0, 0 }, { 6 * F, -6, e }, { 7 * F, 2, e } }, rot = { { 0, 0 }, { 6 * F, -6, e }, { 7 * F, 4, e } } },
		bone = { y = { { 0, 0 }, { 6 * F, -8, e }, { 7 * F, 0, e } } },
		head_neck = { rot = { { 0, 0 }, { 7 * F, 6, e } } },
	},
	ik = {
		arm_back = { { 0, 251, 184, nil, 0 }, { 6 * F, 226, 92, e, -110 }, { 7 * F, 214, 104, e, -130 }, { L, 214, 106, nil, -130 } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 6 * F, 150, 120, e }, { 7 * F, 140, 140, e }, { L, 140, 142 } },
		leg_front = { { 0, 180, 222 }, { 6 * F, 182, 208, e }, { 7 * F, 184, 250, e }, { L, 184, 251 } },
		leg_back = { { 0, 216, 222 }, { 6 * F, 214, 208, e }, { 7 * F, 210, 250, e }, { L, 210, 251 } },
	},
})
