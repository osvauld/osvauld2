local C = require("theme")
local hero = gfx.drawing(require("hero"))
local hero_back, hero_side = gfx.drawing(require("hero_back")), gfx.drawing(require("hero_side"))
local chest = gfx.drawing(require("chest"))
local clips = require("clips")
local idle, walk = gfx.clip(clips.idle), gfx.clip(clips.walk)
local walk_side, jump = gfx.clip(clips.walk_side), gfx.clip(clips.jump)
local carry, carry_walk = gfx.clip(clips.carry), gfx.clip(clips.carry_walk)

-- A view per facing: what to draw, what to play while walking, and whether to mirror. The side
-- view is drawn facing right, so left is the same drawing flipped.
local views = {
	down = { drawing = hero, walk = walk },
	up = { drawing = hero_back, walk = walk },
	right = { drawing = hero_side, walk = walk_side },
	left = { drawing = hero_side, walk = walk_side, flip = true },
}
local open, close = gfx.clip(clips.open), gfx.clip(clips.close)

-- The room's walls: a plain bar each, solid all along. Only the collider stops anyone; the
-- drawing is just so you can see it.
local function bar(w, h)
	return gfx.drawing({ size = { w, h }, parts = { { id = "bar", pivot = { 0, 0 }, shapes = {
		{ path = { {"move",0,0}, {"line",w,0}, {"line",w,h}, {"line",0,h}, {"close"} }, fill = C.border },
	} } } })
end
local T = 12 -- wall thickness
local across, down = bar(C.width, T), bar(T, C.height)
-- The back wall stands tall, as in a room seen from above and in front: the hero walks along its
-- foot with the whole body inside the room. Its collider is the whole wall.
local BACK = 190
local back = gfx.drawing({ size = { C.width, BACK }, parts = { { id = "wall", pivot = { 0, 0 }, shapes = {
	{ path = { {"move",0,0}, {"line",C.width,0}, {"line",C.width,BACK}, {"line",0,BACK}, {"close"} },
		fill = C.wall },
	{ path = { {"move",0,BACK-T}, {"line",C.width,BACK-T}, {"line",C.width,BACK}, {"line",0,BACK},
		{"close"} }, fill = C.border },
} } } })
local function wall(id, x, y, drawing, w, h)
	return { id = id, pos = { x, y }, drawing = drawing, collider = { rect = { w, h } } }
end

-- The keys are content, so they live here: Rust only knows "an axis driven by two key codes".
local wasd = {
	speed = 160,
	axis_x = { neg = "KeyA", pos = "KeyD" },
	axis_y = { neg = "KeyW", pos = "KeyS" },
}

-- Which way to face for a held direction. On a diagonal, keep the current facing if it is one of
-- the two; otherwise the sideways one. Stopping keeps the facing.
local function face_for(dx, dy, current)
	local h = dx < 0 and "left" or dx > 0 and "right" or nil
	local v = dy < 0 and "up" or dy > 0 and "down" or nil
	if h and v then
		return (current == h or current == v) and current or h
	end
	return h or v or current
end

local friend = false -- whether the second hero is described; the world spawns/despawns to match
local hot = nil -- the entity under the pointer
local lid = nil -- the chest's clip: nil at rest, then open/close; a new handle restarts it
local heading = "still" -- the hero's held direction, as the world last reported it
local face, walking = "down", false -- the hero's facing and gait, decided on each on_move
local jumping = false -- set by the jump action, cleared when the jump clip ends
local carrying = false -- whether the chest rides on the hero; E picks it up and puts it down
local near = false -- whether the hero stands in the chest's zone, as the world last reported

local function toggle_lid()
	lid = lid == open and close or open
end

-- The hero's clip for the moment: this is the app's state machine, not the world's.
local function hero_clip()
	if jumping then return jump end
	if carrying then return walking and carry_walk or carry end
	return walking and views[face].walk or idle
end

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
		ui.text({ "WASD walks, walls stop you · Space jumps · E near the chest carries it · a click opens it", color = C.muted, font_size = 13,
			no_wrap = true }),
		-- `pos` is where an entity spawns; after that the world owns where it is. `order = "y"`
		-- stacks by feet: whoever stands lower draws in front.
		ui.world({
			id = "room", width = C.width, height = C.height, fill = C.floor, radius = 10, order = "y",
			stroke = { 3, C.border },
			on_hover = function(e) hot = e.phase ~= "leave" and e.shape or nil end,
			on_click = function(e)
				if e.shape == "chest" then toggle_lid() end
			end,
			-- Moments come to Lua; the per-frame work (walking, playing clips) stays in the world.
			actions = { interact = "KeyE", jump = "Space" },
			on_action = function(e)
				-- Carrying, E puts it down anywhere; otherwise only within reach of the chest.
				if e.action == "interact" and (carrying or near) then carrying = not carrying end
				if e.action == "jump" then jumping = true end -- already in the air: no double jump
			end,
			on_clip_end = function(e)
				if e.id == "hero" then jumping = false end
			end,
			-- The chest's zone goes while it is carried and comes back where it is put down, so
			-- `near` follows without Lua keeping it in step.
			on_zone = function(e)
				if e.id == "chest" and e.who == "hero" then near = e.phase == "enter" end
			end,
			on_move = function(e)
				walking = e.dx ~= 0 or e.dy ~= 0
				heading = walking and (e.dx .. "," .. e.dy) or "still"
				face = face_for(e.dx, e.dy, face)
			end,
			-- The hero collides by its feet: the drawing is a body standing up out of the floor, so
			-- the head may pass in front of the top wall.
			{ id = "hero", pos = { 120, 80 }, drawing = views[face].drawing, flip = views[face].flip,
				clip = hero_clip(), controller = wasd, collider = { circle = 16, at = { 80, 222 } } },
			wall("wall:n", 0, 0, back, C.width, BACK),
			wall("wall:s", 0, C.height - T, across, C.width, T),
			wall("wall:w", 0, 0, down, T, C.height),
			wall("wall:e", C.width - T, 0, down, T, C.height),
			-- Carried, the chest rides the hero's body — through walks and jumps — held in front.
			-- On the floor it is solid by its footprint, the bottom of its base; carried, the world
			-- takes it off the floor.
			{ id = "chest", pos = { 520, 240 }, drawing = chest, clip = lid,
				attach = carrying and { to = "hero", part = "body", at = { 32, 120 } } or nil,
				collider = { rect = { 80, 24 }, at = { 8, 56 } }, sensor = { circle = 72, at = { 48, 68 } } },
			friend and { id = "friend", pos = { 320, 100 }, drawing = hero, clip = idle } or false,
		}),
		ui.row({ gap = 10,
			button("add", "add friend", function() friend = true end),
			button("remove", "remove friend", function() friend = false end),
		}),
		ui.text({ "entity: " .. (hot or "—"), color = C.muted, font_size = 13, no_wrap = true }),
		ui.text({ "heading: " .. heading, color = C.muted, font_size = 13, no_wrap = true }),
		ui.text({ "near the chest: " .. (near and "yes" or "no"), color = C.muted, font_size = 13,
			no_wrap = true }),
	})
end
