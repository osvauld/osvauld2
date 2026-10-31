-- Jetpack content formats (maps, pilots, guns), shared by the game and its editors (each app ships an identical copy;
-- scripts/test_jetpack_format.lua checks they match). Reads a doc mirror or a baked table and
-- returns plain tables, or nil and the first bad field.
local F = {}

-- Per kind: maps are still version 1; pilots are version 2 (a rig) and read version 1 through F.upgrade;
-- guns are version 1.
F.versions = { maps = 1, pilots = 2, guns = 1 }
F.kinds = { ground = true, ledge = true, cover = true }
F.teams = { player = true, bot = true }
F.limits = { min_w = 640, max_w = 20000, min_h = 360, max_h = 8000, min_side = 8, platforms = 512, spawns = 32,
	weapon_spawns = 64, pool = 16, min_respawn = 1, max_respawn = 600, loadout = 2 }

local function num(v, lo, hi)
	return type(v) == "number" and v == v and v >= lo and v <= hi
end

local function list(v)
	local out = {}
	if v == nil then return out end
	if type(v) ~= "table" then return nil end
	for i = 1, #v do out[i] = v[i] end
	return out
end

local function bad(where, why) return nil, where .. ": " .. why end

function F.map(t)
	if type(t) ~= "table" then return bad("map", "not a table") end
	local L = F.limits
	if type(t.id) ~= "string" or t.id == "" then return bad("map", "needs an id") end
	local where = "map " .. t.id
	if not num(t.w, L.min_w, L.max_w) then return bad(where, "w out of range") end
	if not num(t.h, L.min_h, L.max_h) then return bad(where, "h out of range") end
	local m = { id = t.id, name = type(t.name) == "string" and t.name or t.id, w = t.w, h = t.h,
		platforms = {}, spawns = {} }
	local seen = {}
	local platforms = list(t.platforms)
	if not platforms then return bad(where, "platforms is not a list") end
	if #platforms > L.platforms then return bad(where, "too many platforms") end
	for _, p in ipairs(platforms) do
		local id = type(p) == "table" and p.id
		if type(id) ~= "string" or id == "" or seen[id] then return bad(where, "platform without a unique id") end
		seen[id] = true
		local at = where .. " platform " .. id
		if not F.kinds[p.kind] then return bad(at, "kind must be ground, ledge or cover") end
		if not (num(p.w, L.min_side, t.w) and num(p.h, L.min_side, t.h)) then return bad(at, "size out of range") end
		if not (num(p.x, 0, t.w - p.w) and num(p.y, 0, t.h - p.h)) then return bad(at, "outside the map") end
		table.insert(m.platforms, { id = id, kind = p.kind, x = p.x, y = p.y, w = p.w, h = p.h })
	end
	local spawns = list(t.spawns)
	if not spawns then return bad(where, "spawns is not a list") end
	if #spawns > L.spawns then return bad(where, "too many spawns") end
	local teams = {}
	for _, s in ipairs(spawns) do
		local id = type(s) == "table" and s.id
		if type(id) ~= "string" or id == "" or seen[id] then return bad(where, "spawn without a unique id") end
		seen[id] = true
		local at = where .. " spawn " .. id
		if not F.teams[s.team] then return bad(at, "team must be player or bot") end
		if not (num(s.x, 0, t.w) and num(s.y, 0, t.h)) then return bad(at, "outside the map") end
		teams[s.team] = (teams[s.team] or 0) + 1
		table.insert(m.spawns, { id = id, team = s.team, x = s.x, y = s.y })
	end
	if not teams.player then return bad(where, "needs a player spawn") end
	if not teams.bot then return bad(where, "needs a bot spawn") end
	-- Optional: the background palette's name (the view falls back to "yard" for one it lacks) ...
	if t.theme ~= nil then
		if type(t.theme) ~= "string" or t.theme == "" or #t.theme > 32 then return bad(where, "theme must be a short name") end
		m.theme = t.theme
	end
	-- ... the starting loadout: one or two gun ids, in hand first (the game's default when absent) ...
	if t.loadout ~= nil then
		local given = list(t.loadout)
		if not given or #given == 0 or #given > L.loadout then return bad(where, "loadout must list 1 to " .. L.loadout .. " guns") end
		m.loadout = {}
		for _, gun in ipairs(given) do
			if type(gun) ~= "string" or gun == "" then return bad(where, "loadout holds a gun id") end
			table.insert(m.loadout, gun)
		end
	end
	-- ... and the weapon spawns [{ id, x, y, pool = { gun id, ... }, respawn }]: a pool picks one gun at random,
	-- a gun listed twice twice as often; `respawn` is the seconds after a taken gun until the next.
	local weapons = list(t.weapon_spawns)
	if not weapons then return bad(where, "weapon_spawns is not a list") end
	if #weapons > L.weapon_spawns then return bad(where, "too many weapon spawns") end
	if t.weapon_spawns ~= nil then
		m.weapon_spawns = {}
		local wseen = {}
		for _, w in ipairs(weapons) do
			local id = type(w) == "table" and w.id
			if type(id) ~= "string" or id == "" or wseen[id] then return bad(where, "weapon spawn without a unique id") end
			wseen[id] = true
			local at = where .. " weapon spawn " .. id
			if not (num(w.x, 0, t.w) and num(w.y, 0, t.h)) then return bad(at, "outside the map") end
			if not num(w.respawn, L.min_respawn, L.max_respawn) then return bad(at, "respawn out of range") end
			local pool, given = {}, list(w.pool)
			if not given or #given == 0 or #given > L.pool then return bad(at, "pool must list 1 to " .. L.pool .. " guns") end
			for _, gun in ipairs(given) do
				if type(gun) ~= "string" or gun == "" then return bad(at, "pool holds a gun id") end
				table.insert(pool, gun)
			end
			table.insert(m.weapon_spawns, { id = id, x = w.x, y = w.y, pool = pool, respawn = w.respawn })
		end
	end
	return m
end

-- Every valid entry of `t[key]`, checked by `one(raw, version)`; the first error is returned beside
-- them. `accept` lists the info.versions this kind reads.
local function all(t, key, one, accept)
	local version = type(t) == "table" and type(t.info) == "table" and t.info.version
	local known = false
	for _, v in ipairs(accept) do known = known or v == version end
	if not known then return {}, key .. ": wrong or missing info.version" end
	local out, first, ids = {}, nil, {}
	for _, raw in ipairs(list(t[key]) or {}) do
		local v, err = one(raw, version)
		if v and ids[v.id] then v, err = nil, v.id .. ": a second entry with this id" end
		if v then ids[v.id] = true; table.insert(out, v) else first = first or err end
	end
	return out, first
end

-- The asset's entries if it has any valid one, else the baked ones. `err` names what was skipped.
local function load(asset, baked, key, one, accept)
	local mine, err = all(asset, key, one, accept)
	if asset == nil or (asset.info == nil and asset[key] == nil) then err = nil end -- never written
	if #mine > 0 then return mine, err end
	return (all(baked, key, one, accept)), err
end

function F.maps(t) return all(t, "maps", F.map, { F.versions.maps }) end
-- Shipped maps stay playable beside the workspace's: an asset map replaces the baked one with
-- its id, and baked maps the asset doesn't name are added after it.
function F.load_maps(asset, baked)
	local mine, err = load(asset, baked, "maps", F.map, { F.versions.maps })
	local have = {}
	for _, m in ipairs(mine) do have[m.id] = true end
	for _, m in ipairs((all(baked, "maps", F.map, { F.versions.maps }))) do
		if not have[m.id] then table.insert(mine, m) end
	end
	return mine, err
end

-- Pilots ------------------------------------------------------------------------------------

-- Parts are in draw order; the first F.split are the back drawing (free arm, legs, jetpack, torso,
-- head), the rest the front one (the gun arm), so the gun draws between them. Each part is
-- posed about its pivot (the joint) and follows its parent. Drawn at rest: both arms reaching
-- straight forward (aim 0, facing right).
F.parts = { "arm_free", "forearm_free", "hand_free", "leg_back", "leg_front", "jetpack", "torso", "head",
	"arm_gun", "forearm_gun", "hand_gun", "finger_gun", "thumb_gun" }
F.split = 8
F.parents = { forearm_free = "arm_free", hand_free = "forearm_free", head = "torso", forearm_gun = "arm_gun",
	hand_gun = "forearm_gun", finger_gun = "hand_gun", thumb_gun = "hand_gun" }
-- palm_gun: on hand_gun, where the gun's grip sits. palm_free: on hand_free, where it grabs the gun's
-- support. hold: where the grip sits at aim 0 (the gun swings about the gun shoulder at that
-- radius). jet: flame top. eye: where the head looks from.
F.points = { "palm_gun", "palm_free", "hold", "jet", "eye" }
F.team_vars = { "main", "dark" }
F.pilot_limits = { min_side = 8, max_side = 256, shapes = 256, commands = 128, min_scale = 0.05, max_scale = 4 }
local arity = { move = 2, line = 2, quad = 4, cubic = 6, close = 0 }

local function hex(v) return type(v) == "string" and v:match("^#%x+$") ~= nil and (#v == 7 or #v == 9) end
F.hex = hex

local function point(v, w, h)
	return type(v) == "table" and num(v[1], 0, w) and num(v[2], 0, h)
end

local function path(p, w, h, at)
	if type(p) ~= "table" or #p == 0 then return bad(at, "needs a path") end
	if #p > F.pilot_limits.commands then return bad(at, "path too long") end
	local out, open = {}, false
	for i = 1, #p do
		local c = p[i]
		local n = type(c) == "table" and arity[c[1]]
		if not n or #c ~= n + 1 then return bad(at, "path command " .. i .. " is malformed") end
		if i == 1 and c[1] ~= "move" then return bad(at, "path must start with move") end
		-- gfx.path refuses drawing after a close until the next move.
		if c[1] ~= "move" and not open then return bad(at, "path command " .. i .. " draws after close") end
		open = c[1] ~= "close"
		local copy = { c[1] }
		for k = 2, n + 1 do
			-- A point may sit a little outside the box (a curve's control), never wildly.
			if not num(c[k], -w, 2 * w > 2 * h and 2 * w or 2 * h) then
				return bad(at, "path command " .. i .. " has a bad number")
			end
			copy[k] = c[k]
		end
		out[i] = copy
	end
	return out
end

-- A shape's path checked as gfx.path will take it, for a `w`×`h` pilot; nil and why if not.
F.path = path

-- A checked version 2 pilot, or nil and why. Version 1 pilots go through F.upgrade first.
function F.pilot(t)
	if type(t) ~= "table" then return bad("pilot", "not a table") end
	if type(t.id) ~= "string" or t.id == "" then return bad("pilot", "needs an id") end
	local where, L = "pilot " .. t.id, F.pilot_limits
	local size = t.size
	if not (type(size) == "table" and num(size[1], L.min_side, L.max_side) and num(size[2], L.min_side, L.max_side)) then
		return bad(where, "size out of range")
	end
	if not num(t.scale, L.min_scale, L.max_scale) then return bad(where, "scale (game units per drawing unit) out of range") end
	local w, h = size[1], size[2]
	if type(t.vars) ~= "table" or not (t.vars.main and t.vars.dark) then return bad(where, "needs vars main and dark") end
	local vars = {}
	for k, v in pairs(t.vars) do
		if type(k) ~= "string" or not hex(v) then return bad(where .. " var " .. tostring(k), "not a #hex colour") end
		vars[k] = v
	end
	local names = table.concat(F.points, ", ", 1, #F.points - 1) .. " and " .. F.points[#F.points]
	if type(t.points) ~= "table" then return bad(where, "needs points " .. names) end
	local points = {}
	for _, name in ipairs(F.points) do
		local p = t.points[name]
		if p == nil then return bad(where, "needs points " .. names) end
		if not point(p, w, h) then return bad(where .. " point " .. name, "outside the pilot") end
		points[name] = { p[1], p[2] }
	end
	local raw = list(t.parts)
	local order = "parts must be " .. table.concat(F.parts, ", ")
	if not raw or #raw ~= #F.parts then return bad(where, order) end
	local parts, seen, count = {}, {}, 0
	for i, part in ipairs(raw) do
		if type(part) ~= "table" or part.id ~= F.parts[i] then return bad(where, order) end
		local at = where .. " part " .. part.id
		local parent = F.parents[part.id]
		if part.parent ~= parent then return bad(at, "parent must be " .. (parent or "none")) end
		if not point(part.pivot, w, h) then return bad(at, "needs a pivot inside the pilot") end
		local shapes = list(part.shapes)
		if not shapes then return bad(at, "shapes is not a list") end
		local out = { id = part.id, parent = parent, pivot = { part.pivot[1], part.pivot[2] }, shapes = {} }
		for _, s in ipairs(shapes) do
			local id = type(s) == "table" and s.id
			if type(id) ~= "string" or id == "" or seen[id] then return bad(at, "shape without a unique id") end
			seen[id] = true
			count = count + 1
			if count > L.shapes then return bad(where, "too many shapes") end
			local sat = at .. " shape " .. id
			if s.fill == nil then return bad(sat, "needs a fill") end
			if not hex(s.fill) and not (type(s.fill) == "string" and vars[s.fill:match("^@(.+)$") or ""]) then
				return bad(sat, "fill names no var and is no #hex colour")
			end
			local p, err = path(s.path, w, h, sat)
			if not p then return nil, err end
			table.insert(out.shapes, { id = id, fill = s.fill, path = p })
		end
		parts[i] = out
	end
	return { id = t.id, name = type(t.name) == "string" and t.name or t.id, size = { w, h }, scale = t.scale,
		vars = vars, points = points, parts = parts }
end

-- Version 1 pilots (three parts, or five with arms; `grip`, `hand`, `jet` points) as raw version 2
-- tables: everything doubled, scale 0.5, `body` is the torso, the arms are upper arms with a tiny
-- empty forearm and hand so they still turn as one piece, the new parts are empty. Not checked: pass
-- the result to F.pilot. Anything that is not a pilot comes back as it is.
local legacy_ids = { "leg_back", "leg_front", "body" }
local function twice(v)
	if type(v) == "number" then return v * 2 end
	if type(v) ~= "table" then return v end
	local out = {}
	for k, x in pairs(v) do out[k] = (k == 1 and type(x) == "string") and x or twice(x) end
	return out
end
local function plain_shapes(v)
	local out = {}
	for _, s in ipairs(list(v) or {}) do
		if type(s) == "table" then out[#out + 1] = { id = s.id, fill = s.fill, path = twice(list(s.path) or {}) } end
	end
	return out
end
local function between(a, b, k) return { a[1] + (b[1] - a[1]) * k, a[2] + (b[2] - a[2]) * k } end
function F.upgrade(t)
	if type(t) ~= "table" or type(t.size) ~= "table" or type(t.points) ~= "table" then return t end
	local raw = list(t.parts)
	if not raw or (#raw ~= 3 and #raw ~= 5) then return t end
	local by = {}
	for _, p in ipairs(raw) do if type(p) == "table" and type(p.id) == "string" then by[p.id] = p end end
	for _, id in ipairs(legacy_ids) do if not by[id] then return t end end
	local pt = t.points
	if type(pt.grip) ~= "table" or type(pt.jet) ~= "table" then return t end
	local grip, jet = twice({ pt.grip[1], pt.grip[2] }), twice({ pt.jet[1], pt.jet[2] })
	local hand = twice({ (pt.hand or pt.grip)[1], (pt.hand or pt.grip)[2] })
	local function pivot(id, fallback)
		local p = by[id] and by[id].pivot
		return type(p) == "table" and twice({ p[1], p[2] }) or fallback
	end
	local gun_s, free_s = pivot("arm_gun", grip), pivot("arm_free", hand)
	local torso = by.body
	local tp = pivot("body", { t.size[1], t.size[2] })
	local neck = { tp[1], math.max(0, tp[2] - 16) }
	local function part(id, parent, at, shapes) return { id = id, parent = parent, pivot = at, shapes = shapes or {} } end
	local elbow_f, wrist_f = between(free_s, hand, 0.9), between(free_s, hand, 0.95)
	local elbow_g, wrist_g = between(gun_s, grip, 0.9), between(gun_s, grip, 0.95)
	return {
		id = t.id, name = t.name, scale = 0.5, size = twice({ t.size[1], t.size[2] }), vars = t.vars,
		points = { palm_gun = grip, palm_free = hand, hold = grip, jet = jet, eye = { neck[1] + 8, neck[2] - 10 } },
		parts = {
			part("arm_free", nil, free_s, plain_shapes(by.arm_free and by.arm_free.shapes)),
			part("forearm_free", "arm_free", elbow_f), part("hand_free", "forearm_free", wrist_f),
			part("leg_back", nil, pivot("leg_back", tp), plain_shapes(by.leg_back.shapes)),
			part("leg_front", nil, pivot("leg_front", tp), plain_shapes(by.leg_front.shapes)),
			part("jetpack", nil, jet), part("torso", nil, tp, plain_shapes(torso.shapes)), part("head", "torso", neck),
			part("arm_gun", nil, gun_s, plain_shapes(by.arm_gun and by.arm_gun.shapes)),
			part("forearm_gun", "arm_gun", elbow_g), part("hand_gun", "forearm_gun", wrist_g),
			part("finger_gun", "hand_gun", grip), part("thumb_gun", "hand_gun", grip),
		},
	}
end

local function pilot_of(raw, version)
	if version == 1 then raw = F.upgrade(raw) end
	return F.pilot(raw)
end
local pilot_versions = { 1, 2 }
function F.pilots(t) return all(t, "pilots", pilot_of, pilot_versions) end
function F.load_pilots(asset, baked) return load(asset, baked, "pilots", pilot_of, pilot_versions) end

-- The `gfx.drawing` module of a checked pilot: size, vars and parts (with their parents), without
-- the shape ids. `which` is "back" or "front" for the two drawings the pilot is drawn as; none
-- gives all parts at once.
function F.drawing(p, which)
	local parts = {}
	for i, part in ipairs(p.parts) do
		if which == nil or (which == "back") == (i <= F.split) then
			local shapes = {}
			for j, s in ipairs(part.shapes) do shapes[j] = { fill = s.fill, path = s.path } end
			parts[#parts + 1] = { id = part.id, parent = part.parent, pivot = part.pivot, shapes = shapes }
		end
	end
	return { size = p.size, vars = p.vars, parts = parts }
end

-- Guns -------------------------------------------------------------------------------------

-- A gun is a drawing like a pilot (own palette, parts with pivots, shapes with ids) pointing right, with
-- points and stats. Parts are `body` and `mag` (required) with optional `slide`, `bolt`, `barrel`,
-- in this draw order. `mag` is the detachable magazine; `bolt`/`slide` kick back on firing.
-- Points are in drawing units: grip (the gun hand's palm), support (the free hand; missing = a
-- one-handed gun), muzzle (shots leave), eject (casing port), mag (magazine well). `scale` is game
-- units per drawing unit; the scout is drawn at 0.5 and so are the guns, so a rifle is 50 units long.
F.gun_parts = { "mag", "body", "slide", "bolt", "barrel" }
F.gun_points = { "grip", "support", "muzzle", "eject", "mag" }
F.gun_required = { "grip", "muzzle", "eject", "mag" }
F.gun_limits = { min_side = 8, max_side = 256, shapes = 256, min_scale = 0.05, max_scale = 2 }
-- Stat ranges { lo, hi, whole }: stops a 0-interval or 10000-damage gun.
F.stat_limits = {
	damage = { 1, 200 }, fire_interval = { 0.03, 5 }, shot_speed = { 100, 5000 }, spread = { 0, 0.5 },
	pellets = { 1, 20, true }, mag_size = { 1, 200, true }, reload_time = { 0.2, 10 }, range = { 50, 5000 },
	recoil = { 0, 30 }, melee = { 0, 200 }, splash = { 1, 500 },
}
F.stat_order = { "damage", "fire_interval", "shot_speed", "spread", "pellets", "mag_size", "reload_time", "range",
	"recoil", "melee" }
-- Zoom levels (view-state only): a list of camera factors, first 1, strictly ascending, each up to `max`;
-- a level above 1 widens the view by that factor. v1 guns had one `zoom` number (0.5-3): it upgrades to
-- { 1, zoom } (or { 1 } when it is 1 or less).
F.zoom_limits = { max = 4, count = 6, old_max = 3 }

-- Whether `list` is a legal zoom list.
function F.zooms_ok(list)
	local L = F.zoom_limits
	if type(list) ~= "table" or #list < 1 or #list > L.count then return false end
	local count = 0
	for _ in pairs(list) do count = count + 1 end
	if count ~= #list or list[1] ~= 1 then return false end
	for i, v in ipairs(list) do
		if not num(v, 1, L.max) or (i > 1 and v <= list[i - 1]) then return false end
	end
	return true
end

-- Whether `v` is a legal value of stat `name`.
function F.stat_ok(name, v)
	local l = F.stat_limits[name]
	return l ~= nil and num(v, l[1], l[2]) and (not l[3] or v % 1 == 0)
end

-- A checked gun, or nil and why.
function F.gun(t)
	if type(t) ~= "table" then return bad("gun", "not a table") end
	if type(t.id) ~= "string" or t.id == "" then return bad("gun", "needs an id") end
	local where, L = "gun " .. t.id, F.gun_limits
	local size = t.size
	if not (type(size) == "table" and num(size[1], L.min_side, L.max_side) and num(size[2], L.min_side, L.max_side)) then
		return bad(where, "size out of range")
	end
	if not num(t.scale, L.min_scale, L.max_scale) then return bad(where, "scale (game units per drawing unit) out of range") end
	local w, h = size[1], size[2]
	if type(t.vars) ~= "table" then return bad(where, "needs vars") end
	local vars = {}
	for k, v in pairs(t.vars) do
		if type(k) ~= "string" or not hex(v) then return bad(where .. " var " .. tostring(k), "not a #hex colour") end
		vars[k] = v
	end
	local names = table.concat(F.gun_required, ", ", 1, #F.gun_required - 1) .. " and " .. F.gun_required[#F.gun_required]
	if type(t.points) ~= "table" then return bad(where, "needs points " .. names) end
	local known = {}
	for _, name in ipairs(F.gun_points) do known[name] = true end
	for name in pairs(t.points) do
		if not known[name] then return bad(where .. " point " .. tostring(name), "not a gun point") end
	end
	local points = {}
	for _, name in ipairs(F.gun_points) do
		local p = t.points[name]
		if p == nil then
			for _, need in ipairs(F.gun_required) do
				if need == name then return bad(where, "needs points " .. names) end
			end
		else
			if not point(p, w, h) then return bad(where .. " point " .. name, "outside the gun") end
			points[name] = { p[1], p[2] }
		end
	end
	local raw = list(t.parts)
	if not raw then return bad(where, "parts is not a list") end
	local order, rank = "parts must be in the order " .. table.concat(F.gun_parts, ", "), {}
	for i, id in ipairs(F.gun_parts) do rank[id] = i end
	local parts, seen, have, count, last = {}, {}, {}, 0, 0
	for _, part in ipairs(raw) do
		if type(part) ~= "table" or not rank[part.id] then return bad(where .. " part " .. tostring(type(part) == "table" and part.id), "not a gun part") end
		if rank[part.id] <= last then return bad(where, order) end
		last, have[part.id] = rank[part.id], true
		local at = where .. " part " .. part.id
		if not point(part.pivot, w, h) then return bad(at, "needs a pivot inside the gun") end
		local shapes = list(part.shapes)
		if not shapes then return bad(at, "shapes is not a list") end
		local out = { id = part.id, pivot = { part.pivot[1], part.pivot[2] }, shapes = {} }
		for _, s in ipairs(shapes) do
			local id = type(s) == "table" and s.id
			if type(id) ~= "string" or id == "" or seen[id] then return bad(at, "shape without a unique id") end
			seen[id] = true
			count = count + 1
			if count > L.shapes then return bad(where, "too many shapes") end
			local sat = at .. " shape " .. id
			if s.fill == nil then return bad(sat, "needs a fill") end
			if not hex(s.fill) and not (type(s.fill) == "string" and vars[s.fill:match("^@(.+)$") or ""]) then
				return bad(sat, "fill names no var and is no #hex colour")
			end
			local p, err = path(s.path, w, h, sat)
			if not p then return nil, err end
			table.insert(out.shapes, { id = id, fill = s.fill, path = p })
		end
		table.insert(parts, out)
	end
	if not (have.body and have.mag) then return bad(where, "needs the parts body and mag") end
	local st = t.stats
	if type(st) ~= "table" then return bad(where, "needs stats") end
	local stats = {}
	for _, name in ipairs(F.stat_order) do
		if not F.stat_ok(name, st[name]) then return bad(where .. " stat " .. name, "out of range") end
		stats[name] = st[name]
	end
	local zooms = st.zooms
	if zooms == nil and st.zoom ~= nil then
		if not num(st.zoom, 0.5, F.zoom_limits.old_max) then return bad(where .. " stat zoom", "out of range") end
		zooms = st.zoom > 1 and { 1, st.zoom } or { 1 }
	end
	if not F.zooms_ok(zooms) then
		return bad(where .. " stat zooms", "must be 1 to " .. F.zoom_limits.count .. " ascending numbers from 1 to " .. F.zoom_limits.max .. ", the first 1")
	end
	stats.zooms = {}
	for i, v in ipairs(zooms) do stats.zooms[i] = v end
	if type(st.auto) ~= "boolean" then return bad(where .. " stat auto", "must be true or false") end
	stats.auto = st.auto
	if st.kind ~= "bullet" and st.kind ~= "rocket" then return bad(where .. " stat kind", "must be bullet or rocket") end
	stats.kind = st.kind
	if st.kind == "rocket" then
		if not F.stat_ok("splash", st.splash) then return bad(where .. " stat splash", "a rocket needs a splash radius") end
		stats.splash = st.splash
	elseif st.splash ~= nil then
		return bad(where .. " stat splash", "splash is for rockets")
	end
	return { id = t.id, name = type(t.name) == "string" and t.name or t.id, size = { w, h }, scale = t.scale,
		vars = vars, points = points, parts = parts, stats = stats }
end

function F.guns(t) return all(t, "guns", F.gun, { F.versions.guns }) end
function F.load_guns(asset, baked) return load(asset, baked, "guns", F.gun, { F.versions.guns }) end

-- The `gfx.drawing` module of a checked gun: size, vars and parts, without the shape ids.
function F.gun_drawing(g)
	local parts = {}
	for i, part in ipairs(g.parts) do
		local shapes = {}
		for j, s in ipairs(part.shapes) do shapes[j] = { fill = s.fill, path = s.path } end
		parts[i] = { id = part.id, pivot = part.pivot, shapes = shapes }
	end
	return { size = g.size, vars = g.vars, parts = parts }
end

-- The player flies the first pilot; bot n flies the nth, cycling.
function F.pilot_for(pilots, team, n)
	if team == "player" or #pilots == 0 then return pilots[1] end
	return pilots[((n or 1) - 1) % #pilots + 1]
end

function F.find(maps, id)
	for _, m in ipairs(maps) do if m.id == id then return m end end
	return maps[1]
end

-- First spawn of a team, or the nth bot spawn (cycling).
function F.spawn(map, team, n)
	local found = {}
	for _, s in ipairs(map.spawns) do if s.team == team then table.insert(found, s) end end
	return found[((n or 1) - 1) % #found + 1]
end

return F
