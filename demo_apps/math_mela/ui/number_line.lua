local C = require("theme")

local M = {}

local function rect(x, y, w, h)
	return gfx.path({
		{ "move", x, y }, { "line", x + w, y },
		{ "line", x + w, y + h }, { "line", x, y + h }, { "close" },
	})
end

function M.view(exercise, model, S)
	local spec = exercise.manip
	local marker = S.manip.marker
	local ratio = (marker - spec.min) / (spec.max - spec.min)
	local marker_x = 20 + ratio * 240
	local ticks = { { "move", 20, 58 }, { "line", 260, 58 } }
	for i = 0, 10 do
		local x = 20 + i * 24
		ticks[#ticks + 1] = { "move", x, 51 }
		ticks[#ticks + 1] = { "line", x, 65 }
	end
	local marker_path = gfx.path({
		{ "move", marker_x, 25 }, { "line", marker_x + 10, 43 },
		{ "line", marker_x + 5, 43 }, { "line", marker_x + 5, 72 },
		{ "line", marker_x - 5, 72 }, { "line", marker_x - 5, 43 },
		{ "line", marker_x - 10, 43 }, { "close" },
	})
	local visual = gfx.frame({
		width = 280,
		height = 88,
		gfx.stroke({ path = gfx.path(ticks), brush = gfx.solid(C.line_strong), width = 2, cap = "round" }),
		gfx.fill({ id = "track", path = rect(8, 8, 264, 72), brush = gfx.solid("rgba(255,255,255,0.01)") }),
		gfx.fill({ id = "marker", path = marker_path, brush = gfx.solid(C.saffron) }),
	})
	local target = spec.labels[math.min(S.manip.step, #spec.labels)]
	return ui.col({
		gap = 8,
		align_center = true,
		ui.row({
			gap = 8,
			align_center = true,
			ui.text({ "Find " .. target, color = C.teal, font_size = 14, no_wrap = true }),
			ui.text({ "Marker: " .. marker .. spec.unit, color = C.ink, font_size = 14, no_wrap = true }),
		}),
		ui.frame({
			id = "number-line:" .. exercise.id,
			visual = visual,
			on_drag = function(e)
				local ratio_at_pointer = math.max(0, math.min(1, (e.x - 20) / 240))
				local value = math.floor(spec.min + ratio_at_pointer * (spec.max - spec.min) + 0.5)
				model.move_marker(value, e.phase)
			end,
		}),
		ui.row({
			w = 280,
			ui.text({ tostring(spec.min) .. spec.unit, color = C.muted, font_size = 11, no_wrap = true }),
			ui.col({ grow = true }),
			ui.text({ tostring(spec.max) .. spec.unit, color = C.muted, font_size = 11, no_wrap = true }),
		}),
	})
end

return M
