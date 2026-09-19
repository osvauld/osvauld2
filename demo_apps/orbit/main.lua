local C = require("theme")
local F = require("field")
local S = require("shapes")

local R = C.sprite_r

-- One resource, placed 200 times. Names inside it are unreachable, so the instance carries the id.
local sprite_v = gfx.frame({
	width = R * 2,
	height = R * 2,
	gfx.fill({ path = S.circle(R, R, R), brush = gfx.solid(C.sprite) }),
	gfx.fill({ path = S.circle(R * 0.78, R * 0.74, R * 0.32), brush = gfx.solid(C.sprite_core) }),
})

local ring_p = S.circle(R, R, R + C.ring_gap)

local function place(s)
	local k = s.k
	return { k, 0, 0, k, s.x - k * R, s.y - k * R }
end

local function ring(s, color)
	return gfx.group({
		transform = place(s),
		gfx.stroke({ path = ring_p, brush = gfx.solid(color), width = C.ring_w / s.k }),
	})
end

local function trail(s, color)
	return gfx.stroke({
		path = S.ellipse(s.cx, s.cy, s.a, s.b, s.ct, s.st),
		brush = gfx.solid(color),
		width = 1,
	})
end

local function readout()
	local g = F.grab
	if g then
		return string.format(
			"holding %s  ·  grabbed at %.1f, %.1f  ·  that shape still reads %.1f, %.1f  ·  release to fling",
			g.s.id, g.sx, g.sy, F.gx, F.gy
		)
	end
	local s = F.hot
	if not s then
		return "pointer: nothing — hover a sprite, click to pin it, drag it to fling it"
	end
	return string.format(
		"pointer: %s  ·  shape-local %.1f, %.1f  ·  field %.0f, %.0f%s",
		s.id, F.sx, F.sy, s.x, s.y, F.pin == s and "  ·  pinned" or ""
	)
end

return function()
	local sprites = F.sprites
	local held = F.grab and F.grab.s or nil
	local hot, pin = F.hot, F.pin

	-- Highlights, strongest claim first: a sprite in hand outranks a pinned one outranks a
	-- hovered one, and no sprite is marked twice.
	local marks, seen = {}, {}
	local function mark(s, trail_color, ring_color)
		if s and not seen[s] then
			seen[s] = true
			marks[#marks + 1] = { s = s, trail = trail_color, ring = ring_color }
		end
	end
	mark(held, C.grab, C.grab)
	mark(pin, C.pin, C.pin)
	mark(hot, C.guide, C.hot)

	local items = {}
	for i = 1, #marks do
		items[#items + 1] = trail(marks[i].s, marks[i].trail)
	end

	for i = 1, #sprites do
		local s = sprites[i]
		items[#items + 1] = gfx.instance({ id = s.id, visual = sprite_v, transform = place(s) })
	end

	-- Rings go last and stay unnamed: they must not take the hit from the sprite they mark.
	for i = 1, #marks do
		items[#items + 1] = ring(marks[i].s, marks[i].ring)
	end

	items.width = C.field_w
	items.height = C.field_h

	return ui.col({
		id = "orbit",
		full = true,
		pad = C.pad,
		gap = C.gap,
		fill = C.bg,
		on_frame = function(e)
			local dt = e.dt
			F.step(dt)
		end,
		ui.text({
			"Orbit field — every sprite is one placement of a single visual, and the runtime "
				.. "hit-tests it where it actually is this frame. Hover one, click to pin it, "
				.. "or grab it and throw it.",
			color = C.text,
			font_size = C.head_size,
		}),
		ui.text({
			string.format(
				"%d sprites  ·  %d drifting  ·  t %.1fs  ·  the description is rebuilt from math, nothing is retained",
				#sprites, F.drifting, F.t
			),
			color = C.dim,
			font_size = C.line_size,
			no_wrap = true,
		}),
		ui.text({
			readout(),
			color = held and C.grab or (hot and C.hot or C.dim),
			font_size = C.line_size,
			no_wrap = true,
		}),
		ui.frame({
			id = "field",
			visual = gfx.frame(items),
			fill = C.panel,
			radius = 10,
			mt = C.gap,
			on_hover = function(e)
				local phase, x, y, shape, sx, sy = e.phase, e.x, e.y, e.shape, e.sx, e.sy
				if phase == "leave" or not shape then
					F.hot = nil
					return
				end
				F.hot = F.by_id[shape]
				F.sx, F.sy = sx, sy
			end,
			on_click = function(e)
				local x, y, shape = e.x, e.y, e.shape
				local s = shape and F.by_id[shape]
				F.pin = (s and F.pin ~= s) and s or nil
			end,
			on_drag = function(e)
				local phase, x, y, dx, dy, scale, origin_x, origin_y, shape, sx, sy = e.phase, e.x, e.y, e.dx, e.dy, e.scale, e.origin_x, e.origin_y, e.shape, e.sx, e.sy
				if phase == "start" then
					F.grab_at(shape and F.by_id[shape], sx, sy)
				elseif phase == "move" then
					F.hold_to(dx, dy)
					-- The grabbed shape keeps answering in its own coordinates, pointer still on
					-- it or not; that is the whole claim this demo is here to show.
					F.gx, F.gy = sx, sy
				else
					F.release()
				end
			end,
		}),
	})
end
