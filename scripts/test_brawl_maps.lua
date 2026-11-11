-- Brawl's maps in stock Lua (plan: docs/design/brawl-maps.md §0, P-K1..P-K5).
--   lua scripts/test_brawl_maps.lua
package.path = "scripts/?.lua;scripts/fixtures/?.lua;" .. package.path
local U = require("brawl_test_util")
local Map = require("map")
local Library = require("library")
local F = U.F
local lib = Library.load()
local yard = require("maps/yard")
local BODY = { w = U.tune.body_w, h = U.tune.body_h }

local function copy(v)
	if type(v) ~= "table" then return v end
	local out = {}
	for k, x in pairs(v) do out[k] = copy(x) end
	return out
end

local function compile(map, l)
	return Map.compile(map, l or lib, { body = BODY })
end

-- The library with a collision template given to the rock and the bush.
local function lib_with(template)
	local fams = copy(lib.families)
	template(fams)
	return Library.load(fams)
end

-- P-K1: a map only places assets. Every refusal names the piece (index and id) and the field.
do
	assert(Map.check(yard, lib))
	local function refused(edit, want, why)
		local m = copy(yard)
		edit(m)
		local ok, err = Map.check(m, lib)
		assert(not ok, "P-K1: accepted " .. why)
		assert(err:find(want, 1, true) == 1, "P-K1: " .. why .. ": wanted " .. want .. ", got " .. tostring(err))
		assert(not pcall(Map.compile, m, lib, { body = BODY }), "P-K1: compile accepted " .. why)
	end
	-- pieces: ground 1, props 2-7 (tree, tree_2, bush, rock, grass_3 x2), platforms left 8, right 9, top 10.
	refused(function(m) m.pieces[8].image = "art/platform_4.png" end, 'pieces[8] "left".image: unknown field', "a picture in a map")
	refused(function(m) m.pieces[8].size = { 256, 79 } end, 'pieces[8] "left".size: unknown field', "a size in a map")
	refused(function(m) m.pieces[8].src = { 0, 0, 10, 10 } end, 'pieces[8] "left".src: unknown field', "a src in a map")
	refused(function(m) m.back[1].image = "art/cloud_2.png" end, "back[1].image: unknown field", "a picture in a back entry")
	refused(function(m) m.pieces[9].asset = "wood_ledgee" end, 'pieces[9] "right".asset: unknown asset wood_ledgee', "an unknown asset")
	refused(function(m) m.back[2].asset = "nope" end, "back[2].asset: unknown asset nope", "an unknown back asset")
	refused(function(m) m.pieces[1].layer = "front" end, 'pieces[1] "ground".layer: a ground can\'t use layer front', "a layer the kind can't use")
	refused(function(m) m.pieces[5].layer = "sky" end, "pieces[5].layer: a prop can't use layer sky", "a prop on the sky")
	refused(function(m) m.pieces[5].layer = "far" end, "pieces[5].layer: a prop here can't use layer far", "a prop on a back layer among the pieces")
	refused(function(m) m.pieces[1].id = nil end, "pieces[1].id: a ground makes collision and needs an id", "a solid with no id")
	refused(function(m) m.pieces[10].id = nil end, "pieces[10].id: a platform makes collision and needs an id", "a platform with no id")
	refused(function(m) m.pieces[9].id = "left" end, 'pieces[9] "left".id: also pieces[8]', "an id used twice")
	refused(function(m) m.pieces[8].h = 20 end, 'pieces[8] "left".h: unknown field', "a platform with an h")
	refused(function(m) m.pieces[1].scale = 2 end, 'pieces[1] "ground".scale: unknown field', "a scaled ground")
	refused(function(m) m.pieces[8].x = nil end, 'pieces[8] "left".x: missing', "a piece with no x")
	refused(function(m) m.pieces[5].asset = "haze_day" end, "pieces[5].asset: a haze can't be placed in pieces", "a haze among the pieces")
	refused(function(m) m.pieces[5].f = 0.5 end, "pieces[5].f: only a fore piece has a depth factor", "a depth factor on a stage piece")
	refused(function(m) m.back[1].asset = "grass_ground" end, "back[1].asset: a ground can't be placed in back", "a ground in back")
	refused(function(m) m.back[4].f = 0.9 end, "back[4].f: layer far allows 0.02..0.2", "a depth factor outside its layer")
	refused(function(m) m.sky = "dusk" end, "sky: unknown sky dusk", "an unknown sky")
	-- Step 4's back fields: drift and span only on a prop or a cloud, and together; opacity in 0..1.
	refused(function(m) m.back[5].drift, m.back[5].span = 4, 3000 end, "back[5].drift: only a prop or a cloud drifts", "a drifting mountain")
	refused(function(m) m.back[2].span = nil end, "back[2].span: drift and span go together", "drift without a span")
	refused(function(m) m.back[2].span = 0 end, "back[2].span: needs a number above 0", "an empty span")
	refused(function(m) m.back[8].opacity = 1.2 end, "back[8].opacity: needs a number in 0..1", "opacity past 1")
	refused(function(m) m.back[6].layer = "stage" end, "back[6].layer: a haze can't use layer stage", "a haze on the stage")
	refused(function(m) m.colour = "red" end, "colour: unknown field", "an unknown map field")
	refused(function(m) m.spawns = nil end, "spawns: missing", "a map with no spawns")
	-- A piece's own error beats a later one: first problem in order, back before pieces.
	local m = copy(yard)
	m.back[1].asset = "nope"
	m.pieces[1].asset = "nope"
	local _, err = Map.check(m, lib)
	assert(err:find("back[1]", 1, true) == 1, "P-K1: the first problem is reported: " .. tostring(err))
end

-- P-K2: the yard's rules are the old yard's, field by field and in order.
local compiled = compile(yard)
do
	local function same(a, b, path)
		if type(a) ~= "table" or type(b) ~= "table" then
			assert(a == b, "P-K2: " .. path .. ": " .. tostring(a) .. " vs " .. tostring(b))
			return
		end
		for k, v in pairs(b) do assert(a[k] ~= nil, "P-K2: " .. path .. "." .. tostring(k) .. " missing") ; same(a[k], v, path .. "." .. tostring(k)) end
		for k in pairs(a) do assert(b[k] ~= nil, "P-K2: " .. path .. "." .. tostring(k) .. " is extra") end
	end
	local v1 = require("brawl_yard_v1")
	same(compiled.stage, v1, "stage")
	for i, p in ipairs(v1.platforms) do
		assert(compiled.stage.platforms[i].id == p.id, "P-K2: platform order " .. i) -- `on` is an index into this list
	end
	assert(U.yard.platforms and U.yard.solids and U.same(U.yard, v1), "P-K2: every P-B and P-M test runs on the compiled yard")
	-- P-M12's printed final state is the one printed before this plan.
	local p = io.popen("lua5.4 scripts/test_brawl_moves.lua 2>&1")
	local out = p:read("a")
	p:close()
	local final = out:match("P%-M12 final: ([^\n]*)")
	assert(final == "tick=1200 x=-738.5 y=-240.5 vx=-7 vy=3 act=fall act_t=3 face=-1 jumps2=1 dashes=1 wall_jumps=3 stocks=2 peak=0 wall=0 drop=0",
		"P-K2: P-M12's final state moved: " .. tostring(final))
end

-- P-K3: collision comes from templates, exactly.
do
	local l = lib_with(function(f)
		f.props.rock.collide = { solid = { -90, -100, 150, 100 } } -- anchor (98, 120)
		f.props.bush.collide = { platform = { -150, -200, 300 } } -- anchor (188, 224)
	end)
	local function with(pieces)
		local m = copy(yard)
		m.pieces[4].id, m.pieces[5].id = "bush0", "rock0" -- the yard's own bush and rock now collide
		for _, p in ipairs(pieces) do m.pieces[#m.pieces + 1] = p end
		return m
	end
	local function solid_of(m, id)
		for _, s in ipairs(compile(m, l).stage.solids) do if s.id == id then return s end end
	end
	local function platform_of(m, id)
		for _, s in ipairs(compile(m, l).stage.platforms) do if s.id == id then return s end end
	end
	local s = solid_of(with({ { asset = "rock", id = "r", x = 400, y = 0 } }), "r")
	assert(s.x == 310 and s.y == -100 and s.w == 150 and s.h == 100, "P-K3: a prop's solid is its template at the anchor")
	s = solid_of(with({ { asset = "rock", id = "r", x = 400, y = 0, flip = true } }), "r")
	assert(s.x == 340 and s.y == -100 and s.w == 150 and s.h == 100, "P-K3: flipped, it is mirrored about the anchor (" .. s.x .. ")")
	s = solid_of(with({ { asset = "rock", id = "r", x = 400, y = 0, scale = 0.5 } }), "r")
	assert(s.x == 355 and s.y == -50 and s.w == 75 and s.h == 50, "P-K3: scaled about the anchor")
	local p = platform_of(with({ { asset = "bush", id = "b", x = 0, y = 0, scale = 0.5 } }), "b")
	assert(p.x == -75 and p.y == -100 and p.w == 150, "P-K3: a prop's platform is {x, y, w}")
	p = platform_of(with({ { asset = "bush", id = "b", x = 0, y = 0, flip = true } }), "b")
	assert(p.x == -150 and p.w == 300, "P-K3: a symmetric platform flips onto itself")
	-- Pieces keep their order; the stage lists solids and platforms each in pieces order.
	local m = with({ { asset = "rock", id = "r", x = 400, y = 0 }, { asset = "rock", id = "r2", x = 600, y = 0 } })
	local st = compile(m, l).stage
	assert(st.solids[1].id == "ground" and st.solids[2].id == "rock0" and st.solids[3].id == "r" and st.solids[4].id == "r2", "P-K3: solids in pieces order")
	-- Off the 1/8 grid, refused naming the piece.
	local ok, err = pcall(compile, with({ { asset = "rock", id = "odd", x = 400, y = 0, scale = 0.33 } }), l)
	assert(not ok and tostring(err):find('"odd"', 1, true) and tostring(err):find("1/8", 1, true), "P-K3: off the grid is refused: " .. tostring(err))
	-- A spawn inside a derived solid is refused by the stage check.
	local big = lib_with(function(f) f.props.rock.collide = { solid = { -90, -100, 180, 100 } } end)
	ok, err = pcall(compile, with({ { asset = "rock", id = "block", x = 0, y = 60 } }), big)
	assert(not ok and tostring(err):find("spawns[1]", 1, true) and tostring(err):find("solids[3]", 1, true), "P-K3: a spawn inside a solid: " .. tostring(err))
	-- Invisible collision: shapes are appended after the derived ones.
	m = copy(yard)
	m.shapes = { solids = { { id = "wall", x = 900, y = -400, w = 8, h = 400 } }, platforms = { { id = "ledge", x = 700, y = -100, w = 100 } } }
	st = compile(m, lib).stage
	assert(st.solids[2].id == "wall" and st.platforms[4].id == "ledge", "P-K3: shapes follow the derived collision")
end

-- P-K4: art and collision cannot drift.
do
	local function find(list, id)
		for _, it in ipairs(list) do if it.id == id then return it end end
	end
	local function all_items(scene)
		local out = {}
		for _, it in ipairs(scene.under) do out[#out + 1] = it end
		for _, it in ipairs(scene.over) do out[#out + 1] = it end
		return out
	end
	local c = compiled
	local pieces = 0
	for _, piece in ipairs(yard.pieces) do
		if piece.id then
			pieces = pieces + 1
			-- A copy with this piece shifted by 64: its collision and its picture move by exactly 64.
			local m = copy(yard)
			for _, q in ipairs(m.pieces) do if q.id == piece.id then q.x = q.x + 64 end end
			local c2 = compile(m)
			local a = find(all_items(c.scene), piece.id)
			local b = find(all_items(c2.scene), piece.id)
			assert(a and b, "P-K4: a scene item for " .. piece.id)
			assert(b.x - a.x == 64 and b.y == a.y and b.s == a.s, "P-K4: " .. piece.id .. "'s picture moves by 64")
			local s1 = find(c.stage.solids, piece.id) or find(c.stage.platforms, piece.id)
			local s2 = find(c2.stage.solids, piece.id) or find(c2.stage.platforms, piece.id)
			assert(s1 and s2 and s2.x - s1.x == 64 and s2.y == s1.y, "P-K4: " .. piece.id .. "'s collision moves by 64")
			-- The drawn surface is the collision top: picture top + lift x scale.
			local asset = Library.get(lib, piece.asset)
			assert(math.abs(a.y + asset.lift * a.s - s1.y) < 1e-9, string.format("P-K4: %s's surface %g is not its collision top %g", piece.id, a.y + asset.lift * a.s, s1.y))
			-- Same width as the collision: the picture spans the solid or platform.
			assert(a.w == s1.w, "P-K4: " .. piece.id .. "'s picture is as wide as its collision")
		end
	end
	assert(pieces == 4, "P-K4: the yard has four collision pieces, saw " .. pieces)
	-- A prop with a template: the picture's anchor is the template's anchor.
	local l = lib_with(function(f) f.props.rock.collide = { solid = { -90, -100, 150, 100 } } end)
	local m = copy(yard)
	m.pieces[5].id = "rock0"
	m.pieces[#m.pieces + 1] = { asset = "rock", id = "r", x = 400, y = 0, scale = 0.5 }
	local cc = Map.compile(m, l, { body = BODY })
	local it, s = find(all_items(cc.scene), "r"), find(cc.stage.solids, "r")
	local rock = Library.get(l, "rock")
	assert(it.x + rock.anchor[1] * it.s == 400 and it.y + rock.anchor[2] * it.s == 0, "P-K4: a prop is drawn about its anchor")
	assert(s.y == it.y + (rock.anchor[2] - 100) * it.s, "P-K4: its template hangs from the same anchor")
end

-- P-K5: the stage handed to net.match holds no art, and fighter.lua doesn't take any.
do
	local keys = {}
	for k in pairs(compiled.stage) do keys[#keys + 1] = k end
	table.sort(keys)
	assert(table.concat(keys, ",") == "blast,camera,name,platforms,respawn,solids,spawns", "P-K5: stage keys " .. table.concat(keys, ","))
	assert(U.plain(compiled.stage), "P-K5: the stage is plain data")
	local scene_tables = {}
	local function mark(v)
		if type(v) == "table" and not scene_tables[v] then
			scene_tables[v] = true
			for _, x in pairs(v) do mark(x) end
		end
	end
	mark(compiled.scene)
	local function shares(v)
		if type(v) == "table" then
			assert(not scene_tables[v], "P-K5: the stage and the scene share a table")
			for _, x in pairs(v) do shares(x) end
		end
	end
	shares(compiled.stage)
	local stage = copy(compiled.stage)
	stage.art = {}
	local ok, err = pcall(F.check_stage, stage)
	assert(not ok and tostring(err):find("unknown field art", 1, true), "P-K5: fighter refuses an art field: " .. tostring(err))
	-- And the map's own data isn't touched by compiling (a map can be compiled twice).
	local before = copy(yard)
	compile(yard)
	assert(U.same(before, yard), "P-K5: compile leaves the map alone")
end

-- L-1 (revised), stock-Lua half: the pictures the scene draws are exactly those the map's assets name.
do
	local images = compiled.scene.images
	local want = {}
	for _, piece in ipairs(yard.back) do for _, f in ipairs(Library.files_of(Library.get(lib, piece.asset))) do want[f] = true end end
	for _, piece in ipairs(yard.pieces) do for _, f in ipairs(Library.files_of(Library.get(lib, piece.asset))) do want[f] = true end end
	local n = 0
	for _, f in ipairs(images) do
		assert(want[f], "L-1: " .. f .. " is not named by the map's assets")
		want[f] = nil
		n = n + 1
	end
	assert(next(want) == nil, "L-1: a named picture is missing from scene.images")
	-- 16 PNGs (step 4: the tilemap block's top, caps and walls, the fog and the light hills) and 3 SVG clouds.
	assert(n == 19, "L-1: the yard names nineteen pictures, saw " .. n)
	print("L-1 yard pictures: " .. n)
end

print("brawl maps ok")
