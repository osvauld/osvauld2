-- The jab (nlight, 36 ticks): the far fist cocks at the chest, snaps out level at chest height on
-- the active frames (move ticks 11-16), and comes back. The near fist guards the chin; feet planted.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
return A.clip({
	length = 36 / 120,
	keys = {
		body = { x = { { 0, 0 }, { 4 * F, -3, e }, { 5 * F, 6, e }, { 8 * F, 6 }, { 0.3, 0, e } },
			rot = { { 0, 0 }, { 4 * F, -4, e }, { 5 * F, 5, e }, { 8 * F, 5 }, { 0.3, 0, e } } },
		head_neck = { rot = { { 0, 0 }, { 5 * F, -3, e }, { 8 * F, -3 }, { 0.3, 0, e } } },
	},
	ik = {
		arm_back = { { 0, 251, 184 }, { 4 * F, 238, 160, e }, { 5 * F, 270, 140, e }, { 7 * F, 272, 139 }, { 12 * F, 250, 165, e }, { 0.3, 251, 184, e } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 4 * F, 184, 140, e }, { 8 * F, 188, 136 }, { 0.3, 149, 189, e } },
	},
})
