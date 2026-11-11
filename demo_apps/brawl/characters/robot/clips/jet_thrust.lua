-- Thrusting sideways on the jet (K and a side held): leaning into the push, arms swept back, boots
-- trailing; the flame flickers long and angles back. Loops (0.2 s).
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 0.2
local function wave(a, b) return { { 0, a }, { L / 2, b, e }, { L, a, e } } end
return A.clip({
	length = L,
	loop = true,
	keys = {
		body = { y = wave(1, -1), rot = wave(14, 17) },
		bone = { y = wave(-6, -7), rot = wave(14, 18) },
		head_neck = { rot = wave(-8, -10) },
		shoulder_back = { rot = wave(-20, -14) },
		arm_back = { rot = wave(-14, -8) },
		shoulder_front = { rot = wave(34, 40) },
		arm_front = { rot = wave(12, 8) },
		leg_front = { rot = wave(18, 24) },
		hoof_front = { rot = wave(-14, -18) },
		leg_back = { rot = wave(24, 30) },
		["hoof-back"] = { rot = wave(-18, -22) },
		flame = { scale_y = { { 0, 4.6 }, { 2 * F, 4.1 }, { 4 * F, 4.9 }, { 6 * F, 4.2 }, { 8 * F, 4.8 }, { 10 * F, 4.0 }, { L, 4.6 } } },
	},
})
