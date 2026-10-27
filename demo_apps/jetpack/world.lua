local C = require("theme")
local F = require("format")
local Physics = require("physics")
local Weapons = require("weapons")
local Spawns = require("spawns")
local Bots = require("bots")
local M = {}
M.max_bots = 4
M.__index = M

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- `map` is a validated map and `guns` a validated gun list (format.lua); the baked ones when left out.
function M.new(map, guns)
	local m = setmetatable({}, M)
	m.map = map or F.find(F.maps(require("baked/maps")), "yard")
	m.guns = guns or F.guns(require("baked/guns"))
	m.gun_by_id = {}
	for _, g in ipairs(m.guns) do m.gun_by_id[g.id] = g end
	m.rng = Weapons.seed
	m.solids = Physics.solids(m.map)
	m.clock, m.last, m.debt, m.time = 0, nil, 0, 0
	m.state, m.kills, m.deaths, m.fired, m.enemy_fired = "Ready", 0, 0, 0, 0
	Spawns.populate(m, M.max_bots)
	m.shots, m.effects = {}, {}
	m:clear_input()
	return m
end

function M:clear_input()
	self.input = { move = 0, thrust = false, fire = false, reload = false, swap = false, pickup = false, aim = 0, pointer = nil }
end
function M:start() self.state = "Playing"; self:clear_input(); self.last = nil; self.debt = 0 end
function M:pause() self.state = "Paused"; self:clear_input(); self.debt = 0 end
function M:resume() self.state = "Playing"; self:clear_input(); self.last = nil; self.debt = 0 end

-- ammo, mag size, reload seconds left (nil when not reloading) of a soldier (the player by default).
function M:ammo(a) return Weapons.ammo(self, a or self.player) end
function M:effect(x, y, kind) Weapons.effect(self, x, y, kind) end
function M:shoot(a, team) Weapons.shoot(self, a, team) end
function M:damage(a, amount) Weapons.damage(self, a, amount) end

-- Guns (step 5): the one on the back, the loose gun in reach, swapping and taking.
function M:swap(a) return Spawns.swap(self, a or self.player) end
function M:pickup(a) return Spawns.pickup(self, a or self.player) end
function M:nearby(a) return Spawns.nearby(self, a or self.player) end
-- The gun table and rounds of the back slot (nil without one; the rounds of a never-used gun are its full mag).
function M:other(a)
	a = a or self.player
	local g = a.other and self.gun_by_id[a.other]
	if not g then return nil end
	return g, a.other_ammo or g.stats.mag_size
end

-- The player's control for this step: horizontal, thrust (0..1), fire, reload. Sets aim, spends fuel.
-- `input.thrust` is a boolean or a number 0..1 (`true` = 1); force and fuel drain scale with it.
local function player_control(self, a, dt)
	local input = self.input
	local want = input.thrust
	want = type(want) == "number" and clamp(want, 0, 1) or (want and 1 or 0)
	if a.fuel <= 0 then a.exhausted = true end
	if want <= 0 then a.exhausted = false end
	local thrust = (not a.exhausted and a.fuel > 0) and want or 0
	a.aim = input.aim
	if input.pointer then
		a.aim = math.atan2(input.pointer[2] - a.y + 4, input.pointer[1] - a.x)
	end
	a.fuel = clamp(a.fuel + (thrust > 0 and -32 * thrust or (want > 0 and 0 or 23)) * dt, 0, 100)
	local reload, swap, pickup = input.reload, input.swap, input.pickup
	input.reload, input.swap, input.pickup = false, false, false
	return input.move, thrust, input.fire, reload, swap, pickup
end

function M:step(dt)
	self.time = self.time + dt
	local p = self.player
	local all = {p}
	for _, b in ipairs(self.bots) do table.insert(all, b) end
	for index, a in ipairs(all) do
		a.shield = math.max(0, a.shield - dt)
		a.flash = math.max(0, (a.flash or 0) - dt)
		a.swap_left = math.max(0, (a.swap_left or 0) - dt)
		if Spawns.tick(self, a, index, dt) then
			local horizontal, thrust, fire, reload, swap, pickup
			if index == 1 then horizontal, thrust, fire, reload, swap, pickup = player_control(self, a, dt)
			else
				horizontal, thrust, fire = Bots.plan(self, a, index)
				fire = Bots.trigger(self, a, index, fire, dt)
			end
			local amount = thrust == true and 1 or (type(thrust) == "number" and clamp(thrust, 0, 1) or 0)
			a.vx = a.vx + (horizontal * C.speed - a.vx) * math.min(1, dt * 12)
			a.vy = clamp(a.vy + (C.gravity - amount * C.thrust) * dt, -C.max_rise, C.max_fall)
			a.thrusting = amount > 0
			Physics.move(a, dt, self.map, self.solids)
			Weapons.tick(self, a, dt)
			if reload then Weapons.reload(self, a) end
			if swap then Spawns.swap(self, a) end
			if pickup then Spawns.pickup(self, a) end
			if index > 1 then
				local near = Spawns.nearby(self, a)
				if near and Bots.better(self, a, near) then Spawns.pickup(self, a, near) end
			end
			if Weapons.pull(self, a, index == 1 and "player" or "bot", fire) and index > 1 then
				Bots.fired(self, a, index)
			end
		end
	end
	Spawns.update_pickups(self, dt)
	Weapons.update(self, dt)
end

function M:advance(elapsed)
	self.clock = elapsed
	local delta = self.last and math.max(0, elapsed - self.last) or 0
	self.last = elapsed
	if self.state ~= "Playing" then self.debt = 0; return end
	self.debt = math.min(self.debt + delta, 8 * C.step)
	while self.debt + 1e-10 >= C.step do
		self:step(C.step)
		self.debt = math.max(0, self.debt - C.step)
	end
end

return M
