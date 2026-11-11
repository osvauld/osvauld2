-- The side air (sair, 36 ticks): knees in, then the far boot kicks straight out at chest height on
-- the active frames (move ticks 11-16), the body leaning back from it; then tucked again.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 36 / 120
local function kick(a, b) return { { 0, 0 }, { 4 * F, a, e }, { 5 * F, b, e }, { 8 * F, b }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		bone = { x = kick(-2, 8), y = kick(-4, -6) },
		body = { rot = kick(4, -12), x = kick(0, -4) },
		head_neck = { rot = kick(-2, 6) },
		shoulder_back = { rot = kick(-20, -40) },
		shoulder_front = { rot = kick(20, 40) },
	},
	ik = {
		leg_back = { { 0, 216, 222 }, { 4 * F, 212, 208, e }, { 5 * F, 258, 182, e }, { 8 * F, 260, 182 }, { 12 * F, 222, 212, e }, { L, 216, 222, e } },
		leg_front = { { 0, 180, 222 }, { 4 * F, 184, 212, e }, { 8 * F, 180, 216 }, { L, 180, 222, e } },
	},
})
