-- Checks the baked maps (baked/maps.lua; three arenas and the yard): format, geometry, spawns, weapon spawns and a
-- jetpack-reach graph. Stock lua, from the worktree root:  lua scripts/test_jetpack_maps.lua
-- `--broken` runs the checks on a deliberately bad map and exits non-zero (to watch them fail).
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local F = require("format")
local T = require("theme")

-- Reach model from theme numbers. Net climb accel = thrust - gravity; a full tank (100, -32/s) lasts
-- 3.1 s. Hops are capped at 420 up (80% of a tank, ~550) and 480 across; falls drift while dropping.
local RX, RY = T.rx, T.ry
local net = T.thrust - T.gravity
local MAX_UP, MAX_DX = 420, 480
local function climb_time(r) return r / T.max_rise + T.max_rise / net / 2 end -- ramp-up + cruise
assert(climb_time(MAX_UP) * 32 <= 100, "hop cap exceeds one tank")

local function hit(a, b) return a.x < b.x + b.w and b.x < a.x + a.w and a.y < b.y + b.h and b.y < a.y + a.h end
-- Free of every solid except those in `skip`?
local function free(plats, r, skip)
	for _, p in ipairs(plats) do
		if not (skip and skip[p]) and hit(r, p) then return false, p end
	end
	return true
end
local function body(x, base) return { x = x - RX, y = base - 2 * RY, w = 2 * RX, h = 2 * RY } end

local function samples(p, lo, hi, n)
	lo, hi = math.max(lo, p.x + RX + 2), math.min(hi, p.x + p.w - RX - 2)
	local out = {}
	if hi < lo then return out end
	local step = math.max(10, (hi - lo) / n)
	local x = lo
	while x <= hi do out[#out + 1] = x; x = x + step end
	if out[#out] ~= hi then out[#out + 1] = hi end
	return out
end

-- Can a soldier get from `a` to `b`? Path: rise (or fall) in one column, move across at the higher level.
local function hop(plats, a, b)
	local up = a.y - b.y -- >0: b is higher
	if up > MAX_UP then return false end
	if math.max(b.x - (a.x + a.w), a.x - (b.x + b.w), 0) > MAX_DX then return false end
	local skip = { [a] = true, [b] = true }
	for _, xa in ipairs(samples(a, b.x - MAX_DX, b.x + b.w + MAX_DX, 40)) do
		if free(plats, body(xa, a.y), { [a] = true }) then
			local lo, hi = math.max(xa - MAX_DX, b.x), math.min(xa + MAX_DX, b.x + b.w)
			for _, xb in ipairs(samples(b, lo, hi, 7)) do
				if free(plats, body(xb, b.y), { [b] = true }) then
					local level = math.min(a.y, b.y) -- the higher of the two surfaces
					local col_top = level - 2 * RY
					local ok
					if up >= 0 then -- climb at xa, then cross at b's level
						ok = free(plats, { x = xa - RX, y = col_top, w = 2 * RX, h = a.y - col_top }, skip)
							and free(plats, { x = math.min(xa, xb) - RX, y = col_top, w = math.abs(xb - xa) + 2 * RX, h = 2 * RY }, skip)
					else -- cross at a's level, then fall at xb
						local drop = b.y - a.y
						local drift = T.speed * (drop / T.max_fall + 0.3)
						ok = math.abs(xb - xa) <= math.max(MAX_DX, drift)
							and free(plats, { x = math.min(xa, xb) - RX, y = col_top, w = math.abs(xb - xa) + 2 * RX, h = 2 * RY }, skip)
							and free(plats, { x = xb - RX, y = col_top, w = 2 * RX, h = b.y - col_top }, skip)
					end
					if ok then return true end
				end
			end
		end
	end
	return false
end

local function standable(plats, p)
	for _, x in ipairs(samples(p, p.x, p.x + p.w, 20)) do
		if free(plats, body(x, p.y), { [p] = true }) then return true end
	end
	return false
end

local function reach_from(nodes, edges, start)
	local seen, queue = { [start] = true }, { start }
	for _, n in ipairs(queue) do
		for _, m in ipairs(edges[n] or {}) do
			if not seen[m] then seen[m] = true; queue[#queue + 1] = m end
		end
	end
	return seen
end

-- All problems with one map (a raw map with weapon_spawns), as a list of strings.
local function check(raw)
	local errs = {}
	local function err(...) errs[#errs + 1] = raw.id .. ": " .. string.format(...) end
	local plain = {}
	for k, v in pairs(raw) do if k ~= "weapon_spawns" then plain[k] = v end end
	local m, why = F.map(plain)
	if not m then err("F.map: %s", tostring(why)); return errs end
	local plats = m.platforms
	for _, p in ipairs(plats) do
		if p.x < 0 or p.y < 0 or p.x + p.w > m.w or p.y + p.h > m.h then err("platform %s outside the map", p.id) end
	end
	-- standable nodes
	local nodes = {}
	for _, p in ipairs(plats) do
		if standable(plats, p) then nodes[#nodes + 1] = p
		elseif p.kind ~= "ground" then err("platform %s (%s) is buried: its top cannot be stood on", p.id, p.kind) end
	end
	local owner = {} -- the platform a spawn stands on
	local function standing(x, y)
		for _, p in ipairs(nodes) do
			if x >= p.x and x <= p.x + p.w and math.abs(p.y - (y + RY)) <= 2 then return p end
		end
	end
	for _, s in ipairs(m.spawns) do
		local ok, blocker = free(plats, body(s.x, s.y + RY), nil)
		if not ok then err("spawn %s is inside solid %s", s.id, blocker.id) end
		owner[s] = standing(s.x, s.y)
		if not owner[s] then err("spawn %s is not standing on a floor", s.id) end
	end
	local pcount, bcount = 0, 0
	for _, s in ipairs(m.spawns) do if s.team == "player" then pcount = pcount + 1 else bcount = bcount + 1 end end
	if pcount < 1 or bcount < 4 then err("needs 1+ player and 4+ bot spawns (has %d/%d)", pcount, bcount) end
	for _, w in ipairs(raw.weapon_spawns or {}) do
		if type(w.pool) ~= "table" or #w.pool == 0 or type(w.respawn) ~= "number" then err("weapon %s malformed", w.id) end
		if w.x < 0 or w.x > m.w or w.y < 0 or w.y > m.h then err("weapon %s outside the map", w.id) end
		local ok, blocker = free(plats, { x = w.x - 6, y = w.y - 6, w = 12, h = 12 }, nil)
		if not ok then err("weapon %s is inside solid %s", w.id, blocker.id) end
		local on
		for _, p in ipairs(nodes) do
			if w.x >= p.x and w.x <= p.x + p.w and p.y >= w.y and p.y - w.y <= 30 then on = p end
		end
		if not on then err("weapon %s has no floor within 30 below", w.id) end
	end
	-- spawns apart: no two within 150 units
	for i, a in ipairs(m.spawns) do
		for j = i + 1, #m.spawns do
			local b = m.spawns[j]
			if math.abs(a.x - b.x) < 150 and math.abs(a.y - b.y) < 150 then err("spawns %s and %s are on top of each other", a.id, b.id) end
		end
	end
	-- reach graph over standable platforms, both directions
	local fwd, back = {}, {}
	for _, a in ipairs(nodes) do
		fwd[a], back[a] = fwd[a] or {}, back[a] or {}
	end
	for _, a in ipairs(nodes) do
		for _, b in ipairs(nodes) do
			if a ~= b and hop(plats, a, b) then
				table.insert(fwd[a], b); table.insert(back[b], a)
			end
		end
	end
	local start
	for _, s in ipairs(m.spawns) do if s.team == "player" then start = start or owner[s] end end
	if start then
		local out, into = reach_from(nodes, fwd, start), reach_from(nodes, back, start)
		for _, p in ipairs(nodes) do
			if not out[p] then err("platform %s (x=%d y=%d) is unreachable from the player spawn", p.id, p.x, p.y)
			elseif not into[p] then err("platform %s (x=%d y=%d) is a trap: no way back to the player spawn", p.id, p.x, p.y) end
		end
	end
	return errs, #nodes
end

local maps = dofile("demo_apps/jetpack/baked/maps.lua")

if arg[1] == "--broken" then
	local bad = { id = "broken", w = 1000, h = 600, platforms = {
		{ id = "floor", kind = "ground", x = 0, y = 560, w = 1000, h = 40 },
		{ id = "far", kind = "ledge", x = 880, y = 100, w = 100, h = 18 }, -- 460 up, out of tank range
		{ id = "wall", kind = "ground", x = 200, y = 400, w = 100, h = 160 },
	}, spawns = { { id = "p", team = "player", x = 450, y = 541 }, { id = "b", team = "bot", x = 100, y = 541 } },
		weapon_spawns = { { id = "w", x = 450, y = 450, pool = { "rifle" }, respawn = 20 } } }
	local errs = check(bad)
	for _, e in ipairs(errs) do print("FAIL " .. e) end
	os.exit(#errs > 0 and 1 or 0)
end

-- the checks must catch a broken map
do
	local bad = { id = "broken", w = 1000, h = 600, platforms = {
		{ id = "floor", kind = "ground", x = 0, y = 560, w = 1000, h = 40 },
		{ id = "far", kind = "ledge", x = 880, y = 100, w = 100, h = 18 },
	}, spawns = { { id = "p", team = "player", x = 450, y = 450 }, { id = "b", team = "bot", x = 100, y = 541 } } }
	assert(#check(bad) >= 2, "checks missed a broken map")
end

assert(#maps.maps == 4, "expected three arenas and the yard")
assert(F.find(maps.maps, "yard").id == "yard")
local same = io.open("demo_apps/jetpack_map_editor/baked/maps.lua"):read("a") == io.open("demo_apps/jetpack/baked/maps.lua"):read("a")
assert(same, "the map editor's baked maps differ from the game's")
local failed = 0
for _, m in ipairs(maps.maps) do
	if m.id == "yard" then goto continue end -- small and hand-made: not an arena
	assert(m.w >= 3200 and m.w <= 4800 and m.h >= 1400 and m.h <= 2400, m.id .. " size outside Mini Militia range")
	assert(m.w > 960 and m.h > 540)
	local errs, n = check(m)
	for _, e in ipairs(errs) do print("FAIL " .. e); failed = failed + 1 end
	print(string.format("%s: %dx%d, %d platforms (%d standable), %d spawns, %d weapon spawns",
		m.id, m.w, m.h, #m.platforms, n or 0, #m.spawns, #m.weapon_spawns))
	assert(m.theme, m.id .. " needs a theme")
	::continue::
end
if failed > 0 then print(failed .. " problem(s)"); os.exit(1) end
print("jetpack maps ok")
