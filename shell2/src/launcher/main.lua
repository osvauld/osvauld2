local C = require("theme")
local cursor = require("cursor")
local editor = require("editor")

local function card(item)
	local accent = C.kinds[item.kind] or C.accent
	return ui.col({
		id = "launcher:item:" .. item.id,
		w = 200, min_w = 180, h = 144, pad = 18, gap = 12,
		radius = 14, fill = C.panel, stroke = { 1, C.line },
		hover_fill = C.hover, hover_stroke = { 1, accent },
		tint = 140, press_scale = 0.97, cursor = "arrow",
		on_click = function() shell.open_item(item.id) end,
		ui.row({
			gap = 8, align_center = true,
			ui.col({ w = 8, h = 8, radius = 4, fill = accent }),
			ui.text({ string.upper(item.kind), color = accent, font_size = 11, no_wrap = true }),
		}),
		ui.text({ item.name, color = C.text, font_size = 19 }),
		ui.col({ grow = true }),
		ui.text({ "Open app →", color = C.muted, font_size = 12, no_wrap = true }),
	})
end

return function()
	local snapshot = shell.snapshot()
	local cards = {}
	for _, item in ipairs(snapshot.items) do
		cards[#cards + 1] = card(item)
	end
	return ui.col({
		id = "launcher:root", grow = true, stretch = true,
		fill = C.bg, radius = 18, pad = 20, gap = 16,
		on_hover = cursor.track, system_cursor = false,
		ui.row({
			id = "launcher:workspace-header", wrap = true, gap = 12, align_center = true,
			ui.button({
				id = "launcher:back", h = 36, px = 10, center = true, no_shrink = true,
				radius = 6, hover_fill = C.hover, tint = 120,
				on_click = function() shell.back() end,
				ui.text({ "← Back", color = C.muted, font_size = 13, no_wrap = true }),
			}),
			ui.text({ snapshot.workspace.name, grow = true, min_w = 60, color = C.text, font_size = 15 }),
			ui.button({
				id = "launcher:add-item", h = 36, px = 14, center = true, no_shrink = true,
				radius = 6, fill = C.accent, press_scale = 0.97,
				on_click = function() shell.import_item() end,
				ui.text({ "+ Add item", color = C.bg, font_size = 13, no_wrap = true }),
			}),
		}),
		ui.row({
			wrap = true, gap = 12, align_center = true,
			ui.text({ "Your apps", color = C.text, font_size = 24, no_wrap = true }),
			ui.col({ grow = true }), editor.button(),
		}),
		ui.text({ "A workspace made yours", color = C.muted, font_size = 13 }),
		#cards == 0 and ui.col({
			grow = true, center = true,
			ui.text({ "no apps in this workspace yet", color = C.muted, font_size = 13 }),
		}) or ui.col({
			id = "launcher:items", grow = true, scroll_y = true,
			ui.row({ wrap = true, gap = 16, cards }),
		}),
		editor.view(),
		cursor.view(),
	})
end
