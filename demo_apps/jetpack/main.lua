local C = require("theme")
local World = require("world")
local View = require("view")
local Hud = require("hud")
local Controls = require("controls")
local Camera = require("camera")
local Menu = require("menu")
local Rotate = require("rotate")
local F = require("format")
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
-- F3 or ` (laptop F-keys are often media keys) shows frames per second and the slowest frame of each half second (a spike is what reads as lag).
g.fps = {show = false, n = 0, t = 0, worst = 0, text = ""}
local controls

-- The match is the platform's (`net.match`, jetpack-mp.md step 7): it steps `World.step` on the
-- players' inputs and owns the state. `g.m` is that state as this device sees it: `player` is this
-- device's soldier, and the view keeps its cosmetics (casings, smoke) on the wrapper, never in the state.
local function wrap()
	return setmetatable({}, {__index = function(_, k)
		local s = g.match.state
		if getmetatable(s) == nil then setmetatable(s, World.meta) end -- a rewound state is a fresh copy
		if k == "player" then return s.humans[g.match.me] end
		return s[k]
	end})
end

-- What the controls hand the match each input frame (controls.lua `take_input`). Presses are counts.
local function blank_input()
	return {move = 0, thrust = false, fire = false, aim = 0, reload = 0, swap = 0, pickup = 0, mode = nil, mode_n = 0}
end

-- The match now shown: the view, map, guns and pilots follow its setup (in a match with others that is
-- the starter's content, not this device's), the camera goes to this device's soldier.
local function show(match)
	g.match, g.next = match, nil
	setmetatable(match.state, World.meta)
	local ctx = World.context()
	g.map, g.guns, g.pilots = ctx.map, ctx.guns, ctx.pilots
	g.m = wrap()
	g.camera = Camera.new(g.map)
	g.camera:snap(g.size.w, g.size.h, g.m.player.x, g.m.player.y, g.m.player.aim)
	g.last, g.cx, g.cy = nil, nil, nil
	if controls then controls:new_match() end
end

-- A fresh match on the chosen map. Alone (`players` nil) it starts at once, Ready for DEPLOY, with the
-- input read every tick. With others it is 30 input frames a second and starts Playing once every player
-- has said hello; until then the last match stays on screen (`g.next` waits).
local function new_match(players)
	local together = players ~= nil and #players > 1
	g.input = blank_input()
	local setup = {map = g.map, guns = g.guns, pilots = g.pilots, loadout = loadout_asset.loadout,
		state = together and "Playing" or nil}
	local match = net.match({
		id = "arena", tick = C.step, frame = together and 4 or 1,
		players = players,
		setup = setup,
		state = function(given, ids) return World.new(given, #ids) end,
		input = function() return controls:take_input() end,
		step = function(s, inputs)
			setmetatable(s, World.meta)
			World.step(s, C.step, inputs)
		end,
	})
	g.together = together
	if match.state then show(match) else g.next = match end
end
new_match()
controls = Controls.new(g)
g.controls = controls

local function pick_map(id)
	for _, map in ipairs(g.maps) do if map.id == id then g.map = map end end
	controls:reset()
	new_match()
end

-- Start, pause and resume go through the input, as the match's rules decide the state.
local function ask(mode)
	g.input.mode, g.input.mode_n = mode, g.input.mode_n + 1
end
local on = {}
function on.start() controls:reset(); ask("Playing") end
-- With others nobody pauses the match for everyone.
function on.pause() if not g.together then controls:reset(); ask("Paused") end end
function on.resume() controls:reset(); ask("Playing") end
function on.restart() controls:reset(); new_match(); ask("Playing") end

-- Play together: everyone here gets the same roster and map from this device's `lobby` message and
-- makes the same match (jetpack-mp.md M8). The roster is net.peers(), sorted by the platform.
local function together(ids, map_id)
	for _, map in ipairs(maps) do if map.id == map_id then g.map = map end end
	controls:reset()
	new_match(ids)
end
function on.together()
	local ids = {}
	for _, p in ipairs(net.peers()) do ids[#ids + 1] = p.id end
	if #ids < 2 then return end
	net.send("lobby", {start = true, players = ids, map = g.map.id})
	together(ids, g.map.id)
end
net.on("lobby", function(body)
	if type(body) == "table" and body.start and type(body.players) == "table" then together(body.players, body.map) end
end)

-- The view widens by the zoom level the player chose on the gun in hand (controls.lua; 1 = normal).
local function gun_zoom() return (controls:zoom_info()) end

local function frame(e)
	if g.next and g.next.state then show(g.next) end
	local p = g.m.player
	local resized = g.size.w ~= e.width or g.size.h ~= e.height
	if resized then g.size = {w=e.width,h=e.height} end
	-- Landscape only: a playfield taller than wide pauses the match (resuming is the player's tap, in landscape).
	if Rotate.portrait(g.size) and g.m.state == "Playing" then on.pause() end
	-- The camera eases only on new frame time (captures repeat the same elapsed), and jumps with a respawn.
	local dt = g.last and math.max(0, math.min(0.1, e.elapsed - g.last)) or 0
	g.last = e.elapsed
	local f = g.fps
	if dt > 0 then
		f.n, f.t, f.worst = f.n + 1, f.t + dt, math.max(f.worst, dt)
		if f.t >= 0.5 then
			f.text = string.format("%d fps  worst %.0f ms", math.floor(f.n / f.t + 0.5), f.worst * 1000)
			-- Together: rollbacks (a late input corrected) and stalls (waiting on one) per second.
			local st = g.match and #g.match.players > 1 and g.match.stats
			if st then
				local was = f.stats or st
				f.text = f.text .. string.format("  rb %.0f/s  stall %.0f/s",
					(st.rollbacks - was.rollbacks) / f.t, (st.stalls - was.stalls) / f.t)
			end
			f.stats = st or nil
			f.n, f.t, f.worst = 0, 0, 0
		end
	end
	controls:frame(dt)
	local px, py = View.drawn(g, p)
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

-- For tests, never drawn: the match's status and roster, and its checkpoint hashes (jetpack-mp.md M8).
-- Only with others: alone, the hashes keep coming while paused, and a paused game must hold still.
local function readouts()
	if not (g.together or g.next) then return false end
	local m, checks = g.next or g.match, {}
	for _, c in ipairs(m.checks) do checks[#checks + 1] = c.frame .. "=" .. c.hash end
	local function hidden(value, id)
		return ui.text({value, id = id, absolute = true, left = 0, top = 0, font_size = 1, opacity = 0, no_wrap = true})
	end
	-- What a match with others says when it is not simply playing (jetpack-mp.md M9, M10).
	local banner = m.status == "waiting" and ("Waiting for " .. #m.waiting .. " pilot" .. (#m.waiting == 1 and "" or "s") .. "…")
		or m.status == "disputed" and "The pilots disagree: no result"
		or m.status == "ended" and ("Match over" .. (m.ended and (": " .. m.ended) or ""))
		or nil
	return {
		hidden(m.status .. " " .. #m.players .. " " .. m.me .. " " .. table.concat(m.desynced, ","), "match:status"),
		hidden(table.concat(checks, " "), "match:checks"),
		banner and ui.col({absolute = true, top = 70, w_full = true, center = true,
			ui.text({banner, id = "match:banner", color = C.danger, font_size = 16, no_wrap = true})}) or false,
	}
end

return function()
	local portrait = Rotate.portrait(g.size)
	return ui.col({full=true,fill=C.bg,
		Hud.bar(g, on, portrait),
		ui.col({id="playfield",grow=true,w_full=true,min_h=120,
			on_frame=frame,on_wheel=function(e) controls:wheel(e) end,
			on_key=function(e)
					if (e.code == "F3" or e.code == "Backquote") and e.down and not e.repeated then g.fps.show = not g.fps.show; return end
					if not Rotate.portrait(g.size) then controls:key(e, on) end
				end,on_hover=function(e) controls:hover(e) end,
			-- A finger off the sticks and buttons is swallowed here, not turned into a mouse press (which would fire).
			on_touch=function() end,system_cursor=not controls:crosshair(),
			portrait and Rotate.card(g.m) or {
			View.scene(g),
			Hud.status(g),
			controls:view(),
			Hud.overlay(g, on),
			Menu.maps(g, pick_map),
			readouts(),
			g.fps.show and ui.col({absolute = true, top = 4, right = 10, px = 7, py = 3, radius = 6, fill = C.jet_bg,
				ui.text({g.fps.text, id = "hud:fps", color = C.text, font_size = 11, no_wrap = true})}) or false,
			},
		}),
	})
end
