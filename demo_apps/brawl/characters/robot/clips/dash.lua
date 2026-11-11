-- The dash: low and pitched forward, arms swept back, boots trailing. Loops (a dash is 12 ticks).
local A = require("characters/robot/author")
local ease = "in_out"
local function wave(a, b) return { { 0, a }, { 0.1, b, ease }, { 0.2, a, ease } } end
return A.clip({
	length = 0.2,
	loop = true,
	keys = {
		body = { x = wave(8, 9), y = wave(7, 9), rot = wave(24, 26) },
		bone = { x = wave(2, 2.5), y = wave(4, 5) },
		head_neck = { rot = wave(-8, -9) },
		shoulder_back = { rot = wave(58, 64) },
		arm_back = { rot = wave(20, 24) },
		shoulder_front = { rot = wave(4, 10) },
		arm_front = { rot = wave(12, 16) },
		leg_front = { rot = wave(32, 30) },
		hoof_front = { rot = wave(-20, -18) },
		leg_back = { rot = wave(18, 22) },
		["hoof-back"] = { rot = wave(-10, -14) },
	},
})
