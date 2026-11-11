-- The map's pictures, drawn from the `scene` map.lua compiles (maps/yard.lua placing library assets).
-- View-side only: built once per map, placed every frame by the camera, never read by the rules and
-- never part of the match's setup. Items behind the fighter go in `under`, those in front in `over`.
local Camera = require("camera")
local Depth = require("depth")
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

-- Keeps the handles of the pictures in `images` (a map's files) and lets the others go, so a map
-- switched away from doesn't hold its textures; those both maps use are not decoded again.
function A.release(images)
	local keep = {}
	for _, f in ipairs(images) do keep[f] = true end
	for path in pairs(textures) do
		if not keep[path] then textures[path] = nil end
	end
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

-- "#rrggbb" with an alpha 0..1 as "#rrggbbaa".
local function with_alpha(color, a)
	return string.format("%s%02x", color:sub(1, 7), math.floor(math.max(0, math.min(1, a)) * 255 + 0.5))
end

-- A picture (or a region of one) as a frame of its own size, in picture pixels. `tile` copies abut
-- side by side, `overlap` pixels overlapped (4: the edge pixels of a scaled picture are half covered, and
-- abutting copies show the sky through the seam; 1 for a translucent layer); `body` is the colour of a
-- plain block below the picture,
-- `drop` pixels deep, so a layer reaches the bottom of every view (map.lua computes both).
-- `flip` mirrors it about its own vertical centre line.
local function picture(item, drop, flip)
	local src = item.src
	local w, h = src and src[3] or item.size[1], src and src[4] or item.size[2]
	local n = item.tile or 1
	local step = n > 1 and w - (item.overlap or 4) or w
	local items = { width = step * (n - 1) + w, height = h }
	if item.body and drop and drop > 0 then
		items.height = h + drop
		items[#items + 1] = gfx.fill({ path = rect_path(0, h - 2, items.width, drop + 2), brush = brush(item.body) })
	end
	local tex = texture(item.image)
	for i = 0, n - 1 do
		local transform
		if flip then transform = { -1, 0, 0, 1, w + i * step, 0 }
		elseif n > 1 then transform = { 1, 0, 0, 1, i * step, 0 } end
		items[#items + 1] = gfx.image({ texture = tex, src = src, transform = transform })
	end
	return gfx.frame(items)
end

-- The sky: the map's gradient from the view's top to its bottom, and the sun's glow (a radial fade of
-- its colour, `r` px on a 1 600 px playfield). Screen-fixed; rebuilt when the playfield's size changes.
local skies = {}
local function sky(spec, w, h)
	local key = math.floor(w) .. "x" .. math.floor(h)
	local hit = skies[spec]
	if hit and hit.key == key then return hit.frame end
	local list = {}
	for i, st in ipairs(spec.stops) do list[i] = { st[1], st[2] } end
	local items = { width = w, height = h, gfx.fill({
		path = rect_path(0, 0, w, h),
		brush = gfx.linear_gradient({ from = { 0, 0 }, to = { 0, h }, stops = list }),
	}) }
	local sun = spec.sun
	if sun then
		local r = sun.r * w / Camera.params.ref_w
		local cx, cy = sun.x * w, sun.y * h
		items[#items + 1] = gfx.fill({ path = rect_path(0, 0, w, h), brush = gfx.radial_gradient({ center = { cx, cy }, radius = r * 4,
			stops = { { 0, with_alpha(sun.color, 1) }, { 0.12, with_alpha(sun.color, 0.95) }, { 0.2, with_alpha(sun.color, 0.45) },
				{ 0.45, with_alpha(sun.color, 0.14) }, { 1, with_alpha(sun.color, 0) } } }) })
	end
	skies[spec] = { key = key, frame = gfx.frame(items) }
	return skies[spec].frame
end

-- A haze: a band of colour from alpha `from` at its top to `to` at `height`, then `to` down to its
-- bottom. Layer units, `w` wide.
local function haze(item)
	local a = item.a
	local h = item.bottom - item.y
	local t = math.min(1, a.height / h)
	return gfx.frame({ width = item.w, height = h, gfx.fill({ path = rect_path(0, 0, item.w, h),
		brush = gfx.linear_gradient({ from = { 0, 0 }, to = { 0, h },
			stops = { { 0, with_alpha(a.color, a.from) }, { t, with_alpha(a.color, a.to) }, { 1, with_alpha(a.color, a.to) } } }) }) })
end

-- Deterministic noise, so a ridge always has the same hills (Jetpack's scenery.lua).
local function noise(seed)
	local s = seed
	return function()
		s = (s * 1103515245 + 12345) % 2147483648
		return s / 2147483648
	end
end

-- A ridge: rounded hills `h` high, a hill every `step` or so, in one colour down to its bottom.
-- Layer units. The seed is the asset's name, so the same ridge is the same hills in every map.
local function ridge(item)
	local a = item.a
	local seed = 7919
	for i = 1, #item.asset do seed = (seed * 31 + item.asset:byte(i)) % 2147483647 end
	local r = noise(seed)
	local H, w = a.h, item.w
	local bottom = item.bottom - item.y
	local pts, x = {}, -a.step
	while x <= w + a.step do
		pts[#pts + 1] = { x, H * (1 - (0.35 + 0.65 * r())) }
		x = x + a.step * (0.6 + 0.8 * r())
	end
	local cmd = { { "move", pts[1][1], bottom }, { "line", pts[1][1], pts[1][2] } }
	for i = 2, #pts do -- a quad through each peak's midpoints: round tops, soft valleys
		local p, q = pts[i - 1], pts[i]
		cmd[#cmd + 1] = { "quad", p[1], p[2], (p[1] + q[1]) / 2, (p[2] + q[2]) / 2 }
	end
	local last = pts[#pts]
	cmd[#cmd + 1] = { "line", last[1], last[2] }
	cmd[#cmd + 1] = { "line", last[1], bottom }
	cmd[#cmd + 1] = { "close" }
	return gfx.frame({ width = w, height = bottom, gfx.fill({ path = gfx.path(cmd), brush = brush(a.color) }) })
end

-- The ground, in picture pixels (the item's scale `s` is world units per pixel), down to the camera's
-- lowest edge. Tiles overlap by a unit and a half of world: smoothed scaling samples a pixel past a
-- tile's edge and shows a seam.
-- Without caps: the pack's grass strip `tiles` times across the solid, over a plain body.
-- With caps (decision 9): the left and right caps and the walls under them sit `inset` columns outside
-- the solid (the frame's own box is the solid's width; pictures may lie outside it), the top strip
-- stretched between the caps, the body colour between the walls.
local function ground(item)
	local a = item.a
	local top = a.top
	local tw, th, k = top.size[1], top.size[2], item.s
	local height = (item.bottom - item.y) / k
	local W = item.w / k
	local items = { width = W, height = height }
	items[1] = gfx.fill({ path = rect_path(0, th - 2 / k, W, height - th + 2 / k), brush = brush(a.body) })
	local tex = texture(top.image)
	if not a.left then
		for i = 0, item.tiles - 1 do
			items[#items + 1] = gfx.image({ texture = tex, transform = { 1 + 1.5 / (k * tw), 0, 0, 1, i * tw, 0 } })
		end
		return gfx.frame(items)
	end
	local L, R = a.left, a.right
	local xl, xr = -a.inset[1], W + a.inset[2] - R.size[1]
	local from, to = xl + L.size[1] - 2, xr + 2 -- under the caps by 2 px: abutting shows a hairline
	local n = math.max(1, math.floor((to - from) / tw + 0.5))
	local sx = (to - from) / (n * tw)
	for i = 0, n - 1 do
		items[#items + 1] = gfx.image({ texture = tex, transform = { sx + 1.5 / (k * tw), 0, 0, 1, from + i * tw * sx, 0 } })
	end
	if a.left_wall then
		local wl, wr = texture(a.left_wall.image), texture(a.right_wall.image)
		local wh = a.left_wall.size[2]
		local y = L.size[2] - 1
		while y < height do
			items[#items + 1] = gfx.image({ texture = wl, transform = { 1, 0, 0, 1 + 1.5 / wh, xl, y } })
			items[#items + 1] = gfx.image({ texture = wr, transform = { 1, 0, 0, 1 + 1.5 / wh, xr, y } })
			y = y + wh
		end
	end
	items[#items + 1] = gfx.image({ texture = texture(L.image), transform = { 1, 0, 0, 1, xl, 0 } })
	items[#items + 1] = gfx.image({ texture = texture(R.image), transform = { 1, 0, 0, 1, xr, 0 } })
	return gfx.frame(items)
end

-- A one-way ledge, 3-sliced (decision 9): the end columns at the item's scale, the middle stretched
-- between them (a pixel under each end, so no seam shows). Picture pixels; `w / s` wide.
local function platform(item)
	local a = item.a
	local W, H = a.size[1], a.size[2]
	local width = item.w / item.s
	local tex = texture(a.image)
	if not a.slice then
		return gfx.frame({ width = W, height = H, gfx.image({ texture = tex }) })
	end
	local l, r = a.slice[1], a.slice[2]
	local mid = width - l - (W - r)
	return gfx.frame({ width = width, height = H,
		gfx.image({ texture = tex, src = { l, 0, r - l, H }, transform = { (mid + 2) / (r - l), 0, 0, 1, l - 1, 0 } }),
		gfx.image({ texture = tex, src = { 0, 0, l, H } }),
		gfx.image({ texture = tex, src = { r, 0, W - r, H }, transform = { 1, 0, 0, 1, width - (W - r), 0 } }),
	})
end

local clouds = {}
local function cloud(a)
	if not clouds[a.svg] then clouds[a.svg] = gfx.drawing((gfx.svg(a.svg))):pose({}) end
	return clouds[a.svg]
end

-- A stage without art: flat rectangles in theme colours, so a stage file works the day it is written.
local function plain(stage)
	local C = require("theme")
	local function flat(x, y, w, h, color)
		return { frame = gfx.frame({ width = w, height = h, gfx.fill({ path = rect_path(0, 0, w, h), brush = brush(color) }) }), x = x, y = y, s = 1, f = 1 }
	end
	local out = { home = { x = 0, y = 0 }, sky = { stops = { { 0, C.sky }, { 1, C.sky } } }, under = {}, over = {} }
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

-- Decode cost of an item's step, in pixels: one decode per file however many copies; vector items
-- count as a small picture.
local function weight(it)
	local a = it.a
	if a.kind == "ground" then return a.top.size[1] * a.top.size[2] * (a.left and 5 or 1) end
	if a.size then return a.size[1] * a.size[2] end
	return 20000
end

-- Everything for a map: `under` is back to front behind the fighter, `over` in front of it. Pictures
-- are decoded a few per call (`load`), not all in one: a decode is native work inside the calling
-- entry and counts against its 1 s of CPU, which a debug build spends on about a megapixel. An
-- entry that isn't loaded yet is skipped when drawing. With no scene, or a broken library, `stage`
-- is drawn plain.
function A.build(stage, scene)
	if not scene or A.problem then return plain(stage) end
	local out = { home = scene.home, sky = scene.sky, under = {}, over = {}, steps = {}, next = 1, total = 0,
		images = scene.images }
	local function add(list, it)
		local item = { s = it.s, f = it.f, x = it.x, y = it.y, w = it.w, id = it.id, drift = it.drift, span = it.span,
			opacity = it.opacity }
		list[#list + 1] = item
		local a = it.a
		out.steps[#out.steps + 1] = { weight = weight(it), item = item, make = function()
			if it.kind == "ground" then item.frame = ground(it)
			elseif it.kind == "platform" then item.frame = platform(it)
			elseif it.kind == "layer" then
				item.frame = picture({ image = a.image, size = a.size, src = a.src, tile = it.tile, overlap = it.overlap, body = a.body }, it.drop)
			elseif it.kind == "haze" then item.frame = haze(it)
			elseif it.kind == "ridge" then item.frame = ridge(it)
			elseif it.kind == "cloud" then item.frame = cloud(a)
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

-- Each item placed by its depth (depth.lua): the stage (f = 1) at the camera's k exactly, layers
-- behind and in front by depth zoom. `t` is render time, for drifting clouds.
local function elements(items, art, cam, k, ox, oy, t)
	local out = {}
	local k1 = k / cam.zoom
	for _, it in ipairs(items) do
		if it.frame then
			local x = it.x
			if it.drift then x = Depth.drift(it.x + it.w / 2, it.drift, it.span, t or 0) - it.w / 2 end
			local sx, sy, s
			if it.f == 1 then sx, sy, s = (x - cam.x) * k, (it.y - cam.y) * k, k
			else sx, sy, s = Depth.place(cam, it.f, art.home, x, it.y, k1) end
			-- `piece:<id>`: a piece that makes collision answers to its id, so the overlay's `col:<id>` can be
			-- held against it (S-M3). The handler is what makes an id reachable to rects().
			out[#out + 1] = ui.frame({ id = it.id and ("piece:" .. it.id) or nil, visual = it.frame, absolute = true,
				on_hover = it.id and function() end or nil, opacity = it.opacity and it.opacity < 1 and it.opacity or nil,
				left = ox + sx, top = oy + sy, scale = s * it.s })
		end
	end
	return out
end

-- Ui elements for the camera `cam`, k px per unit, the playfield's corner at (ox, oy) and its size
-- w x h; `t` is render time (seconds).
function A.under(art, cam, k, ox, oy, w, h, t)
	local list = { ui.frame({ visual = sky(art.sky, w, h), absolute = true, left = ox, top = oy }) }
	for _, e in ipairs(elements(art.under, art, cam, k, ox, oy, t)) do list[#list + 1] = e end
	if A.problem then
		list[#list + 1] = ui.text({ A.problem, id = "library:problem", absolute = true, left = 0, top = 0, font_size = 1, opacity = 0, no_wrap = true })
	end
	return list
end

function A.over(art, cam, k, ox, oy, t)
	return elements(art.over, art, cam, k, ox, oy, t)
end

return A
