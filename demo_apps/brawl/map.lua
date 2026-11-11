-- Maps (docs/design/brawl-maps.md): plain data that only places assets from the library, compiled
-- into the two things the game needs, from the same placements so they can't drift apart:
--   stage  the rules' view (fighter.lua's stage shape, nothing else), what net.match carries;
--   scene  the view-only list of pictures to draw (scene_art.lua).
-- Pure: runs unchanged under stock Lua 5.4 and Luau (no `+=`, no goto/continue). Errors name the
-- piece: `pieces[8] "left".x: missing`.
local Library = require("library")
local M = {}

-- Depth layers (decision 6): allowed parallax factors and the default. Back entries sit on far,
-- mid or near; pieces on stage, front or fore.
local DEPTH = {
	far = { 0.02, 0.2, 0.1 },
	mid = { 0.2, 0.5, 0.3 },
	near = { 0.5, 0.85, 0.6 },
	fore = { 1.1, 1.5, 1.25 },
}
local BACK_LAYERS = { far = true, mid = true, near = true }
local PIECE_LAYERS = { stage = true, front = true, fore = true }

-- Which kinds go where. Haze, ridge and cloud are in the library but not drawn yet (step 4).
local BACK_KINDS = { layer = true, prop = true }
local PIECE_KINDS = { ground = true, platform = true, prop = true }

local MAP_FIELDS = { name = true, sky = true, home = true, bottom = false, back = true, pieces = true,
	spawns = true, respawn = true, blast = true, camera = true, shapes = false }
local BACK_FIELDS = { asset = true, x = true, y = true, scale = false, f = false, layer = false }
local PIECE_FIELDS = {
	ground = { asset = true, id = false, x = true, y = true, w = true, h = true, layer = false },
	platform = { asset = true, id = false, x = true, y = true, w = true, layer = false },
	prop = { asset = true, id = false, x = true, y = true, scale = false, flip = false, layer = false, f = false },
}

-- Layers behind the stage keep a few pixels overlapped between copies (step 3's seam fix); how many
-- copies is fixed for now. Step 4 computes it from the camera bounds.
local TILES = 3
local OVERLAP = 4

local function fail(where, msg) error({ brawl_map = where .. ": " .. msg }, 0) end

local function sorted_keys(t)
	local keys = {}
	for k in pairs(t) do keys[#keys + 1] = k end
	table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
	return keys
end

local function fields(t, allowed, where)
	if type(t) ~= "table" then fail(where == "" and "map" or where, "must be a table") end
	local function at(k) return where == "" and k or where .. "." .. k end
	for _, k in ipairs(sorted_keys(t)) do
		if allowed[k] == nil then fail(at(tostring(k)), "unknown field") end
	end
	for _, k in ipairs(sorted_keys(allowed)) do
		if allowed[k] and t[k] == nil then fail(at(k), "missing") end
	end
end

local function number(v, where)
	if type(v) ~= "number" or v ~= v or v == math.huge or v == -math.huge then fail(where, "needs a number") end
end

local function lookup(lib, name)
	if type(name) ~= "string" then return nil end
	local ok, a = pcall(Library.get, lib, name)
	if ok then return a end
	return nil
end

local function sky_of(lib, name)
	local a = lookup(lib, name)
	if a and a.kind == "sky" then return a end
	return nil
end

local function label(list, i, e)
	local at = list .. "[" .. i .. "]"
	if type(e) == "table" and type(e.id) == "string" then at = at .. ' "' .. e.id .. '"' end
	return at
end

local function range(lo, hi) return string.format("%g..%g", lo, hi) end

-- The depth factor of a placement: the layer's default, or the written one when it is in range.
local function depth(e, layer, where)
	local d = DEPTH[layer]
	if e.f == nil then return d and d[3] or 1 end
	number(e.f, where .. ".f")
	if not d then fail(where .. ".f", "only a layer behind or in front of the stage has a depth factor") end
	if e.f < d[1] or e.f > d[2] then fail(where .. ".f", "layer " .. layer .. " allows " .. range(d[1], d[2])) end
	return e.f
end

local function check_layer(e, a, plane, where)
	local layer = e.layer or a.layer
	if e.layer ~= nil then
		local ok = false
		for _, l in ipairs(Library.KINDS[a.kind].layers) do ok = ok or l == e.layer end
		if not ok then fail(where .. ".layer", "a " .. a.kind .. " can't use layer " .. tostring(e.layer)) end
	end
	if not plane[layer] then fail(where .. ".layer", "a " .. a.kind .. " here can't use layer " .. layer) end
	return layer
end

local function check_map(map, lib)
	fields(map, MAP_FIELDS, "")
	if type(map.name) ~= "string" then fail("name", "needs a string") end
	if not sky_of(lib, map.sky) then fail("sky", "unknown sky " .. tostring(map.sky)) end
	for _, k in ipairs({ "home", "respawn" }) do
		fields(map[k], { x = true, y = true }, k)
		number(map[k].x, k .. ".x")
		number(map[k].y, k .. ".y")
	end
	for _, k in ipairs({ "blast", "camera" }) do
		fields(map[k], { x = true, y = true, w = true, h = true }, k)
	end
	if map.bottom ~= nil then number(map.bottom, "bottom") end
	if type(map.spawns) ~= "table" or #map.spawns == 0 then fail("spawns", "needs at least one {x, y}") end
	if map.shapes ~= nil then fields(map.shapes, { solids = false, platforms = false }, "shapes") end
	if type(map.back) ~= "table" then fail("back", "must be a list") end
	for i, e in ipairs(map.back) do
		local at = label("back", i, e)
		fields(e, BACK_FIELDS, at)
		local a = lookup(lib, e.asset)
		if not a then fail(at .. ".asset", "unknown asset " .. tostring(e.asset)) end
		if not BACK_KINDS[a.kind] then fail(at .. ".asset", "a " .. a.kind .. " can't be placed in back") end
		number(e.x, at .. ".x")
		number(e.y, at .. ".y")
		if e.scale ~= nil then
			number(e.scale, at .. ".scale")
			if e.scale <= 0 then fail(at .. ".scale", "needs a number above 0") end
		end
		local layer = check_layer(e, a, BACK_LAYERS, at)
		depth(e, layer, at)
	end
	if type(map.pieces) ~= "table" then fail("pieces", "must be a list") end
	local ids = {}
	for i, e in ipairs(map.pieces) do
		local at = label("pieces", i, e)
		if type(e) ~= "table" then fail(at, "must be a table") end
		local a = lookup(lib, e.asset)
		if not a then fail(at .. ".asset", e.asset == nil and "missing" or "unknown asset " .. tostring(e.asset)) end
		if not PIECE_KINDS[a.kind] then fail(at .. ".asset", "a " .. a.kind .. " can't be placed in pieces") end
		fields(e, PIECE_FIELDS[a.kind], at)
		if e.id ~= nil then
			if type(e.id) ~= "string" or e.id == "" then fail(at .. ".id", "needs a name") end
			if ids[e.id] then fail(at .. ".id", "also pieces[" .. ids[e.id] .. "]") end
			ids[e.id] = i
		end
		local collides = a.kind == "ground" or a.kind == "platform" or (a.kind == "prop" and a.collide ~= nil)
		if collides and e.id == nil then fail(at .. ".id", "a " .. a.kind .. " makes collision and needs an id") end
		for _, k in ipairs({ "x", "y", "w", "h", "scale" }) do
			if e[k] ~= nil then number(e[k], at .. "." .. k) end
		end
		if (e.w ~= nil and e.w <= 0) or (e.h ~= nil and e.h <= 0) then fail(at, "w and h need numbers above 0") end
		if e.scale ~= nil and e.scale <= 0 then fail(at .. ".scale", "needs a number above 0") end
		if e.flip ~= nil and type(e.flip) ~= "boolean" then fail(at .. ".flip", "needs true or false") end
		local layer = check_layer(e, a, PIECE_LAYERS, at)
		if e.f ~= nil and layer ~= "fore" then fail(at .. ".f", "only a fore piece has a depth factor") end
		depth(e, layer, at)
	end
end

-- True, or nil and the first problem: the map's fields, then back in order, then pieces in order.
function M.check(map, lib)
	local ok, err = pcall(check_map, map, lib)
	if ok then return true end
	if type(err) == "table" and err.brawl_map then return nil, err.brawl_map end
	error(err, 0)
end

local function copy(v)
	if type(v) ~= "table" then return v end
	local out = {}
	for k, x in pairs(v) do out[k] = copy(x) end
	return out
end

-- A derived number must sit on the 1/8 grid, so the rules' `==` tests stay exact.
local function grid(v, at, what, scale)
	if v * 8 ~= math.floor(v * 8) then
		fail(at, string.format("%s = %.17g is off the 1/8 grid%s", what, v, scale and (" (scale " .. scale .. ")") or ""))
	end
	return v
end

-- The prop's template box in world units: mirrored about the anchor when flipped.
local function template(a, e, kind, at)
	local t = a.collide[kind]
	local sc, ax, ay = e.scale or a.scale, a.anchor[1], a.anchor[2]
	local left = t[1] * sc
	if e.flip then left = -(t[1] + t[3]) * sc end
	local box = { x = grid(e.x + left, at, "x", sc), y = grid(e.y + t[2] * sc, at, "y", sc), w = grid(t[3] * sc, at, "w", sc) }
	if kind == "solid" then box.h = grid(t[4] * sc, at, "h", sc) end
	return box
end

-- The rules' stage from the placements: solids and platforms in pieces order (a fighter's `on` is an
-- index into the platform list), then the map's invisible `shapes`. Assumes the map has been checked.
-- `opts.body` ({w, h}) is the fighter's box for fighter.check_stage.
function M.stage(map, lib, opts)
	opts = opts or {}
	local stage = {
		name = map.name, solids = {}, platforms = {}, spawns = copy(map.spawns), respawn = copy(map.respawn),
		blast = copy(map.blast), camera = copy(map.camera),
	}
	for i, e in ipairs(map.pieces) do
		local a = Library.get(lib, e.asset)
		local at = label("pieces", i, e)
		if a.kind == "ground" then
			stage.solids[#stage.solids + 1] = { id = e.id, x = grid(e.x, at, "x"), y = grid(e.y, at, "y"), w = grid(e.w, at, "w"), h = grid(e.h, at, "h") }
		elseif a.kind == "platform" then
			stage.platforms[#stage.platforms + 1] = { id = e.id, x = grid(e.x, at, "x"), y = grid(e.y, at, "y"), w = grid(e.w, at, "w") }
		elseif a.collide and a.collide.solid then
			local box = template(a, e, "solid", at)
			box.id = e.id
			stage.solids[#stage.solids + 1] = box
		elseif a.collide and a.collide.platform then
			local box = template(a, e, "platform", at)
			box.id = e.id
			stage.platforms[#stage.platforms + 1] = box
		end
	end
	if map.shapes then
		for _, s in ipairs(map.shapes.solids or {}) do stage.solids[#stage.solids + 1] = copy(s) end
		for _, s in ipairs(map.shapes.platforms or {}) do stage.platforms[#stage.platforms + 1] = copy(s) end
	end
	require("fighter").check_stage(stage, opts.body)
	return stage
end

-- A prop's picture placed by its anchor (mirrored about it when flipped); `f` is the caller's.
local function placed(a, e, layer)
	local sc = e.scale or a.scale
	local W = a.size[1]
	local left = e.flip and (W - a.anchor[1]) or a.anchor[1]
	return { kind = "prop", asset = e.asset, id = e.id, a = a, layer = layer, flip = e.flip or false,
		x = e.x - left * sc, y = e.y - a.anchor[2] * sc, s = sc, w = W * sc, f = 1 }
end

-- Everything to draw, in draw order: `under` is behind the fighters, `over` in front. Assumes the
-- map has been checked and `stage` is its compiled stage.
function M.scene(map, lib, stage)
	local scene = { name = map.name, home = copy(map.home), bottom = map.bottom or (map.camera.y + map.camera.h),
		under = {}, over = {}, images = {} }
	local sky = Library.get(lib, map.sky)
	scene.sky = { name = map.sky, stops = copy(sky.stops), sun = copy(sky.sun) }
	local used = {}
	local function use(a) for _, f in ipairs(Library.files_of(a)) do used[f] = true end end
	for _, e in ipairs(map.back) do
		local a = Library.get(lib, e.asset)
		local layer = e.layer or a.layer
		local f = depth(e, layer, "")
		use(a)
		if a.kind == "layer" then
			local sc = e.scale or a.scale
			local w = a.src and a.src[3] or a.size[1]
			local total = (w - OVERLAP) * (TILES - 1) + w
			scene.under[#scene.under + 1] = { kind = "layer", asset = e.asset, a = a, layer = layer, tile = TILES, s = sc, f = f,
				x = e.x - total * sc / 2, y = e.y, w = total * sc }
		else
			local item = placed(a, e, layer)
			item.f = f
			scene.under[#scene.under + 1] = item
		end
	end
	local front, fore = {}, {}
	local solids, platforms = {}, {}
	for _, s in ipairs(stage.solids) do solids[s.id] = s end
	for _, p in ipairs(stage.platforms) do platforms[p.id] = p end
	for _, e in ipairs(map.pieces) do
		local a = Library.get(lib, e.asset)
		use(a)
		local layer = e.layer or a.layer
		if a.kind == "ground" then
			local s = solids[e.id]
			local tw = a.top.size[1]
			local n = math.max(1, math.floor(s.w / tw + 0.5))
			local k = s.w / (n * tw)
			scene.under[#scene.under + 1] = { kind = "ground", asset = e.asset, id = e.id, a = a, layer = "stage", tiles = n, f = 1,
				s = k, x = s.x, y = s.y - a.lift * k, w = s.w, bottom = map.camera.y + map.camera.h }
		elseif a.kind == "platform" then
			local p = platforms[e.id]
			local k = p.w / a.size[1]
			scene.under[#scene.under + 1] = { kind = "platform", asset = e.asset, id = e.id, a = a, layer = "stage", f = 1,
				s = k, x = p.x, y = p.y - a.lift * k, w = p.w }
		else
			local item = placed(a, e, layer)
			item.f = e.f or (layer == "fore" and DEPTH.fore[3]) or 1
			if layer == "front" then front[#front + 1] = item
			elseif layer == "fore" then fore[#fore + 1] = item
			else scene.under[#scene.under + 1] = item end
		end
	end
	for _, it in ipairs(front) do scene.over[#scene.over + 1] = it end
	for _, it in ipairs(fore) do scene.over[#scene.over + 1] = it end
	for f in pairs(used) do scene.images[#scene.images + 1] = f end
	table.sort(scene.images)
	return scene
end

-- check + stage + scene. Errors (a string) when the map or the stage is refused.
function M.compile(map, lib, opts)
	local ok, err = M.check(map, lib)
	if not ok then error(err, 0) end
	local built, stage = pcall(M.stage, map, lib, opts)
	if not built then
		if type(stage) == "table" and stage.brawl_map then error(stage.brawl_map, 0) end
		error(stage, 0)
	end
	return { stage = stage, scene = M.scene(map, lib, stage) }
end

return M
