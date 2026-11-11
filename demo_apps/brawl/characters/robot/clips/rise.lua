-- Going up: stretched, leaning into the jump, the far arm reaching up, the near one swung back, boots trailing. Once; held at its end.
local A = require("characters/robot/author")
local ease = "in_out"
return A.clip({
	length = 0.3,
	keys = {
		body = { y = { { 0, -6 }, { 0.3, -2, ease } }, rot = { { 0, 6 }, { 0.3, 3, ease } } },
		bone = { y = { { 0, -4 }, { 0.3, -2, ease } } },
		head_neck = { rot = { { 0, -4 }, { 0.3, -2, ease } } },
		shoulder_back = { rot = { { 0, -50 }, { 0.12, -112, ease }, { 0.3, -100, ease } } },
		arm_back = { rot = { { 0, -10 }, { 0.12, -16, ease }, { 0.3, -10, ease } } },
		shoulder_front = { rot = { { 0, 20 }, { 0.12, 38, ease }, { 0.3, 30, ease } } },
		arm_front = { rot = { { 0, 6 }, { 0.12, 22, ease }, { 0.3, 16, ease } } },
		leg_front = { rot = { { 0, 14 }, { 0.3, 22, ease } } },
		hoof_front = { rot = { { 0, -6 }, { 0.3, -10, ease } } },
		leg_back = { rot = { { 0, 20 }, { 0.3, 28, ease } } },
		["hoof-back"] = { rot = { { 0, -8 }, { 0.3, -12, ease } } },
	},
})
