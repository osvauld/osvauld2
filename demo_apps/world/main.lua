local C = require("theme")
local hero = gfx.drawing(require("hero"))
local hero_back, hero_side = gfx.drawing(require("hero_back")), gfx.drawing(require("hero_side"))
local chest = gfx.drawing(require("chest"))
local clips = require("clips")
local idle, walk = gfx.clip(clips.idle), gfx.clip(clips.walk)
local walk_side = gfx.clip(clips.walk_side)

-- Front is the entity's own drawing (facing down); the world picks a view by the way it moves,
-- and mirrors `side` for left.
local views = {
	up = { drawing = hero_back },
	side = { drawing = hero_side, moving = walk_side },
}
local open, close = gfx.clip(clips.open), gfx.clip(clips.close)

-- The keys are content, so they live here: Rust only knows "an axis driven by two key codes".
local wasd = {
	speed = 160,
	axis_x = { neg = "KeyA", pos = "KeyD" },
	axis_y = { neg = "KeyW", pos = "KeyS" },
	moving = walk,
}

local friend = false -- whether the second hero is described; the world spawns/despawns to match
local hot = nil -- the entity under the pointer
local lid = nil -- the chest's clip: nil at rest, then open/close; a new handle restarts it

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
		ui.text({ "WASD walks the hero · click the chest", color = C.muted, font_size = 13, no_wrap = true }),
		-- `pos` is where an entity spawns; after that the world owns where it is. `order = "y"`
		-- stacks by feet: whoever stands lower draws in front.
		ui.world({
			id = "room", width = C.width, height = C.height, fill = C.floor, radius = 10, order = "y",
			stroke = { 3, C.border },
			on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
			on_click = function(e)
				if e.shape == "chest" then lid = lid == open and close or open end
			end,
			{ id = "hero", pos = { 120, 80 }, drawing = hero, clip = idle, controller = wasd,
				facing = views },
			{ id = "chest", pos = { 520, 240 }, drawing = chest, clip = lid },
			friend and { id = "friend", pos = { 320, 100 }, drawing = hero, clip = idle } or false,
		}),
		ui.row({ gap = 10,
			button("add", "add friend", function() friend = true end),
			button("remove", "remove friend", function() friend = false end),
		}),
		ui.text({ "entity: " .. (hot or "—"), color = C.muted, font_size = 13, no_wrap = true }),
	})
end
