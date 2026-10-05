local C = require("theme")

local function card(workspace, select_workspace)
	return ui.col({
		id = "launcher:space:" .. workspace.id,
		w = 200, min_w = 160, min_h = 144, pad = 18, gap = 12,
		radius = 14, fill = C.panel, stroke = { 1, C.line },
		hover_fill = C.hover, hover_stroke = { 1, C.accent },
		tint = 140, press_scale = 0.97, cursor = "arrow",
		on_click = function() select_workspace(workspace.id) end,
		ui.text({ "WORKSPACE", color = C.accent, font_size = 11, no_wrap = true }),
		ui.text({ workspace.name, color = C.text, font_size = 19 }),
		ui.col({ grow = true }),
		ui.text({ "Open workspace →", color = C.muted, font_size = 12, no_wrap = true }),
	})
end

return function(workspaces, select_workspace, creation)
	local cards = {}
	for _, workspace in ipairs(workspaces) do
		cards[#cards + 1] = card(workspace, select_workspace)
	end
	return ui.col({
		id = "launcher:workspace-list", grow = true, stretch = true,
		fill = C.bg, radius = 18, pad = 20, gap = 16,
		ui.text({ "Your workspaces", color = C.text, font_size = 24 }),
		ui.text({ "Choose a place to work", color = C.muted, font_size = 13 }),
		creation or false,
		#cards == 0 and ui.col({
			grow = true, center = true,
			ui.text({ "no workspaces yet", color = C.muted, font_size = 13 }),
		}) or ui.col({
			id = "launcher:spaces", grow = true, scroll_y = true,
			ui.row({ wrap = true, gap = 16, cards }),
		}),
	})
end
