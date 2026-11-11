-- The asset sheet (docs/design/brawl-maps.md decision 12): every library asset in a grid, drawn at
-- its natural scale or smaller, with its name, kind and default layer, a cross on its anchor, and
-- its collision template (a solid in red, a one-way platform as a yellow line; a platform's 3-slice
-- columns in cyan). The layout is Library.sheet (pure, P-V4); this file only draws it.
-- View-side only, like scene_art: nothing here is read by the rules.
local C = require("theme")
local Library = require("library")
local SceneArt = require("scene_art")
local G = {}

local BAR = 44 -- the title bar above the grid
local CELL_BG, PAGE_BG = "#66748a", "#2b3342"
local RED, YELLOW, CYAN, PINK = "#ff3a3a", "#ffd23f", "#38e1ff", "#ff2fd0"

local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end

local function rect_path(x, y, w, h)
	return gfx.path({ { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } })
end

local function line_path(x1, y1, x2, y2)
	return gfx.path({ { "move", x1, y1 }, { "line", x2, y2 } })
end

local function circle_path(cx, cy, r)
	local k = 0.5523 * r
	return gfx.path({ { "move", cx + r, cy }, { "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
		{ "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy }, { "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
		{ "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy }, { "close" } })
end

-- "#rrggbb" with an alpha 0..1 as "#rrggbbaa".
local function with_alpha(color, a)
	return string.format("%s%02x", color:sub(1, 7), math.floor(math.max(0, math.min(1, a)) * 255 + 0.5))
end

local function stroke(path, color, width) return gfx.stroke({ path = path, brush = brush(color), width = width }) end

-- A picture (or a region of one) at (x, y) in the content frame.
local function picture(p, x, y)
	return gfx.image({ texture = SceneArt.texture(p.image), src = p.src, transform = { 1, 0, 0, 1, x, y } })
end

-- The anchor cross: arms 9 screen pixels long, two pixels wide, however small the asset is drawn.
local function cross(items, ax, ay, s)
	local arm, w = 9 / s, 2 / s
	items[#items + 1] = stroke(line_path(ax - arm, ay, ax + arm, ay), PINK, w)
	items[#items + 1] = stroke(line_path(ax, ay - arm, ax, ay + arm), PINK, w)
end

local function swatch_frame(a, w, h)
	local items = { width = w, height = h }
	if a.kind == "sky" then
		local stops = {}
		for i, st in ipairs(a.stops) do stops[i] = { st[1], st[2] } end
		items[#items + 1] = gfx.fill({ path = rect_path(0, 0, w, h),
			brush = gfx.linear_gradient({ from = { 0, 0 }, to = { 0, h }, stops = stops }) })
		if a.sun then
			local r = math.max(10, a.sun.r * 0.25)
			items[#items + 1] = gfx.fill({ path = circle_path(a.sun.x * w, a.sun.y * h, r),
				brush = gfx.radial_gradient({ center = { a.sun.x * w, a.sun.y * h }, radius = r,
					stops = { { 0, with_alpha(a.sun.color, 1) }, { 1, with_alpha(a.sun.color, 0) } } }) })
		end
	elseif a.kind == "haze" then
		items[#items + 1] = gfx.fill({ path = rect_path(0, 0, w, h), brush = brush("#3a4252") })
		items[#items + 1] = gfx.fill({ path = rect_path(0, 0, w, h),
			brush = gfx.linear_gradient({ from = { 0, 0 }, to = { 0, h }, stops = { { 0, with_alpha(a.color, a.from) }, { 1, with_alpha(a.color, a.to) } } }) })
	else -- ridge: three hills in the asset's colour
		items[#items + 1] = gfx.fill({ path = gfx.path({ { "move", 0, h }, { "line", 0, h * 0.6 }, { "quad", w * 0.12, h * 0.1, w * 0.3, h * 0.5 },
			{ "quad", w * 0.45, h * 0.85, w * 0.6, h * 0.35 }, { "quad", w * 0.8, h * 0.05, w, h * 0.55 }, { "line", w, h }, { "close" } }),
			brush = brush(a.color) })
	end
	return items
end

-- One asset's content frame, in picture pixels (`ext` wide and tall), with its anchor and collision
-- drawn over it. `s` is the scale it will be shown at, so the lines stay two pixels wide.
local function content(a, ext, s)
	local w, h = ext[1], ext[2]
	local lw = 2 / s
	if a.kind == "sky" or a.kind == "haze" or a.kind == "ridge" then return gfx.frame(swatch_frame(a, w, h)) end
	local items = { width = w, height = h }
	if a.kind == "cloud" then
		local spec = gfx.svg(a.svg)
		items[#items + 1] = gfx.instance({ visual = gfx.drawing(spec):pose({}), transform = { 1, 0, 0, 1, 0, 0 } })
		cross(items, a.anchor[1], a.anchor[2], s)
	elseif a.kind == "layer" then
		items[#items + 1] = picture(a, 0, 0)
	elseif a.kind == "ground" then
		local x = 0
		for _, side in ipairs({ "left", "top", "right" }) do
			local p = a[side]
			if p then
				items[#items + 1] = picture(p, x, 0)
				if side == "top" then
					-- the solid: its top edge sits on the first opaque row, `lift`
					items[#items + 1] = gfx.fill({ path = rect_path(x, a.lift, p.size[1], p.size[2] - a.lift), brush = brush(with_alpha(RED, 0.18)) })
					items[#items + 1] = stroke(rect_path(x + lw / 2, a.lift + lw / 2, p.size[1] - lw, p.size[2] - a.lift - lw), RED, lw)
				end
				x = x + p.size[1]
			end
		end
	elseif a.kind == "platform" then
		items[#items + 1] = picture(a, 0, 0)
		if a.slice then
			for _, col in ipairs(a.slice) do items[#items + 1] = stroke(line_path(col, 0, col, h), CYAN, 1 / s) end
		end
		items[#items + 1] = stroke(line_path(0, a.lift, w, a.lift), YELLOW, 3 / s)
	else -- prop
		items[#items + 1] = picture(a, 0, 0)
		local ax, ay = a.anchor[1], a.anchor[2]
		local col = a.collide
		if col and col.solid then
			local t = col.solid
			items[#items + 1] = gfx.fill({ path = rect_path(ax + t[1], ay + t[2], t[3], t[4]), brush = brush(with_alpha(RED, 0.18)) })
			items[#items + 1] = stroke(rect_path(ax + t[1], ay + t[2], t[3], t[4]), RED, lw)
		elseif col and col.platform then
			local t = col.platform
			items[#items + 1] = stroke(line_path(ax + t[1], ay + t[2], ax + t[1] + t[3], ay + t[2]), YELLOW, 3 / s)
		end
		cross(items, ax, ay, s)
	end
	return gfx.frame(items)
end

-- The sheet for `lib`, its frames built a few per call (a decode is native work counted against the
-- entry's CPU budget, as in scene_art). `scales` are the cells' scales: a cell is a fixed size, so
-- they don't depend on the page.
function G.new(lib)
	local g = { lib = lib, steps = {}, next = 1, frames = {}, page = 1 }
	local cells = Library.sheet(lib, { w = Library.CELL.w * 64, h = Library.CELL.h * 64 }).cells
	g.cells = cells
	for _, c in ipairs(cells) do
		local a = Library.get(lib, c.name)
		local pixels = a.src and a.src[3] * a.src[4] or (a.size and a.size[1] * a.size[2]) or (a.top and a.top.size[1] * a.top.size[2]) or 20000
		g.steps[#g.steps + 1] = { weight = pixels, name = c.name, make = function()
			g.frames[c.name] = content(a, c.ext, c.scale)
		end }
	end
	g.total = #g.steps
	return g
end

-- Builds the next frames, up to about `pixels` of picture (at least one). True when all are built.
function G.load(g, pixels)
	local spent = 0
	while g.next <= #g.steps and (spent == 0 or spent + g.steps[g.next].weight <= pixels) do
		local step = g.steps[g.next]
		step.make()
		spent = spent + step.weight
		g.next = g.next + 1
	end
	return g.next > #g.steps
end

function G.progress(g) return string.format("%d/%d", g.next - 1, g.total) end

-- The elements for a playfield `w` x `h`: a bar, then the page's cells. `toggle` closes the sheet.
function G.view(g, w, h, toggle)
	local sheet = Library.sheet(g.lib, { w = w, h = h - BAR })
	g.page = math.max(1, math.min(g.page, sheet.pages))
	local out = { ui.col({ absolute = true, left = 0, top = 0, w = w, h = h, fill = PAGE_BG }) }
	local pages = sheet.pages
	out[#out + 1] = ui.text({ "Assets  " .. G.progress(g) .. "  page " .. g.page .. "/" .. pages, id = "gallery:title",
		absolute = true, left = 16, top = 12, color = C.text, font_size = 15, no_wrap = true })
	out[#out + 1] = ui.button({ "Back to the map", id = "assets:back", absolute = true, right = 16, top = 8, on_click = toggle })
	if pages > 1 then
		out[#out + 1] = ui.button({ "<", id = "assets:prev", absolute = true, left = 300, top = 8, on_click = function() g.page = math.max(1, g.page - 1) end })
		out[#out + 1] = ui.button({ ">", id = "assets:next", absolute = true, left = 340, top = 8, on_click = function() g.page = math.min(pages, g.page + 1) end })
	end
	for _, c in ipairs(sheet.cells) do
		if c.page == g.page then
			local y = c.y + BAR
			out[#out + 1] = ui.col({ absolute = true, left = c.x + 2, top = y + 2, w = c.w - 4, h = c.h - 4, fill = CELL_BG })
			local frame = g.frames[c.name]
			if frame then
				local fw, fh = c.ext[1] * c.scale, c.ext[2] * c.scale
				out[#out + 1] = ui.frame({ id = "cell:" .. c.name, visual = frame, absolute = true, scale = c.scale,
					left = c.box.x + (c.box.w - fw) / 2, top = c.box.y + BAR + (c.box.h - fh) / 2 })
			end
			local a = Library.get(g.lib, c.name)
			local detail = c.kind .. " · " .. c.layer .. (a.size and string.format(" · %d×%d", a.size[1], a.size[2]) or "")
			out[#out + 1] = ui.text({ c.name, absolute = true, left = c.x + 10, top = y + c.h - c.label_h + 2, color = "#ffffff", font_size = 12, no_wrap = true })
			out[#out + 1] = ui.text({ detail, absolute = true, left = c.x + 10, top = y + c.h - c.label_h + 18, color = "#dbe3ee", font_size = 10, no_wrap = true })
		end
	end
	return out
end

return G
