-- The top of the arc: arms spread level, boots drawn up under the body. Once, about as long as an
-- apex lasts (|vy| <= apex_vy is 16 ticks).
local A = require("characters/robot/author")
local ease = "in_out"
return A.clip({
	length = 0.15,
	keys = {
		body = { y = { { 0, -2 }, { 0.15, 0, ease } }, rot = { { 0, 2 }, { 0.15, 0, ease } } },
		bone = { y = { { 0, -6 }, { 0.15, -7, ease } } },
		shoulder_back = { rot = { { 0, -95 }, { 0.15, -80, ease } } },
		arm_back = { rot = { { 0, -12 }, { 0.15, -6, ease } } },
		shoulder_front = { rot = { { 0, 70 }, { 0.15, 62, ease } } },
		arm_front = { rot = { { 0, 14 }, { 0.15, 8, ease } } },
		leg_front = { rot = { { 0, 10 }, { 0.15, -6, ease } } },
		hoof_front = { rot = { { 0, -6 }, { 0.15, 4, ease } } },
		leg_back = { rot = { { 0, 14 }, { 0.15, -4, ease } } },
		["hoof-back"] = { rot = { { 0, -8 }, { 0.15, 2, ease } } },
	},
})
