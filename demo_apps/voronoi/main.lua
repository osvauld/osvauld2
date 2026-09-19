local C = require("theme")
local V = require("voronoi")
local M = require("model")

local KAPPA = 0.5522847498307933

local function poly_path(poly, ox, oy)
	local cmds = { { "move", poly[1][1] - ox, poly[1][2] - oy } }
	for i = 2, #poly do
		cmds[i] = { "line", poly[i][1] - ox, poly[i][2] - oy }
	end
	cmds[#cmds + 1] = { "close" }
	return gfx.path(cmds)
end

local function circle_path(cx, cy, r)
	local k = r * KAPPA
	return gfx.path({
		{ "move", cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
		{ "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
		{ "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "close" },
	})
end

local function on_hover(phase, x, y, shape, sx, sy)
	if phase == "leave" then
		M.pointer, M.hit = nil, nil
	else
		M.pointer = { x, y }
		M.hit = shape and { shape = shape, sx = sx, sy = sy } or nil
	end
end

-- A drag past the press slop cancels the click, so selection and dragging share the frame without
-- a guard between them.
local function on_drag(phase, x, y, dx, dy, scale, origin_x, origin_y, shape, sx, sy)
	if phase == "start" then
		M.grab(shape, sx, sy)
	end
	M.drag_to(x, y)
	if phase == "end" then
		M.drag = nil
	end
end

local function on_click(x, y, shape)
	local i = M.index_of(shape)
	if not i then
		local _, live = M.under(x, y)
		i = live
	end
	M.sel = M.sel ~= i and i or nil
end

local function label(text, color, size)
	return ui.text({ text, color = color, font_size = size, no_wrap = true })
end

local function readout(live_name)
	local d = M.drag
	if d then
		local s = M.sites[d.i]
		return string.format(
			"holding %s   site (%.0f, %.0f)   grabbed at sx %.1f  sy %.1f   pointer over %s",
			d.shape,
			s.x,
			s.y,
			d.sx,
			d.sy,
			live_name
		)
	end

	local h = M.hit
	if not h then
		return "hover a cell"
	end
	local s = M.sites[M.index_of(h.shape)]
	local line = string.format(
		"%s   site (%.0f, %.0f)   sx %.1f  sy %.1f",
		h.shape,
		s.x,
		s.y,
		h.sx,
		h.sy
	)
	if live_name ~= h.shape then
		line = line .. "   →  now " .. live_name .. ", the diagram moved under a still pointer"
	end
	return line
end

return function()
	local sites = M.sites
	local cells = V.cells(sites, C.w, C.h)

	local hot_kind, hot_i
	if M.pointer then
		hot_kind, hot_i = M.under(M.pointer[1], M.pointer[2])
	end

	-- Each cell is drawn in its own site's frame, so the pointer reports sx, sy as an offset from
	-- that site instead of repeating the frame coordinates it already gets as x, y.
	local items, paths, live = { width = C.w, height = C.h }, {}, 0
	for i = 1, #cells do
		local poly = cells[i]
		if #poly >= 3 then
			live = live + 1
			local s = sites[i]
			paths[i] = poly_path(poly, s.x, s.y)
			local lit = hot_kind == "cell" and hot_i == i
			items[#items + 1] = gfx.group({
				id = "cell:" .. i,
				transform = { 1, 0, 0, 1, s.x, s.y },
				gfx.fill({
					path = paths[i],
					brush = gfx.solid(lit and C.cells_hot[i] or C.cells[i]),
				}),
				gfx.stroke({
					path = paths[i],
					brush = gfx.solid(C.edge),
					width = C.edge_w,
				}),
			})
		end
	end

	-- the halo rides an unnamed group so it paints over every cell without claiming the pointer
	if M.sel and paths[M.sel] then
		items[#items + 1] = gfx.group({
			transform = { 1, 0, 0, 1, sites[M.sel].x, sites[M.sel].y },
			gfx.stroke({
				path = paths[M.sel],
				brush = gfx.solid(C.halo),
				width = C.halo_w,
				join = "round",
			}),
		})
	end

	-- dots last: where two named shapes overlap, the later one answers the pointer.
	-- Each is a named group holding one dot drawn at the origin, so the pointer reports
	-- sx, sy as an offset from its site rather than as frame coordinates again.
	local rim, rim_brush = circle_path(0, 0, C.site_hit), gfx.solid(C.site_rim)
	local core, core_brush = circle_path(0, 0, C.site_r), gfx.solid(C.site_core)
	local core_lit, core_lit_brush = circle_path(0, 0, C.site_r + 1.5), gfx.solid(C.site_core_hot)
	for i = 1, #sites do
		local s = sites[i]
		local lit = hot_kind == "site" and hot_i == i
		items[#items + 1] = gfx.group({
			id = "site:" .. i,
			transform = { 1, 0, 0, 1, s.x, s.y },
			gfx.fill({ path = rim, brush = rim_brush }),
			gfx.fill({
				path = lit and core_lit or core,
				brush = lit and core_lit_brush or core_brush,
			}),
		})
	end

	local canvas = {
		id = "canvas",
		visual = gfx.frame(items),
		on_hover = on_hover,
		on_click = on_click,
		on_drag = on_drag,
	}
	if not M.paused then
		canvas.on_frame = M.step
	end

	local live_name = hot_kind and (hot_kind .. ":" .. hot_i) or "nothing"

	return ui.col({
		id = "voronoi",
		grow = true,
		fill = C.bg,
		pad = 20,
		gap = 14,
		align_center = true,
		ui.row({
			w = C.w,
			gap = 12,
			align_center = true,
			label("Voronoi", C.text, 16),
			label("every cell is clipped out of the rectangle, this frame — drag a site", C.dim, 12),
			ui.col({ grow = true }),
			label(live .. " / " .. #sites .. " cells", C.dim, 12),
			ui.button({
				id = "pause",
				h = 28,
				px = 12,
				radius = 7,
				center = true,
				fill = C.accent,
				hover_fill = C.accent_hot,
				press_scale = 0.96,
				label(M.paused and "Resume" or "Pause", "#ffffff", 12),
				on_click = function()
					M.paused = not M.paused
				end,
			}),
		}),
		ui.frame(canvas),
		ui.col({
			w = C.w,
			gap = 4,
			label(readout(live_name), C.text, 12),
			label(
				M.sel and ("selected cell:" .. M.sel .. " — click it again to clear")
					or "click to select a cell; drag a site, or anywhere in a cell, to move it",
				C.dim,
				12
			),
		}),
	})
end
