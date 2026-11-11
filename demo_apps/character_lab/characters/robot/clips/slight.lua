-- The side light (slight, 42 ticks): leaning into the lunge (move ticks 1-12) with the far fist
-- cocked, a long straight punch on the active frames (13-18), then back. Feet stay on the floor.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 42 / 120
return A.clip({
	length = L,
	keys = {
		body = { x = { { 0, 0 }, { 5 * F, 4, e }, { 6 * F, 12, e }, { 9 * F, 12 }, { L, 0, e } },
			rot = { { 0, 0 }, { 5 * F, 6, e }, { 6 * F, 12, e }, { 9 * F, 12 }, { L, 0, e } } },
		bone = { x = { { 0, 0 }, { 6 * F, 6, e }, { 9 * F, 6 }, { L, 0, e } } },
		head_neck = { rot = { { 0, 0 }, { 6 * F, -6, e }, { 9 * F, -6 }, { L, 0, e } } },
	},
	ik = {
		arm_back = { { 0, 251, 184 }, { 5 * F, 236, 158, e }, { 6 * F, 284, 140, e }, { 8 * F, 287, 139 }, { 13 * F, 255, 165, e }, { L, 251, 184, e } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 5 * F, 186, 142, e }, { 9 * F, 190, 138 }, { L, 149, 189, e } },
		leg_back = { { 0, 217.9, 229.3 }, { 6 * F, 230, 229.3, e }, { 9 * F, 230, 229.3 }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 6 * F, 170, 229.5, e }, { 9 * F, 170, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
