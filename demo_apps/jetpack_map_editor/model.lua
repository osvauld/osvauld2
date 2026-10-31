-- The `jetpack-maps` workspace asset: every map the game can play. Seeded once from the baked
-- maps; after that the asset is the truth. One write per finished gesture.
local C = require("theme")
local F = require("format")
local M = {}

M.asset = asset:open("jetpack-maps")
M.map_id = "yard"

-- Doc writes are tagged at every depth: positional-only tables are lists, the rest maps.
local function tag(v)
	if type(v) ~= "table" then return v end
	local out, n = {}, 0
	for k, x in pairs(v) do
		out[k] = tag(x)
		n += 1
	end
	return n == #v and doc.list(out) or doc.map(out) -- an empty table is an empty list
end

if M.asset.info == nil then
	local baked = require("baked/maps")
	M.asset:set({ "maps" }, tag(baked.maps))
	M.asset:set({ "info" }, tag({ version = F.versions.maps }))
end

function M.snap(v) return math.floor(v / C.snap + 0.5) * C.snap end
local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- The raw map (drawn even when invalid) and its validation.
function M.current()
	for _, m in ipairs(M.asset.maps or {}) do
		if m.id == M.map_id then
			local ok, err = F.map(m)
			return m, ok, err
		end
	end
	return nil, nil, "no map " .. M.map_id
end

local function path(...) return { "maps", M.map_id, ... } end

function M.find(map, id)
	for _, p in ipairs(map.platforms or {}) do if p.id == id then return p, "platform" end end
	for _, s in ipairs(map.spawns or {}) do if s.id == id then return s, "spawn" end end
	for _, w in ipairs(map.weapon_spawns or {}) do if w.id == id then return w, "weapon" end end
end

-- The guns a weapon spawn can offer: the gun editor's asset if it holds valid ones, else the baked six.
M.guns = F.load_guns(asset:open("jetpack-guns"), require("baked/guns"))

-- A weapon spawn's weight for `gun`: how many times its pool lists it.
function M.weight(thing, gun)
	local n = 0
	for _, id in ipairs(thing.pool or {}) do if id == gun then n += 1 end end
	return n
end

-- Keeps a box inside the map, snapped (heights to whole units, so 18-high ledges stay).
-- `grow` keeps the corner where it is and stops the size at the map's edge instead.
function M.fit(map, x, y, w, h, grow)
	x, y = M.snap(x), M.snap(y)
	if grow then
		x, y = clamp(x, 0, map.w - F.limits.min_side), clamp(y, 0, map.h - F.limits.min_side)
		return x, y, clamp(M.snap(w), F.limits.min_side, map.w - x), clamp(math.floor(h + 0.5), F.limits.min_side, map.h - y)
	end
	w = clamp(M.snap(w), F.limits.min_side, map.w)
	h = clamp(math.floor(h + 0.5), F.limits.min_side, map.h)
	return clamp(x, 0, map.w - w), clamp(y, 0, map.h - h), w, h
end

M.notice = nil -- why the last add or resize was refused, shown in the status

function M.add_platform(map, kind, x, y)
	if #(map.platforms or {}) >= F.limits.platforms then
		M.notice = "At the limit of " .. F.limits.platforms .. " platforms"
		return nil
	end
	M.notice = nil
	local size = C.sizes[kind]
	local px, py, w, h = M.fit(map, x, y, size[1], size[2])
	local id = kind .. ":" .. string.sub(uuid(), 1, 8)
	M.asset:insert(path("platforms"), tag({ id = id, kind = kind, x = px, y = py, w = w, h = h }))
	return id
end

function M.place_platform(map, id, x, y, w, h, grow)
	local px, py, pw, ph = M.fit(map, x, y, w, h, grow)
	M.asset:set(path("platforms", id, "x"), px)
	M.asset:set(path("platforms", id, "y"), py)
	M.asset:set(path("platforms", id, "w"), pw)
	M.asset:set(path("platforms", id, "h"), ph)
end

function M.place_spawn(map, id, x, y)
	M.asset:set(path("spawns", id, "x"), clamp(M.snap(x), 0, map.w))
	M.asset:set(path("spawns", id, "y"), clamp(M.snap(y), 0, map.h))
end

function M.player_spawn(map)
	for _, s in ipairs(map.spawns or {}) do if s.team == "player" then return s end end
end

function M.add_bot(map, x, y)
	if #(map.spawns or {}) >= F.limits.spawns then
		M.notice = "At the limit of " .. F.limits.spawns .. " spawns"
		return nil
	end
	M.notice = nil
	local id = "bot:" .. string.sub(uuid(), 1, 8)
	M.asset:insert(path("spawns"), tag({ id = id, team = "bot",
		x = clamp(M.snap(x), 0, map.w), y = clamp(M.snap(y), 0, map.h) }))
	return id
end

-- Weapon spawns (step 5): a point with a pool of gun ids (a gun listed twice is twice as likely) and a
-- respawn time. A new one offers the first gun and respawns after 30 s.
M.respawn = 30
function M.add_weapon(map, x, y)
	if #(map.weapon_spawns or {}) >= F.limits.weapon_spawns then
		M.notice = "At the limit of " .. F.limits.weapon_spawns .. " weapon spawns"
		return nil
	end
	M.notice = nil
	local id = "weapon:" .. string.sub(uuid(), 1, 8)
	local spawn = tag({ id = id, x = clamp(M.snap(x), 0, map.w), y = clamp(M.snap(y), 0, map.h),
		pool = { M.guns[1].id }, respawn = M.respawn })
	if map.weapon_spawns == nil then M.asset:set(path("weapon_spawns"), doc.list({ spawn }))
	else M.asset:insert(path("weapon_spawns"), spawn) end
	return id
end

function M.place_weapon(map, id, x, y)
	M.asset:set(path("weapon_spawns", id, "x"), clamp(M.snap(x), 0, map.w))
	M.asset:set(path("weapon_spawns", id, "y"), clamp(M.snap(y), 0, map.h))
end

-- One more (or fewer) of `gun` in the pool; the pool never empties and never outgrows the limit.
function M.add_to_pool(thing, gun, delta)
	local pool, found = {}, false
	for _, id in ipairs(thing.pool or {}) do
		if id == gun and delta < 0 and not found then found = true else table.insert(pool, id) end
	end
	if delta > 0 then
		if #pool >= F.limits.pool then M.notice = "A pool lists at most " .. F.limits.pool .. " guns"; return false end
		table.insert(pool, gun)
	end
	if #pool == 0 then M.notice = "A weapon spawn needs at least one gun"; return false end
	M.notice = nil
	M.asset:set(path("weapon_spawns", thing.id, "pool"), tag(pool))
	return true
end

function M.set_respawn(thing, seconds)
	if type(seconds) ~= "number" or seconds < F.limits.min_respawn or seconds > F.limits.max_respawn then
		M.notice = "Respawn is " .. F.limits.min_respawn .. " to " .. F.limits.max_respawn .. " seconds"
		return false
	end
	M.notice = nil
	M.asset:set(path("weapon_spawns", thing.id, "respawn"), seconds)
	return true
end

-- The last player spawn and the last bot spawn stay: the map would not validate without them.
function M.delete(map, id)
	local thing, kind = M.find(map, id)
	if not thing then return false end
	if kind == "spawn" then
		local same = 0
		for _, s in ipairs(map.spawns) do if s.team == thing.team then same += 1 end end
		if same <= 1 then return false end
		M.asset:delete(path("spawns", id))
	elseif kind == "weapon" then
		M.asset:delete(path("weapon_spawns", id))
	else
		M.asset:delete(path("platforms", id))
	end
	return true
end

-- Growing just grows. Shrinking trims platforms that cross the new edge, in the same write
-- set; it is refused while anything starts beyond the edge — deleting is the author's call.
function M.set_size(map, key, value)
	local lo, hi = F.limits["min_" .. key], F.limits["max_" .. key]
	if type(value) ~= "number" or value < lo or value > hi then return false end
	value = M.snap(value)
	local at, side = key == "w" and "x" or "y", key
	for _, p in ipairs(map.platforms or {}) do
		if p[at] + F.limits.min_side > value then
			M.notice = "Move or delete " .. p.id .. " first: it starts past the new edge"
			return false
		end
	end
	for _, s in ipairs(map.spawns or {}) do
		if s[at] > value then
			M.notice = "Move or delete spawn " .. s.id .. " first: it is past the new edge"
			return false
		end
	end
	for _, w in ipairs(map.weapon_spawns or {}) do
		if w[at] > value then
			M.notice = "Move or delete " .. w.id .. " first: it is past the new edge"
			return false
		end
	end
	M.notice = nil
	for _, p in ipairs(map.platforms or {}) do
		if p[at] + p[side] > value then M.asset:set(path("platforms", p.id, side), value - p[at]) end
	end
	M.asset:set(path(key), value)
	return true
end

return M
