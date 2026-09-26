local C = require("theme")
local NumberLine = require("ui/number_line")

local M = {}

local place_names = { "THOUSANDS", "HUNDREDS", "TENS", "ONES" }

local function place_value(exercise, model, S)
	local wheels = {}
	for place, name in ipairs(place_names) do
		local digit = S.manip.digits[place]
		wheels[#wheels + 1] = ui.button({
			id = "place:" .. exercise.id .. ":" .. place,
			grow = true,
			min_w = 78,
			min_h = 104,
			gap = 7,
			center = true,
			radius = 15,
			fill = digit == exercise.manip.digits[place] and C.teal_soft or C.paper_warm,
			hover_fill = C.saffron_soft,
			press_scale = 0.96,
			stroke = { 1, C.line_strong },
			on_click = function() model.cycle_digit(place) end,
			ui.text({ tostring(digit), color = C.ink, font_size = 34, no_wrap = true }),
			ui.text({ name, color = C.muted, font_size = 9, no_wrap = true }),
			ui.text({ "tap to turn", color = C.saffron, font_size = 9, no_wrap = true }),
		})
	end
	return ui.col({
		gap = 10,
		ui.row({ gap = 9, wrap = true, stretch = true, wheels }),
		ui.text({ "Live number: " .. table.concat(S.manip.digits), color = C.teal, font_size = 14, no_wrap = true }),
	})
end

local function counter(fill)
	return ui.col({
		w = 18,
		h = 18,
		radius = 9,
		fill = fill,
		stroke = { 1, fill == C.paper and C.line_strong or fill },
	})
end

local function grid(exercise, model, S)
	local rows = {}
	local total = 0
	for row = 1, exercise.manip.rows do
		local selected = S.manip.rows[row] == true
		if selected then total = total + exercise.manip.cols end
		local cells = {}
		for _ = 1, exercise.manip.cols do
			cells[#cells + 1] = counter(selected and (row <= exercise.manip.split and C.saffron or C.teal) or C.paper)
		end
		rows[#rows + 1] = ui.button({
			id = "grid-row:" .. exercise.id .. ":" .. row,
			min_h = 32,
			px = 10,
			gap = 8,
			align_center = true,
			radius = 11,
			fill = selected and C.paper_warm or C.bg,
			hover_fill = C.saffron_soft,
			press_scale = 0.98,
			on_click = function() model.toggle_row(row) end,
			ui.text({ tostring(row), color = C.muted, font_size = 10, no_wrap = true }),
			cells,
		})
	end
	return ui.row({
		gap = 14,
		wrap = true,
		align_center = true,
		ui.col({ gap = 5, rows }),
		ui.col({
			gap = 5,
			ui.text({ tostring(total), color = C.ink, font_size = 38, no_wrap = true }),
			ui.text({ "counters shaded", color = C.muted, font_size = 11, no_wrap = true }),
			ui.text({ "orange = 6 × 4", color = C.saffron, font_size = 11, no_wrap = true }),
			ui.text({ "teal = + 18", color = C.teal, font_size = 11, no_wrap = true }),
		}),
	})
end

function M.view(exercise, model, S)
	if exercise.manip.kind == "place" then return place_value(exercise, model, S) end
	if exercise.manip.kind == "grid" then return grid(exercise, model, S) end
	return NumberLine.view(exercise, model, S)
end

return M
