-- Brawl's asset library in stock Lua (plan: docs/design/brawl-maps.md §0, P-L1..P-L3).
--   lua scripts/test_brawl_library.lua
package.path = "demo_apps/brawl/?.lua;" .. package.path
local Library = require("library")
local APP = "demo_apps/brawl/"

local function png_size(path)
	local f = io.open(APP .. path, "rb")
	if not f then return nil end
	local head = f:read(24)
	f:close()
	assert(head and head:sub(2, 4) == "PNG", path .. " is not a PNG")
	return string.unpack(">I4I4", head, 17)
end

local function svg_size(path)
	local f = io.open(APP .. path, "rb")
	if not f then return nil end
	local text = f:read("a")
	f:close()
	local tag = assert(text:match("<svg[^>]*>"), path .. " has no <svg>")
	return tonumber(tag:match('%swidth="([%d.]+)"')), tonumber(tag:match('%sheight="([%d.]+)"'))
end

local FILES = { png_size = png_size, svg_size = svg_size }

local function copy(v)
	if type(v) ~= "table" then return v end
	local out = {}
	for k, x in pairs(v) do out[k] = copy(x) end
	return out
end

-- The first picture row with any opaque pixel, through ImageMagick (nil when it isn't installed):
-- stock Lua can't inflate a PNG.
local function first_opaque_row(path)
	local p = io.popen("command -v magick >/dev/null 2>&1 && magick '" .. APP .. path
		.. "' -alpha on -alpha extract -scale 1x! -depth 8 txt:- 2>/dev/null")
	local out = p:read("a")
	p:close()
	if out == "" then return nil end
	for y, v in out:gmatch("\n0,(%d+): %(([%d.]+)") do
		if tonumber(v) > 0 then return tonumber(y) end
	end
	error("no opaque row in " .. path)
end

-- A picture's opaque box as x, y, w, h, through ImageMagick (nil when it isn't installed).
local function opaque_box(path)
	local p = io.popen("command -v magick >/dev/null 2>&1 && magick '" .. APP .. path .. "' -format '%@' info: 2>/dev/null")
	local out = p:read("a")
	p:close()
	local w, h, x, y = out:match("(%d+)x(%d+)%+(%d+)%+(%d+)")
	if not w then return nil end
	return tonumber(x), tonumber(y), tonumber(w), tonumber(h)
end

local lib = Library.load()

-- P-L1: every asset passes check against the files, and every lift is the measured first opaque row.
do
	local ok, err = Library.check(lib, FILES)
	assert(ok, "P-L1: " .. tostring(err))
	local measured = 0
	for _, name in ipairs(Library.names(lib)) do
		local a = Library.get(lib, name)
		local where = a.family .. "." .. name
		local layers = Library.KINDS[a.kind].layers
		local allowed = false
		for _, l in ipairs(layers) do allowed = allowed or l == a.layer end
		assert(allowed, "P-L1: " .. where .. " default layer " .. a.layer)
		if a.opacity then assert(a.opacity >= 0 and a.opacity <= 1, "P-L1: " .. where .. ".opacity") end
		local pic = a.kind == "ground" and a.top or a.kind == "platform" and a or nil
		if pic then
			local row = first_opaque_row(pic.image)
			if row then
				-- `rail`: picture rows of decoration (a fence) above the walking surface, so the lift is
				-- the first opaque row plus the rail.
				local want = row + (a.rail or 0)
				assert(math.abs(a.lift - want) <= 1, string.format("P-L1: %s.lift is %g; %s's first opaque row is %d (+ rail %d)", where, a.lift, pic.image, row, a.rail or 0))
				measured = measured + 1
			end
		end
		-- A capped ground's inset: the left wall is opaque from column inset[1], the right one to
		-- inset[2] columns short of its right side (step 4: the walls' outer edge is the solid's).
		if a.kind == "ground" and a.left_wall then
			local lx = opaque_box(a.left_wall.image)
			local rx, _, rw = opaque_box(a.right_wall.image)
			if lx then
				assert(lx == a.inset[1], "P-L1: " .. where .. ".inset[1] is " .. a.inset[1] .. "; the left wall starts at column " .. lx)
				local right = a.right_wall.size[1] - (rx + rw)
				assert(right == a.inset[2], "P-L1: " .. where .. ".inset[2] is " .. a.inset[2] .. "; the right wall stops " .. right .. " short")
			end
		end
	end
	print("P-L1 assets: " .. #Library.names(lib) .. (measured > 0 and (", lifts measured: " .. measured) or ", lifts NOT measured (no magick)"))
end

-- P-L2: bad assets are refused, the error naming the family, the asset and the field.
do
	local function refused(edit, want, why)
		local fams = copy(lib.families)
		edit(fams)
		local ok, err = Library.check(Library.load(fams), FILES)
		assert(not ok, "P-L2: accepted " .. why)
		assert(err:find(want, 1, true) == 1, "P-L2: " .. why .. ": wanted " .. want .. ", got " .. err)
	end
	refused(function(f) f.props.rock.colour = "#fff" end, "props.rock.colour: unknown field", "an unknown field")
	refused(function(f) f.props.rock.kind = "statue" end, "props.rock.kind: unknown kind", "an unknown kind")
	refused(function(f) f.props.rock.kind = "layer" end, "props.rock.kind: a layer belongs in backdrop.lua", "a kind in the wrong family")
	refused(function(f) f.props.tree_2.size = { 445, 600 } end, "props.tree_2.size: is {445, 600}, but art/tree_2.png is 445 x 615", "a wrong size")
	refused(function(f) f.props.tree_2.size = { 445 } end, "props.tree_2.size: needs {w, h}", "a size that isn't two numbers")
	refused(function(f) f.props.bush.anchor = { 188, 230 } end, "props.bush.anchor: needs a number in 0..224", "an anchor below the picture")
	refused(function(f) f.props.rock.collide = { platform = { -90, -100, 180, 20 } } end,
		"props.rock.collide.platform: a platform is a line {x, y, w}; it has no h", "a platform template with an h")
	refused(function(f) f.props.rock.collide = { solid = { -90, -100, 300, 100 } } end,
		"props.rock.collide.solid: lies outside the picture", "a collision box outside the picture")
	refused(function(f) f.backdrop.rock = { kind = "haze", color = "#ffffff", from = 0, to = 0.5, height = 100 } end,
		"props.rock: also defined in backdrop", "one name in two families")
	refused(function(f) f.backdrop.hedge.src = { 0, 0, 1025, 700 } end, "backdrop.hedge.src: lies outside the picture", "a src outside")
	refused(function(f) f.backdrop.hedge.layer = "stage" end, "backdrop.hedge.layer: a layer can't use layer stage", "a layer the kind can't use")
	refused(function(f) f.props.rock.opacity = 1.5 end, "props.rock.opacity: needs a number in 0..1", "opacity past 1")
	refused(function(f) f.platforms.wood_ledge.lift = 79 end, "platforms.wood_ledge.lift: needs a number in 0..78", "a lift below the picture")
	refused(function(f) f.ground.grass_ground.top.image = "art/nope.png" end, "ground.grass_ground.top.size: no file art/nope.png", "a missing file")
	refused(function(f) f.clouds.cloud_soft_1.size = { 480, 200 } end, "clouds.cloud_soft_1.size: is {480, 200}, but art/cloud_soft_1.svg is 480 x 220", "an SVG's size")
	refused(function(f) f.platforms.wood_ledge.rail = -1 end, "platforms.wood_ledge.rail: needs a number in 0..78", "a negative rail")
	refused(function(f) f.platforms.wood_ledge.slice = { 0, 200 } end, "platforms.wood_ledge.slice: needs whole columns 0 < left < right < 256", "a 3-slice from column 0")
	refused(function(f) f.sky.day.stops = { { 0, "#78c4d6" } } end, "sky.day.stops: needs 2 to 64", "a one-stop sky")
	refused(function(f) f.sky.day.stops[2][1] = -1 end, "sky.day.stops[2]: needs a number in 0..1", "a stop offset outside 0..1")
	refused(function(f) f.weather = {} end, "weather: unknown family", "an unknown family")
	-- Step 4: caps and walls come in pairs, walls hang under caps, caps need an inset.
	refused(function(f) f.ground.grass_block.right = nil end, "ground.grass_block.right: caps come in pairs", "one cap")
	refused(function(f) f.ground.grass_block.left_wall = nil end, "ground.grass_block.left_wall: walls come in pairs", "one wall")
	refused(function(f) f.ground.grass_block.inset = nil end, "ground.grass_block.inset: missing (the caps need it)", "caps with no inset")
	refused(function(f) f.ground.grass_block.inset = { 29, 128 } end, "ground.grass_block.inset: needs {left, right} whole columns", "an inset past the cap")
	refused(function(f) f.ground.grass_ground.inset = { 1, 1 } end, "ground.grass_ground.inset: only caps have an inset", "an inset with no caps")
	-- Valid templates pass, and get fills defaults without touching the library.
	local fams = copy(lib.families)
	fams.props.rock.collide = { solid = { -90, -100, 180, 100 } }
	fams.props.bush.collide = { platform = { -150, -200, 300 } }
	assert(Library.check(Library.load(fams), FILES), "P-L2: valid templates pass")
	local rock = Library.get(lib, "rock")
	assert(rock.family == "props" and rock.layer == "stage" and rock.scale == 1 and rock.opacity == 1
		and rock.anchor[1] == 98 and rock.anchor[2] == 120, "P-L2: get fills the defaults")
	rock.scale = 3
	assert(lib.families.props.rock.scale == nil, "P-L2: get returns a copy")
	assert(not pcall(Library.get, lib, "nope"), "P-L2: an unknown name is an error")
end

-- P-L3: the library is closed (every picture in art/ is named, every named file exists), the
-- licence is beside the PNGs, and the whole library fits 56 textures and 64 MiB.
do
	local named = {}
	for _, f in ipairs(Library.files(lib)) do
		named[f] = true
		local fh = assert(io.open(APP .. f, "rb"), "P-L3: no file " .. f)
		fh:close()
	end
	local p = io.popen("ls '" .. APP .. "art'")
	local on_disk = 0
	for name in p:lines() do
		if name:match("%.png$") or name:match("%.svg$") then
			on_disk = on_disk + 1
			assert(named["art/" .. name], "P-L3: art/" .. name .. " is named by no asset")
		end
	end
	p:close()
	assert(on_disk == #Library.files(lib), "P-L3: the files named are the files in art/")
	local fh = assert(io.open(APP .. "art/License.txt", "rb"), "P-L3: art/License.txt")
	local lic = fh:read("a")
	fh:close()
	assert(lic:find("Dumivid", 1, true) and lic:find("CC0", 1, true), "P-L3: the licence names Dumivid and CC0")
	local e = Library.estimate(lib)
	assert(e.textures <= 56, "P-L3: " .. e.textures .. " textures")
	assert(e.textures >= 44, "P-L3: step 3 packs in about 48 pictures, found " .. e.textures)
	-- Templates and slices (decision 2): boulders are solid, the bridge a one-way platform, every
	-- ledge 3-sliced, the tilemap's caps are there, and there are 2-3 soft SVG clouds.
	for _, n in ipairs({ "boulder", "boulder_2", "boulder_3", "boulder_5" }) do
		assert(Library.get(lib, n).collide and Library.get(lib, n).collide.solid, "P-L3: " .. n .. " has a solid template")
	end
	local bridge = Library.get(lib, "bridge")
	assert(bridge.kind == "platform" and bridge.slice and bridge.rail, "P-L3: the bridge is a 3-sliced platform with a rail")
	for _, n in ipairs(Library.names(lib)) do
		local a = Library.get(lib, n)
		if a.kind == "platform" then assert(a.slice, "P-L3: " .. n .. " is 3-sliced") end
	end
	local block = Library.get(lib, "grass_block")
	assert(block.kind == "ground" and block.left and block.right, "P-L3: grass_block has both caps")
	local clouds = 0
	for _, n in ipairs(Library.names(lib)) do if Library.get(lib, n).kind == "cloud" then clouds = clouds + 1 end end
	assert(clouds >= 3 and clouds <= 4, "P-L3: soft clouds: " .. clouds)
	assert(e.mib <= 64, "P-L3: " .. e.mib .. " MiB")
	print(string.format("P-L3 library: %d assets, %d files, %d textures, %.2f MiB (mips + copied src)",
		#Library.names(lib), on_disk, e.textures, e.mib))
end

-- P-V4: the asset sheet places every asset exactly once, sorted by family then name; cells don't
-- overlap and each asset fits its cell at scale <= 1. Checked at several page sizes.
do
	local names = Library.names(lib) -- family order, then name
	local pages_seen = {}
	for _, size in ipairs({ { 1800, 1100 }, { 900, 700 }, { 500, 400 } }) do
		local sheet = Library.sheet(lib, { w = size[1], h = size[2] })
		assert(#sheet.cells == #names, "P-V4: " .. #sheet.cells .. " cells for " .. #names .. " assets at " .. size[1])
		local seen, last_page, last_idx = {}, 1, 0
		for i, c in ipairs(sheet.cells) do
			assert(c.name == names[i], "P-V4: cell " .. i .. " is " .. c.name .. ", want " .. names[i] .. " (family, then name)")
			assert(not seen[c.name], "P-V4: " .. c.name .. " twice")
			seen[c.name] = true
			assert(c.page >= last_page and c.page <= sheet.pages, "P-V4: pages go up")
			last_page = c.page
			assert(c.x >= 0 and c.y >= 0 and c.x + c.w <= size[1] and c.y + c.h <= size[2], "P-V4: " .. c.name .. " is off the page")
			assert(c.scale > 0 and c.scale <= 1, "P-V4: " .. c.name .. " scale " .. c.scale)
			-- the picture box inside the cell: the content at that scale fits it
			assert(c.ext[1] * c.scale <= c.box.w + 1e-9 and c.ext[2] * c.scale <= c.box.h + 1e-9, "P-V4: " .. c.name .. " overflows its box")
			assert(c.box.x >= c.x and c.box.y >= c.y and c.box.x + c.box.w <= c.x + c.w and c.box.y + c.box.h <= c.y + c.h - c.label_h + 1e-9,
				"P-V4: " .. c.name .. "'s box leaves its cell")
			for j = 1, i - 1 do
				local o = sheet.cells[j]
				if o.page == c.page then
					assert(c.x >= o.x + o.w or o.x >= c.x + c.w or c.y >= o.y + o.h or o.y >= c.y + c.h, "P-V4: " .. c.name .. " overlaps " .. o.name)
				end
			end
		end
		pages_seen[#pages_seen + 1] = sheet.pages
	end
	assert(pages_seen[1] == 1, "P-V4: a 1800 x 1100 page holds the whole library")
	assert(pages_seen[3] > pages_seen[2], "P-V4: a smaller page takes more pages")
	assert(not pcall(Library.sheet, lib, { w = 50, h = 50 }), "P-V4: a page smaller than a cell is refused")
	print("P-V4 sheet: " .. #names .. " assets, pages at 1800x1100 / 900x700 / 500x400: " .. table.concat(pages_seen, " / "))
end

print("brawl library ok")
