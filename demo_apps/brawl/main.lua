local C = require("theme")
local T = require("tune")
local F = require("fighter")
local Input = require("input")
local Script = require("script")
local Camera = require("camera")
local SceneArt = require("scene_art")
local Gallery = require("gallery")
local Library = require("library")
local Map = require("map")
local Look = require("look")

-- One fighter alone is a one-player match: no input delay, an input every tick. With others it
-- will be `frame = 4` and a roster, as Jetpack's. `rules` comes from the setup the match hands
-- `state` (with others, the starter's), so step reads no state of this device's.
-- A scripted run (the hidden `script` button) feeds script.lua's timeline by match frame and stops
-- stepping at its end, so two runs, and P-M12's straight run, end in the same state.
local TICK = 1 / 120
local input, rules, stop, match
local scripted = false

-- The map: maps/index.lua lists them, the first is played. It places library assets; compiling
-- gives the stage the rules and the match carry (no art in it) and the scene only this view draws.
-- A broken library still gives the rules their stage (collision doesn't need the pictures); the
-- scene is skipped and the stage is drawn plain (SceneArt.problem says why).
local lib = Library.load()
local body = { w = T.body_w, h = T.body_h }
local index = require("maps/index")
local which = 1 -- the index of the map being played
local map, yard, scene
local function load_map(i)
	which = i
	map = require("maps/" .. index[i])
	if SceneArt.problem then
		yard, scene = Map.stage(map, lib, { body = body }), nil
	else
		local compiled = Map.compile(map, lib, { body = body })
		yard, scene = compiled.stage, compiled.scene
	end
end
-- A map that doesn't compile errors here, naming the piece (map.lua), and the shell keeps the running app.
load_map(1)

local function start()
	input = Input.new() -- a new match counts presses from 0 again
	match = net.match({
		id = "brawl", tick = TICK, frame = 1,
		setup = { stage = yard, tune = T, stop = scripted and Script.length or 0 },
		state = function(setup, ids)
			rules, stop = F.rules(setup), setup.stop
			return F.new(rules, #ids)
		end,
		input = function()
			if scripted then return Script.at((match and match.frame or 0) + 1) end
			return input:take()
		end,
		step = function(s, inputs)
			if stop > 0 and s.tick >= stop then return end
			F.step(s, inputs, rules)
		end,
	})
end
start()

local size = { w = 900, h = 600 }

-- The scene's pictures, built once (view-side only: no rule reads them).
local art
local function art_of(stage)
	if not art then art = SceneArt.build(stage, scene) end
	return art
end

-- `map:next`: the next map in the index, a new match on it. Only the new map's pictures stay in the
-- texture cache (the ones both maps use are kept, not decoded again), and the camera starts on its spawn.
local cam, last_elapsed
local function next_map()
	load_map(which % #index + 1)
	art = nil
	SceneArt.release(scene and scene.images or {})
	cam, last_elapsed, rules = nil, nil, nil
	start()
end

-- The camera is presentation, not state: it eases on render time and nothing in `step` reads it.
-- `cam` is nil until the first frame, when it starts on its target (the spawn, until a match runs).
local function aspect() return size.w / size.h end
-- `wide` (a hidden button, view-only): the camera pulls all the way out, as if fighters stood at
-- both far corners of its bounds, so the zoomed-out backdrop can be seen and shot (S-M6) while the
-- match has one fighter.
local wide = false
local function camera_target(stage)
	local fighters = {}
	if match.state then
		fighters = match.state.fighters
	else
		for _, p in ipairs(stage.spawns) do fighters[#fighters + 1] = { x = p.x, y = p.y, act = "idle" } end
	end
	local boxes = Camera.boxes(fighters, T.body_w, T.body_h)
	if wide then
		local b = stage.camera
		boxes[#boxes + 1] = { x = b.x, y = b.y, w = 1, h = 1 }
		boxes[#boxes + 1] = { x = b.x + b.w - 1, y = b.y + b.h - 1, w = 1, h = 1 }
	end
	return Camera.target(boxes, stage.camera, aspect())
end
local function follow(e)
	local stage = rules and rules.stage or yard
	local dt = last_elapsed and math.max(0, math.min(0.1, e.elapsed - last_elapsed)) or 0
	last_elapsed = e.elapsed
	local target = camera_target(stage)
	if target then cam = Camera.ease(cam, target, dt) end
end

-- The fighter is a robot posed from its state (look.lua): clip and time from act and act_t,
-- mirrored by face, cached per frame, blended out of prev_act for a few ticks after a switch. `pair` (a hidden button) draws a second robot beside it from
-- the same state, facing the other way, so the cost of two can be measured (decision 7).
local look = Look.new()
local ROBOT, PAIR_ROBOT = "scout", "brute"
local pair = false
local hovered -- the robot part under the pointer, for S-8's mirror check

-- The robot's frame with its feet at world (x, y): placed by its top-left and scaled about it, as the
-- camera scales the stage. Standing and running (`plant`), the frame's lowest boot point is on the
-- feet line; in the air the root's pivot is. Returns the element and the picked look.
local function robot_el(id, robot, f, x, y, k)
	local fr, p = look:frame(robot, f)
	if not fr then return nil, p end
	local _, _, px, py = look:anchor(robot)
	local feet = p.plant and p.sole or py
	return ui.frame({ id = id, visual = fr, absolute = true, left = (x - px - cam.x) * k, top = (y - feet - cam.y) * k, scale = k,
		on_hover = function(e)
			if e.phase ~= "leave" and e.shape then hovered = id .. "/" .. e.shape else hovered = nil end
		end }), p
end

-- The asset sheet (the hidden `assets` button swaps the playfield for it) and the collision overlay
-- (hidden `overlay`, or F2): `col:<id>` over every solid and platform, so art and collision can be
-- held against each other (brawl-maps.md decisions 12-13). Both are view-only.
local gallery, showing, overlay = nil, false, false
local overlay_frames = {}
local function overlay_frame(kind, w, h)
	local key = kind .. ":" .. w .. "x" .. h
	if not overlay_frames[key] then
		local line = 3 -- units: two or three pixels at the usual zoom
		if kind == "solid" then
			overlay_frames[key] = gfx.frame({ width = w, height = h,
				gfx.fill({ path = gfx.path({ { "move", 0, 0 }, { "line", w, 0 }, { "line", w, h }, { "line", 0, h }, { "close" } }), brush = gfx.solid("#ff3a3a2e") }),
				gfx.stroke({ path = gfx.path({ { "move", line / 2, line / 2 }, { "line", w - line / 2, line / 2 }, { "line", w - line / 2, h - line / 2 },
					{ "line", line / 2, h - line / 2 }, { "close" } }), brush = gfx.solid("#ff3a3a"), width = line }) })
		else -- a one-way platform is a line: a yellow bar hanging 6 units under it
			overlay_frames[key] = gfx.frame({ width = w, height = 6,
				gfx.fill({ path = gfx.path({ { "move", 0, 0 }, { "line", w, 0 }, { "line", w, 6 }, { "line", 0, 6 }, { "close" } }), brush = gfx.solid("#ffd23f") }) })
		end
	end
	return overlay_frames[key]
end

local function overlay_elements(stage, k)
	local out = {}
	local function add(kind, b)
		out[#out + 1] = ui.frame({ id = "col:" .. b.id, visual = overlay_frame(kind, b.w, b.h or 6), absolute = true, on_hover = function() end,
			left = (b.x - cam.x) * k, top = (b.y - cam.y) * k, scale = k })
	end
	for _, b in ipairs(stage.solids) do add("solid", b) end
	for _, b in ipairs(stage.platforms) do add("platform", b) end
	return out
end

local function hidden(value, id)
	return ui.text({ value, id = id, absolute = true, left = 0, top = 0, font_size = 1, opacity = 0, no_wrap = true })
end

local function control(label, id, left, run)
	return ui.button({ label, id = id, absolute = true, left = left, top = 0, opacity = 0, on_click = run })
end

local function scene()
	local s = match.state
	local stage = rules and rules.stage or yard
	local art = art_of(stage)
	SceneArt.load(art, 400000) -- a few pictures a frame; see scene_art.lua
	look:warm() -- the robots, one a frame; see look.lua
	cam = cam or camera_target(stage)
	local k = size.w / cam.w
	local out = SceneArt.under(art, cam, k, 0, 0, size.w, size.h, last_elapsed or 0)
	out[#out + 1] = control("script", "script", 0, function() scripted = true; start() end)
	out[#out + 1] = control("play", "play", 60, function() scripted = false; start() end)
	out[#out + 1] = control("pair", "pair", 120, function() pair = not pair end)
	out[#out + 1] = control("assets", "assets", 180, function() showing = true end)
	out[#out + 1] = control("overlay", "overlay", 240, function() overlay = not overlay end)
	out[#out + 1] = control("wide", "wide", 300, function() wide = not wide end)
	out[#out + 1] = ui.button({ "Map: " .. stage.name, id = "map:next", absolute = true, right = 16, top = 34, on_click = next_map })
	if not s then return out end
	local f = s.fighters[1]
	local drawn = "hidden"
	if f.act ~= "out" and f.act ~= "ko" then
		-- Drawn on by the time the match has not stepped yet (velocities are per tick), so 120 ticks
		-- on a 75 Hz screen don't judder.
		local ahead = match.ahead / TICK
		local x, y = f.x + f.vx * ahead, f.y + f.vy * ahead
		-- The collision box: not drawn, but where rects() finds the fighter (S-7).
		out[#out + 1] = ui.col({ id = "fighter", absolute = true, on_hover = function() end,
			left = (x - T.body_w / 2 - cam.x) * k, top = (y - T.body_h - cam.y) * k, w = T.body_w * k, h = T.body_h * k })
		if pair then
			local twin = { act = f.act, act_t = f.act_t, face = -f.face, prev_act = f.prev_act, prev_t = f.prev_t }
			out[#out + 1] = robot_el("fighter:pair", PAIR_ROBOT, twin, x + 260, y, k)
		end
		local el, p = robot_el("fighter:robot", ROBOT, f, x, y, k)
		out[#out + 1] = el
		local _, _, _, py = look:anchor(ROBOT)
		drawn = string.format("robot=%s clip=%s index=%d time=%.17g face=%d x=%.17g y=%.17g k=%.17g sole=%.17g feet=%.17g blend=%.17g",
			ROBOT, p.clip, p.index, p.time, f.face, x, y, k, p.sole, p.plant and p.sole or py, p.blend or -1)
	end
	for _, e in ipairs(SceneArt.over(art, cam, k, 0, 0, last_elapsed or 0)) do out[#out + 1] = e end
	if overlay then
		for _, e in ipairs(overlay_elements(stage, k)) do out[#out + 1] = e end
	end
	local stocks = string.rep("● ", f.stocks) .. string.rep("○ ", T.stocks - f.stocks)
	out[#out + 1] = ui.row({ absolute = true, left = 16, top = 12, gap = 10,
		ui.text({ "Stocks", color = C.muted, font_size = 13, no_wrap = true }),
		ui.text({ stocks, id = "hud:stocks", color = C.text, font_size = 13, no_wrap = true }),
	})
	if f.act == "ko" then
		out[#out + 1] = ui.col({ absolute = true, top = 80, w_full = true, center = true,
			ui.text({ "KO — press Enter", id = "hud:ko", color = C.danger, font_size = 22, no_wrap = true }) })
	end
	out[#out + 1] = ui.col({ absolute = true, bottom = 10, w_full = true, center = true,
		ui.text({ "A/D or ←/→ move · Space jump (tap: short hop) · S/↓ drop, fast fall · Shift/K dash (+ direction) · Enter restart",
			color = C.muted, font_size = 11, no_wrap = true }) })
	if scripted then
		out[#out + 1] = ui.text({ "scripted run", id = "hud:script", absolute = true, right = 16, top = 12, color = C.muted, font_size = 13, no_wrap = true })
	end
	-- For tests, never drawn: the fighter and the match's checkpoint hashes. (The `script` and `play`
	-- buttons above start a scripted run, again if need be, or hand the keys back.)
	out[#out + 1] = hidden(F.describe(f, s.tick), "fighter:state")
	local checks = {}
	for _, c in ipairs(match.checks) do checks[#checks + 1] = c.frame .. "=" .. c.hash end
	out[#out + 1] = hidden(table.concat(checks, " "), "match:checks")
	out[#out + 1] = hidden(SceneArt.progress(art_of(stage)), "art:loaded")
	out[#out + 1] = hidden(overlay and "on" or "off", "overlay:state")
	out[#out + 1] = hidden(stage.name, "map:state")
	out[#out + 1] = hidden(table.concat(art.images or {}, " "), "map:art")
	out[#out + 1] = hidden(drawn, "look:state")
	out[#out + 1] = hidden(hovered or "none", "look:hover")
	out[#out + 1] = hidden(look:cost(), "look:cost")
	out[#out + 1] = hidden(look:loaded(), "look:loaded")
	local zmin, zmax = Camera.limits(stage.camera, aspect())
	out[#out + 1] = hidden(string.format("x=%.17g y=%.17g w=%.17g h=%.17g zoom=%.17g zoom_min=%.17g zoom_max=%.17g",
		cam.x, cam.y, cam.w, cam.h, cam.zoom, zmin, zmax), "camera:view")
	return out
end

-- The sheet: built the first time it is opened, a few pictures a frame, kept after. A broken library
-- (SceneArt.problem) has no sheet; the stage stays and the problem readout says why.
local function sheet_scene()
	if not gallery then gallery = Gallery.new(lib) end
	Gallery.load(gallery, 400000)
	local function toggle() showing = false end
	local out = Gallery.view(gallery, size.w, size.h, toggle)
	out[#out + 1] = control("assets", "assets", 180, toggle)
	out[#out + 1] = hidden(Gallery.progress(gallery) .. " page " .. gallery.page, "gallery:state")
	return out
end

return function()
	return ui.col({ full = true, fill = C.bg,
		ui.col({ id = "playfield", grow = true, w_full = true, clip = true, fill = C.bg,
			on_key = function(e)
				if e.code == "F2" and e.down and not e.repeated then overlay = not overlay end
				input:key(e)
			end,
			on_frame = function(e)
				if e.width and e.width > 0 and (e.width ~= size.w or e.height ~= size.h) then size = { w = e.width, h = e.height } end
				follow(e)
			end,
			showing and not SceneArt.problem and sheet_scene() or scene(),
		}),
	})
end
