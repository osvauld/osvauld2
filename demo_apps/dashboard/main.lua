-- main.lua — the view, and everything that reads or writes pointer state.
local C = require("theme")
local D = require("data")
local Chart = require("chart")

-- viewer/gesture state: dies with the app instance, which is what we want for a cursor
-- `band` is the live pixel sweep; `sel` is the months it snaps to.
local S = { hover = nil, drag = nil, sel = nil, band = nil }

local function range()
	if S.sel then return S.sel.lo, S.sel.hi end
	return 1, D.n
end

local function stat_of(s, lo, hi)
	local sum = 0
	for i = lo, hi do sum = sum + s.values[i] end
	if s.stat == "total" then return sum end
	return sum / (hi - lo + 1)
end

-- shared cursor: any chart's hover sets the one index all three draw
local function on_plot_hover(e)
	local phase, x, y = e.phase, e.x, e.y
	if phase == "leave" then
		if not S.drag then S.hover = nil end
		return
	end
	S.hover = Chart.index_at(x)
end

-- range selection. `x` is the pointer in plot units, the same units on_hover reports.
local function on_plot_drag(e)
	local phase, x = e.phase, e.x
	if phase == "start" then
		local i = Chart.index_at(x)
		S.drag = { from = i, x0 = x }
		S.sel = { lo = i, hi = i }
		S.band = { x, x }
		return
	end
	if not S.drag then return end -- a stale drag
	local j = Chart.index_at(x)
	local a, b = S.drag.from, j
	if a > b then a, b = b, a end
	S.sel = { lo = a, hi = b }
	-- The band edge tracks the pointer; only the tiles snap to whole months.
	S.band = { S.drag.x0, x }
	S.hover = j
	if phase == "end" then
		S.drag, S.band = nil, nil
		if S.sel.lo == S.sel.hi then S.sel = nil end -- a sweep that never moved clears
	end
end

local function tile(s, lo, hi)
	local v = D.fmt_value(s, stat_of(s, lo, hi))
	local cursor = S.hover and (D.months[S.hover] .. ": " .. D.fmt_value(s, s.values[S.hover])) or " "
	return ui.col({
		grow = true,
		min_w = 150,
		fill = C.tile,
		radius = 10,
		pad = 12,
		gap = 4,
		stroke = { 1, C.line },
		ui.text({ s.stat_label, id = "tile:" .. s.key .. ":label", color = C.dim, font_size = 10, no_wrap = true }),
		ui.text({ v, id = "tile:" .. s.key .. ":value", color = s.color, font_size = 22, no_wrap = true }),
		ui.text({ cursor, id = "tile:" .. s.key .. ":cursor", color = C.dim, font_size = 10, no_wrap = true }),
	})
end

return function()
	local lo, hi = range()
	local range_text = S.sel
			and (D.months[lo] .. " to " .. D.months[hi] .. "  (" .. (hi - lo + 1) .. " mo)")
		or ("All " .. D.n .. " months")

	local tiles = {}
	for i = 1, #D.series do
		tiles[i] = tile(D.series[i], lo, hi)
	end

	local charts = {}
	for i = 1, #D.series do
		charts[i] = Chart.card(D.series[i], S.hover, S.sel, S.band, on_plot_hover, on_plot_drag)
	end

	return ui.col({
		id = "dashboard",
		grow = true,
		fill = C.bg,
		pad = 16,
		gap = 12,
		scroll_y = true,
		ui.row({
			gap = 10,
			align_center = true,
			ui.text({ "Dashboard", color = C.text, font_size = 20, no_wrap = true }),
			ui.text({ range_text, id = "range", color = C.dim, font_size = 12, no_wrap = true }),
			ui.col({ grow = true }),
			ui.button({
				id = "clear",
				h = 28,
				px = 12,
				radius = 8,
				center = true,
				fill = C.btn,
				hover_fill = C.btn_hover,
				press_scale = 0.96,
				stroke = { 1, C.line },
				ui.text({ "Clear selection", color = C.text, font_size = 11, no_wrap = true }),
				on_click = function()
					S.sel = nil
					S.drag = nil
					S.band = nil
				end,
			}),
		}),
		ui.row({ gap = 12, stretch = true, tiles }),
		ui.text({ "Hover a month to compare all three; drag sideways to select a range.", color = C.dim, font_size = 10 }),
		ui.col({ gap = 12, charts }),
	})
end
