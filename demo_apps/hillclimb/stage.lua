-- A stage from a seed: the ground as one line (world units, the entity placed at 0, 0), where
-- the car starts, and the fuel cans and coins along it. Pure data; the same seed is the same
-- stage in Luau and in plain Lua (the generator is integer-exact in both).
local U = require("units")
local S = {}

local STEP = 50                   -- units between ground points (0.5 m)
local LENGTH = U.length(1000)     -- 1 km of road
local FLAT = U.length(15)         -- a flat start
local MAX_SLOPE = 40              -- degrees, no steeper anywhere
local CHASSIS_CLEAR = 98          -- chassis top above the ground at the start (wheel bottoms 94 below it)

-- Park–Miller: exact in doubles (products stay under 2^53) and in Lua integers.
local function rng(seed)
	local state = (seed % 2147483646) + 1
	return function()
		state = (state * 16807) % 2147483647
		return state / 2147483647
	end
end

function S.build(seed)
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
	local points, y = {}, 0
	for x = 0, LENGTH, STEP do
		local dy = math.max(-most, math.min(most, target(x) - y))
		if x > 0 then y = y + dy end
		table.insert(points, x)
		table.insert(points, y)
	end
	local stage = { points = points, step = STEP, max_slope = MAX_SLOPE, finish_x = LENGTH - U.length(10) }
	local start_x = 300
	stage.start = { start_x, S.ground_y(stage, start_x) - CHASSIS_CLEAR }

	local items, by_id = {}, {}
	local function add(kind, x, above, n)
		local item = { id = kind .. n, kind = kind, at = { x, S.ground_y(stage, x) - above } }
		table.insert(items, item)
		by_id[item.id] = item
	end
	local n = 0
	local x = U.length(140)
	while x < stage.finish_x do
		n = n + 1
		add("fuel", x, 60, n)
		x = x + U.length(180 + 60 * rand())
	end
	n = 0
	x = U.length(25)
	while x < stage.finish_x do
		for k = 0, 4 do
			n = n + 1
			add("coin", x + k * 60, 70, n)
		end
		x = x + U.length(30 + 20 * rand())
	end
	table.sort(items, function(p, q) return p.at[1] < q.at[1] end)
	stage.items, stage.items_by_id = items, by_id
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

return S
