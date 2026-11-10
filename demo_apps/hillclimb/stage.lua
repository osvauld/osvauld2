-- A stage from a map: the ground as one line (world units, the entity placed at 0, 0), where the
-- car starts, the fuel cans and coins along it, and the scenery. A map is plain data in metres
-- (maps/*.lua; the format is docs/design/hillclimb-maps.md §1); `generate` makes one from a seed.
-- Pure: the same map is the same stage in Luau and in plain Lua.
local U = require("units")
local S = {}

local STEP = 50                   -- units between ground points (0.5 m)
local MAX_SLOPE = 40              -- degrees, no steeper anywhere
local CHASSIS_CLEAR = 98          -- chassis top above the ground at the start (wheel bottoms 94 below it)
local FUEL_UP, COIN_UP, COIN_GAP = 0.6, 0.7, 0.6 -- metres

local FIELDS = { name = true, ground = true, smooth = true, start = true, finish = true,
	fuel = true, coins = true, scenery = true }
local SCENERY_FIELDS = { [1] = true, [2] = true, front = true }

local function fail(msg) error("map: " .. msg, 0) end
local function number(v) return type(v) == "number" and v == v and v > -math.huge and v < math.huge end
local function m(x) return string.format("%g m", x) end

-- Monotone cubic (Fritsch–Carlson) tangents: through every point, never past its neighbours.
local function tangents(xs, ys)
	local n, d, t = #xs, {}, {}
	for k = 1, n - 1 do d[k] = (ys[k + 1] - ys[k]) / (xs[k + 1] - xs[k]) end
	t[1], t[n] = d[1], d[n - 1]
	for k = 2, n - 1 do
		t[k] = (d[k - 1] * d[k] <= 0) and 0 or (d[k - 1] + d[k]) / 2
	end
	for k = 1, n - 1 do
		if d[k] == 0 then
			t[k], t[k + 1] = 0, 0
		else
			local a, b = t[k] / d[k], t[k + 1] / d[k]
			local h = a * a + b * b
			if h > 9 then
				local s = 3 / math.sqrt(h)
				t[k], t[k + 1] = s * a * d[k], s * b * d[k]
			end
		end
	end
	return t
end

-- The ground line in world units, a point every STEP: { x1, y1, x2, y2, … }.
local function line(map)
	local g, xs, ys = map.ground, {}, {}
	for k = 1, #g, 2 do
		table.insert(xs, U.length(g[k]))
		table.insert(ys, -U.length(g[k + 1]))
	end
	local t = map.smooth ~= false and tangents(xs, ys)
	local points, seg = {}, 1
	for x = 0, xs[#xs], STEP do
		while x > xs[seg + 1] do seg = seg + 1 end
		local x0, x1, y0, y1 = xs[seg], xs[seg + 1], ys[seg], ys[seg + 1]
		local h = x1 - x0
		local u = (x - x0) / h
		local y
		if u == 0 then y = y0
		elseif u == 1 then y = y1
		elseif not t then y = y0 + (y1 - y0) * u
		else
			local u2, u3 = u * u, u * u * u
			y = (2 * u3 - 3 * u2 + 1) * y0 + (u3 - 2 * u2 + u) * h * t[seg]
				+ (-2 * u3 + 3 * u2) * y1 + (u3 - u2) * h * t[seg + 1]
		end
		table.insert(points, x)
		table.insert(points, y)
	end
	return points
end

-- Refuses a bad map whole, naming the field (and for ground, the point). Returns the line.
function S.check(map)
	if type(map) ~= "table" then fail("a map is a table") end
	for k in pairs(map) do
		if not FIELDS[k] then fail("unknown field " .. tostring(k)) end
	end
	if map.name ~= nil and type(map.name) ~= "string" then fail("name must be a string") end
	if map.smooth ~= nil and type(map.smooth) ~= "boolean" then fail("smooth must be true or false") end
	local g = map.ground
	if type(g) ~= "table" or #g < 4 then fail("ground needs at least 2 points") end
	if #g % 2 ~= 0 then fail("ground is x, height pairs: an odd count") end
	for k = 1, #g, 2 do
		local i, x, h = math.floor((k + 1) / 2), g[k], g[k + 1]
		if not number(x) or not number(h) then fail("ground point " .. i .. " is not two numbers") end
		if k == 1 and x ~= 0 then fail("the first ground x must be 0, not " .. m(x)) end
		if x * 2 ~= math.floor(x * 2) then
			fail("ground point " .. i .. " (x " .. m(x) .. "): put control points on the half metre")
		end
		if k > 1 and x <= g[k - 2] then
			fail("ground point " .. i .. " (x " .. m(x) .. ") must be right of point " .. (i - 1) .. " (x " .. m(g[k - 2]) .. ")")
		end
	end
	local points = line(map)
	local most = math.tan(math.rad(MAX_SLOPE))
	for k = 3, #points, 2 do
		local slope = math.abs(points[k + 1] - points[k - 1]) / STEP
		if slope > most + 1e-9 then
			fail(string.format("ground too steep at %s (%.0f°, at most %d°): ease the points around it",
				m(U.metres(points[k - 2])), math.deg(math.atan(slope)), MAX_SLOPE))
		end
	end
	local length = g[#g - 1]
	local function on(x) return number(x) and x >= 0 and x <= length end
	if not on(map.start) then fail("start must be a metre on the ground (0 to " .. m(length) .. ")") end
	if not on(map.finish) or map.finish <= map.start then
		fail("finish must be on the ground (to " .. m(length) .. ") and past the start")
	end
	if map.fuel ~= nil then
		if type(map.fuel) ~= "table" then fail("fuel is a list of x") end
		for i, x in ipairs(map.fuel) do
			if not on(x) then fail("fuel " .. i .. " must be on the ground (0 to " .. m(length) .. ")") end
		end
	end
	if map.coins ~= nil then
		if type(map.coins) ~= "table" then fail("coins is a list of { x, count }") end
		for i, row in ipairs(map.coins) do
			local ok = type(row) == "table" and on(row[1]) and number(row[2]) and row[2] >= 1
				and row[2] == math.floor(row[2]) and on(row[1] + (row[2] - 1) * COIN_GAP)
			if not ok then fail("coins " .. i .. " must be { x, count } with the whole row on the ground") end
		end
	end
	if map.scenery ~= nil then
		if type(map.scenery) ~= "table" then fail("scenery is a list of { art, x }") end
		for i, piece in ipairs(map.scenery) do
			if type(piece) ~= "table" then fail("scenery " .. i .. " must be { art, x }") end
			for k in pairs(piece) do
				if not SCENERY_FIELDS[k] then fail("scenery " .. i .. ": unknown field " .. tostring(k)) end
			end
			if type(piece[1]) ~= "string" or piece[1]:sub(-4) ~= ".svg" then
				fail("scenery " .. i .. ": art must be an .svg file name")
			end
			if not on(piece[2]) then fail("scenery " .. i .. " must be on the ground") end
			if piece.front ~= nil and type(piece.front) ~= "boolean" then fail("scenery " .. i .. ": front is true or false") end
		end
	end
	return points
end

-- The stage a map describes (checked first): what run.lua, art.lua and main.lua read.
function S.from_map(map)
	local points = S.check(map)
	local stage = { name = map.name or "Untitled", points = points, step = STEP, max_slope = MAX_SLOPE,
		finish_x = U.length(map.finish) }
	local start_x = U.length(map.start)
	stage.start = { start_x, S.ground_y(stage, start_x) - CHASSIS_CLEAR }

	local items, by_id = {}, {}
	local function add(kind, x, above, n)
		local item = { id = kind .. n, kind = kind, at = { x, S.ground_y(stage, x) - above } }
		table.insert(items, item)
		by_id[item.id] = item
	end
	for i, x in ipairs(map.fuel or {}) do add("fuel", U.length(x), U.length(FUEL_UP), i) end
	local n = 0
	for _, row in ipairs(map.coins or {}) do
		for k = 0, row[2] - 1 do
			n = n + 1
			add("coin", U.length(row[1]) + k * U.length(COIN_GAP), U.length(COIN_UP), n)
		end
	end
	table.sort(items, function(p, q) return p.at[1] < q.at[1] end)
	stage.items, stage.items_by_id = items, by_id

	stage.scenery = {}
	for _, piece in ipairs(map.scenery or {}) do
		table.insert(stage.scenery, { art = piece[1], x = U.length(piece[2]), front = piece.front == true })
	end
	return stage
end

-- The ground's height (world y) under world x; flat past either end.
function S.ground_y(stage, x)
	local p, step = stage.points, stage.step
	local last = #p / 2
	local i = math.floor(x / step) + 1
	if i < 1 then return p[2] end
	if i >= last then return p[#p] end
	local x0, y0, y1 = p[2 * i - 1], p[2 * i], p[2 * i + 2]
	return y0 + (y1 - y0) * (x - x0) / step
end

-- Park–Miller: exact in doubles (products stay under 2^53) and in Lua integers.
local function rng(seed)
	local state = (seed % 2147483646) + 1
	return function()
		state = (state * 16807) % 2147483647
		return state / 2147483647
	end
end

-- A seeded 1 km map: a flat start, then hills that grow harder and climb overall.
function S.generate(seed)
	local LENGTH = U.length(1000)
	local FLAT = U.length(15)
	local rand = rng(seed or 1)
	local phase = { rand() * 6.283, rand() * 6.283, rand() * 6.283 }
	local function target(x)
		if x < FLAT then return 0 end
		local d = x - FLAT
		-- Gentle at first, then crests that throw a car flat out into the air and climbs
		-- steep enough to stand it on its tail.
		local hard = 0.4 + 1.1 * math.min(1, d / (LENGTH * 0.5))
		local wave = 220 * math.sin(d / 1000 + phase[1]) + 110 * math.sin(d / 380 + phase[2])
			+ 40 * math.sin(d / 150 + phase[3])
		local ramp = math.min(1, d / U.length(20)) -- ease in from the flat
		return ramp * (hard * wave - d * 0.03)   -- and climb overall (−y is up)
	end
	local most = math.tan(math.rad(MAX_SLOPE)) * STEP
	local ground, y = {}, 0
	for x = 0, LENGTH, STEP do
		local dy = math.max(-most, math.min(most, target(x) - y))
		if x > 0 then y = y + dy end
		table.insert(ground, U.metres(x))
		table.insert(ground, -U.metres(y))
	end
	local finish = U.metres(LENGTH) - 10
	local fuel, coins = {}, {}
	local x = 140
	while x < finish do
		table.insert(fuel, x)
		x = x + 180 + 60 * rand()
	end
	x = 25
	while x < finish do
		table.insert(coins, { x, 5 })
		x = x + 30 + 20 * rand()
	end
	return { name = "Seed " .. tostring(seed or 1), ground = ground, smooth = false, start = 3,
		finish = finish, fuel = fuel, coins = coins }
end

return S
