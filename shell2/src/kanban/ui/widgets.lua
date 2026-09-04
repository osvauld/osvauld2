-- Small pieces that draw and nothing else.
--
-- The boundary here is not "small" but *pure*: every function below is a function of its
-- arguments and the palette. None of them touch the board, the pointer state or `update`, which
-- is what lets them sit in another file without anything being passed down to reach them.
local C = require("theme")

local M = {}

-- Guides are rendered on *every* gap, always, so the space is permanently reserved and nothing
-- shifts when a drop target appears — only the line's alpha changes. `fade` is keyed by id:
-- that's where the runtime parks the tween, so retargeting mid-flight reverses smoothly.
function M.guide(id, on)
	return ui.col({
		py = 3,
		w_full = true,
		ui.col({ id = id, h = 2, radius = 1, fill = C.accent, fade = { on and 1 or 0, 140 } }),
	})
end

-- Vertical twin, for column reordering. No height: the board row is `stretch`, so it fills.
function M.guide_v(id, on)
	return ui.col({
		px = 7,
		ui.col({
			id = id,
			w = 2,
			grow = true,
			radius = 1,
			fill = C.accent,
			fade = { on and 1 or 0, 140 },
		}),
	})
end

function M.badge(n)
	return ui.row({
		px = 7,
		py = 1,
		radius = 10,
		fill = C.line_soft,
		center = true,
		ui.text({ tostring(n), color = C.muted, font_size = 11 }),
	})
end

function M.icon_button(glyph, on_press, hover)
	return ui.button({
		w = 22,
		h = 22,
		radius = 6,
		center = true,
		hover_fill = hover or C.line,
		ui.text({ glyph, color = C.muted, font_size = 13 }),
		on_click = on_press,
	})
end

function M.empty_slot(on)
	return ui.col({
		h = 64,
		center = true,
		radius = 8,
		stroke = { 1, on and C.accent or C.line_soft },
		ui.text({ "drop a card here", color = on and C.accent or C.muted, font_size = 12 }),
	})
end

return M
