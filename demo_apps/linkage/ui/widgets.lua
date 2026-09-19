local C = require("theme")

local W = {}

function W.label(s, color, size)
	return ui.text({ s, color = color or C.dim, font_size = size or 12, no_wrap = true })
end

function W.step(id, glyph, on_click)
	return ui.button({
		id = id,
		on_click = on_click,
		w = 26,
		h = 24,
		center = true,
		radius = 6,
		fill = C.panel,
		hover_fill = C.line,
		press_scale = 0.94,
		ui.text({ glyph, color = C.text, font_size = 13, no_wrap = true }),
	})
end

function W.joint(name, key, value, nudge)
	return ui.row({
		gap = 6,
		align_center = true,
		ui.col({ w = 58, W.label(name) }),
		W.step("dec:" .. key, "-", function() nudge(key, -C.nudge) end),
		W.step("inc:" .. key, "+", function() nudge(key, C.nudge) end),
		ui.col({ w = 96, W.label(value, C.text) }),
	})
end

function W.toggle(id, text, on, on_click)
	return ui.button({
		id = id,
		on_click = on_click,
		h = 28,
		px = 12,
		center = true,
		radius = 7,
		fill = on and C.accent or C.panel,
		hover_fill = on and C.accent or C.line,
		press_scale = 0.96,
		ui.text({ text, color = on and "#0b1220" or C.text, font_size = 12, no_wrap = true }),
	})
end

return W
