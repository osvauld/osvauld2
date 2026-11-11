-- Load-time checks of the rules' data: the stage and the moves. Not a rule module: it runs when the
-- rules are built, never inside `step`, so it may walk tables with `pairs` (it sorts what it finds).
-- The rule modules (fighter, combat) stay clear of `pairs` (P-H5). Plans: brawl-movement.md P-B7,
-- brawl-fighting.md P-H1.
local K = {}

-- The stage ------------------------------------------------------------------------------------

local SHAPES = {
	stage = { { "name", "string" }, { "solids", "table" }, { "platforms", "table" }, { "spawns", "table" },
		{ "respawn", "table" }, { "blast", "table" }, { "camera", "table" } },
	solid = { { "id", "string" }, { "x", "number" }, { "y", "number" }, { "w", "number" }, { "h", "number" } },
	platform = { { "id", "string" }, { "x", "number" }, { "y", "number" }, { "w", "number" } },
	point = { { "x", "number" }, { "y", "number" } },
	rect = { { "x", "number" }, { "y", "number" }, { "w", "number" }, { "h", "number" } },
}

local function bad(where, why) error("stage: " .. where .. ": " .. why, 0) end

local function unknown_of(t, known)
	local unknown = {}
	for k in pairs(t) do
		if not known[k] then unknown[#unknown + 1] = tostring(k) end
	end
	table.sort(unknown)
	return unknown[1]
end

local function fields(t, shape, where)
	if type(t) ~= "table" then bad(where, "not a table") end
	local known = {}
	for _, f in ipairs(SHAPES[shape]) do known[f[1]] = true end
	local u = unknown_of(t, known)
	if u then bad(where, "unknown field " .. u) end
	for _, f in ipairs(SHAPES[shape]) do
		if t[f[1]] == nil then bad(where, "no " .. (f[1] == "w" and "width" or f[1] == "h" and "height" or f[1])) end
		if type(t[f[1]]) ~= f[2] then bad(where, f[1] .. " must be a " .. f[2]) end
	end
end

local function list(t, shape, where)
	for i, v in ipairs(t) do
		local at = where .. "[" .. i .. "]"
		if type(v) == "table" and type(v.id) == "string" then at = at .. " \"" .. v.id .. "\"" end
		fields(v, shape, at)
		if v.w ~= nil and not (v.w > 0) then bad(at, "w (width) must be above 0") end
		if v.h ~= nil and not (v.h > 0) then bad(at, "h (height) must be above 0") end
	end
end

local function inside(r, outer)
	return r.x >= outer.x and r.y >= outer.y and r.x + r.w <= outer.x + outer.w and r.y + (r.h or 0) <= outer.y + outer.h
end

-- The stage, checked: unknown fields, a platform with no width, a spawn inside a solid, or a blast
-- zone that doesn't hold the solids and platforms are errors naming the entry.
function K.stage(stage, body)
	body = body or { w = 80, h = 200 }
	fields(stage, "stage", "stage")
	list(stage.solids, "solid", "solids")
	list(stage.platforms, "platform", "platforms")
	list(stage.spawns, "point", "spawns")
	if not stage.spawns[1] then bad("spawns", "none") end
	fields(stage.respawn, "point", "respawn")
	fields(stage.blast, "rect", "blast")
	fields(stage.camera, "rect", "camera")
	local function clear(p, where)
		for i, s in ipairs(stage.solids) do
			if p.x + body.w / 2 > s.x and p.x - body.w / 2 < s.x + s.w and p.y > s.y and p.y - body.h < s.y + s.h then
				bad(where, "inside solids[" .. i .. "] \"" .. s.id .. "\"")
			end
		end
	end
	for i, p in ipairs(stage.spawns) do clear(p, "spawns[" .. i .. "]") end
	clear(stage.respawn, "respawn")
	for i, s in ipairs(stage.solids) do
		if not inside(s, stage.blast) then bad("solids[" .. i .. "] \"" .. s.id .. "\"", "outside the blast zone") end
	end
	for i, p in ipairs(stage.platforms) do
		if not inside(p, stage.blast) then bad("platforms[" .. i .. "] \"" .. p.id .. "\"", "outside the blast zone") end
	end
	return stage
end

-- The moves (P-H1) -----------------------------------------------------------------------------

local MOVE = { clip = "string", air = "boolean", startup = "number", active = "number", recovery = "number",
	windows = "table", lunge = "table", land_lag = "number", stall = "boolean", fall = "number", until_land = "boolean" }
local WINDOW = { from = "number", to = "number", dmg = "number", base = "number", growth = "number", angle = "number",
	circles = "table" }
-- `at`: a point of the setup drawing on `part`, which the bake carries through the clip (step 3).
local CIRCLE = { part = "string", r = "number", at = "table" }
local LUNGE = { from = "number", to = "number", speed = "number" }

local function on_grid(v, n) return v * n == math.floor(v * n) end

local function shape(t, spec, where)
	if type(t) ~= "table" then error(where .. ": not a table", 0) end
	local u = unknown_of(t, spec)
	if u then error(where .. ": unknown field " .. u, 0) end
	for k, kind in pairs(spec) do
		if t[k] ~= nil and type(t[k]) ~= kind then error(where .. "." .. k .. " must be a " .. kind, 0) end
	end
end

-- `moves` is moves.lua, `baked` moves_baked.lua, `clips` (optional) { [name] = length in seconds }
-- for clips that exist (a move whose clip is missing is drawn on a stand-in, look.lua). Returns the
-- rules' table: per move its frame data, `total`, and `ticks` (the baked circles per move tick).
function K.moves(moves, baked, clips)
	local angles = {}
	for _, a in ipairs(baked.angles) do angles[a[1]] = { x = a[2], y = a[3] } end
	local names = {}
	for name in pairs(moves) do names[#names + 1] = name end
	table.sort(names)
	local out = {}
	for _, name in ipairs(names) do
		local m, at = moves[name], "moves." .. name
		shape(m, MOVE, at)
		for _, k in ipairs({ "clip", "startup", "active", "recovery", "windows" }) do
			if m[k] == nil then error(at .. ": no " .. k, 0) end
		end
		for _, k in ipairs({ "startup", "active", "recovery" }) do
			if m[k] < 0 or m[k] % 2 ~= 0 then error(at .. "." .. k .. " must be even (a picture frame is 2 ticks): " .. m[k], 0) end
		end
		local total = m.startup + m.active + m.recovery
		if clips and clips[m.clip] and clips[m.clip] * 120 ~= total then
			error(at .. ": clip " .. m.clip .. " is " .. string.format("%g", clips[m.clip] * 120) .. " ticks, the move " .. total, 0)
		end
		if m.air and m.land_lag == nil then error(at .. ": an aerial needs land_lag", 0) end
		if m.until_land and not m.air then error(at .. ": until_land is for aerials", 0) end
		if #m.windows == 0 then error(at .. ": no windows", 0) end
		for wi, w in ipairs(m.windows) do
			local wat = at .. ".windows[" .. wi .. "]"
			shape(w, WINDOW, wat)
			for k in pairs(WINDOW) do
				if w[k] == nil then error(wat .. ": no " .. k, 0) end
			end
			if w.from < m.startup + 1 or w.to > m.startup + m.active or w.from > w.to then
				error(wat .. ": ticks " .. w.from .. "-" .. w.to .. " are not inside active " .. (m.startup + 1) .. "-" .. (m.startup + m.active), 0)
			end
			if w.dmg < 1 or w.dmg ~= math.floor(w.dmg) then error(wat .. ".dmg must be a whole number >= 1: " .. w.dmg, 0) end
			if not on_grid(w.base, 64) then error(wat .. ".base is not on the 1/64 grid: " .. w.base, 0) end
			if not on_grid(w.growth, 64) then error(wat .. ".growth is not on the 1/64 grid: " .. w.growth, 0) end
			if not angles[w.angle] then error(wat .. ".angle " .. w.angle .. " is not in the angle table", 0) end
			if w.angle > 180 and w.angle < 360 and not angles[360 - w.angle] then
				error(wat .. ".angle " .. w.angle .. ": its mirror " .. (360 - w.angle) .. " is not in the angle table", 0)
			end
			if #w.circles == 0 then error(wat .. ": no circles", 0) end
			for ci, c in ipairs(w.circles) do
				local cat = wat .. ".circles[" .. ci .. "]"
				shape(c, CIRCLE, cat)
				if not (c.part and c.at and c.r and c.r > 0) then error(cat .. ": needs part, at and r > 0", 0) end
			end
		end
		if m.lunge then
			shape(m.lunge, LUNGE, at .. ".lunge")
			if not (m.lunge.from and m.lunge.to and m.lunge.speed) then error(at .. ".lunge: needs from, to, speed", 0) end
		end
		local b = baked.moves[name]
		if not b then error(at .. ": not in moves_baked.lua (run scripts/bake_brawl_moves.lua)", 0) end
		if #b.ticks ~= total then error(at .. ": baked for " .. #b.ticks .. " ticks, the move has " .. total, 0) end
		out[name] = { clip = m.clip, air = m.air == true, startup = m.startup, active = m.active, recovery = m.recovery,
			total = total, windows = m.windows, lunge = m.lunge, land_lag = m.land_lag or 0, stall = m.stall == true,
			fall = m.fall, until_land = m.until_land == true, ticks = b.ticks, reach = b.reach }
	end
	for name in pairs(baked.moves) do
		if not moves[name] then error("moves_baked.lua: " .. name .. " is not in moves.lua (rebake)", 0) end
	end
	return out, angles
end

return K
