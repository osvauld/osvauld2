local C = require("theme")

local W = {}

function W.label(text, color, size)
	return ui.text({ text, color = color or C.muted, font_size = size or 12, no_wrap = true })
end

function W.button(id, label, on_click, style)
	style = style or "quiet"
	local fill = C.paper
	local hover = C.saffron_soft
	local color = C.ink
	local stroke = { 1, C.line }
	if style == "primary" then
		fill, hover, color, stroke = C.saffron, C.saffron_hi, "#ffffff", { 1, C.saffron }
	elseif style == "teal" then
		fill, hover, color, stroke = C.teal, C.teal_hi, "#ffffff", { 1, C.teal }
	elseif style == "soft" then
		fill, hover, color, stroke = C.saffron_soft, "#ffe4bb", C.saffron, { 1, "#f4cf99" }
	end
	return ui.button({
		id = id,
		h = 42,
		px = 18,
		center = true,
		no_shrink = true,
		radius = 12,
		fill = fill,
		hover_fill = hover,
		press_fill = hover,
		press_scale = 0.97,
		tint = 100,
		stroke = stroke,
		on_click = on_click,
		ui.text({ label, color = color, font_size = 13, no_wrap = true }),
	})
end

function W.badge(text, tone)
	local fill, color = C.blue_soft, C.blue
	if tone == "green" then
		fill, color = C.green_soft, C.green
	elseif tone == "pink" then
		fill, color = C.pink_soft, C.pink
	elseif tone == "saffron" then
		fill, color = C.saffron_soft, C.saffron
	end
	return ui.row({
		px = 10,
		py = 4,
		radius = 12,
		fill = fill,
		no_shrink = true,
		ui.text({ text, color = color, font_size = 11, no_wrap = true }),
	})
end

function W.divider()
	return ui.col({ h = 1, w_full = true, fill = C.line })
end

return W
