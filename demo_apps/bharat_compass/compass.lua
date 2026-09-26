local C = require("theme")

local K = 0.5522847498307933
local function circle(cx, cy, r)
	local k = r * K
	return gfx.path({
		{ "move", cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
		{ "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
		{ "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "close" },
	})
end

local ring = circle(34, 34, 29)
local inner = circle(34, 34, 5)
local cross = gfx.path({
	{ "move", 34, 8 }, { "line", 34, 60 },
	{ "move", 8, 34 }, { "line", 60, 34 },
})
local north = gfx.path({
	{ "move", 34, 10 }, { "line", 42, 36 }, { "line", 34, 32 }, { "line", 26, 36 }, { "close" },
})
local south = gfx.path({
	{ "move", 34, 58 }, { "line", 42, 32 }, { "line", 34, 36 }, { "line", 26, 32 }, { "close" },
})

local visual = gfx.frame({
	width = 68,
	height = 68,
	gfx.fill({ path = circle(34, 34, 33), brush = gfx.solid(C.panel) }),
	gfx.stroke({ path = ring, brush = gfx.solid(C.green), width = 2 }),
	gfx.stroke({ path = cross, brush = gfx.solid(C.line), width = 1, dashes = { 3, 4 } }),
	gfx.fill({ path = south, brush = gfx.solid(C.blue_soft) }),
	gfx.fill({ path = north, brush = gfx.solid(C.saffron) }),
	gfx.fill({ path = inner, brush = gfx.solid(C.green) }),
})

return visual
