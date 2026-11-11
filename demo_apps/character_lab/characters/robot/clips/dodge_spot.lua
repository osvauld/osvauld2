-- The spot dodge (30 ticks): a quick duck behind raised forearms, held while invulnerable (dodge
-- ticks 3-22), then up. Feet planted.
local A = require("characters/robot/author")
local e = "in_out"
local F = 1 / 60
local L = 30 / 120
local function duck(a) return { { 0, 0 }, { 2 * F, a, e }, { 11 * F, a }, { L, 0, e } } end
return A.clip({
	length = L,
	keys = {
		bone = { y = duck(12) },
		body = { y = duck(18), rot = duck(-4) },
		head_neck = { rot = duck(8) },
	},
	ik = {
		arm_back = { { 0, 251, 184 }, { 2 * F, 238, 150, e }, { 11 * F, 238, 150 }, { L, 251, 184, e } },
		arm_front = { bend = -1, { 0, 149, 189 }, { 2 * F, 196, 148, e }, { 11 * F, 196, 148 }, { L, 149, 189, e } },
		leg_back = { { 0, 217.9, 229.3 }, { 2 * F, 224, 229.3, e }, { 11 * F, 224, 229.3 }, { L, 217.9, 229.3, e } },
		leg_front = { { 0, 177.2, 229.5 }, { 2 * F, 170, 229.5, e }, { 11 * F, 170, 229.5 }, { L, 177.2, 229.5, e } },
	},
})
