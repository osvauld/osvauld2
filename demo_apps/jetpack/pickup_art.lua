-- Guns on the floor (view only; the rules are spawns.lua): each loose gun drawn with gun_art, bobbing over a
-- soft glow on the surface; a weapon spawn with no gun shows a ring that fills as its respawn nears. The gun
-- the pickup button would take is lit brighter. Also the gun icon on that button.
local C = require("theme")
local G = require("gun_art")
local Spawns = require("spawns")
local P = {}

P.glow, P.glow_near, P.ring, P.ring_fill = "#fbd48433", "#fbd48488", "#9baead88", "#fbd484"

local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local function ellipse(cx, cy, rx, ry)
	local commands = {}
	for i = 0, 23 do
		local a = i * math.pi / 12
		commands[#commands + 1] = {i == 0 and "move" or "line", cx + math.cos(a) * rx, cy + math.sin(a) * ry}
	end
	commands[#commands + 1] = {"close"}
	return gfx.path(commands)
end
-- The first `frac` of a circle, clockwise from the top.
local function arc(cx, cy, r, frac)
	local commands, n = {}, math.max(2, math.floor(frac * 28))
	for i = 0, n do
		local a = -math.pi / 2 + frac * 2 * math.pi * i / n
		commands[#commands + 1] = {i == 0 and "move" or "line", cx + math.cos(a) * r, cy + math.sin(a) * r}
	end
	return gfx.path(commands)
end

-- Offset from the grip to the centre of a gun's drawing, in game units.
local function centre(gun)
	local g = gun.points.grip
	return (gun.size[1] / 2 - g[1]) * gun.scale, (gun.size[2] / 2 - g[2]) * gun.scale
end

-- The gfx items for the loose guns and empty pads in the view `geo` (camera geometry), at match time `time`.
-- `near` is the loose gun in the player's reach. `guns` is the validated gun list.
function P.items(m, guns, geo, time, near)
	local items = {}
	local by_id = {}
	for _, g in ipairs(guns) do by_id[g.id] = g end
	local x0, x1, y0, y1 = geo.x - 60, geo.x + geo.vw + 60, geo.y - 60, geo.y + geo.vh + 60
	for _, pad in ipairs(m.pads) do
		local s = pad.spawn
		if not pad.loose and s.x > x0 and s.x < x1 and s.y > y0 and s.y < y1 then
			local ground = s.y + Spawns.lie
			table.insert(items, gfx.fill({path = ellipse(s.x, ground, 16, 4), brush = brush(P.glow)}))
			table.insert(items, gfx.stroke({path = ellipse(s.x, s.y, 9, 9), brush = brush(P.ring), width = 1.5}))
			local frac = 1 - math.max(0, math.min(1, pad.timer / s.respawn))
			if frac > 0.02 then
				table.insert(items, gfx.stroke({path = arc(s.x, s.y, 9, frac), brush = brush(P.ring_fill), width = 2.5}))
			end
		end
	end
	for _, l in ipairs(m.loose) do
		local gun = by_id[l.gun]
		if gun and l.x > x0 and l.x < x1 and l.y > y0 and l.y < y1 then
			local lit = l == near
			local bob = l.rest and math.sin(time * 2.6 + l.x * 0.017) * 2 or 0
			local ox, oy = centre(gun)
			table.insert(items, gfx.fill({path = ellipse(l.x, l.y + Spawns.lie, lit and 26 or 20, lit and 6 or 4.5),
				brush = brush(lit and P.glow_near or P.glow)}))
			table.insert(items, G.item(gun, {l.x - ox, l.y - oy + bob - 2}, 0, 1))
		end
	end
	return items
end

-- A gfx.frame over the whole map holding those items (placed like the effects frame).
function P.layer(g, geo)
	local m = g.m
	local items = P.items(m, g.guns, geo, m.time, m:nearby())
	items.width, items.height = m.map.w, m.map.h
	return gfx.frame(items)
end

-- The gun drawn as a button icon: a `box` x `box` visual with the gun fitted inside (with `pad` to spare).
-- Cached per gun table and box.
local icons = {}
function P.icon(gun, box, pad)
	local key = gun.id .. box
	local c = icons[key]
	if not c or c.gun ~= gun then
		local w, h = gun.size[1] * gun.scale, gun.size[2] * gun.scale
		local ox, oy = centre(gun)
		local k = math.min(1.6, (box - 2 * (pad or 6)) / w, (box - 2 * (pad or 6)) / h)
		local inner = gfx.frame({width = w, height = h, G.item(gun, {w / 2 - ox, h / 2 - oy}, 0, 1)})
		c = {gun = gun, visual = gfx.frame({width = box, height = box,
			gfx.instance({visual = inner, transform = {k, 0, 0, k, (box - w * k) / 2, (box - h * k) / 2}})})}
		icons[key] = c
	end
	return c.visual
end

return P
