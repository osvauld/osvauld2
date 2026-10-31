local C = require("theme")
local Camera = require("camera")
local Sticks = require("sticks")
local F = require("format")
local Pickups = require("pickup_art")
local K = {}
K.__index = K

-- Dual-stick touch controls and the keyboard. Left stick: walk, and push up to fly (the throttle
-- follows the stick's height). Right stick: aim, and fire once pushed past the ring. There is no JET
-- button: both thumbs are on the sticks, and a boolean third-finger button was the "hard to control"
-- part. The buttons sit in the reserved slots in sticks.lua: reload (R), swap (Q; shows the gun on the
-- back) and pick up (E; beside swap, only over a gun on the floor), zoom (Z; steps the gun's scope levels and wraps to 1;
-- the wheel steps without wrapping); grenade and melee stay unwired.
-- Mouse (desktop): the left button held on the playfield fires (auto guns keep going, semi guns fire per click, the rules
-- read the edges); the cursor aims from the soldier's screen position (sticks.lua `mouse_step`: dead radius, light smoothing)
-- and the camera leads toward the cursor's screen position (camera.lua `cursor_lead`), so aim and view cannot feed each
-- other. A press on a button or stick box fires nothing, and the playfield swallows touches (a no-op `on_touch` in
-- main.lua), so a finger off the sticks never becomes a mouse press. Right button: nothing (hover events carry only the primary). The zoom level is view state: it lives here, the camera
-- widens by it, and any gun change, equip or death puts it back to 1 (Sticks.zoom_track).
-- `g` is the game table main.lua owns: g.m (the world), g.map, g.size, g.camera.
function K.new(g)
	return setmetatable({g = g, sticks = {}, held = {}, zoom = Sticks.zoom_new(), mouse = Sticks.mouse_new(),
		cursor = nil, aim_at = nil, was_down = false, armed = false, latch = false}, K)
end

-- Keys, fingers and a held mouse button let go; the cursor's place is kept (it is still where it was).
function K:reset()
	self.sticks, self.held, self.armed, self.latch = {}, {}, false, false
	self.g.m:clear_input()
end

-- A new match starts at 1x.
function K:new_match() self.zoom = Sticks.zoom_new() end

-- The player's zoom levels and the current index, after following the equipment.
function K:zoom_state()
	local p, z = self.g.m.player, self.zoom
	Sticks.zoom_track(z, p.gun, p.swap_left, p.hp > 0)
	local levels = Sticks.zoom_levels(F.find(self.g.guns, p.gun))
	if z.i > #levels then z.i = 1 end
	return levels, z.i
end

-- The camera factor now asked for (1 = normal), the gun's widest level, and how many levels it has.
function K:zoom_info()
	local levels, i = self:zoom_state()
	return levels[i], levels[#levels], #levels
end

-- One step: a tap (dir nil) wraps to 1, the wheel (dir +1 wider, -1 closer) stops at the ends.
function K:zoom_step(dir)
	if self.g.m.state ~= "Playing" then return end
	local levels, i = self:zoom_state()
	self.zoom.i = Sticks.zoom_step(levels, i, dir)
end

function K:wheel(e)
	if e.dy and e.dy ~= 0 then self:zoom_step(e.dy > 0 and 1 or -1) end
end

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

function K:size() return Sticks.size(math.min(self.g.size.w, self.g.size.h)) end

-- Is the playfield point (x, y) on a stick box or a button slot?
function K:in_zone(x, y)
	for _, z in ipairs(Camera.zones(self.g.size.w, self.g.size.h)) do
		if x >= z.x0 and x <= z.x1 and y >= z.y0 and y <= z.y1 then return true end
	end
	return false
end

-- The mouse is steering: a cursor was seen on the playfield and no thumb holds the aim stick.
function K:mouse_aiming() return self.aim_at ~= nil and not self.sticks.aim and self.g.m.state == "Playing" end

-- The camera's look-ahead vector while the mouse steers (nil otherwise: the camera leads by the aim).
function K:lead()
	if not self:mouse_aiming() then return nil end
	return Camera.cursor_lead(self.g.size.w, self.g.size.h, self.aim_at[1], self.aim_at[2])
end

-- Where to draw the crosshair (playfield px), or nil: only over the playfield proper, never over a control.
function K:crosshair()
	if self.cursor and self:mouse_aiming() then return self.cursor[1], self.cursor[2] end
end

-- Fold sticks, keys and the pointer into the world's input.
function K:apply()
	local g, sticks, held = self.g, self.sticks, self.held
	local input = g.m.input
	local ring = Sticks.ring(self:size())
	local move = sticks.move
	input.move = clamp((move and Sticks.walk(move.dx, ring) or 0)
		+ ((held.KeyD or held.ArrowRight) and 1 or 0) - ((held.KeyA or held.ArrowLeft) and 1 or 0), -1, 1)
	-- A number 0..1 (the world reads it as the throttle); false when idle.
	input.thrust = (held.Space or held.KeyW or held.ArrowUp) and 1 or (move and Sticks.thrust(move.dy, ring)) or false
	local aim = sticks.aim
	local angle, pushed
	if aim then angle, pushed = Sticks.aim(aim.dx, aim.dy, ring) end
	input.fire = held.KeyF or pushed or self.armed or self.latch or false
	input.pointer = nil
	if angle then
		input.aim = angle
		input.pointer = nil
	end
end

function K:touch(which, e)
	if self.g.m.state ~= "Playing" then return end
	local stick = self.sticks[which]
	if e.phase == "start" and not stick then
		self.sticks[which] = {finger=e.finger,x=e.x,y=e.y,dx=0,dy=0}
		if which == "aim" then self.aim_at, self.armed, self.latch = nil, false, false end
	elseif stick and stick.finger == e.finger then
		if e.phase == "end" or e.phase == "cancel" then self.sticks[which] = nil
		else stick.dx,stick.dy=e.x-stick.x,e.y-stick.y end
	end
	self:apply()
end

-- Escape pauses/resumes, Enter deploys; `on` supplies start/pause/resume.
function K:key(e, on)
	local m = self.g.m
	if e.cancelled then self:reset(); return end
	if e.code == "Escape" and e.down and not e.repeated then
		if m.state == "Playing" then on.pause() elseif m.state == "Paused" then on.resume() end
	elseif e.code == "Enter" and e.down and m.state == "Ready" then on.start()
	else
		if e.down and not e.repeated and m.state == "Playing" then
			if e.code == "KeyR" then m.input.reload = true
			elseif e.code == "KeyQ" then m.input.swap = true
			elseif e.code == "KeyE" then m.input.pickup = true
			elseif e.code == "KeyZ" then self:zoom_step() end
		end
		self.held[e.code or ""] = e.down or nil; self:apply()
	end
end

-- The pointer over the playfield. `e.down` is the primary button: a press (down after not-down, at a point off the
-- controls, while playing) starts firing; release, or leaving, stops it. A click too quick for a world step is kept
-- (`latch`) until a step has run.
function K:hover(e)
	local down = e.down and true or false
	if e.phase == "leave" then
		self.cursor, self.armed, down = nil, false, false
	else
		local zone = self:in_zone(e.x, e.y)
		self.cursor = {e.x, e.y}
		if not zone then self.aim_at = {e.x, e.y} end
		if e.phase ~= "enter" and down and not self.was_down and not zone and self.g.m.state == "Playing" and not self.sticks.aim then
			self.armed, self.latch = true, true
		end
		if not down then self.armed = false end
	end
	self.was_down = down
	self:apply()
end

-- Per frame (`dt` seconds, before the world advances): the mouse's smoothed aim, from the soldier's screen position.
function K:frame(dt)
	self:zoom_state()
	local g = self.g
	if self:mouse_aiming() then
		local p = g.m.player
		local geo = g.camera:geometry(g.size.w, g.size.h, p.x, p.y)
		local sx, sy = Camera.to_screen(geo, p.x, p.y - 4)
		g.m.input.aim = Sticks.mouse_step(self.mouse, dt or 0, self.aim_at[1], self.aim_at[2], sx, sy, Sticks.mouse_dead(geo.scale), p.aim)
	else
		self.mouse.angle = nil
	end
	self:apply()
end

-- After the world advanced: a quick click has had its step once one ran.
function K:after_advance(stepped)
	if stepped and self.latch and not self.armed then self.latch = false end
end

-- Stick art, built once per size. The move stick marks the hover height; the aim stick draws the fire ring.
local function circle_path(cx, cy, r)
	local commands = {}
	for i = 0, 35 do
		local a = i * math.pi / 18
		commands[#commands + 1] = {i == 0 and "move" or "line", cx + math.cos(a) * r, cy + math.sin(a) * r}
	end
	commands[#commands + 1] = {"close"}
	return gfx.path(commands)
end
local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local arts = {}
local function art(kind, size)
	local key = kind .. size
	if arts[key] then return arts[key] end
	local c, ring = size / 2, Sticks.ring(size)
	local items = {width = size, height = size,
		gfx.fill({path = circle_path(c, c, c - 2), brush = brush(C.stick_outer)}),
		gfx.fill({path = circle_path(c, c, ring * Sticks.deadzone * 2.2), brush = brush(C.stick_inner)})}
	if kind == "aim" then
		items[#items + 1] = gfx.stroke({path = circle_path(c, c, ring * Sticks.fire_at), brush = brush(C.cross), width = 2})
	else
		local y = c - ring * Sticks.hover
		items[#items + 1] = gfx.stroke({path = gfx.path({{"move", c - 14, y}, {"line", c + 14, y}}),
			brush = brush(C.cross), width = 2, cap = "round"})
		items[#items + 1] = gfx.stroke({path = gfx.path({{"move", c, c - ring}, {"line", c, c + ring * 0.35}}),
			brush = brush(C.cross), width = 1})
	end
	arts[key] = gfx.frame(items)
	return arts[key]
end
local knob = gfx.frame({width=30,height=30,
	gfx.fill({path = circle_path(15, 15, 14), brush = brush(C.knob)}),
	gfx.fill({path = circle_path(12, 11, 5), brush = brush(C.knob_light)})})

-- The mouse crosshair: a ring with four ticks, drawn dark then light so it reads on sky and rock alike.
local cross_art
local function crosshair_art()
	if cross_art then return cross_art end
	local ticks = {}
	for _, d in ipairs({{1,0},{-1,0},{0,1},{0,-1}}) do
		ticks[#ticks + 1] = {"move", 15 + d[1] * 5, 15 + d[2] * 5}
		ticks[#ticks + 1] = {"line", 15 + d[1] * 13, 15 + d[2] * 13}
	end
	local function layer(color, width)
		return {gfx.stroke({path = circle_path(15, 15, 8), brush = brush(color), width = width}),
			gfx.stroke({path = gfx.path(ticks), brush = brush(color), width = width, cap = "round"})}
	end
	local items = {width = 30, height = 30}
	for _, item in ipairs(layer(C.ink, 4)) do items[#items + 1] = item end
	for _, item in ipairs(layer(C.flame, 2)) do items[#items + 1] = item end
	cross_art = gfx.frame(items)
	return cross_art
end

-- The stick floats: its drawn base sits where the thumb landed (kept inside the control's box).
function K:stick(which,kind,label,left,right)
	local size=self:size()
	local ring=Sticks.ring(size)
	local state=self.sticks[which]
	local dx,dy=state and state.dx or 0,state and state.dy or 0
	local length=math.sqrt(dx*dx+dy*dy)
	if length>ring then dx,dy=dx*ring/length,dy*ring/length end
	local bx,by=0,0
	if state then bx,by=clamp(state.x-size/2,-size/2,size/2),clamp(state.y-size/2,-size/2,size/2) end
	return ui.col({id="control:"..which,absolute=true,bottom=10,left=left,right=right,w=size,h=size,
		on_touch=function(e) self:touch(which,e) end,
		ui.frame({visual=art(kind,size),absolute=true,top=by,left=bx}),
		ui.frame({visual=knob,absolute=true,top=size/2-15+by+dy,left=size/2-15+bx+dx}),
		ui.col({absolute=true,bottom=2,w_full=true,center=true,
			ui.text({label,color=C.text,font_size=9,no_wrap=true})}),
	})
end

-- A round slot button; `art` (a gfx visual) or `label` fills it.
local function slot_button(id, slot, art, label, on_click, lit, dim)
	return ui.button({id=id,absolute=true,left=slot.x,top=slot.y,w=slot.w,h=slot.h,radius=slot.w/2,center=true,
		fill=C.stick_outer,hover_fill=C.stick_inner,stroke={lit and 2 or 1,lit and C.flame or C.cross},opacity=dim and 0.45 or 1,
		on_click=on_click,
		art and ui.frame({visual=art}) or ui.text({label,color=C.text,font_size=14,no_wrap=true})})
end

-- The on-screen controls drawn over the playfield.
function K:view()
	local g = self.g
	local slots = Sticks.slots(g.size.w, g.size.h)
	local m = g.m
	local playing = m.state == "Playing"
	local out = {self:stick("move","move","MOVE / FLY",10,nil),self:stick("aim","aim","AIM / FIRE",nil,10),
		slot_button("button:reload",slots.reload,nil,"R",function() if m.state == "Playing" then m.input.reload = true end end)}
	local back = m:other()
	table.insert(out, slot_button("button:swap",slots.swap,back and Pickups.icon(back, slots.swap.w) or nil,"⇄",
		function() if m.state == "Playing" then m.input.swap = true end end, false, not back))
	local near = playing and m:nearby() or nil
	local gun = near and F.find(g.guns, near.gun)
	if gun then
		table.insert(out, slot_button("button:pickup",slots.pickup,Pickups.icon(gun, slots.pickup.w),"E",
			function() if m.state == "Playing" then m.input.pickup = true end end, true))
	end
	local level, top, count = self:zoom_info()
	table.insert(out, ui.button({id="button:zoom",absolute=true,left=slots.zoom.x,top=slots.zoom.y,w=slots.zoom.w,h=slots.zoom.h,
		radius=slots.zoom.w/2,center=true,fill=C.stick_outer,hover_fill=C.stick_inner,
		stroke={level>1 and 2 or 1,level>1 and C.flame or C.cross},opacity=count>1 and 1 or 0.45,
		on_click=function() self:zoom_step() end,
		ui.col({center=true,gap=0,
			ui.text({Sticks.zoom_label(level),id="zoom:level",color=C.text,font_size=13,no_wrap=true}),
			ui.text({count>1 and ("/"..Sticks.zoom_label(top)) or "scope",id="zoom:max",color=C.muted,font_size=8,no_wrap=true})})}))
	local cx, cy = self:crosshair()
	if cx and not self:in_zone(cx, cy) then
		table.insert(out, ui.frame({id="crosshair",visual=crosshair_art(),absolute=true,left=cx-15,top=cy-15}))
	end
	return out
end

return K
