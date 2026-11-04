-- Hillclimb: drive a sprung buggy over a seeded kilometre of hills. Gas and brake (→/D, ←/A or
-- the pedals); fuel cans keep you going, coins are for counting; land on your head and the run is
-- over. Rapier does all the moving (ui.world: line ground, wheel joints, motors); Lua describes
-- the car each view and keeps the run's rules (run.lua). Plan: docs/design/hillclimb.md.
local V = require("vehicle")
local U = require("units")
local Setup = require("setup")
local Drive = require("drive")
local Stage = require("stage")
local Run = require("run")
local Art = require("art")
local Controls = require("controls")

local C = Art.colours
local W, H = 880, 470
local SEED = 7

local car = V.check(V.car)
local stage = Stage.build(SEED)
local function mounts()
	local axles = {}
	for _, w in ipairs(car.wheels) do
		local at = U.point(w.at)
		table.insert(axles, { at[1], at[2], r = U.length(w.radius) })
	end
	return { axles = axles, neck = U.point(car.driver.neck) }
end
local looks = Art.build(Setup.sizes(car), stage, mounts())
local controls = Controls.new()

local game = { n = 0 }

-- A run is a fresh world (its id changes), so the car spawns again at the start.
local function fresh()
	game.n = game.n + 1
	game.run = Run.new(car, stage)
	game.last = nil
	game.car = Setup.build(car, stage.start).entities
	for _, e in ipairs(game.car) do e.drawing = looks[e.id] end
	controls:reset()
end
fresh()

local function world_id() return "stage" .. game.n end

local GROUND = {
	id = "ground", pos = looks.ground_at, drawing = looks.ground,
	collider = { line = looks.ground_line }, group = "ground",
}
local BACKDROP = { id = "backdrop", pos = looks.backdrop_at, drawing = looks.backdrop }
local CLIFF = {
	id = "cliff", pos = looks.cliff_at, drawing = looks.cliff,
	collider = { rect = Art.CLIFF }, group = "ground",
}

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
	local road = world(world_id())
	local body = road:get("chassis")
	local pedals = run.over and { gas = 0, brake = 0 } or controls:state()
	local stage_world = {
		id = world_id(), width = W, height = H, radius = 12, fill = C.sky,
		gravity = { 0, U.gravity },
		camera = { follow = "chassis", ease = 6, bounds = looks.bounds },
		joints = Setup.joints(car, Drive.motors(car, pedals, run)),
		on_hit = function(e) run:hit(e) end,
		on_zone = function(e) run:zone(e) end,
		BACKDROP, GROUND, CLIFF,
	}
	for _, item in ipairs(run:items()) do
		local r = Art.PICKUP_R[item.kind]
		table.insert(stage_world, {
			id = item.id, pos = { item.at[1] - r, item.at[2] - r }, drawing = looks[item.kind],
			sensor = { circle = r, at = { r, r } },
		})
	end
	for _, e in ipairs(game.car) do table.insert(stage_world, e) end

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
		}),
		run.over and over_panel(run) or false,
	})
end
