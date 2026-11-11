-- Hovering on the jet (K held, no direction): upright, bobbing a little, arms out for balance, boots
-- hanging; the flame flickers. Loops (0.2 s).
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 0.2
local function wave(a, b) return { { 0, a }, { L / 2, b, e }, { L, a, e } } end
return A.clip({
	length = L,
	loop = true,
	keys = {
		body = { y = wave(0, -2), rot = wave(-1, 1) },
		bone = { y = wave(-4, -6) },
		head_neck = { rot = wave(2, 0) },
		shoulder_back = { rot = wave(-62, -55) },
		arm_back = { rot = wave(-18, -12) },
		shoulder_front = { rot = wave(55, 62) },
		arm_front = { rot = wave(16, 10) },
		leg_front = { rot = wave(4, 0) },
		hoof_front = { rot = wave(-4, -2) },
		leg_back = { rot = wave(-2, 2) },
		["hoof-back"] = { rot = wave(2, -2) },
		flame = { scale_y = { { 0, 4 }, { 2 * F, 3.5 }, { 4 * F, 4.4 }, { 6 * F, 3.7 }, { 8 * F, 4.3 }, { 10 * F, 3.6 }, { L, 4 } } },
	},
})
