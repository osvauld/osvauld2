-- The down light (dlight, 38 ticks): a crouch, then the hips drop forward and the far boot sweeps out
-- low along the floor on the active frames (move ticks 11-16), and back under. The near boot stays put.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 38 / 120
local function crouch(a, b) return { { 0, 0 }, { 4 * F, a, e }, { 5 * F, b, e }, { 8 * F, b }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		bone = { y = crouch(8, 12), x = crouch(-2, 12) },
		body = { y = crouch(10, 14), x = crouch(-2, 10), rot = crouch(-2, -6) },
		head_neck = { rot = crouch(2, 4) },
		shoulder_front = { rot = crouch(10, 24) },
		shoulder_back = { rot = crouch(-10, -36) },
	},
	ik = {
		leg_back = { { 0, 217.9, 229.3 }, { 4 * F, 214, 229.3, e }, { 5 * F, 262, 225, e }, { 7 * F, 265, 225 }, { 12 * F, 224, 229.3, e }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 4 * F, 176, 229.5, e }, { 8 * F, 176, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
