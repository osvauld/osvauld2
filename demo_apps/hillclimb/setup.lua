-- The car (vehicle.lua) as world bodies and joints, in world units. Takes only the car and where
-- to put it: no art, no input, no stage. The view adds each entity's `drawing` by id.
--
-- Bodies: `chassis` (carrying the driver's mass), `wheel1..n` on `axle1..n` wheel joints, and
-- `head` on a `neck` pin. All in group "car", meeting only "ground": a car does not hit itself.
local U = require("units")
local S = {}

local CAR = { group = "car", blocks = { "ground" } }
local HEAD_NOD = { -20, 20 } -- degrees either way on the neck

local function sum(a, b) return { a[1] + b[1], a[2] + b[2] } end

-- Box sizes (the drawing each entity needs), in units, by id.
function S.sizes(car)
	local out = { chassis = U.point(car.chassis.size) }
	for i, w in ipairs(car.wheels) do
		local d = 2 * U.length(w.radius)
		out["wheel" .. i] = { d, d }
	end
	local h = 2 * U.length(car.driver.head_radius)
	out.head = { h, h }
	return out
end

-- Entities (no drawings) and joints (no motors) for the car with its chassis box's top-left at `at`.
function S.build(car, at)
	local entities = {
		{
			id = "chassis", pos = { at[1], at[2] },
			collider = { rect = U.point(car.chassis.size) },
			loose = {
				mass = U.mass(car.chassis.mass + car.driver.mass),
				center = U.point(car.chassis.center),
				grip = 0.6, friction = 0, bounce = 0,
			},
			group = CAR.group, blocks = CAR.blocks,
		},
	}
	for i, w in ipairs(car.wheels) do
		local r = U.length(w.radius)
		local centre = sum(at, U.point(w.at))
		table.insert(entities, {
			id = "wheel" .. i, pos = { centre[1] - r, centre[2] - r },
			collider = { circle = r, at = { r, r } },
			loose = { mass = U.mass(w.mass), grip = w.grip, friction = 0, bounce = 0.05 },
			group = CAR.group, blocks = CAR.blocks,
		})
	end
	local hr = U.length(car.driver.head_radius)
	local neck = sum(at, U.point(car.driver.neck))
	table.insert(entities, {
		id = "head", pos = { neck[1] - hr, neck[2] - 2 * hr },
		collider = { circle = hr, at = { hr, hr } },
		loose = { mass = 4, grip = 0.5, friction = 0, bounce = 0 },
		group = CAR.group, blocks = CAR.blocks,
	})
	return { entities = entities, joints = S.joints(car, {}) }
end

-- The joints, each axle with `motors["axle" .. i]` if there is one. Described every view: a
-- changed motor is updated in place.
function S.joints(car, motors)
	local out = {}
	local s = car.suspension
	for i, w in ipairs(car.wheels) do
		local r = U.length(w.radius)
		table.insert(out, {
			id = "axle" .. i, kind = "wheel",
			a = "chassis", a_at = U.point(w.at),
			b = "wheel" .. i, b_at = { r, r },
			axis = { 0, 1 },
			stiffness = U.stiffness(s.stiffness), damping = U.damping(s.damping),
			travel = U.point(s.travel),
			motor = motors["axle" .. i],
		})
	end
	local hr = U.length(car.driver.head_radius)
	table.insert(out, {
		id = "neck", kind = "pin",
		a = "chassis", a_at = U.point(car.driver.neck),
		b = "head", b_at = { hr, 2 * hr },
		limits = HEAD_NOD,
	})
	return out
end

return S
