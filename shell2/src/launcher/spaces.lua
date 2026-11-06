local C = require("theme")

-- A compact row: a lettered badge, the name, an arrow. Rows fill a phone's width and wrap
-- into columns on a desktop.
local function card(workspace, select_workspace)
	local initial = (workspace.name:match("[%w]") or "?"):upper()
	return ui.row({
		id = "launcher:space:" .. workspace.id,
		grow = true, min_w = 240, max_w = 420, h = 64, px = 14, gap = 12, align_center = true,
		radius = 12, fill = C.panel, stroke = { 1, C.line },
		hover_fill = C.hover, hover_stroke = { 1, C.accent },
		tint = 140, press_scale = 0.98, cursor = "arrow",
		on_click = function() select_workspace(workspace.id) end,
		ui.col({
			w = 36, h = 36, radius = 9, center = true, no_shrink = true, fill = C.hover,
			ui.text({ initial, color = C.accent, font_size = 16, no_wrap = true }),
		}),
		ui.text({ workspace.name, grow = true, min_w = 0, color = C.text, font_size = 16, no_wrap = true }),
		ui.text({ "›", color = C.muted, font_size = 20, no_wrap = true, no_shrink = true }),
	})
end

return function(workspaces, select_workspace, creation)
	local cards = {}
	for _, workspace in ipairs(workspaces) do
		cards[#cards + 1] = card(workspace, select_workspace)
	end
	return ui.col({
		id = "launcher:workspace-list", grow = true, stretch = true,
		fill = C.bg, radius = 18, pad = 16, gap = 12,
		ui.text({ "Workspaces", color = C.text, font_size = 22 }),
		creation or false,
		#cards == 0 and ui.col({
			grow = true, center = true,
			ui.text({ "no workspaces yet", color = C.muted, font_size = 13 }),
		}) or ui.col({
			id = "launcher:spaces", grow = true, scroll_y = true,
			ui.row({ wrap = true, gap = 10, cards }),
		}),
	})
end
