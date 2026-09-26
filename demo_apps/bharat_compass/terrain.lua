local C = require("theme")

local CELL = 54
local cache = {}

local function rect(x, y, w, h)
	return gfx.path({
		{ "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h },
		{ "line", x, y + h }, { "close" },
	})
end

local function diamond(cx, cy, r)
	return gfx.path({
		{ "move", cx, cy - r }, { "line", cx + r, cy }, { "line", cx, cy + r },
		{ "line", cx - r, cy }, { "close" },
	})
end

local K = 0.5522847498307933
local function circle(cx, cy, r)
	local k = r * K
	return gfx.path({
		{ "move", cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
		{ "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
		{ "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy }, { "close" },
	})
end

local function center(col, row)
	return (col - 0.5) * CELL, (row - 0.5) * CELL
end

local function at(list, col, row)
	for _, spot in ipairs(list or {}) do
		if spot[1] == col and spot[2] == row then return spot end
	end
	return nil
end

local function route_key(exercise, trail)
	local bits = { exercise.id }
	for _, point in ipairs(trail) do bits[#bits + 1] = point[1] .. ":" .. point[2] end
	return table.concat(bits, "|")
end

local function make(exercise, trail)
	local map = exercise.map
	local key = route_key(exercise, trail)
	if cache[key] then return cache[key] end
	local items = { width = map.cols * CELL, height = map.rows * CELL }
	for row = 1, map.rows do
		for col = 1, map.cols do
			local x, y = (col - 1) * CELL, (row - 1) * CELL
			local blocked = at(map.blocked, col, row)
			local fill = blocked and (blocked[3] == "hill" and C.saffron_soft or C.blue_soft)
				or (((row + col) % 2 == 0) and C.map_grass or C.panel)
			local tile = rect(x + 1, y + 1, CELL - 2, CELL - 2)
			items[#items + 1] = gfx.fill({ path = tile, brush = gfx.solid(fill) })
			items[#items + 1] = gfx.stroke({ path = tile, brush = gfx.solid(C.line), width = 1 })
			if blocked then
				local bx, by = center(col, row)
				items[#items + 1] = gfx.fill({ path = diamond(bx, by, 9), brush = gfx.solid(C.map_blocked) })
			end
		end
	end
	if #trail > 1 then
		local commands = {}
		for i, point in ipairs(trail) do
			local x, y = center(point[1], point[2])
			commands[#commands + 1] = { i == 1 and "move" or "line", x, y }
		end
		items[#items + 1] = gfx.stroke({ path = gfx.path(commands), brush = gfx.solid(C.saffron), width = 5, cap = "round", join = "round" })
	end
	for _, spot in ipairs(map.landmarks or {}) do
		local x, y = center(spot[1], spot[2])
		items[#items + 1] = gfx.fill({ path = circle(x, y, 7), brush = gfx.solid(C.blue) })
	end
	local gx, gy = center(map.goal[1], map.goal[2])
	items[#items + 1] = gfx.fill({ path = diamond(gx, gy, 15), brush = gfx.solid(C.green) })
	local here = trail[#trail]
	local tx, ty = center(here[1], here[2])
	items[#items + 1] = gfx.fill({ path = circle(tx, ty, 11), brush = gfx.solid(C.saffron) })
	items[#items + 1] = gfx.stroke({ path = circle(tx, ty, 11), brush = gfx.solid(C.white), width = 3 })
	cache[key] = gfx.frame(items)
	return cache[key]
end

return { make = make, cell = CELL }
