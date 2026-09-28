local C = require("theme")
local hero = gfx.drawing(require("hero"))
local chest = gfx.drawing(require("chest"))

local friend = false -- whether the second hero is described; the world spawns/despawns to match
local hot = nil -- the entity under the pointer

local function button(id, label, on_click)
	return ui.button({
		id = id, px = 14, py = 8, radius = 8, fill = C.button, hover_fill = C.button_hover,
		ui.text({ label, color = C.text, font_size = 13, no_wrap = true }),
		on_click = on_click,
	})
end

return function()
	return ui.col({
		full = true, center = true, gap = 14, fill = C.bg,
		ui.text({ "A retained world", color = C.text, font_size = 22, no_wrap = true }),
		-- `pos` is where an entity spawns; after that the world owns where it is.
		ui.world({
			id = "room", width = C.width, height = C.height, fill = C.floor, radius = 10,
			stroke = { 3, C.border },
			on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
			{ id = "hero", pos = { 120, 80 }, drawing = hero },
			{ id = "chest", pos = { 520, 240 }, drawing = chest },
			friend and { id = "friend", pos = { 320, 100 }, drawing = hero } or false,
		}),
		ui.row({ gap = 10,
			button("add", "add friend", function() friend = true end),
			button("remove", "remove friend", function() friend = false end),
		}),
		ui.text({ "entity: " .. (hot or "—"), color = C.muted, font_size = 13, no_wrap = true }),
	})
end
