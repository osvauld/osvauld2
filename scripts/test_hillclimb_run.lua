-- Hillclimb's stage and run rules, in plain Lua on a virtual clock (plan: docs/design/hillclimb.md).
--   lua scripts/test_hillclimb_run.lua
package.path = "demo_apps/hillclimb/?.lua;" .. package.path
local Vehicle = require("vehicle")
local Stage = require("stage")
local Run = require("run")
local U = require("units")

local car = Vehicle.check(Vehicle.car)
local function near(a, b, eps) return math.abs(a - b) <= (eps or 1e-9) end

-- P-S1: a seed is a stage, the same every time.
local a, b, c = Stage.from_map(Stage.generate(7)), Stage.from_map(Stage.generate(7)), Stage.from_map(Stage.generate(8))
assert(#a.points == #b.points, "same seed, same length")
for i = 1, #a.points do assert(a.points[i] == b.points[i], "same seed, same point " .. i) end
local differs = false
for i = 1, math.min(#a.points, #c.points) do differs = differs or a.points[i] ~= c.points[i] end
assert(differs, "another seed, another stage")

-- Left to right (the solid below), slopes bounded, a flat start.
local s = a
local steepest = 0
for i = 3, #s.points, 2 do
	local dx, dy = s.points[i] - s.points[i - 2], s.points[i + 1] - s.points[i - 1]
	assert(dx > 0, "points run left to right")
	steepest = math.max(steepest, math.abs(dy / dx))
end
assert(steepest <= math.tan(math.rad(s.max_slope)) + 1e-9, "no slope past " .. s.max_slope .. "°: " .. steepest)
assert(near(Stage.ground_y(s, 0), Stage.ground_y(s, U.length(10))), "the first 10 m are flat")
assert(s.finish_x > U.length(500), "a stage worth driving")

-- The car starts resting just above the start, its wheels clear of the ground.
local lowest = s.start[2] + U.length(car.wheels[1].at[2] + car.wheels[1].radius)
assert(lowest < Stage.ground_y(s, s.start[1]) and lowest > Stage.ground_y(s, s.start[1]) - 20, "starts just above the ground")

-- Fuel cans close enough that a full tank on the flat reaches the next with half to spare.
local per_metre = (car.fuel.burn + car.fuel.idle) / (car.drive.top_speed / 3.6)
local reach = U.length(car.fuel.capacity / per_metre)
local last, cans, coins = s.start[1], 0, 0
for _, item in ipairs(s.items) do
	if item.kind == "fuel" then
		assert(item.at[1] - last <= reach / 2, "fuel gap " .. (item.at[1] - last) .. " past half a tank's reach " .. reach)
		last, cans = item.at[1], cans + 1
	else
		coins = coins + 1
	end
	assert(item.at[2] < Stage.ground_y(s, item.at[1]), "an item floats above the ground: " .. item.id)
	assert(s.items_by_id[item.id] == item)
end
assert(cans >= 2 and coins >= 20, "fuel and coins along the way")

-- P-G1: fuel burns with the gas, idles without, and a can fills the tank.
local r = Run.new(car, s)
assert(r.fuel == car.fuel.capacity and r.coins == 0 and r.best == 0 and not r.over)
local body = { pos = { s.start[1], s.start[2] }, rot = 0, velocity = { 0, 0 } }
for _ = 1, 60 do r:advance(1 / 60, { gas = 1, brake = 0 }, body) end
assert(near(r.fuel, car.fuel.capacity - car.fuel.burn - car.fuel.idle, 1e-6), "one second of full gas")
for _ = 1, 60 do r:advance(1 / 60, { gas = 0, brake = 0 }, body) end
assert(near(r.fuel, car.fuel.capacity - car.fuel.burn - 2 * car.fuel.idle, 1e-6), "one second idling")
local can
for _, item in ipairs(s.items) do if item.kind == "fuel" then can = item break end end
r:zone({ id = can.id, who = "wheel1", phase = "enter" })
assert(r.fuel == car.fuel.capacity and r.taken[can.id], "a can fills the tank and is gone")
r.fuel = 1
r:zone({ id = can.id, who = "chassis", phase = "enter" })
assert(r.fuel == 1, "a taken can is taken")
local coin
for _, item in ipairs(s.items) do if item.kind == "coin" then coin = item break end end
r:zone({ id = coin.id, who = "head", phase = "enter" })
r:zone({ id = coin.id, who = "chassis", phase = "enter" })
assert(r.coins == 1, "a coin counts once")
local left = 0
for _, item in ipairs(r:items()) do if item.id == coin.id or item.id == can.id then left = left + 1 end end
assert(left == 0, "taken items drop out of the description")

-- Distance is the furthest the chassis got, in metres.
body.pos[1] = s.start[1] + U.length(42)
r:advance(1 / 60, { gas = 1, brake = 0 }, body)
body.pos[1] = s.start[1] + U.length(30)
r:advance(1 / 60, { gas = 0, brake = 1 }, body)
assert(near(r.best, 42), "furthest, not latest: " .. r.best)

-- Empty, and stopped for 3 s: out of fuel.
r.fuel = 0
body.velocity = { 0, 0 }
for _ = 1, 170 do r:advance(1 / 60, { gas = 1, brake = 0 }, body) end
assert(not r.over, "not yet")
for _ = 1, 20 do r:advance(1 / 60, { gas = 1, brake = 0 }, body) end
assert(r.over == "out of fuel", tostring(r.over))

-- P-G2: the head on the ground ends it; the body on the ground does not.
r = Run.new(car, s)
r:hit({ id = "chassis", who = "ground", speed = 400 })
r:hit({ id = "wheel1", who = "ground", speed = 900 })
assert(not r.over, "wheels and body may land hard")
r:hit({ id = "head", who = "ground", speed = 50 })
assert(r.over == "crashed", tostring(r.over))
local fuel = r.fuel
r:advance(1, { gas = 1, brake = 0 }, body)
assert(r.fuel == fuel, "over is over: nothing burns")

-- Upside down for 2 s ends it; a brief roll does not.
r = Run.new(car, s)
body = { pos = { s.start[1], s.start[2] }, rot = 170, velocity = { 0, 0 } }
for _ = 1, 100 do r:advance(1 / 60, { gas = 0, brake = 0 }, body) end
body.rot = -10
r:advance(1 / 60, { gas = 0, brake = 0 }, body)
body.rot = -175
for _ = 1, 110 do r:advance(1 / 60, { gas = 0, brake = 0 }, body) end
assert(not r.over, "the clock restarted when it came upright")
for _ = 1, 15 do r:advance(1 / 60, { gas = 0, brake = 0 }, body) end
assert(r.over == "flipped", tostring(r.over))

-- Past the finish: done.
r = Run.new(car, s)
body = { pos = { s.finish_x + 1, s.start[2] }, rot = 0, velocity = { 500, 0 } }
r:advance(1 / 60, { gas = 1, brake = 0 }, body)
assert(r.over == "finished", tostring(r.over))

print("hillclimb run: ok")
