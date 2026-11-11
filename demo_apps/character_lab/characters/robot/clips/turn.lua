-- The skid before facing flips: leaning back against the run, the far boot braced ahead, the arms
-- thrown out for balance. Both boots stay on the floor. Once; a skid is at most 5 ticks.
local A = require("characters/robot/author")
local front, back = A.home("hoof_front"), A.home("hoof-back")
return A.clip({
	length = 0.1,
	keys = {
		body = { x = { { 0, -6 } }, y = { { 0, 7 } }, rot = { { 0, -12 }, { 0.1, -14 } } },
		bone = { x = { { 0, -3 } }, y = { { 0, 6 } } },
		head_neck = { rot = { { 0, -6 } } },
		shoulder_front = { rot = { { 0, 40 }, { 0.1, 46 } } },
		arm_front = { rot = { { 0, -20 } } },
		shoulder_back = { rot = { { 0, -62 }, { 0.1, -70 } } },
		arm_back = { rot = { { 0, -16 } } },
	},
	ik = {
		leg_front = { { 0, front[1] - 6, front[2] } },
		leg_back = { { 0, back[1] + 14, back[2] } },
	},
})
