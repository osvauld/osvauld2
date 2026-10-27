local C = require("theme")
local World = require("world")
local View = require("view")
local Hud = require("hud")
local Controls = require("controls")
local Camera = require("camera")
local Menu = require("menu")
local Rotate = require("rotate")
local F = require("format")
local Spawns = require("spawns")
-- The workspace's maps if the map editor made any valid one, else the baked ones.
local maps, problem = F.load_maps(asset:open("jetpack-maps"), require("baked/maps"))
if problem then print("jetpack-maps: " .. problem .. " (playing the baked maps)") end
-- Likewise the pilots: the player flies the first, bots cycle the list.
local pilots, pilot_problem = F.load_pilots(asset:open("jetpack-pilots"), require("baked/pilots"))
if pilot_problem then print("jetpack-pilots: " .. pilot_problem .. " (flying the baked pilots)") end
-- And the guns: every soldier holds one (rifle unless the loadout says otherwise).
local guns, gun_problem = F.load_guns(asset:open("jetpack-guns"), require("baked/guns"))
if gun_problem then print("jetpack-guns: " .. gun_problem .. " (using the baked guns)") end
-- Test-only until step 5's loadouts: the asset `jetpack-loadout` holds `loadout` = { player = gun id, bot = gun id }.
local loadout_asset = asset:open("jetpack-loadout")

local g = {maps = maps, map = maps[1], pilots = pilots, guns = guns, size = {w = 800, h = 430}}
local controls

-- A fresh match on the chosen map, the camera on the player.
local function new_match()
	g.m = World.new(g.map, g.guns)
	Spawns.arm(g.m, loadout_asset.loadout)
	g.camera = Camera.new(g.map)
	g.camera:snap(g.size.w, g.size.h, g.m.player.x, g.m.player.y, g.m.player.aim)
	g.last, g.cx, g.cy = nil, nil, nil
	if controls then controls:new_match() end
end
new_match()
controls = Controls.new(g)
g.controls = controls

local function pick_map(id)
	for _, map in ipairs(g.maps) do if map.id == id then g.map = map end end
	controls:reset()
	new_match()
end

local on = {}
function on.start() controls:reset(); g.m:start() end
function on.pause() controls:reset(); g.m:pause() end
function on.resume() controls:reset(); g.m:resume() end
function on.restart() controls:reset(); new_match(); g.m:start() end

-- The view widens by the zoom level the player chose on the gun in hand (controls.lua; 1 = normal).
local function gun_zoom() return (controls:zoom_info()) end

local function frame(e)
	local p = g.m.player
	local resized = g.size.w ~= e.width or g.size.h ~= e.height
	if resized then g.size = {w=e.width,h=e.height} end
	-- Landscape only: a playfield taller than wide pauses the match (resuming is the player's tap, in landscape).
	if Rotate.portrait(g.size) and g.m.state == "Playing" then on.pause() end
	-- The camera eases only on new frame time (captures repeat the same elapsed), and jumps with a respawn.
	local dt = g.last and math.max(0, math.min(0.1, e.elapsed - g.last)) or 0
	g.last = e.elapsed
	controls:frame(dt)
	local time = g.m.time
	g.m:advance(e.elapsed)
	controls:after_advance(g.m.time > time)
	local px, py = p.x, p.y
	if g.m.state ~= "Playing" then
		if resized then g.camera:snap(g.size.w, g.size.h, px, py, p.aim, gun_zoom(), controls:lead()) end
		return
	end
	if g.cx and math.abs(px - g.cx) + math.abs(py - g.cy) > 150 then
		g.camera:snap(g.size.w, g.size.h, px, py, p.aim, gun_zoom(), controls:lead())
	end
	g.cx, g.cy = px, py
	g.camera:update(dt, g.size.w, g.size.h, px, py, p.aim, gun_zoom(), controls:lead())
end

return function()
	local portrait = Rotate.portrait(g.size)
	return ui.col({full=true,fill=C.bg,
		Hud.bar(g, on, portrait),
		ui.col({id="playfield",grow=true,w_full=true,min_h=120,
			on_frame=frame,on_wheel=function(e) controls:wheel(e) end,
			on_key=function(e) if not Rotate.portrait(g.size) then controls:key(e, on) end end,on_hover=function(e) controls:hover(e) end,
			-- A finger off the sticks and buttons is swallowed here, not turned into a mouse press (which would fire).
			on_touch=function() end,system_cursor=not controls:crosshair(),
			portrait and Rotate.card(g.m) or {
			View.scene(g),
			Hud.status(g),
			controls:view(),
			Hud.overlay(g, on),
			Menu.maps(g, pick_map),
			},
		}),
	})
end
