-- The car as data, in SI units: no world, no art, no input. `setup.lua` turns it into bodies and
-- joints; `drive.lua` turns pedals into motors. Every field is required and checked.
--
-- Positions are metres from the chassis box's top-left, +y down (the drawing's frame).
local V = {}

V.car = {
	chassis = {
		mass = 220,               -- kg, without the driver
		size = { 2.2, 0.45 },     -- m, the collider box
		center = { 1.05, 0.34 },  -- m, centre of mass: low and a touch forward of the middle
	},
	driver = {
		mass = 70,                -- kg, carried in the chassis
		head_radius = 0.16,       -- m; the head is its own body, and its touching the ground ends the run
		neck = { 0.95, -0.25 },   -- m, where the head's base sits on the chassis
	},
	wheels = {
		{ at = { 0.35, 0.62 }, radius = 0.32, mass = 20, grip = 1.1, driven = true },  -- rear
		{ at = { 1.85, 0.62 }, radius = 0.32, mass = 20, grip = 1.1, driven = true },  -- front
	},
	suspension = {
		stiffness = 18000,        -- N/m per wheel: sags (290 kg / 2) · 9.81 / 18000 ≈ 8 cm
		damping = 1600,           -- N·s/m per wheel: ζ ≈ 0.5 for 145 kg (critical 2√(k·m) ≈ 3230)
		travel = { 0.12, 0.10 },  -- m, compression and extension from the mount
	},
	drive = {
		torque = 420,             -- N·m per driven wheel
		top_speed = 60,           -- km/h
	},
	brake = {
		torque = 600,             -- N·m per wheel
		reverse_speed = 15,       -- km/h: held at rest, the brake backs up
	},
	fuel = {
		capacity = 10,            -- L
		burn = 0.25,              -- L/s at full gas
		idle = 0.02,              -- L/s with the engine on
	},
}

-- The monster truck (art/monster_truck.svg at half size, truck.lua): measured off the drawing, so
-- its wheels are the buggy's size and the buggy's tuning carries over. Positions are from the
-- collider box over the lower body; the head is a crash sensor up in the cab, not drawn. The
-- artist drew it facing left; truck.lua mirrors it, and these are the mirrored measurements.
V.truck = {
	chassis = { mass = 220, size = { 2.345, 0.355 }, center = { 1.195, 0.25 } },
	driver = { mass = 70, head_radius = 0.16, neck = { 1.22, 0.125 } },
	wheels = {
		{ at = { 0.5725, 0.685 }, radius = 0.32, mass = 20, grip = 1.1, driven = true },
		{ at = { 1.855, 0.685 }, radius = 0.32, mass = 20, grip = 1.1, driven = true },
	},
	suspension = V.car.suspension,
	drive = V.car.drive,
	brake = V.car.brake,
	fuel = V.car.fuel,
}

local FIELDS = {
	car = { chassis = true, driver = true, wheels = true, suspension = true, drive = true, brake = true, fuel = true },
	chassis = { mass = true, size = true, center = true },
	driver = { mass = true, head_radius = true, neck = true },
	wheel = { at = true, radius = true, mass = true, grip = true, driven = true },
	suspension = { stiffness = true, damping = true, travel = true },
	drive = { torque = true, top_speed = true },
	brake = { torque = true, reverse_speed = true },
	fuel = { capacity = true, burn = true, idle = true },
}

local function only(t, fields, where)
	if type(t) ~= "table" then error(where .. " must be a table", 0) end
	for k in pairs(t) do
		if not fields[k] then error(where .. ": unknown field " .. tostring(k), 0) end
	end
end

local function positive(t, k, where, unit)
	local v = t[k]
	if v == nil then error(where .. "." .. k .. " is required (" .. unit .. ")", 0) end
	if type(v) ~= "number" or not (v > 0) then error(where .. "." .. k .. " must be above 0 " .. unit, 0) end
end

local function at_least_zero(t, k, where)
	local v = t[k]
	if type(v) ~= "number" or not (v >= 0) then error(where .. "." .. k .. " must be 0 or more", 0) end
end

local function pair(t, k, where, what)
	local v = t[k]
	if type(v) ~= "table" or #v ~= 2 or type(v[1]) ~= "number" or type(v[2]) ~= "number" then
		error(where .. "." .. k .. " needs " .. what, 0)
	end
end

-- Raises a message naming the field for anything missing, unknown or out of range.
function V.check(car)
	only(car, FIELDS.car, "car")
	only(car.chassis, FIELDS.chassis, "chassis")
	positive(car.chassis, "mass", "chassis", "kg")
	pair(car.chassis, "size", "chassis", "width and height (m)")
	pair(car.chassis, "center", "chassis", "x and y (m)")
	only(car.driver, FIELDS.driver, "driver")
	positive(car.driver, "mass", "driver", "kg")
	positive(car.driver, "head_radius", "driver", "m")
	pair(car.driver, "neck", "driver", "x and y (m)")
	if type(car.wheels) ~= "table" or #car.wheels == 0 then error("wheels needs at least one wheel", 0) end
	local driven = false
	for i, w in ipairs(car.wheels) do
		local where = "wheels[" .. i .. "]"
		only(w, FIELDS.wheel, where)
		pair(w, "at", where, "x and y (m)")
		positive(w, "radius", where, "m")
		positive(w, "mass", where, "kg")
		at_least_zero(w, "grip", where)
		if type(w.driven) ~= "boolean" then error(where .. ".driven must be true or false", 0) end
		driven = driven or w.driven
	end
	if not driven then error("no wheel is driven", 0) end
	only(car.suspension, FIELDS.suspension, "suspension")
	positive(car.suspension, "stiffness", "suspension", "N/m")
	at_least_zero(car.suspension, "damping", "suspension")
	pair(car.suspension, "travel", "suspension", "up and down (m)")
	only(car.drive, FIELDS.drive, "drive")
	positive(car.drive, "torque", "drive", "N·m")
	positive(car.drive, "top_speed", "drive", "km/h")
	only(car.brake, FIELDS.brake, "brake")
	positive(car.brake, "torque", "brake", "N·m")
	positive(car.brake, "reverse_speed", "brake", "km/h")
	only(car.fuel, FIELDS.fuel, "fuel")
	positive(car.fuel, "capacity", "fuel", "L")
	at_least_zero(car.fuel, "burn", "fuel")
	at_least_zero(car.fuel, "idle", "fuel")
	return car
end

return V
