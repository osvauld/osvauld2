-- Hillclimb maps in plain Lua (plan: docs/design/hillclimb-maps.md §0, P-M1..P-M3).
--   lua scripts/test_hillclimb_maps.lua
package.path = "demo_apps/hillclimb/?.lua;" .. package.path
local Stage = require("stage")

local function near(a, b, eps) return math.abs(a - b) <= (eps or 1e-9) end

local function good()
	return {
		name = "Test Hills",
		ground = { 0, 0, 15, 0, 30, 4, 45, -2, 60, 6, 80, 6, 100, 0 },
		start = 3,
		finish = 95,
		fuel = { 40, 70 },
		coins = { { 20, 5 }, { 85, 3 } },
		scenery = { { "art/tree.svg", 25 }, { "art/rock.svg", 52, front = true } },
	}
end

local function refuses(edit, want)
	local map = good()
	edit(map)
	local ok, err = pcall(Stage.check, map)
	assert(not ok, "should refuse: " .. want)
	assert(tostring(err):find(want, 1, true), "error should say '" .. want .. "', got: " .. tostring(err))
end

-- P-M1: a good map passes; each bad one is refused with a message naming the field and point.
Stage.check(good())
Stage.check(require("maps/seeded"))
refuses(function(m) m.hills = true end, "unknown field hills")
refuses(function(m) m.ground = { 0, 0 } end, "at least 2 points")
refuses(function(m) m.ground = { 0, 0, 10, 0, 20 } end, "x, height pairs")
refuses(function(m) m.ground[5] = 10 end, "ground point 3 (x 10 m)")
refuses(function(m) m.ground[1] = 1 end, "first ground x must be 0")
refuses(function(m) m.ground[3] = 15.3 end, "half metre")
refuses(function(m) m.ground[6] = 30 end, "too steep")
refuses(function(m) m.start = 120 end, "start")
refuses(function(m) m.finish = 2 end, "finish")
refuses(function(m) m.fuel[2] = 200 end, "fuel 2")
refuses(function(m) m.coins[2] = { 99, 5 } end, "coins 2")
refuses(function(m) m.coins[1] = { 20, 0 } end, "coins 1")
refuses(function(m) m.scenery[1] = { "art/tree.png", 25 } end, "scenery 1")
refuses(function(m) m.scenery[2].behind = true end, "scenery 2: unknown field behind")

-- The too-steep error says where, in metres.
do
	local map = good()
	map.ground[6] = 30
	local _, err = pcall(Stage.check, map)
	assert(tostring(err):find(" m ", 1, true) and tostring(err):find("40°", 1, true), err)
end

-- P-M2: the line is every 0.5 m, through every control point, never past its neighbours.
local map = good()
local s = Stage.from_map(map)
assert(s.step == 50 and s.points[1] == 0, "a point every 0.5 m from 0")
for i = 3, #s.points, 2 do assert(near(s.points[i] - s.points[i - 2], 50), "evenly spaced") end
assert(s.points[#s.points - 1] == 10000, "to the last control point")
local g = map.ground
for k = 1, #g, 2 do
	assert(near(Stage.ground_y(s, g[k] * 100), -g[k + 1] * 100, 1e-6), "through control point " .. (k + 1) / 2)
end
for k = 1, #g - 2, 2 do
	local lo, hi = math.min(-g[k + 1], -g[k + 3]) * 100, math.max(-g[k + 1], -g[k + 3]) * 100
	for x = g[k] * 100, g[k + 2] * 100, 50 do
		local y = Stage.ground_y(s, x)
		assert(y >= lo - 1e-6 and y <= hi + 1e-6, "no overshoot at " .. x / 100 .. " m")
	end
end
assert(s.finish_x == 9500 and s.start[1] == 300, "start and finish in units")
local fuel, coins = 0, 0
for _, item in ipairs(s.items) do
	local above = Stage.ground_y(s, item.at[1]) - item.at[2]
	if item.kind == "fuel" then fuel = fuel + 1; assert(near(above, 60, 1e-6), "a can 0.6 m up")
	else coins = coins + 1; assert(near(above, 70, 1e-6), "a coin 0.7 m up") end
	assert(s.items_by_id[item.id] == item)
end
assert(fuel == 2 and coins == 8, "every can and coin")
assert(s.items_by_id.coin2.at[1] == 2060, "a row's coins 0.6 m apart")
assert(#s.scenery == 2 and s.scenery[1].art == "art/tree.svg" and s.scenery[1].x == 2500, "scenery in units")
assert(not s.scenery[1].front and s.scenery[2].front, "front only where asked")
assert(s.name == "Test Hills")

-- Straight lines when asked.
map = good()
map.smooth = false
s = Stage.from_map(map)
assert(near(Stage.ground_y(s, 2250), -200, 1e-9), "half way up a straight segment")

-- P-M3: the seeded stage survives as a map. Fingerprint of the old Stage.build(7) (2026-11-05).
s = Stage.from_map(Stage.generate(7))
local sx, sy, sxy = 0, 0, 0
for i = 1, #s.points, 2 do
	sx, sy = sx + s.points[i], sy + s.points[i + 1]
	sxy = sxy + s.points[i] * s.points[i + 1] * 1e-6
end
assert(#s.points == 4002, "same length: " .. #s.points)
assert(near(sx, 100050000, 1e-3) and near(sy, -2906498.412710, 1e-3) and near(sxy, -195217.185960, 1e-3),
	string.format("same points: %.6f %.6f %.6f", sx, sy, sxy))
local ix, iy = 0, 0
for _, item in ipairs(s.items) do ix, iy = ix + item.at[1], iy + item.at[2] end
assert(#s.items == 135 and near(ix, 6673820.926358, 1e-3) and near(iy, -203807.984022, 1e-3),
	string.format("same items: %d %.6f %.6f", #s.items, ix, iy))
assert(s.items[1].id == "coin1" and s.items[#s.items].id == "fuel5", "same item order")
assert(s.start[1] == 300 and near(s.start[2], -98, 1e-9) and s.finish_x == 99000, "same start and finish")

print("hillclimb maps: ok")
