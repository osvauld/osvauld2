-- Sliding down a wall it faces (a slide holds toward the wall): leaning back off it, the far hand and
-- boot pressed to the wall line (body_w / 2 in front of the centre), the near arm out behind for
-- balance and the near boot hanging.
-- Loops, with a judder as it scrapes.
local A = require("characters/robot/author")
local ease = "in_out"
local function wave(a, b) return { { 0, a }, { 0.25, b, ease }, { 0.5, a, ease } } end
local function at(x, y, dy) return { { 0, x, y }, { 0.25, x, y + dy, ease }, { 0.5, x, y, ease } } end
local far, boot = at(218, 96, 2), at(216, 212, 1)
far.bend = -1
return A.clip({
	length = 0.5,
	loop = true,
	keys = {
		body = { x = wave(-12, -11), y = wave(-2, -1), rot = wave(-18, -17) },
		bone = { x = wave(-4, -3) },
		head_neck = { rot = wave(-4, -6) },
		shoulder_front = { rot = wave(38, 44) },
		arm_front = { rot = wave(-18, -24) },
		leg_front = { rot = wave(10, 7) },
		hoof_front = { rot = wave(-5, -3) },
	},
	ik = { arm_back = far, leg_back = boot },
})
