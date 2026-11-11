-- Landing: a squash on the piston legs, both boots kept where they stand, then back up. Once,
-- land_ticks (0.1 s) long; the blend from `fall` covers its first 4 ticks.
local A = require("characters/robot/author")
local front, back = A.home("hoof_front"), A.home("hoof-back")
local out = { 0.24, 0, 0.4, 1 }
return A.clip({
	length = 0.1,
	keys = {
		body = { y = { { 0, 11 }, { 0.04, 10 }, { 0.1, 1, out } } },
		bone = { y = { { 0, 9 }, { 0.04, 8 }, { 0.1, 1, out } } },
		body4 = { scale_y = { { 0, 0.94 }, { 0.1, 1, out } }, scale_x = { { 0, 1.04 }, { 0.1, 1, out } } },
		head_neck = { y = { { 0, 3 }, { 0.1, 0, out } } },
		shoulder_front = { rot = { { 0, 22 }, { 0.1, 4, out } } },
		shoulder_back = { rot = { { 0, -22 }, { 0.1, -4, out } } },
		arm_front = { rot = { { 0, -14 }, { 0.1, -2, out } } },
		arm_back = { rot = { { 0, 14 }, { 0.1, 2, out } } },
	},
	ik = {
		leg_front = { { 0, front[1], front[2] } },
		leg_back = { { 0, back[1], back[2] } },
	},
})
