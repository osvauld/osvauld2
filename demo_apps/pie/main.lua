-- main.lua — the view, and everything that reads or writes pointer state.
local C = require("theme")
local D = require("data")
local Pie = require("pie")

-- viewer/gesture state: dies with the app instance, which is what we want for a cursor.
-- `hot` and `sel` hold shape ids the runtime gave us; `at` is the point inside the hot slice.
local S = { hot = nil, sel = nil, at = nil }

local function on_pie_hover(phase, _, _, shape, sx, sy)
	if phase == "leave" or not shape then
		S.hot, S.at = nil, nil
		return
	end
	S.hot, S.at = shape, { sx, sy }
end

local function on_pie_click(_, _, shape)
	S.sel = (shape ~= S.sel) and shape or nil
end

local function pct(v)
	return string.format("%.1f%%", 100 * v / D.total)
end

local function label(txt, color, size)
	return ui.text({ txt, color = color, font_size = size or 11, no_wrap = true })
end

-- One legend row. Clicking it selects the same slice the pie would.
local function row(w)
	local id = "slice:" .. w.slice.key
	local on = (id == S.hot or id == S.sel)
	return ui.row({
		id = "legend:" .. w.slice.key,
		gap = 8,
		align_center = true,
		px = 8,
		h = 26,
		radius = 6,
		fill = on and "#1c2128" or C.panel,
		on_click = function() S.sel = (id ~= S.sel) and id or nil end,
		ui.frame({
			visual = gfx.frame({
				width = 10,
				height = 10,
				gfx.fill({
					path = gfx.path({
						{ "move", 0, 0 }, { "line", 10, 0 }, { "line", 10, 10 }, { "line", 0, 10 }, { "close" },
					}),
					brush = gfx.solid(w.slice.color),
				}),
			}),
		}),
		label(w.slice.label, on and C.text or C.dim),
		ui.col({ grow = true, min_w = 10 }),
		label(pct(w.slice.value), C.dim, 10),
	})
end

local function readout()
	local s, into, depth = Pie.reading(S.hot, S.at and S.at[1] or 0, S.at and S.at[2] or 0)
	if not s then
		return label("Hover a slice — the runtime names it, the app doesn't guess", C.dim, 11)
	end
	local text = string.format(
		"%s · %s of %d · %.0f° into the wedge, %.0f%% out from the middle",
		s.label, pct(s.value), D.total, into, depth * 100
	)
	return ui.text({ text, id = "readout", color = s.color, font_size = 11, no_wrap = true })
end

return function()
	local legend = {}
	for i, w in ipairs(Pie.layout()) do
		legend[i] = row(w)
	end

	local chosen = S.sel and D.find(S.sel:sub(7))

	return ui.col({
		id = "pie-app",
		grow = true,
		fill = C.bg,
		pad = 20,
		gap = 14,
		scroll_y = true,
		ui.row({
			gap = 10,
			align_center = true,
			label("Where visits come from", C.text, 18),
			label(chosen and (chosen.label .. " selected — click it again to clear") or "Click a slice to keep it", C.dim, 11),
		}),
		ui.row({
			gap = 24,
			ui.col({
				-- The handlers live on the frame itself: one element, one visual, and the shape
				-- ids come back from the thing that drew them.
				ui.frame({
					id = "pie",
					visual = Pie.visual(S.hot, S.sel),
					on_hover = on_pie_hover,
					on_click = on_pie_click,
				}),
			}),
			ui.col({
				gap = 4,
				min_w = 220,
				label("Share of visits", C.dim, 10),
				legend,
				ui.col({ h = 8 }),
				label("The hub in the middle is unnamed paint —", C.dim, 10),
				label("the pointer falls through it to the slice below.", C.dim, 10),
			}),
		}),
		readout(),
	})
end
