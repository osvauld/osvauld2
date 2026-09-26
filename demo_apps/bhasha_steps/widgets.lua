local C = require("theme")

local W = {}

function W.label(text, color, size)
	return ui.text({ text, color = color or C.ink, font_size = size or 13 })
end

function W.badge(text, fill, color)
	return ui.row({
		no_shrink = true,
		px = 10,
		py = 5,
		radius = 20,
		fill = fill,
		center = true,
		ui.text({ text, color = color, font_size = 11, no_wrap = true }),
	})
end

function W.button(id, text, on_click, primary)
	return ui.button({
		id = id,
		no_shrink = true,
		min_w = primary and 128 or 92,
		h = 42,
		px = 16,
		radius = C.control_radius,
		center = true,
		fill = primary and C.navy or C.paper,
		hover_fill = primary and C.navy_hi or C.paper_warm,
		press_scale = 0.97,
		stroke = primary and nil or { 1, C.line_strong },
		on_click = on_click,
		ui.text({ text, color = primary and C.white or C.ink, font_size = 13, no_wrap = true }),
	})
end

function W.class_button(level, active, done, on_click)
	return ui.button({
		id = "class:" .. level.class,
		no_shrink = true,
		min_w = 126,
		grow = true,
		px = 13,
		py = 11,
		gap = 8,
		radius = 13,
		align_center = true,
		fill = active and C.navy or C.paper,
		hover_fill = active and C.navy_hi or C.paper_warm,
		stroke = { 1, active and C.navy or C.line },
		press_scale = 0.98,
		on_click = on_click,
		ui.col({
			grow = true,
			gap = 2,
			ui.text({ "Class " .. level.class, color = active and C.white or C.ink, font_size = 13, no_wrap = true }),
			ui.text({ level.tagline, color = active and "#cbd8e2" or C.muted, font_size = 10, no_wrap = true }),
		}),
		W.badge(done .. "/3", active and "#ffffff20" or C.green_soft, active and C.white or C.green),
	})
end

function W.activity_button(exercise, active, done, index, on_click)
	local names = { choose = "Choose", build = "Build", type = "Type" }
	return ui.button({
		id = "open:" .. exercise.id,
		no_shrink = true,
		min_w = 170,
		grow = true,
		px = 14,
		py = 11,
		gap = 10,
		radius = 13,
		align_center = true,
		fill = active and C.saffron_soft or C.paper,
		hover_fill = C.saffron_soft,
		stroke = { active and 2 or 1, active and C.saffron or C.line },
		press_scale = 0.98,
		on_click = on_click,
		W.badge(tostring(index), active and C.saffron or C.paper_warm, active and C.white or C.muted),
		ui.col({
			grow = true,
			gap = 2,
			ui.text({ names[exercise.kind], color = C.ink, font_size = 12, no_wrap = true }),
			ui.text({ exercise.skill, color = C.muted, font_size = 10, no_wrap = true }),
		}),
		ui.text({ done and "✓" or "·", color = done and C.green or C.faint, font_size = 16, no_wrap = true }),
	})
end

function W.option(id, text, selected, state, on_click)
	local fill, stroke, mark = C.paper, C.line, "○"
	if selected then
		fill, stroke, mark = C.blue_soft, C.blue, "●"
	end
	if state == "correct" then
		fill, stroke, mark = C.green_soft, C.green, "✓"
	elseif state == "wrong" then
		fill, stroke, mark = C.wrong_soft, C.wrong, "×"
	end
	return ui.button({
		id = id,
		w_full = true,
		min_h = 48,
		px = 14,
		py = 10,
		gap = 11,
		radius = 12,
		align_center = true,
		fill = fill,
		hover_fill = selected and C.blue_soft or C.paper_warm,
		stroke = { 1, stroke },
		press_scale = 0.99,
		on_click = on_click,
		ui.text({ mark, color = stroke, font_size = 15, no_wrap = true }),
		ui.col({ grow = true, ui.text({ text, color = C.ink, font_size = 13 }) }),
	})
end

function W.progress(done, total)
	local width = 172
	return ui.row({
		gap = 9,
		align_center = true,
		ui.row({
			w = width,
			h = 7,
			radius = 4,
			fill = C.line,
			ui.col({ w = width * done / total, h = 7, radius = 4, fill = C.green }),
		}),
		ui.text({ done .. " of " .. total, color = C.muted, font_size = 11, no_wrap = true }),
	})
end

return W
