-- Falling: leaning back a little, arms up and out, the boots kicking under it. Loops.
local A = require("characters/robot/author")
local ease = "in_out"
local function wave(a, b) return { { 0, a }, { 0.3, b, ease }, { 0.6, a, ease } } end
return A.clip({
	length = 0.6,
	loop = true,
	keys = {
		body = { y = wave(0, -1.5), rot = wave(-4, -2) },
		bone = { y = wave(-3, -5) },
		head_neck = { rot = wave(3, 1) },
		shoulder_back = { rot = wave(-115, -100) },
		arm_back = { rot = wave(-30, -18) },
		shoulder_front = { rot = wave(115, 100) },
		arm_front = { rot = wave(30, 18) },
		leg_front = { rot = wave(-10, 4) },
		hoof_front = { rot = wave(8, -2) },
		leg_back = { rot = wave(6, -8) },
		["hoof-back"] = { rot = wave(-4, 6) },
	},
})
