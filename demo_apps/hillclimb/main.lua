-- Hillclimb: drive a sprung car over hills. Gas and brake (→/D, ←/A or the pedals); fuel cans
-- keep you going, coins are for counting; land on your head and the run is over. Rapier does all
-- the moving (ui.world: line ground, wheel joints, motors); Lua describes the car each view and
-- keeps the run's rules (run.lua). The courses are map files (maps/*.lua) and the art is SVG.
-- Plans: docs/design/hillclimb.md, hillclimb-maps.md.
local V = require("vehicle")
local U = require("units")
local Setup = require("setup")
local Drive = require("drive")
local Stage = require("stage")
local Run = require("run")
local Art = require("art")
local Controls = require("controls")
local Truck = require("truck")

local C = Art.colours
local W, H = 880, 470
local MAPS = { "maps/green_hills", "maps/seeded" } -- the first is the default
local SINK = 12 -- scenery stands this far into the ground, so the grass covers its foot (units)

local function mounts(car)
	local axles = {}
	for _, w in ipairs(car.wheels) do
		local at = U.point(w.at)
		table.insert(axles, { at[1], at[2], r = U.length(w.radius) })
	end
	return { axles = axles, neck = U.point(car.driver.neck) }
end
local buggy = V.check(V.car)
local controls = Controls.new()

-- SVG scenery, one drawing per art file.
local art = {}
local function piece(name)
	if not art[name] then
		local spec = gfx.svg(name)
		art[name] = { drawing = gfx.drawing(spec), w = spec.size[1], h = spec.size[2] }
	end
	return art[name]
end

-- A course: a map's stage, its looks and its fixed entities. Built when first picked.
local courses = {}
local function course(i)
	if courses[i] then return courses[i] end
	local stage = Stage.from_map(require(MAPS[i]))
	local looks = Art.build(Setup.sizes(buggy), stage, mounts(buggy))
	local c = {
		stage = stage, looks = looks, back = {}, front = {},
		ground = {
			id = "ground", pos = looks.ground_at, drawing = looks.ground,
			collider = { line = looks.ground_line }, group = "ground",
		},
		backdrop = { id = "backdrop", pos = looks.backdrop_at, drawing = looks.backdrop },
		cliff = {
			id = "cliff", pos = looks.cliff_at, drawing = looks.cliff,
			collider = { rect = Art.CLIFF }, group = "ground",
		},
	}
	for n, s in ipairs(stage.scenery) do
		local p = piece(s.art)
		local e = { id = "scenery" .. n, drawing = p.drawing,
			pos = { s.x - p.w / 2, Stage.ground_y(stage, s.x) - p.h + SINK } }
		table.insert(s.front and c.front or c.back, e)
	end
	-- A fingerprint of what the world spawns from the map. Worlds outlive hot reload and never
	-- move what already exists, so an edited map must be a new world id: this is in it.
	local h = 0
	local function mix(v) h = (h * 31 + math.floor(v * 16)) % 2147483647 end
	for _, v in ipairs(stage.points) do mix(v) end
	for _, item in ipairs(stage.items) do mix(item.at[1]); mix(item.at[2]) end
	for _, list in ipairs({ c.back, c.front }) do
		for _, e in ipairs(list) do mix(e.pos[1]); mix(e.pos[2]) end
	end
	c.key = string.format("%x", h)
	courses[i] = c
	return c
end

-- The cars to pick from. Each brings its own body drawings; the course's looks are shared.
-- `clear` is how far above the ground the chassis top spawns, so its wheels just clear it.
local function choice(name, car, drawings)
	local m, clear = mounts(car), 0
	for _, a in ipairs(m.axles) do clear = math.max(clear, a[2] + a.r + 4) end
	local rear = m.axles[1]
	return { name = name, car = car, drawings = drawings, clear = clear,
		dust_at = { rear[1], rear[2] + rear.r - 6 } } -- under the rear tyre: where its dust leaves from
end
local truck = V.check(V.truck)
local CARS = {
	choice("Buggy", buggy, course(1).looks), -- the buggy's parts come with any course's looks
	choice("Truck", truck, Truck.looks(Setup.sizes(truck))),
}

local game = { n = 0, pick = 1, map = 1 }
local car = buggy

-- A run is a fresh world (its id changes), so the car spawns again at the start.
local function fresh()
	local pick, stage = CARS[game.pick], course(game.map).stage
	car = pick.car
	game.n = game.n + 1
	game.run = Run.new(car, stage)
	game.last = nil
	local x = stage.start[1]
	game.car = Setup.build(car, { x, Stage.ground_y(stage, x) - pick.clear }).entities
	for _, e in ipairs(game.car) do e.drawing = pick.drawings[e.id] end
	controls:reset()
end
fresh()

local function swap()
	game.pick = game.pick % #CARS + 1
	fresh()
end

local function swap_map()
	game.map = game.map % #MAPS + 1
	fresh()
end

local function world_id() return "stage" .. game.n .. "-" .. course(game.map).key end

-- The run's clock is the frame clock (virtual offscreen): fuel and the end rules advance on it.
local function frame(e)
	if game.last == e.elapsed then return end
	local dt = game.last and math.min(e.elapsed - game.last, 0.1) or 0
	game.last = e.elapsed
	game.run:advance(dt, controls:state(), world(world_id()):get("chassis"))
end

local function label(text, size, id)
	return ui.text({ text, id = id, color = C.text, font_size = size or 15, no_wrap = true })
end

local function pedal(name, text)
	local held = controls:state()[name] > 0
	return ui.col({
		id = "pedal_" .. name, w = 120, h = 64, radius = 12, center = true,
		fill = held and C.pedal_held or C.pedal,
		on_hover = function(e) controls:hover(name, e) end,
		ui.text({ text, color = C.pedal_text, font_size = 16, no_wrap = true }),
	})
end

local function hud(run, body)
	local kmh = body and U.kmh(math.sqrt(body.velocity[1] ^ 2 + body.velocity[2] ^ 2)) or 0
	local share = run.fuel / car.fuel.capacity
	return ui.row({
		gap = 22, align_center = true, px = 14, py = 8, radius = 10, fill = C.panel,
		label(string.format("%d m", math.floor(run.best)), 18, "distance"),
		label(string.format("%d km/h", math.floor(kmh + 0.5)), 15, "speed"),
		label("coins " .. run.coins, 15, "coins"),
		ui.row({
			gap = 8, align_center = true,
			label("fuel", 13),
			ui.col({
				w = 140, h = 12, radius = 6, fill = "#00000022",
				ui.col({ w = math.max(0, 140 * share), h = 12, radius = 6, fill = share < 0.25 and C.fuel_low or C.fuel }),
			}),
			label(string.format("%.1f L", run.fuel), 13, "fuel"),
		}),
	})
end

local REASONS = {
	crashed = "Head first. Run over.", flipped = "On the roof. Run over.",
	["out of fuel"] = "Out of fuel.", finished = "Made it to the end!",
}

local function over_panel(run)
	return ui.col({
		absolute = true, top = 150, left = 260, w = 360, pad = 20, gap = 12, radius = 14,
		fill = "#ffffffee", align_center = true, fade_in = 200,
		ui.text({ REASONS[run.over] or run.over, id = "over", color = C.text, font_size = 20, no_wrap = true }),
		label(string.format("%d m · %d coins", math.floor(run.best), run.coins), 15, "result"),
		ui.button({
			id = "retry", px = 22, py = 10, radius = 10, fill = C.pedal, hover_fill = C.pedal_held,
			ui.text({ "Retry", color = C.pedal_text, font_size = 15, no_wrap = true }),
			on_click = fresh,
		}),
	})
end

return function()
	local run = game.run
	local here = course(game.map)
	local road = world(world_id())
	local body = road:get("chassis")
	local pedals = run.over and { gas = 0, brake = 0 } or controls:state()
	local stage_world = {
		id = world_id(), width = W, height = H, radius = 12, fill = C.sky,
		sky = { stops = { { 0, C.sky_top }, { 0.7, C.sky_low }, { 1, C.sky_low } } },
		gravity = { 0, U.gravity },
		camera = { follow = "chassis", ease = 6, bounds = here.looks.bounds },
		joints = Setup.joints(car, Drive.motors(car, pedals, run)),
		on_hit = function(e) run:hit(e) end,
		on_zone = function(e) run:zone(e) end,
		here.backdrop,
	}
	for _, e in ipairs(here.back) do table.insert(stage_world, e) end
	table.insert(stage_world, here.ground)
	table.insert(stage_world, here.cliff)
	for _, item in ipairs(run:items()) do
		local r = Art.PICKUP_R[item.kind]
		table.insert(stage_world, {
			id = item.id, pos = { item.at[1] - r, item.at[2] - r }, drawing = here.looks[item.kind],
			sensor = { circle = r, at = { r, r } },
		})
	end
	-- Dust thrown back off the rear tyre while it drives: from the chassis (the wheel spins, its
	-- frame would spray in circles), where the tyre meets the ground.
	local moving = body and math.abs(body.velocity[1]) > 60
	local throwing = pedals.gas > 0 and moving
	for _, e in ipairs(game.car) do
		if e.id == "chassis" then
			e.emit = { rate = throwing and 45 * pedals.gas or 0, life = 0.7, color = C.dust,
				speed = 140, angle = 205, spread = 40, size = 4, grow = 14, gravity = 160, at = CARS[game.pick].dust_at }
		end
		table.insert(stage_world, e)
	end
	for _, e in ipairs(here.front) do table.insert(stage_world, e) end

	return ui.col({
		id = "hillclimb", full = true, center = true, gap = 10, fill = "#dff1fb",
		on_frame = frame,
		on_key = function(e) controls:key(e) end,
		hud(run, body),
		ui.world(stage_world),
		ui.row({
			gap = 24,
			pedal("brake", "BRAKE"),
			label("→ / D gas · ← / A brake", 13),
			pedal("gas", "GAS"),
			ui.button({
				id = "swap_car", px = 16, py = 10, radius = 10, fill = C.pedal, hover_fill = C.pedal_held,
				ui.text({ "Car: " .. CARS[game.pick].name, color = C.pedal_text, font_size = 14, no_wrap = true }),
				on_click = swap,
			}),
			ui.button({
				id = "swap_map", px = 16, py = 10, radius = 10, fill = C.pedal, hover_fill = C.pedal_held,
				ui.text({ "Map: " .. here.stage.name, color = C.pedal_text, font_size = 14, no_wrap = true }),
				on_click = swap_map,
			}),
		}),
		run.over and over_panel(run) or false,
	})
end
