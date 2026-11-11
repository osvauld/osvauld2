-- The map's pictures, drawn from the `scene` map.lua compiles (maps/yard.lua placing library assets).
-- View-side only: built once per map, placed every frame by the camera, never read by the rules and
-- never part of the match's setup. Items behind the fighter go in `under`, those in front in `over`.
local Camera = require("camera")
local Library = require("library")
local A = {}

-- The asset library (brawl-maps.md step 1) is checked once per open, before any scene is built;
-- a broken asset draws the stage plain and names itself. The VM can't read PNG headers (P-L1
-- checks those), but every SVG goes through gfx.svg and gfx.drawing here, dropped features refused.
local function svg_size(path)
	local ok, spec, report = pcall(gfx.svg, path)
	if not ok then return nil, tostring(spec) end
	if #report.dropped > 0 then
		local what = {}
		for _, d in ipairs(report.dropped) do what[#what + 1] = tostring(d.feature) end
		return nil, path .. " drops " .. table.concat(what, ", ")
	end
	gfx.drawing(spec)
	return spec.size[1], spec.size[2]
end

local function library_problem()
	local ok, lib = pcall(Library.load)
	if not ok then return tostring(lib) end
	local checked, fine, err = pcall(Library.check, lib, { svg_size = svg_size })
	if not checked then return tostring(fine) end
	return not fine and err or nil
end
A.problem = library_problem()
-- Not the console: an app's print goes to the shell's stdout (brawl-maps.md step 1 note).
if A.problem then print("brawl library: " .. A.problem .. " (drawing the stage plain)") end

local textures = {}
local function texture(path) -- one handle per file: every load_texture decodes again
	if not textures[path] then textures[path] = gfx.load_texture(path) end
	return textures[path]
end

A.texture = texture -- the gallery shares the handles, so a picture the map drew isn't decoded twice

local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end

local function rect_path(x, y, w, h)
	return gfx.path({ { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } })
end

-- A picture (or a region of one) as a frame of its own size, in picture pixels. `tile` copies abut
-- side by side (4 pixels overlapped: the edge pixels of a scaled picture are half covered, and abutting
-- copies show the sky through the seam); `body` is the colour
-- of a plain block below the picture, `drop` pixels deep, for a layer that must reach the bottom
-- of the view where its picture ends. `flip` mirrors it about its own vertical centre line.
local function picture(item, drop, flip)
	local src = item.src
	local w, h = src and src[3] or item.size[1], src and src[4] or item.size[2]
	local n = item.tile or 1
	local step = n > 1 and w - 4 or w
	local items = { width = step * (n - 1) + w, height = h }
	local tex = texture(item.image)
	for i = 0, n - 1 do
		local transform
		if flip then transform = { -1, 0, 0, 1, w + i * step, 0 }
		elseif n > 1 then transform = { 1, 0, 0, 1, i * step, 0 } end
		items[#items + 1] = gfx.image({ texture = tex, src = src, transform = transform })
	end
	if item.body and drop and drop > 0 then
		items.height = h + drop
		items[#items + 1] = gfx.fill({ path = rect_path(0, h - 1, items.width, drop + 1), brush = brush(item.body) })
	end
	return gfx.frame(items)
end

local function sky(stops)
	local side = 1000
	local list = {}
	for i, st in ipairs(stops) do list[i] = { st[1], st[2] } end
	return gfx.frame({ width = side, height = side, gfx.fill({
		path = rect_path(0, 0, side, side),
		brush = gfx.linear_gradient({ from = { 0, 0 }, to = { 0, side }, stops = list }),
	}) })
end

-- The ground, in picture pixels (the item's scale `s` is world units per pixel): the pack's grass
-- strip `tiles` times across the solid, over a plain body in the strip's own colour down to the
-- camera's lowest edge. Tiles overlap by a unit and a half of world: smoothed scaling samples a pixel
-- past a tile's edge and shows a seam.
local function ground(item)
	local top = item.a.top
	local tex = texture(top.image)
	local tw, th, k = top.size[1], top.size[2], item.s
	local height = (item.bottom - item.y) / k
	local items = { width = item.w / k, height = height }
	items[1] = gfx.fill({ path = rect_path(0, th - 2 / k, items.width, height - th + 2 / k), brush = brush(item.a.body) })
	for i = 0, item.tiles - 1 do
		items[#items + 1] = gfx.image({ texture = tex, transform = { 1 + 1.5 / (k * tw), 0, 0, 1, i * tw, 0 } })
	end
	return gfx.frame(items)
end

-- A stage without art: flat rectangles in theme colours, so a stage file works the day it is written.
local function plain(stage)
	local C = require("theme")
	local function flat(x, y, w, h, color)
		return { frame = gfx.frame({ width = w, height = h, gfx.fill({ path = rect_path(0, 0, w, h), brush = brush(color) }) }), x = x, y = y, s = 1, f = 1 }
	end
	local out = { home = { x = 0, y = 0 }, sky = sky({ { 0, C.sky }, { 1, C.sky } }), under = {}, over = {} }
	for _, s in ipairs(stage.solids) do
		out.under[#out.under + 1] = flat(s.x, s.y, s.w, s.h, C.ground)
		out.under[#out.under + 1] = flat(s.x, s.y, s.w, 14, C.ground_top)
	end
	for _, p in ipairs(stage.platforms) do
		out.under[#out.under + 1] = flat(p.x, p.y, p.w, 22, C.platform)
		out.under[#out.under + 1] = flat(p.x, p.y, p.w, 6, C.platform_top)
	end
	return out
end

-- Everything for a map: `order` is back to front, `over` is in front of the fighter. Pictures are
-- decoded a few per call (`load`), not all in one: a decode is native work inside the calling
-- entry and counts against its 1 s of CPU, which a debug build spends on about a megapixel. An
-- entry that isn't loaded yet is skipped when drawing. With no scene, or a broken library, `stage`
-- is drawn plain.
function A.build(stage, scene)
	if not scene or A.problem then return plain(stage) end
	local out = { home = scene.home, sky = sky(scene.sky.stops), under = {}, over = {}, steps = {}, next = 1, total = 0,
		images = scene.images }
	local function add(list, it)
		local item = { s = it.s, f = it.f, x = it.x, y = it.y, id = it.id }
		list[#list + 1] = item
		local a = it.a
		local pixels = a.kind == "ground" and a.top.size[1] * a.top.size[2] or a.size[1] * a.size[2] -- one decode, however many copies
		out.steps[#out.steps + 1] = { weight = pixels, item = item, make = function()
			if it.kind == "ground" then item.frame = ground(it)
			elseif it.kind == "layer" then
				local h = a.src and a.src[4] or a.size[2]
				item.frame = picture({ image = a.image, size = a.size, src = a.src, tile = it.tile, body = a.body }, (scene.bottom - it.y) / it.s - h)
			else item.frame = picture(a, nil, it.flip) end
		end }
	end
	for _, it in ipairs(scene.under) do add(out.under, it) end
	for _, it in ipairs(scene.over) do add(out.over, it) end
	out.total = #out.steps
	return out
end

-- Builds the next pictures, up to about `pixels` of them (at least one). True when all are built.
function A.load(art, pixels)
	local steps = art.steps
	if not steps then return true end
	local spent = 0
	while art.next <= #steps and (spent == 0 or spent + steps[art.next].weight <= pixels) do
		local step = steps[art.next]
		step.make(step.item)
		spent = spent + step.weight
		art.next = art.next + 1
	end
	return art.next > #steps
end

-- How many of the stage's pictures are built, as "n/total".
function A.progress(art)
	return string.format("%d/%d", art.steps and art.next - 1 or 0, art.total or 0)
end

local function elements(items, art, cam, k, ox, oy)
	local out = {}
	for _, it in ipairs(items) do
		if it.frame then
			local dx, dy = Camera.parallax(cam, it.f, art.home)
			-- `piece:<id>`: a piece that makes collision answers to its id, so the overlay's `col:<id>` can be
			-- held against it (S-M3). The handler is what makes an id reachable to rects().
			out[#out + 1] = ui.frame({ id = it.id and ("piece:" .. it.id) or nil, visual = it.frame, absolute = true,
				on_hover = it.id and function() end or nil,
				left = ox + (it.x + dx - cam.x) * k, top = oy + (it.y + dy - cam.y) * k, scale = k * it.s })
		end
	end
	return out
end

-- Ui elements for the camera `cam`, k px per unit, the playfield's corner at (ox, oy).
function A.under(art, cam, k, ox, oy, w, h)
	local side = math.max(w, h)
	local list = { ui.frame({ visual = art.sky, absolute = true, left = ox, top = oy, scale = side / 1000 }) }
	for _, e in ipairs(elements(art.under, art, cam, k, ox, oy)) do list[#list + 1] = e end
	if A.problem then
		list[#list + 1] = ui.text({ A.problem, id = "library:problem", absolute = true, left = 0, top = 0, font_size = 1, opacity = 0, no_wrap = true })
	end
	return list
end

function A.over(art, cam, k, ox, oy)
	return elements(art.over, art, cam, k, ox, oy)
end

return A
