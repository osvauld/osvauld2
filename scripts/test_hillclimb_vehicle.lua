-- Hillclimb's vehicle setup, in plain Lua: no renderer, no world (plan: docs/design/hillclimb.md).
--   lua scripts/test_hillclimb_vehicle.lua
package.path = "demo_apps/hillclimb/?.lua;" .. package.path
local Vehicle = require("vehicle")
local U = require("units")
local Setup = require("setup")
local Drive = require("drive")

local function near(a, b, eps) return math.abs(a - b) <= (eps or 1e-9) end
local function copy(t)
	if type(t) ~= "table" then return t end
	local out = {}
	for k, v in pairs(t) do out[k] = copy(v) end
	return out
end
local function refused(edit, wanted)
	local car = copy(Vehicle.car)
	edit(car)
	local ok, err = pcall(Vehicle.check, car)
	assert(not ok, "accepted a bad car, wanted: " .. wanted)
	assert(tostring(err):find(wanted, 1, true), "wanted " .. wanted .. ", got " .. tostring(err))
end

-- P-V1: the car checks, and every mistake is named.
Vehicle.check(Vehicle.car)
refused(function(c) c.chassis.mass = nil end, "chassis.mass is required (kg)")
refused(function(c) c.chassis.mass = -1 end, "chassis.mass must be above 0 kg")
refused(function(c) c.chassis.colour = "red" end, "chassis: unknown field colour")
refused(function(c) c.wheels[1].radius = 0 end, "wheels[1].radius must be above 0 m")
refused(function(c) c.wheels[2].grip = -0.1 end, "wheels[2].grip must be 0 or more")
refused(function(c) c.suspension.travel = { 0.1 } end, "suspension.travel needs up and down (m)")
refused(function(c) c.drive.top_speed = 0 end, "drive.top_speed must be above 0 km/h")
refused(function(c) c.wheels = {} end, "wheels needs at least one wheel")
refused(function(c) c.turbo = true end, "car: unknown field turbo")
refused(function(c) for _, w in ipairs(c.wheels) do w.driven = false end end, "no wheel is driven")

-- P-V2: units, both ways.
assert(U.length(1.5) == 150 and near(U.metres(150), 1.5))
assert(U.mass(220) == 220)
assert(U.stiffness(18000) == 18000 and U.damping(1600) == 1600, "kg/s² and kg/s pass through")
assert(near(U.torque(700), 7e6) and near(U.newton_metres(7e6), 700))
-- 36 km/h is 10 m/s; on a 0.25 m wheel that is 40 rad/s.
assert(near(U.wheel_speed(36, 0.25), math.deg(40), 1e-9))
assert(near(U.kmh(U.length(10)), 36), "1000 u/s is 36 km/h")
assert(U.gravity == 981)

-- P-V3: the world description comes from the car alone.
local car = Vehicle.car
local made = Setup.build(car, { 400, 300 })
local by = {}
for _, e in ipairs(made.entities) do by[e.id] = e end
local c = by.chassis
assert(c and c.pos[1] == 400 and c.pos[2] == 300, "chassis spawns at the place given")
assert(c.loose.mass == car.chassis.mass + car.driver.mass, "the driver rides in the chassis")
assert(near(c.collider.rect[1], U.length(car.chassis.size[1])), "chassis box in units")
assert(near(c.loose.center[1], U.length(car.chassis.center[1])), "centre of mass in units")
for i, w in ipairs(car.wheels) do
	local e = by["wheel" .. i]
	local r = U.length(w.radius)
	assert(e and e.collider.circle == r and e.loose.mass == w.mass and e.loose.grip == w.grip)
	-- Unloaded, the wheel's centre is on its mount.
	assert(near(e.pos[1] + r, 400 + U.length(w.at[1])) and near(e.pos[2] + r, 300 + U.length(w.at[2])))
	assert(e.group == "car" and e.blocks[1] == "ground", "car parts meet only the ground")
end
assert(by.head and by.head.loose.grip, "the head is its own body, free to nod")
local joints = {}
for _, j in ipairs(made.joints) do joints[j.id] = j end
for i, w in ipairs(car.wheels) do
	local j = joints["axle" .. i]
	assert(j.kind == "wheel" and j.a == "chassis" and j.b == "wheel" .. i)
	assert(j.stiffness == car.suspension.stiffness and j.damping == car.suspension.damping)
	assert(near(j.a_at[1], U.length(w.at[1])) and near(j.a_at[2], U.length(w.at[2])))
	assert(near(j.travel[1], U.length(car.suspension.travel[1])))
end
assert(joints.neck and joints.neck.kind == "pin" and joints.neck.b == "head")

-- Drive: pedals to motors, with no world.
local m = Drive.motors(car, { gas = 1, brake = 0 }, { fuel = 5 })
for i, w in ipairs(car.wheels) do
	local motor = m["axle" .. i]
	if w.driven then
		assert(near(motor.speed, U.wheel_speed(car.drive.top_speed, w.radius)), "gas: toward top speed")
		assert(near(motor.torque, U.torque(car.drive.torque)), "at full torque")
	else
		assert(motor == nil, "an undriven wheel rolls free on the gas")
	end
end
m = Drive.motors(car, { gas = 0.5, brake = 0 }, { fuel = 5 })
assert(near(m.axle1.torque, U.torque(car.drive.torque) * 0.5) or near(m.axle2.torque, U.torque(car.drive.torque) * 0.5), "half pedal, half torque")
m = Drive.motors(car, { gas = 0, brake = 1 }, { fuel = 5 })
for i in ipairs(car.wheels) do
	local motor = m["axle" .. i]
	assert(motor.speed < 0, "the brake drives backwards: braking rolling forward, reversing at rest")
	assert(near(motor.torque, U.torque(car.brake.torque)), "every wheel brakes")
end
m = Drive.motors(car, { gas = 1, brake = 0 }, { fuel = 0 })
for i in ipairs(car.wheels) do assert(m["axle" .. i] == nil, "no fuel, no drive") end
m = Drive.motors(car, { gas = 0, brake = 1 }, { fuel = 0 })
assert(m.axle1 and m.axle1.torque > 0, "the brake works without fuel")
m = Drive.motors(car, { gas = 1, brake = 1 }, { fuel = 5 })
assert(m.axle1.speed < 0, "both pedals: the brake wins")

-- The joints with motors are plain data to describe each view.
local described = Setup.joints(car, Drive.motors(car, { gas = 1, brake = 0 }, { fuel = 5 }))
local driven = 0
for _, j in ipairs(described) do if j.motor then driven = driven + 1 end end
assert(driven >= 1)

print("hillclimb vehicle: ok")
