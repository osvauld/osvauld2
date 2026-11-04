-- One run's rules, on the clock the app gives it: fuel, distance, coins, and how it ends. Pure:
-- the app feeds it the chassis as `world:get` reads it, and the world's `on_hit` / `on_zone`.
local U = require("units")
local R = {}
R.__index = R

local UPSIDE = 110     -- degrees past which the car is on its roof
local ROOF_SECS = 2    -- that long on its roof ends the run
local STALL_SECS = 3   -- that long stopped with an empty tank ends it
local STOPPED = 20     -- units a second: as good as stopped

local PARTS = { chassis = true, head = true }

function R.new(car, stage)
	return setmetatable({
		car = car, stage = stage,
		fuel = car.fuel.capacity, coins = 0, best = 0, t = 0,
		taken = {}, over = nil, roof = 0, stall = 0,
		start_x = stage.start[1],
	}, R)
end

local function part_of_car(id)
	return PARTS[id] or (type(id) == "string" and id:sub(1, 5) == "wheel")
end

-- `pedals` as drive.lua takes them; `body` the chassis as `world:get` gives it, or nil.
function R:advance(dt, pedals, body)
	if self.over then return end
	self.t = self.t + dt
	local f = self.car.fuel
	self.fuel = math.max(0, self.fuel - (f.idle + f.burn * (pedals.gas or 0)) * dt)
	if not body then return end
	self.best = math.max(self.best, U.metres(body.pos[1] - self.start_x))
	local rot = (body.rot + 180) % 360 - 180
	self.roof = math.abs(rot) > UPSIDE and self.roof + dt or 0
	local still = math.abs(body.velocity[1]) < STOPPED and math.abs(body.velocity[2]) < STOPPED
	self.stall = (self.fuel <= 0 and still) and self.stall + dt or 0
	if body.pos[1] >= self.stage.finish_x then
		self.over = "finished"
	elseif self.roof >= ROOF_SECS then
		self.over = "flipped"
	elseif self.stall >= STALL_SECS then
		self.over = "out of fuel"
	end
end

-- The world's `on_hit`: the driver's head on the ground ends the run.
function R:hit(e)
	if not self.over and e.id == "head" and e.who == "ground" then
		self.over = "crashed"
	end
end

-- The world's `on_zone`: a part of the car entering a fuel can or a coin takes it.
function R:zone(e)
	if self.over or e.phase ~= "enter" or self.taken[e.id] or not part_of_car(e.who) then return end
	local item = self.stage.items_by_id[e.id]
	if not item then return end
	self.taken[e.id] = true
	if item.kind == "fuel" then
		self.fuel = self.car.fuel.capacity
	else
		self.coins = self.coins + 1
	end
end

-- The stage's items not yet taken, in order along the road.
function R:items()
	local out = {}
	for _, item in ipairs(self.stage.items) do
		if not self.taken[item.id] then table.insert(out, item) end
	end
	return out
end

return R
