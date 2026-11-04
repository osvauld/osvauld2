-- The look: drawings for the car's bodies (sized from setup.lua's boxes), the pickups and the
-- ground. Built once at module scope by main.lua, so the handles are stable. Nothing here
-- changes how anything moves.
local A = {}

A.colours = {
	sky = "#9fd3f0", ground = "#6b4a2f", grass = "#4caf50", dirt_line = "#3e2a1a",
	body = "#e8533f", body_dark = "#a83224", cab = "#2b2f3a", glass = "#bfe6ff",
	tyre = "#1d1f24", rim = "#c9ced6", helmet = "#ffd23f", visor = "#22324a",
	can = "#d7262e", can_dark = "#8e1218", coin = "#ffc93c", coin_dark = "#c98a00",
	text = "#14202e", panel = "#ffffffcc", muted = "#4b5b6b", fuel = "#f0a000", fuel_low = "#e03030",
	pedal = "#2b2f3a", pedal_held = "#4a8ff0", pedal_text = "#ffffff",
}
local C = A.colours

local function rect(x, y, w, h)
	return { { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } }
end

local function disc(cx, cy, r)
	local k = r * 0.5523
	return {
		{ "move", cx, cy - r }, { "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r }, { "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r }, { "close" },
	}
end

local function one_part(w, h, shapes)
	return gfx.drawing({ size = { w, h }, parts = { { id = "it", pivot = { w / 2, h / 2 }, shapes = shapes } } })
end

-- A buggy seen from the side: a low tub, a roll cage, a seat back. The head is its own body.
local function chassis(w, h)
	return one_part(w, h, {
		{ path = { { "move", 0, h * 0.25 }, { "line", w * 0.08, 0 }, { "line", w * 0.92, 0 }, { "line", w, h * 0.3 },
			{ "line", w * 0.96, h }, { "line", w * 0.04, h }, { "close" } }, fill = C.body, stroke = { 2, C.body_dark } },
		{ path = rect(w * 0.36, h * 0.15, w * 0.1, h * 0.55), fill = C.cab },
		{ path = { { "move", w * 0.38, 0 }, { "line", w * 0.41, -h * 1.2 }, { "line", w * 0.66, -h * 1.2 }, { "line", w * 0.74, 0 } },
			stroke = { 4, C.cab } },
		{ path = rect(w * 0.08, h * 0.4, w * 0.84, h * 0.12), fill = C.body_dark },
	})
end

-- Spokes, so a turning wheel shows that it turns.
local function wheel(d)
	local r = d / 2
	local shapes = {
		{ path = disc(r, r, r), fill = C.tyre },
		{ path = disc(r, r, r * 0.58), fill = C.rim, stroke = { 2, C.tyre } },
	}
	for k = 0, 2 do
		local a = k * math.pi / 3
		local dx, dy = math.cos(a) * r * 0.55, math.sin(a) * r * 0.55
		table.insert(shapes, { path = { { "move", r - dx, r - dy }, { "line", r + dx, r + dy } }, stroke = { 3, C.tyre } })
	end
	return one_part(d, d, shapes)
end

local function head(d)
	local r = d / 2
	return one_part(d, d, {
		{ path = disc(r, r, r), fill = C.helmet, stroke = { 2, C.dirt_line } },
		{ path = rect(r * 0.9, r * 0.6, r * 1.05, r * 0.55), fill = C.visor },
	})
end

local function fuel_can(w, h)
	return one_part(w, h, {
		{ path = rect(2, h * 0.18, w - 4, h * 0.8), fill = C.can, stroke = { 2, C.can_dark } },
		{ path = rect(w * 0.55, 0, w * 0.3, h * 0.2), fill = C.can_dark },
		{ path = { { "move", w * 0.2, h * 0.35 }, { "line", w * 0.8, h * 0.85 } }, stroke = { 3, C.can_dark } },
	})
end

local function coin(d)
	local r = d / 2
	return one_part(d, d, {
		{ path = disc(r, r, r), fill = C.coin, stroke = { 2, C.coin_dark } },
		{ path = disc(r, r, r * 0.55), stroke = { 2, C.coin_dark } },
	})
end

-- The ground under the stage's line, `points` already shifted into the drawing (y from its top).
local function ground(points, w, h)
	local outline = { { "move", points[1], h } }
	for i = 1, #points, 2 do table.insert(outline, { "line", points[i], points[i + 1] }) end
	table.insert(outline, { "line", points[#points - 1], h })
	table.insert(outline, { "close" })
	local top = { { "move", points[1], points[2] } }
	for i = 3, #points, 2 do table.insert(top, { "line", points[i], points[i + 1] }) end
	return one_part(w, h, {
		{ path = outline, fill = C.ground },
		{ path = top, stroke = { 8, C.grass } },
	})
end

A.PICKUP_R = { fuel = 26, coin = 15 } -- sensor radius, units

-- Every drawing the game needs, by entity id or item kind, plus the ground entity's placement.
function A.build(sizes, stage)
	local looks = {
		chassis = chassis(sizes.chassis[1], sizes.chassis[2]),
		head = head(sizes.head[1]),
		fuel = fuel_can(2 * A.PICKUP_R.fuel, 2 * A.PICKUP_R.fuel),
		coin = coin(2 * A.PICKUP_R.coin),
	}
	for id, size in pairs(sizes) do
		if id:sub(1, 5) == "wheel" then looks[id] = wheel(size[1]) end
	end
	-- The ground entity sits at the line's highest point, so its drawing's y never goes negative.
	local p, top, bottom = stage.points, math.huge, -math.huge
	for i = 2, #p, 2 do top, bottom = math.min(top, p[i]), math.max(bottom, p[i]) end
	top = top - 40
	local shifted = {}
	for i = 1, #p, 2 do
		table.insert(shifted, p[i])
		table.insert(shifted, p[i + 1] - top)
	end
	local width, depth = p[#p - 1], bottom - top + 800
	looks.ground = ground(shifted, width, depth)
	looks.ground_at = { 0, top }
	looks.ground_line = shifted
	-- The camera stays over the ground: no sky past its ends, none below it.
	looks.bounds = { 0, top - 1500, width, depth + 1500 }
	return looks
end

return A
