-- The directional dodge (32 ticks): low and pitched forward, arms swept back like the dash, through
-- the slide (invulnerable on dodge ticks 3-20), then up. Feet on the floor.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 32 / 120
local function low(a) return { { 0, 0 }, { 2 * F, a, e }, { 10 * F, a }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		bone = { y = low(10), x = low(4) },
		body = { y = low(14), x = low(10), rot = low(22) },
		head_neck = { rot = low(-8) },
		shoulder_back = { rot = low(58) },
		arm_back = { rot = low(20) },
		shoulder_front = { rot = low(4) },
		arm_front = { rot = low(12) },
	},
	ik = {
		leg_back = { { 0, 217.9, 229.3 }, { 2 * F, 230, 229.3, e }, { 10 * F, 230, 229.3 }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 2 * F, 166, 229.5, e }, { 10 * F, 166, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
