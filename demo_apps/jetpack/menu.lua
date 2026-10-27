local C = require("theme")
local M = {}

-- The arena picker on the start overlay: one button per map, the chosen one lit. `g` is main.lua's game
-- table (g.maps, g.map); `pick(id)` selects and returns to the start screen. Shown on the start and pause
-- overlays, hidden while a match runs.
function M.maps(g, pick)
	if g.m.state ~= "Ready" and g.m.state ~= "Paused" then return false end
	local buttons = {}
	for _, map in ipairs(g.maps) do
		local chosen = map.id == g.map.id
		table.insert(buttons, ui.button({id="map:pick:"..map.id,h=34,px=12,radius=8,center=true,
			fill=chosen and C.edge or C.panel,hover_fill=C.steel,stroke={chosen and 2 or 1,chosen and C.player or C.edge},
			on_click=function() pick(map.id) end,
			ui.text({map.name,color=chosen and C.bg or C.text,font_size=12,no_wrap=true})}))
	end
	return ui.col({id="map:picker",absolute=true,bottom=8,w_full=true,align_center=true,gap=6,
		ui.text({"ARENA",color=C.muted,font_size=10,no_wrap=true}),
		ui.row({w_full=true,wrap=true,center=true,gap=6,px=8,buttons}),
	})
end

return M
