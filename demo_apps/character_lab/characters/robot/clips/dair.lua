-- The down air (dair, 38 ticks): boots drawn up under the body, then both stamp straight down below
-- the feet line on the active frames (move ticks 11-18), and draw back up.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 38 / 120
local function stomp(a, b) return { { 0, 0 }, { 4 * F, a, e }, { 5 * F, b, e }, { 9 * F, b }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		body = { y = stomp(-6, 2), rot = stomp(6, 0) },
		bone = { y = stomp(-8, 0) },
		head_neck = { rot = stomp(6, 2) },
		shoulder_back = { rot = stomp(-40, -70) },
		shoulder_front = { rot = stomp(40, 70) },
	},
	ik = {
		leg_front = { { 0, 180, 222 }, { 4 * F, 182, 208, e }, { 5 * F, 181, 250, e }, { 9 * F, 181, 251 }, { L, 180, 222, e } },
		leg_back = { { 0, 216, 222 }, { 4 * F, 214, 208, e }, { 5 * F, 211, 250, e }, { 9 * F, 211, 251 }, { L, 216, 222, e } },
	},
})
