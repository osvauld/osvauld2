-- A day/night toggle: a sky pill with clouds, a sun knob that rolls across and becomes a moon,
-- and a night sky with stars. The helpers only generate paths; what lands in the asset is data.
--
-- The rest pose shows every part at once (handy for editing). The states are clips: `day` and
-- `night` hold a state, `to_night` and `to_day` animate between them. Hidden parts are scaled to
-- zero — there is no opacity track yet.

local K = 0.5523 -- a quarter circle's cubic handle, as a fraction of the radius

local function circle(cx, cy, r, reverse)
	local k = K * r
	local s = reverse and -1 or 1 -- reversed winding cuts a hole in a non-zero fill
	return {
		{ "move", cx + r, cy },
		{ "cubic", cx + r, cy + s * k, cx + k, cy + s * r, cx, cy + s * r },
		{ "cubic", cx - k, cy + s * r, cx - r, cy + s * k, cx - r, cy },
		{ "cubic", cx - r, cy - s * k, cx - k, cy - s * r, cx, cy - s * r },
		{ "cubic", cx + k, cy - s * r, cx + r, cy - s * k, cx + r, cy },
		{ "close" },
	}
end

local function ring(cx, cy, outer, inner)
	local path = circle(cx, cy, outer)
	for _, cmd in ipairs(circle(cx, cy, inner, true)) do
		table.insert(path, cmd)
	end
	return path
end

-- A stadium from x0..x1, y0..y1: the ends are half circles.
local function pill(x0, y0, x1, y1)
	local r = (y1 - y0) / 2
	local k, cy = K * r, y0 + r
	return {
		{ "move", x0 + r, y0 },
		{ "line", x1 - r, y0 },
		{ "cubic", x1 - r + k, y0, x1, cy - k, x1, cy },
		{ "cubic", x1, cy + k, x1 - r + k, y1, x1 - r, y1 },
		{ "line", x0 + r, y1 },
		{ "cubic", x0 + r - k, y1, x0, cy + k, x0, cy },
		{ "cubic", x0, cy - k, x0 + r - k, y0, x0 + r, y0 },
		{ "close" },
	}
end

-- A four-pointed sparkle: curves pinched toward the centre.
local function sparkle(x, y, s)
	return {
		{ "move", x, y - s },
		{ "quad", x, y, x + s, y },
		{ "quad", x, y, x, y + s },
		{ "quad", x, y, x - s, y },
		{ "quad", x, y, x, y - s },
		{ "close" },
	}
end

local SKY = pill(4, 4, 196, 80)
local KNOB = { 42, 42 } -- the knob's centre by day; by night it has rolled 116 to the right

local function shapes_of(list, fill)
	local out = {}
	for _, c in ipairs(list) do
		table.insert(out, { path = circle(c[1], c[2], c[3]), fill = fill })
	end
	return out
end

local stars = {}
for _, s in ipairs({ { 34, 26, 5 }, { 58, 52, 4 }, { 80, 24, 3.5 }, { 98, 46, 5 }, { 46, 64, 3 }, { 114, 30, 3 } }) do
	table.insert(stars, { path = sparkle(s[1], s[2], s[3]), fill = "#fff8d6" })
end
for _, d in ipairs({ { 66, 36, 1.2 }, { 24, 46, 1 }, { 88, 62, 1.1 }, { 120, 52, 0.9 } }) do
	table.insert(stars, { path = circle(d[1], d[2], d[3]), fill = "#e8ecff" })
end

-- Each cloud is overlapping circles: a shaded copy a little lower, then the white on top.
local cloud = { { 130, 56, 12 }, { 146, 48, 15 }, { 162, 56, 12 }, { 146, 60, 12 }, { 172, 38, 8 }, { 182, 42, 7 }, { 164, 42, 7 } }
local cloud_shade = {}
for _, c in ipairs(cloud) do
	table.insert(cloud_shade, { c[1], c[2] + 2.5, c[3] })
end
local clouds = shapes_of(cloud_shade, "#c4dbf3")
for _, s in ipairs(shapes_of(cloud, "#ffffff")) do
	table.insert(clouds, s)
end

local craters = {}
for _, c in ipairs({ { 32, 48, 6 }, { 52, 34, 4 }, { 50, 54, 3 }, { 36, 31, 2.5 } }) do
	table.insert(craters, { path = circle(c[1], c[2], c[3]), fill = "#b3b9c9" })
	table.insert(craters, { path = circle(c[1] + 0.8, c[2] + 0.8, c[3] * 0.7), fill = "#c8cddb" })
end

local moon = {
	{ path = circle(42, 42, 30), fill = { radial = { center = KNOB, radius = 30, focus = { 34, 32 },
		focus_radius = 2, stops = { { 0, "#ffffff" }, { 0.6, "#dfe3ec" }, { 1, "#a9b0c2" } } } } },
}
for _, c in ipairs(craters) do
	table.insert(moon, c)
end
table.insert(moon, { path = ring(42, 42, 31, 29), fill = { sweep = { center = KNOB, stops = {
	{ 0, "#f4f6fb" }, { 0.25, "#8c93a6" }, { 0.5, "#f4f6fb" }, { 0.75, "#8c93a6" }, { 1, "#f4f6fb" },
} } } })

local parts = {
	{ id = "track", pivot = { 100, 42 }, shapes = {
		{ shadow = { rect = { 4, 9, 192, 76 }, radius = 38, blur = 5, color = "#0b1a3a66" } },
		{ path = SKY, fill = { linear = { from = { 0, 4 }, to = { 0, 80 },
			stops = { { 0, "#8fd3ff" }, { 1, "#3b8ee8" } } } } },
	} },
	-- Grows from where the moon lands: a convex pill scaled about a point inside it stays inside.
	{ id = "night_sky", pivot = { 158, 42 }, shapes = {
		{ path = SKY, fill = { linear = { from = { 0, 4 }, to = { 0, 80 },
			stops = { { 0, "#2a2f74" }, { 1, "#0b0e2e" } } } } },
	} },
	{ id = "stars", pivot = { 70, 42 }, shapes = stars },
	{ id = "clouds", pivot = { 152, 52 }, shapes = clouds },
	{ id = "rim", pivot = { 100, 42 }, shapes = { { path = SKY, stroke = { 2, "#1d3f73" } } } },
	{ id = "glow", parent = "knob", pivot = KNOB, shapes = {
		{ path = circle(42, 42, 37), fill = { radial = { center = KNOB, radius = 37,
			stops = { { 0.7, "#ffd16699" }, { 1, "#ffd16600" } } } } },
	} },
	{ id = "knob", pivot = KNOB, shapes = {
		{ shadow = { rect = { 12, 15, 60, 60 }, radius = 30, blur = 4, color = "#00000055" } },
		{ path = circle(42, 42, 30), fill = { radial = { center = KNOB, radius = 30, focus = { 34, 32 },
			focus_radius = 2, stops = { { 0, "#fff6c4" }, { 0.55, "#ffc93c" }, { 1, "#f48c06" } } } } },
		{ path = ring(42, 42, 31, 29), fill = { sweep = { center = KNOB, stops = {
			{ 0, "#ffe08a" }, { 0.5, "#e07a00" }, { 1, "#ffe08a" },
		} } } },
	} },
	{ id = "moon", parent = "knob", pivot = KNOB, shapes = moon },
}

-- The knob rolls: 116 across a circle of radius 30 is 222 degrees of turn, eased together.
local roll = { x = { { 0, 0 }, { 0.6, 116, "in_out" } }, rot = { { 0, 0 }, { 0.6, 222, "in_out" } } }
local unroll = { x = { { 0, 116 }, { 0.6, 0, "in_out" } }, rot = { { 0, 222 }, { 0.6, 0, "in_out" } } }

local clips = {
	day = { length = 0.1, tracks = {
		night_sky = { scale = { { 0, 0 } } }, stars = { scale = { { 0, 0 } } }, moon = { scale = { { 0, 0 } } },
	} },
	night = { length = 0.1, tracks = {
		knob = { x = { { 0, 116 } }, rot = { { 0, 222 } } },
		clouds = { scale = { { 0, 0 } } }, glow = { scale = { { 0, 0 } } },
	} },
	to_night = { length = 0.6, tracks = {
		knob = roll,
		moon = { scale = { { 0, 0 }, { 0.2, 0 }, { 0.5, 1, "in_out" } } },
		glow = { scale = { { 0, 1 }, { 0.3, 0, "in_out" } } },
		night_sky = { scale = { { 0, 0 }, { 0.6, 1, "in_out" } } },
		clouds = { scale = { { 0, 1 }, { 0.25, 0, "in_out" } }, y = { { 0, 0 }, { 0.25, 8, "in_out" } } },
		stars = { scale = { { 0, 0 }, { 0.35, 0 }, { 0.6, 1, "in_out" } } },
	} },
	to_day = { length = 0.6, tracks = {
		knob = unroll,
		moon = { scale = { { 0, 1 }, { 0.3, 0, "in_out" } } },
		glow = { scale = { { 0, 0 }, { 0.3, 0 }, { 0.6, 1, "in_out" } } },
		night_sky = { scale = { { 0, 1 }, { 0.6, 0, "in_out" } } },
		clouds = { scale = { { 0, 0 }, { 0.3, 0 }, { 0.6, 1, "in_out" } }, y = { { 0, 8 }, { 0.3, 8 }, { 0.6, 0, "in_out" } } },
		stars = { scale = { { 0, 1 }, { 0.25, 0, "in_out" } } },
	} },
}

return { size = { 200, 84 }, parts = parts, clips = clips }
