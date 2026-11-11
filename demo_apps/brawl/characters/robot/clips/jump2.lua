-- The double jump: a tuck and a forward flip about the chassis, then open out. The root turns
-- (its pivot is at the feet), moved so the turn is about the body's centre: keyed every 1/60 s.
local A = require("characters/robot/author")
local LENGTH, FROM, TO = 0.4, 0.03, 0.33
local feet = A.parts.root.pivot
local cx, cy = feet[1], feet[2] - 95 -- between the chassis and the helmet: the robot's weight
local rot, x, y = {}, {}, {}
local function eased(u) return u * u * (3 - 2 * u) end
for i = 0, math.floor(LENGTH * 60 + 1e-9) do
	local t = i / 60
	local u = math.max(0, math.min(1, (t - FROM) / (TO - FROM)))
	local deg = 360 * eased(u)
	local r = math.rad(deg)
	local dx, dy = cx - feet[1], cy - feet[2]
	rot[#rot + 1] = { t, deg }
	x[#x + 1] = { t, dx - (math.cos(r) * dx - math.sin(r) * dy) }
	y[#y + 1] = { t, dy - (math.sin(r) * dx + math.cos(r) * dy) }
end
if rot[#rot][1] < LENGTH then
	rot[#rot + 1], x[#x + 1], y[#y + 1] = { LENGTH, 360 }, { LENGTH, 0 }, { LENGTH, 0 }
end
local ease = "in_out"
local function tuck(a, b, c) return { { 0, a }, { 0.08, b, ease }, { 0.28, b }, { 0.4, c, ease } } end
return A.clip({
	length = LENGTH,
	keys = {
		root = { x = x, y = y, rot = rot },
		body = { y = tuck(-2, 6, 0), rot = tuck(4, 14, 4) },
		bone = { y = tuck(-3, -16, -4) },
		head_neck = { rot = tuck(-2, 10, 0) },
		shoulder_back = { rot = tuck(-60, -20, -90) },
		arm_back = { rot = tuck(-10, -70, -12) },
		shoulder_front = { rot = tuck(40, -10, 70) },
		arm_front = { rot = tuck(10, -60, 14) },
		leg_front = { rot = tuck(10, -35, 6), scale_x = tuck(1, 0.7, 1) },
		hoof_front = { rot = tuck(-4, 30, -4) },
		leg_back = { rot = tuck(14, -30, 8), scale_x = tuck(1, 0.7, 1) },
		["hoof-back"] = { rot = tuck(-6, 26, -6) },
	},
})
