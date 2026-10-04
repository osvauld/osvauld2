local C = require("theme")
local model = require("editor_model")
local M = {}

local function button(id, label, action, active)
	return ui.row({
		id = id, h = 32, px = 10, radius = 7, center = true,
		fill = active and C.hover or C.panel, hover_fill = C.hover,
		stroke = { 1, active and C.accent or C.line }, cursor = "arrow",
		on_click = action,
		ui.text({ label, color = C.text, font_size = 12, no_wrap = true }),
	})
end

function M.button()
	return button("launcher:customize", "Customize", function() model.open("theme.lua") end)
end

function M.view()
	if not model.visible() then return {} end
	local S = model.state()
	local files = {}
	for _, path in ipairs({ "theme.lua", "main.lua", "cursor.lua" }) do
		files[#files + 1] = button("launcher:edit:" .. path, path, function() model.open(path) end, S.path == path)
	end
	return ui.col({
		id = "launcher:editor:scrim", absolute = true,
		left = 0, right = 0, top = 0, bottom = 0,
		fill = "rgba(0,0,0,0.65)", on_click = model.cancel,
		ui.col({
			id = "launcher:editor:panel", absolute = true,
			left = 12, right = 12, top = 12, bottom = 12,
			pad = 12, gap = 8, radius = 12, fill = C.panel, stroke = { 1, C.line },
			on_click = function() end,
			ui.text({ "Make this workspace yours", font_size = 18, color = C.text }),
			ui.row({ wrap = true, gap = 8, files }),
			ui.text({ "Save validates your code before changing the launcher. Failed edits keep this draft.",
				font_size = 12, color = C.muted }),
			ui.text_area({
				id = "launcher:editor:source", value = model.value(),
				grow = true, min_h = 80, w_full = true, font_size = 13,
				fill = C.bg, color = C.text, radius = 8, pad = 10, cursor = "text",
				on_input = model.input,
			}),
			ui.row({
				wrap = true, gap = 8,
				button("launcher:editor:save", "Save & apply", model.save, true),
				button("launcher:editor:cancel", "Cancel", model.cancel),
			}),
		}),
	})
end

return M
