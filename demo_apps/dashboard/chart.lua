-- chart.lua — pure functions of their arguments. Handlers are passed in, never reached for.
local C = require("theme")
local D = require("data")

local M = {}

local W, H = C.plot_w, C.plot_h
local PX, PY = C.pad_x, C.pad_y

-- x of sample i (1..n) inside the plot rect
function M.x_of(i)
	if D.n < 2 then return PX end
	return PX + (i - 1) * ((W - 2 * PX) / (D.n - 1))
end

local step = (W - 2 * PX) / (D.n - 1)

-- nearest sample index for a pointer x in plot-local units
function M.index_at(x)
	local i = math.floor((x - PX) / step + 0.5) + 1
	if i < 1 then i = 1 end
	if i > D.n then i = D.n end
	return i
end

-- y range for a series, padded so the line never touches the edge
function M.bounds(s)
	local lo, hi = s.values[1], s.values[1]
	for i = 1, D.n do
		local v = s.values[i]
		if v < lo then lo = v end
		if v > hi then hi = v end
	end
	if hi == lo then hi = lo + 1 end
	local slack = (hi - lo) * 0.1
	return lo - slack, hi + slack
end

local function y_of(v, lo, hi)
	local t = (v - lo) / (hi - lo)
	return (H - PY) - t * (H - 2 * PY)
end

local function rect(x0, y0, x1, y1)
	return gfx.path({ { "move", x0, y0 }, { "line", x1, y0 }, { "line", x1, y1 }, { "line", x0, y1 }, { "close" } })
end

local function vline(x)
	return gfx.path({ { "move", x, PY - 4 }, { "line", x, H - PY + 4 } })
end

-- the whole plot rect as one frame: band, grid, line, cursor, dots
-- `band` is a live pixel sweep, which wins over `sel`'s whole-month edges while dragging.
function M.visual(s, hover_i, sel, band)
	local lo, hi = M.bounds(s)
	local items = {}

	items[#items + 1] = gfx.fill({ path = rect(0, 0, W, H), brush = gfx.solid(C.panel) })

	local a, b
	if band then
		a, b = band[1], band[2]
		if a > b then a, b = b, a end
	elseif sel then
		a, b = M.x_of(sel.lo) - step * 0.5, M.x_of(sel.hi) + step * 0.5
	end
	if a then
		if a < 0 then a = 0 end
		if b > W then b = W end
		items[#items + 1] = gfx.fill({ path = rect(a, 0, b, H), brush = gfx.solid(C.band) })
	end

	local grid = gfx.solid(C.grid)
	for k = 0, 3 do
		local y = PY + k * ((H - 2 * PY) / 3)
		items[#items + 1] = gfx.stroke({
			path = gfx.path({ { "move", 0, y }, { "line", W, y } }),
			brush = grid,
			width = 1,
		})
	end

	if hover_i then
		items[#items + 1] = gfx.stroke({
			path = vline(M.x_of(hover_i)),
			brush = gfx.solid(C.cursor),
			width = 1,
			dashes = { 3, 3 },
		})
	end

	local cmds = {}
	for i = 1, D.n do
		cmds[i] = { i == 1 and "move" or "line", M.x_of(i), y_of(s.values[i], lo, hi) }
	end
	items[#items + 1] = gfx.stroke({
		path = gfx.path(cmds),
		brush = gfx.solid(s.color),
		width = 2,
		join = "round",
		cap = "round",
	})

	if hover_i then
		local x, y = M.x_of(hover_i), y_of(s.values[hover_i], lo, hi)
		items[#items + 1] = gfx.fill({ path = rect(x - 3, y - 3, x + 3, y + 3), brush = gfx.solid(s.color) })
	end

	items.width = W
	items.height = H
	return gfx.frame(items)
end

local function label(txt, color, size)
	return ui.text({ txt, color = color, font_size = size or 10, no_wrap = true })
end

-- y gutter: max at top, min at bottom, one midpoint
local function y_axis(s)
	local lo, hi = M.bounds(s)
	local mid = (lo + hi) / 2
	return ui.col({
		w = C.axis_w,
		h = H,
		ui.col({ h = PY }),
		label(D.fmt_value(s, hi), C.dim),
		ui.col({ grow = true }),
		label(D.fmt_value(s, mid), C.dim),
		ui.col({ grow = true }),
		label(D.fmt_value(s, lo), C.dim),
		ui.col({ h = PY }),
	})
end

-- x ticks under the plot: every 4th month, each label starting at its sample's x.
-- No text alignment or measurement exists, so labels sit left-aligned on their tick
-- and the last column takes the remainder rather than overflowing the row.
local function x_axis()
	local row = { w = W, h = 14, ui.col({ w = PX }) }
	local cols = math.floor((D.n - 1) / 4) + 1
	local k = 0
	for i = 1, D.n, 4 do
		k = k + 1
		local w = step * 4
		if k == cols then w = W - PX - (cols - 1) * step * 4 end
		row[#row + 1] = ui.col({ w = w, no_shrink = true, label(D.months[i], C.dim, 9) })
	end
	return ui.row(row)
end

-- one chart card. `on_hover` / `on_drag` are the caller's handlers for this plot.
function M.card(s, hover_i, sel, band, on_hover, on_drag)
	local readout = "-"
	if hover_i then
		readout = D.months[hover_i] .. ": " .. D.fmt_value(s, s.values[hover_i])
	end

	return ui.col({
		fill = C.panel,
		radius = 10,
		pad = 12,
		gap = 6,
		stroke = { 1, C.line },
		ui.row({
			gap = 8,
			align_center = true,
			label(s.title, C.text, 13),
			label(s.y_label, C.dim, 10),
			ui.col({ grow = true }),
			ui.text({ readout, id = "readout:" .. s.key, color = s.color, font_size = 12, no_wrap = true }),
		}),
		ui.row({
			gap = 6,
			y_axis(s),
			ui.col({
				id = "plot:" .. s.key,
				w = W,
				h = H,
				on_hover = on_hover,
				on_drag = on_drag,
				ui.frame({ visual = M.visual(s, hover_i, sel, band) }),
			}),
		}),
		ui.row({
			gap = 6,
			ui.col({ w = C.axis_w }),
			x_axis(),
		}),
	})
end

return M
