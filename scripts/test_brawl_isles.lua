-- Brawl's second map, Twin Isles, in stock Lua (plan: docs/design/brawl-maps.md §0, P-K6 and P-K7).
--   lua scripts/test_brawl_isles.lua
package.path = "scripts/?.lua;scripts/fixtures/?.lua;" .. package.path
local U = require("brawl_test_util")
local Map = require("map")
local Library = require("library")
local lib = Library.load()
local T = U.tune
local BODY = { w = T.body_w, h = T.body_h }

local function compile(id) return Map.compile(require("maps/" .. id), lib, { body = BODY }) end

-- The assets a map names: its sky, its back entries and its pieces.
local function assets(id)
	local m = require("maps/" .. id)
	local set = { [m.sky] = true }
	for _, e in ipairs(m.back) do set[e.asset] = true end
	for _, e in ipairs(m.pieces) do set[e.asset] = true end
	return set
end

-- P-K6: two maps from one library. Both compile and check, they share at least 6 assets, and each
-- uses at least 3 the other doesn't.
do
	local index = require("maps/index")
	assert(index[1] == "yard" and index[2] == "isles", "P-K6: the index lists yard then isles")
	for _, id in ipairs(index) do
		assert(Map.check(require("maps/" .. id), lib))
		compile(id)
	end
	local a, b = assets("yard"), assets("isles")
	local shared, only_a, only_b = {}, {}, {}
	for n in pairs(a) do if b[n] then shared[#shared + 1] = n else only_a[#only_a + 1] = n end end
	for n in pairs(b) do if not a[n] then only_b[#only_b + 1] = n end end
	table.sort(shared); table.sort(only_a); table.sort(only_b)
	assert(#shared >= 6, "P-K6: shared " .. #shared .. ": " .. table.concat(shared, " "))
	assert(#only_a >= 3 and #only_b >= 3, "P-K6: only yard " .. #only_a .. ", only isles " .. #only_b)
	print(string.format("P-K6 shared %d (%s); yard only %d; isles only %d (%s)", #shared, table.concat(shared, " "), #only_a, #only_b,
		table.concat(only_b, " ")))
end

-- The layout of the plan (step 5): islands, bridge, side ledges, top platform, boulder.
local isles = compile("isles")
local stage = isles.stage
local function solid(id) for _, s in ipairs(stage.solids) do if s.id == id then return s end end error("no solid " .. id) end
local function platform(id) for i, p in ipairs(stage.platforms) do if p.id == id then return p, i end end error("no platform " .. id) end
do
	local l, r = solid("isle_l"), solid("isle_r")
	assert(l.x == -1100 and l.y == 0 and l.w == 840 and l.h == 600, "the left island")
	assert(r.x == 260 and r.y == 0 and r.w == 840 and r.h == 600, "the right island")
	local br = platform("bridge")
	assert(br.x == -260 and br.y == 0 and br.w == 520, "the bridge spans the pit at y 0")
	local ll, lr, top = platform("ledge_l"), platform("ledge_r"), platform("top")
	assert(ll.x == -900 and ll.y == -280 and ll.w == 280 and lr.x == 620 and lr.y == -280 and lr.w == 280, "side ledges")
	assert(top.x == -160 and top.y == -420 and top.w == 320, "the top platform")
	local b = solid("boulder")
	assert(b.w == 156 and b.h == 156 and b.x + b.w / 2 == -480 and b.y == -156, "the boulder, 156 x 156 at x -480")
	assert(stage.spawns[1].x == -700 and stage.spawns[1].y == 0 and stage.spawns[2].x == 700, "spawns")
	assert(stage.respawn.x == 0 and stage.respawn.y == -700, "respawn")
	assert(stage.name == "isles", "named isles")
end

local function rules() return U.F.rules({ stage = stage, tune = T }) end
local function fresh()
	local r = rules()
	return U.F.new(r, 1), r
end
local function go(s, r, input, n, until_)
	for i = 1, n do
		U.step(s, r, input)
		if until_ and until_(s.fighters[1], i) then return i end
	end
end
local function air(x, y, vy)
	local s, r = fresh()
	local f = s.fighters[1]
	f.x, f.y, f.vx, f.vy, f.grounded, f.on, f.act = x, y, 0, vy or 0, false, 0, "fall"
	f.take_y, f.top_y, f.coyote = y, y, 0
	return s, r, f
end

-- P-K7 (a): a fighter spawns standing on the left island.
do
	local s = fresh()
	local f = s.fighters[1]
	assert(f.x == -700 and f.y == 0 and f.grounded and f.on == 0 and f.act == "idle", "spawns standing on the left island")
end

-- P-K7 (b): dropping through the bridge into the pit loses a stock; the respawn lands on the top
-- platform (y == -420).
do
	local s, r = fresh()
	local f = s.fighters[1]
	local _, bridge = platform("bridge")
	-- Stand on the bridge, as the respawn-and-walk would: place it there.
	f.x, f.y, f.grounded, f.on = 0, 0, true, bridge
	U.step(s, r)
	assert(f.grounded and f.on == bridge and f.y == 0, "standing on the bridge")
	U.step(s, r, U.input({ down_n = 1, y = 1 }))
	assert(not f.grounded and f.drop > 0, "Down drops through the bridge")
	local n = go(s, r, U.input({ down_n = 1 }), 600, function(g) return g.act == "out" end)
	assert(n and f.stocks == T.stocks - 1, "the pit loses a stock, after " .. tostring(n) .. " ticks, stocks " .. f.stocks)
	local m = go(s, r, U.input({ down_n = 1 }), 600, function(g) return g.grounded and g.act ~= "out" end)
	assert(m and f.y == -420 and f.on == select(2, platform("top")), "the respawn lands on the top platform, y=" .. f.y)
	assert(f.stocks == T.stocks - 1, "one stock lost")
end

-- P-K7 (c): on an island's inner wall the fighter can wall slide and wall jump.
for _, side in ipairs({ { x = -260 - 0 + T.body_w / 2 + 0, dir = -1, name = "left island" }, { x = 260 - T.body_w / 2, dir = 1, name = "right island" } }) do
	local s, r, f = air(side.x, 300, 10)
	U.step(s, r) -- touching is read at the end of a tick
	f.vy = 10
	assert(f.wall == side.dir, side.name .. ": touching the wall, wall=" .. f.wall)
	U.step(s, r, U.input({ x = side.dir }))
	assert(f.vy == T.wall_slide and f.act == "wall", side.name .. ": slides, vy=" .. f.vy .. " act=" .. f.act)
	U.step(s, r, U.input({ x = side.dir, jump_n = 1, jump = true }))
	assert(f.vx == -side.dir * T.wall_jump_x and f.vy < 0 and f.wall_jumps == T.wall_jumps - 1, side.name .. ": wall jump vx=" .. f.vx .. " vy=" .. f.vy)
end

-- P-K7 (d): the boulder stops a run (vx == 0 at its side), and the fighter can stand on it.
do
	local s, r = fresh()
	local f = s.fighters[1]
	local b = solid("boulder")
	go(s, r, U.input({ x = 1 }), 200, function(g) return g.x + T.body_w / 2 == b.x end)
	go(s, r, U.input({ x = 1 }), 3)
	assert(f.x == b.x - T.body_w / 2 and f.vx == 0, "the boulder stops the run at its side, x=" .. f.x .. " vx=" .. f.vx)
	assert(f.wall == 1, "touching the boulder's side")
	-- Jump onto it holding toward it until the feet clear its top, then let go and drift down onto it.
	U.step(s, r, U.input({ x = 1, jump = true, jump_n = 1 }))
	go(s, r, U.input({ x = 1, jump = true, jump_n = 1 }), 60, function(g) return g.y < b.y - 25 end)
	local m = go(s, r, U.input({ jump_n = 1 }), 200, function(g) return g.grounded end)
	assert(m and f.y == b.y and f.on == 0, "stands on the boulder, y=" .. f.y .. " (top " .. b.y .. ")")
	assert(f.x + T.body_w / 2 > b.x and f.x - T.body_w / 2 < b.x + b.w, "over the boulder")
end

print("brawl isles ok")
