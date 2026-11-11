-- The asset library (docs/design/brawl-maps.md): named, reusable map pieces, defined once in
-- maps/assets/<family>.lua and placed by name from maps. Pure: runs under stock Lua 5.4 and Luau.
-- Distances inside an asset are picture pixels; a placement's scale turns them into world units.
-- Errors name `family.asset.field`, so a broken edit says where it is.
local L = {}

L.FAMILIES = { "sky", "backdrop", "ground", "platforms", "props", "clouds" }

-- Per kind: the family it lives in, its fields (true = required), the layers it may use (first is
-- the default). `layer` is a draw layer of decision 6.
local KINDS = {
	sky = { family = "sky", layers = { "sky" }, fields = { kind = true, stops = true, sun = false } },
	layer = { family = "backdrop", layers = { "far", "mid", "near" },
		fields = { kind = true, image = true, size = true, src = false, body = false, layer = false, scale = false, opacity = false } },
	haze = { family = "backdrop", layers = { "mid", "far", "near" },
		fields = { kind = true, color = true, from = true, to = true, height = true, layer = false } },
	ridge = { family = "backdrop", layers = { "far", "mid" }, fields = { kind = true, color = true, h = true, step = true, layer = false } },
	ground = { family = "ground", layers = { "stage" },
		fields = { kind = true, top = true, left = false, right = false, left_wall = false, right_wall = false, inset = false,
			lift = true, rail = false, body = true, edge = false } },
	platform = { family = "platforms", layers = { "stage" },
		fields = { kind = true, image = true, size = true, lift = true, rail = false, slice = false } },
	prop = { family = "props", layers = { "stage", "far", "mid", "near", "front", "fore" },
		fields = { kind = true, image = true, size = true, src = false, anchor = false, scale = false, layer = false, opacity = false, collide = false } },
	cloud = { family = "clouds", layers = { "far", "mid", "near" },
		fields = { kind = true, svg = true, size = true, anchor = false, scale = false, layer = false, opacity = false } },
}
L.KINDS = KINDS

local PICTURE = { image = true, size = true, src = false } -- a ground's top/left/right

local function fail(where, msg) error({ brawl_library = where .. ": " .. msg }, 0) end

local function is_int(v) return type(v) == "number" and v == math.floor(v) end

local function sorted_keys(t)
	local keys = {}
	for k in pairs(t) do keys[#keys + 1] = k end
	table.sort(keys, function(a, b) return tostring(a) < tostring(b) end)
	return keys
end

local function fields(t, allowed, where)
	if type(t) ~= "table" then fail(where, "must be a table") end
	for _, k in ipairs(sorted_keys(t)) do
		if allowed[k] == nil then fail(where .. "." .. tostring(k), "unknown field") end
	end
	for _, k in ipairs(sorted_keys(allowed)) do
		if allowed[k] and t[k] == nil then fail(where .. "." .. k, "missing") end
	end
end

local function list(v, n, where, what)
	if type(v) ~= "table" or #v ~= n then fail(where, "needs " .. what) end
	for _, k in ipairs(sorted_keys(v)) do
		if not is_int(k) or k < 1 or k > n then fail(where, "needs " .. what) end
	end
end

local function size(v, where)
	list(v, 2, where, "{w, h} in whole pixels > 0")
	if not (is_int(v[1]) and is_int(v[2]) and v[1] > 0 and v[2] > 0) then fail(where, "needs {w, h} in whole pixels > 0") end
end

local function number(v, where, lo, hi)
	if type(v) ~= "number" or v ~= v or (lo and v < lo) or (hi and v > hi) then
		fail(where, "needs a number" .. (lo and hi and (" in " .. lo .. ".." .. hi) or lo and (" >= " .. lo) or ""))
	end
end

local function color(v, where)
	if type(v) ~= "string" or not (v:match("^#%x%x%x%x%x%x$") or v:match("^#%x%x%x%x%x%x%x%x$") or v:match("^#%x%x%x$")) then
		fail(where, "needs a colour #rrggbb or #rrggbbaa")
	end
end

local function point_in(p, w, h, where)
	list(p, 2, where, "{x, y}")
	number(p[1], where, 0, w)
	number(p[2], where, 0, h)
end

local function image_path(v, where, ext)
	if type(v) ~= "string" or not v:match("^art/[%w_%-]+%." .. ext .. "$") then fail(where, "needs a file art/<name>." .. ext) end
end

-- A file's size against what the asset says, when the caller can read files (`opts.png_size`,
-- `opts.svg_size` return w, h, or nil and why: nil alone is a missing file).
local function against_file(reader, path, sz, where)
	if not reader then return end
	local w, h = reader(path)
	if not w then fail(where, h or ("no file " .. path)) end
	if w ~= sz[1] or h ~= sz[2] then
		fail(where, string.format("is {%d, %d}, but %s is %d x %d", sz[1], sz[2], path, w, h))
	end
end

local function picture(t, where, opts)
	image_path(t.image, where .. ".image", "png")
	size(t.size, where .. ".size")
	against_file(opts.png_size, t.image, t.size, where .. ".size")
	if t.src ~= nil then
		local s, w, h = t.src, t.size[1], t.size[2]
		list(s, 4, where .. ".src", "{x, y, w, h} in whole pixels")
		for i = 1, 4 do
			if not is_int(s[i]) then fail(where .. ".src", "needs {x, y, w, h} in whole pixels") end
		end
		if s[1] < 0 or s[2] < 0 or s[3] <= 0 or s[4] <= 0 or s[1] + s[3] > w or s[2] + s[4] > h then
			fail(where .. ".src", "lies outside the picture")
		end
	end
end

local function layer_of(a, k, where)
	if a.layer == nil then return end
	for _, l in ipairs(k.layers) do
		if l == a.layer then return end
	end
	fail(where .. ".layer", "a " .. a.kind .. " can't use layer " .. tostring(a.layer) .. " (" .. table.concat(k.layers, ", ") .. ")")
end

local function lift(a, h, where)
	number(a.lift, where .. ".lift", 0, h - 1)
end

local CHECK = {}

function CHECK.sky(a, where)
	local stops = a.stops
	if type(stops) ~= "table" or #stops < 2 or #stops > 64 then fail(where .. ".stops", "needs 2 to 64 {offset, colour}") end
	local last = -1
	for i, s in ipairs(stops) do
		local at = where .. ".stops[" .. i .. "]"
		list(s, 2, at, "{offset, colour}")
		number(s[1], at, 0, 1)
		if s[1] < last then fail(at, "offsets go up") end
		last = s[1]
		color(s[2], at)
	end
	if a.sun ~= nil then
		fields(a.sun, { x = true, y = true, r = true, color = true }, where .. ".sun")
		number(a.sun.x, where .. ".sun.x", 0, 1)
		number(a.sun.y, where .. ".sun.y", 0, 1)
		number(a.sun.r, where .. ".sun.r", 1)
		color(a.sun.color, where .. ".sun.color")
	end
end

function CHECK.layer(a, where, opts)
	picture(a, where, opts)
	if a.body ~= nil then color(a.body, where .. ".body") end
end

function CHECK.haze(a, where)
	color(a.color, where .. ".color")
	number(a.from, where .. ".from", 0, 1)
	number(a.to, where .. ".to", 0, 1)
	number(a.height, where .. ".height", 1)
end

function CHECK.ridge(a, where)
	color(a.color, where .. ".color")
	number(a.h, where .. ".h", 1)
	number(a.step, where .. ".step", 1)
end

local GROUND_PICTURES = { "top", "left", "right", "left_wall", "right_wall" }

function CHECK.ground(a, where, opts)
	for _, side in ipairs(GROUND_PICTURES) do
		if a[side] ~= nil then
			fields(a[side], PICTURE, where .. "." .. side)
			picture(a[side], where .. "." .. side, opts)
		end
	end
	lift(a, a.top.size[2], where)
	if a.rail ~= nil then number(a.rail, where .. ".rail", 0, a.top.size[2] - 1) end
	color(a.body, where .. ".body")
	if a.edge ~= nil then color(a.edge, where .. ".edge") end
	-- Caps come in pairs, as do walls, and the walls hang under caps of the same width. `inset` is
	-- the transparent columns outside the solid: the left pieces' left, the right pieces' right.
	if (a.left == nil) ~= (a.right == nil) then fail(where .. (a.left and ".right" or ".left"), "caps come in pairs") end
	if (a.left_wall == nil) ~= (a.right_wall == nil) then fail(where .. (a.left_wall and ".right_wall" or ".left_wall"), "walls come in pairs") end
	if a.left_wall and not a.left then fail(where .. ".left_wall", "walls hang under caps") end
	if a.left_wall and (a.left_wall.size[1] ~= a.left.size[1] or a.right_wall.size[1] ~= a.right.size[1]) then
		fail(where .. ".left_wall", "a wall is as wide as its cap")
	end
	if (a.inset ~= nil) ~= (a.left ~= nil) then fail(where .. ".inset", a.left and "missing (the caps need it)" or "only caps have an inset") end
	if a.inset then
		list(a.inset, 2, where .. ".inset", "{left, right} whole columns")
		if not (is_int(a.inset[1]) and is_int(a.inset[2]) and a.inset[1] >= 0 and a.inset[2] >= 0
			and a.inset[1] < a.left.size[1] and a.inset[2] < a.right.size[1]) then
			fail(where .. ".inset", "needs {left, right} whole columns inside the caps")
		end
	end
end

function CHECK.platform(a, where, opts)
	picture(a, where, opts)
	lift(a, a.size[2], where)
	if a.rail ~= nil then number(a.rail, where .. ".rail", 0, a.size[2] - 1) end
	if a.slice ~= nil then
		list(a.slice, 2, where .. ".slice", "{left, right} columns")
		local l, r = a.slice[1], a.slice[2]
		if not (is_int(l) and is_int(r) and l > 0 and l < r and r < a.size[1]) then
			fail(where .. ".slice", "needs whole columns 0 < left < right < " .. a.size[1])
		end
	end
end

local function anchored(a, where)
	if a.anchor ~= nil then point_in(a.anchor, a.size[1], a.size[2], where .. ".anchor") end
	if a.scale ~= nil then number(a.scale, where .. ".scale", 0.01) end
	if a.opacity ~= nil then number(a.opacity, where .. ".opacity", 0, 1) end
end

function CHECK.prop(a, where, opts)
	picture(a, where, opts)
	anchored(a, where)
	if a.collide ~= nil then
		local c = a.collide
		fields(c, { solid = false, platform = false }, where .. ".collide")
		if (c.solid == nil) == (c.platform == nil) then fail(where .. ".collide", "needs one of solid or platform") end
		local ax, ay = L.anchor(a)
		local function inside(x, y, w, h, at)
			if x + ax < 0 or y + ay < 0 or x + ax + w > a.size[1] or y + ay + h > a.size[2] then
				fail(at, "lies outside the picture")
			end
		end
		if c.solid then
			local at = where .. ".collide.solid"
			list(c.solid, 4, at, "{x, y, w, h} from the anchor")
			for i = 1, 4 do number(c.solid[i], at) end
			if c.solid[3] <= 0 or c.solid[4] <= 0 then fail(at, "needs w, h > 0") end
			inside(c.solid[1], c.solid[2], c.solid[3], c.solid[4], at)
		else
			local at = where .. ".collide.platform"
			if type(c.platform) == "table" and #c.platform == 4 then fail(at, "a platform is a line {x, y, w}; it has no h") end
			list(c.platform, 3, at, "{x, y, w} from the anchor")
			for i = 1, 3 do number(c.platform[i], at) end
			if c.platform[3] <= 0 then fail(at, "needs w > 0") end
			inside(c.platform[1], c.platform[2], c.platform[3], 0, at)
		end
	end
end

function CHECK.cloud(a, where, opts)
	image_path(a.svg, where .. ".svg", "svg")
	size(a.size, where .. ".size")
	against_file(opts.svg_size, a.svg, a.size, where .. ".size")
	anchored(a, where)
end

local function check_asset(family, name, a, opts)
	local where = family .. "." .. tostring(name)
	if type(name) ~= "string" or not name:match("^[%a_][%w_]*$") then fail(where, "an asset's name is a plain identifier") end
	if type(a) ~= "table" then fail(where, "must be a table") end
	local k = KINDS[a.kind]
	if not k then fail(where .. ".kind", "unknown kind " .. tostring(a.kind)) end
	if k.family ~= family then fail(where .. ".kind", "a " .. a.kind .. " belongs in " .. k.family .. ".lua") end
	fields(a, k.fields, where)
	CHECK[a.kind](a, where, opts)
	layer_of(a, k, where)
end

-- The families as tables, `{ family = { name = asset } }`. With no argument they are required from
-- maps/assets/<family>.lua; a test passes its own.
function L.load(families)
	if not families then
		families = {}
		for _, f in ipairs(L.FAMILIES) do families[f] = require("maps/assets/" .. f) end
	end
	return { families = families }
end

-- Checks every asset; true, or nil and the first problem in family order, then name order.
-- `opts.png_size(path)` / `opts.svg_size(path)` -> w, h (nil when missing), where files can be read.
function L.check(lib, opts)
	opts = opts or {}
	local ok, err = pcall(function()
		local fams = lib.families
		for _, f in ipairs(sorted_keys(fams)) do
			local known = false
			for _, g in ipairs(L.FAMILIES) do known = known or g == f end
			if not known then fail(tostring(f), "unknown family (" .. table.concat(L.FAMILIES, ", ") .. ")") end
		end
		local seen = {}
		for _, f in ipairs(L.FAMILIES) do
			local fam = fams[f]
			if fam ~= nil then
				if type(fam) ~= "table" then fail(f, "a family is a table of name = asset") end
				for _, name in ipairs(sorted_keys(fam)) do
					if seen[name] then fail(f .. "." .. tostring(name), "also defined in " .. seen[name]) end
					seen[name] = f
					check_asset(f, name, fam[name], opts)
				end
			end
		end
	end)
	if ok then return true end
	if type(err) == "table" and err.brawl_library then return nil, err.brawl_library end
	error(err, 0)
end

-- The anchor in picture pixels: as written, else the picture's bottom centre.
function L.anchor(a)
	if a.anchor then return a.anchor[1], a.anchor[2] end
	return a.size[1] / 2, a.size[2]
end

-- One asset by name, with its family and its defaults filled (`layer`, `scale`, `opacity`, `anchor`
-- where the kind has them). A copy: changing it changes nothing in the library.
function L.get(lib, name)
	for _, f in ipairs(L.FAMILIES) do
		local a = lib.families[f] and lib.families[f][name]
		if a ~= nil then
			local out = { name = name, family = f }
			for k, v in pairs(a) do out[k] = v end
			local k = KINDS[a.kind]
			out.layer = out.layer or k.layers[1]
			if k.fields.scale ~= nil then out.scale = out.scale or 1 end
			if k.fields.opacity ~= nil then out.opacity = out.opacity or 1 end
			if k.fields.anchor ~= nil then out.anchor = { L.anchor(a) } end
			return out
		end
	end
	error("brawl library: no asset " .. tostring(name), 2)
end

-- The sheet's cell, in sheet pixels (the page is `w` x `h`): a picture box, then `label_h` for the name and kind.
L.CELL = { w = 168, h = 176, pad = 8, label_h = 34 }
local SWATCH = { 160, 100 } -- a vector asset (sky, haze, ridge) is drawn as a swatch this big

-- The size, in picture pixels, of what the sheet draws for an asset: a picture or its `src`; a
-- ground's left cap, top and right cap side by side; a swatch for the vector kinds.
function L.extent(a)
	if a.kind == "ground" then
		local w, h = 0, 0
		for _, side in ipairs({ "left", "top", "right" }) do
			local p = a[side]
			if p then
				w = w + p.size[1]
				h = math.max(h, p.size[2])
			end
		end
		return { w, h }
	end
	if a.kind == "layer" and a.src then return { a.src[3], a.src[4] } end
	if a.size then return { a.size[1], a.size[2] } end
	return { SWATCH[1], SWATCH[2] }
end

-- The asset sheet's layout (P-V4): every asset once, in `names` order, row by row over pages of
-- `opts.w` x `opts.h`. A cell holds a picture box and a label; the picture is drawn at `scale` <= 1.
-- Cells are `L.CELL` unless `opts.cell_w` / `opts.cell_h` say otherwise. Pure, so it is tested without a VM.
function L.sheet(lib, opts)
	local cw, ch = opts.cell_w or L.CELL.w, opts.cell_h or L.CELL.h
	local pad, label = L.CELL.pad, L.CELL.label_h
	local cols, rows = math.floor(opts.w / cw), math.floor(opts.h / ch)
	if cols < 1 or rows < 1 then error("brawl sheet: a page of " .. opts.w .. " x " .. opts.h .. " holds no " .. cw .. " x " .. ch .. " cell", 2) end
	local ox = math.floor((opts.w - cols * cw) / 2)
	local out = { cells = {}, cols = cols, rows = rows, pages = 1 }
	for i, name in ipairs(L.names(lib)) do
		local a = L.get(lib, name)
		local at = i - 1
		local page, slot = math.floor(at / (cols * rows)) + 1, at % (cols * rows)
		local x, y = ox + (slot % cols) * cw, math.floor(slot / cols) * ch
		local box = { x = x + pad, y = y + pad, w = cw - 2 * pad, h = ch - label - 2 * pad }
		local ext = L.extent(a)
		out.cells[i] = { name = name, family = a.family, kind = a.kind, layer = a.layer, page = page, x = x, y = y, w = cw, h = ch,
			box = box, label_h = label, ext = ext, scale = math.min(1, box.w / ext[1], box.h / ext[2]) }
		out.pages = page
	end
	return out
end

-- Every asset's name, sorted by family (in FAMILIES order) then name.
function L.names(lib)
	local out = {}
	for _, f in ipairs(L.FAMILIES) do
		for _, name in ipairs(sorted_keys(lib.families[f] or {})) do out[#out + 1] = name end
	end
	return out
end

-- The pictures each asset draws, as { image, size, src } (a ground's top/left/right, else its own).
local function pictures(a)
	if a.kind == "ground" then
		local out = {}
		for _, side in ipairs(GROUND_PICTURES) do
			if a[side] then out[#out + 1] = a[side] end
		end
		return out
	end
	if a.image then return { a } end
	return {}
end

-- The files one asset (as `get` returns it) draws: its PNGs, or its SVG.
function L.files_of(a)
	local out = {}
	for _, p in ipairs(pictures(a)) do out[#out + 1] = p.image end
	if a.svg then out[#out + 1] = a.svg end
	return out
end

-- Every file the library names, sorted: PNGs and SVGs.
function L.files(lib)
	local set, out = {}, {}
	for _, name in ipairs(L.names(lib)) do
		for _, f in ipairs(L.files_of(L.get(lib, name))) do set[f] = true end
	end
	for f in pairs(set) do out[#out + 1] = f end
	table.sort(out)
	return out
end

-- The texture budget the whole library would take if every asset were loaded at once: one texture
-- per PNG (w·h·4 with mips, ×4/3) plus each distinct `src` region copied (w·h·4). SVGs are paths,
-- not textures.
function L.estimate(lib)
	local textures, regions = {}, {}
	local n, bytes = 0, 0
	for _, name in ipairs(L.names(lib)) do
		for _, p in ipairs(pictures(L.get(lib, name))) do
			if not textures[p.image] then
				textures[p.image] = true
				n = n + 1
				bytes = bytes + p.size[1] * p.size[2] * 4 * 4 / 3
			end
			if p.src then
				local key = p.image .. ":" .. table.concat(p.src, ",")
				if not regions[key] then
					regions[key] = true
					bytes = bytes + p.src[3] * p.src[4] * 4
				end
			end
		end
	end
	return { textures = n, bytes = bytes, mib = bytes / (1024 * 1024) }
end

return L
