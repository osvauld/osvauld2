local C = require("theme")
local Physics = require("physics")
local Weapons = require("weapons")
local Spawns = require("spawns")
local Bots = require("bots")
local M = {}
M.max_bots = 4

-- A match's state is plain data (jetpack-mp.md step 7): the platform copies it before every input
-- frame and puts a copy back on a rewind, so nothing in it may be shared, cyclic or a function. What
-- never changes in a match — the map, the guns, the solids, the loadout seam — is the match's `setup`,
-- reached through the metatable (`w.map` reads it), never stored. `setmetatable(w, M.meta)` again after
-- any copy: a copy has no metatable.
local ctx = {}
M.meta = { __index = function(_, k)
	local v = M[k]
	if v ~= nil then return v end
	return ctx[k]
end }

-- The match's constants from its setup: `{ map, guns, pilots, loadout }`, validated content.
function M.bind(setup)
	local gun_by_id = {}
	for _, g in ipairs(setup.guns) do gun_by_id[g.id] = g end
	ctx = { map = setup.map, guns = setup.guns, pilots = setup.pilots, gun_by_id = gun_by_id,
		solids = Physics.solids(setup.map), arm = setup.loadout }
	return ctx
end

-- The constants of the match last made (`bind`): what the view draws with.
function M.context() return ctx end

-- `humans` players fly soldiers (`w.humans[i]` is player i's); bots fill the rest of the map's spawns.
function M.new(setup, humans)
	M.bind(setup)
	local m = setmetatable({}, M.meta)
	m.rng = Weapons.seed
	m.time = 0
	m.state = setup.state or "Ready"
	m.kills, m.deaths, m.fired, m.enemy_fired = 0, 0, 0, 0
	Spawns.populate(m, humans or 1, M.max_bots)
	m.shots, m.effects = {}, {}
	return m
end

-- The view's wrapper (main.lua) gives `player`; rules never ask for it.
-- ammo, mag size, reload seconds left (nil when not reloading) of a soldier (the player by default).
function M:ammo(a) return Weapons.ammo(self, a or self.player) end
function M:effect(x, y, kind) Weapons.effect(self, x, y, kind) end
function M:damage(a, amount) Weapons.damage(self, a, amount) end
function M:nearby(a) return Spawns.nearby(self, a or self.player) end
-- The gun table and rounds of the back slot (nil without one; the rounds of a never-used gun are its full mag).
function M:other(a)
	a = a or self.player
	local g = a.other and self.gun_by_id[a.other]
	if not g then return nil end
	return g, a.other_ammo or g.stats.mag_size
end

-- Every soldier, humans first: the order shots, splash and the view go by.
function M.all(w)
	local all = {}
	for _, h in ipairs(w.humans) do table.insert(all, h) end
	for _, b in ipairs(w.bots) do table.insert(all, b) end
	return all
end

-- A press counted in an input (`reload`, `swap`, `pickup`): the input carries how many there have been,
-- so a frame's input applied on several ticks — or again after a rewind — acts once per press.
local function pressed(a, input, key)
	local n = input[key] or 0
	local seen = a[key .. "_seen"] or 0
	if n > seen then a[key .. "_seen"] = n; return true end
	return false
end

-- A human's control for this step from its input: horizontal, thrust (0..1), fire, reload, swap, pickup.
-- Sets aim, spends fuel. `input.thrust` is a number 0..1 or false.
local function human_control(a, input, dt)
	local want = input.thrust
	want = type(want) == "number" and math.clamp(want, 0, 1) or (want and 1 or 0)
	if a.fuel <= 0 then a.exhausted = true end
	if want <= 0 then a.exhausted = false end
	local thrust = (not a.exhausted and a.fuel > 0) and want or 0
	a.aim = input.aim or a.aim
	a.fuel = math.clamp(a.fuel + (thrust > 0 and -32 * thrust or (want > 0 and 0 or 23)) * dt, 0, 100)
	return input.move or 0, thrust, input.fire or false,
		pressed(a, input, "reload"), pressed(a, input, "swap"), pressed(a, input, "pickup")
end

-- A player asked for the match to start, pause or resume (`input.mode`, counted like a press).
local function mode(w, a, input)
	local n = input.mode_n or 0
	if n > (a.mode_seen or 0) then
		a.mode_seen = n
		if input.mode == "Playing" or (input.mode == "Paused" and w.state == "Playing") then w.state = input.mode end
	end
end

local EMPTY = {}

-- One tick of the rules. `inputs[i]` is player i's input (nil before any arrived).
function M.step(w, dt, inputs)
	for i, h in ipairs(w.humans) do mode(w, h, inputs[i] or EMPTY) end
	if w.state ~= "Playing" then return end
	w.time = w.time + dt
	local humans = #w.humans
	for index, a in ipairs(M.all(w)) do
		a.shield = math.max(0, a.shield - dt)
		a.flash = math.max(0, (a.flash or 0) - dt)
		a.swap_left = math.max(0, (a.swap_left or 0) - dt)
		if Spawns.tick(w, a, dt) then
			local horizontal, thrust, fire, reload, swap, pickup
			local bot = index > humans
			if not bot then horizontal, thrust, fire, reload, swap, pickup = human_control(a, inputs[index] or EMPTY, dt)
			else
				-- Bots keep the staggering they had beside one player: the first bot is index 2.
				local slot = index - humans + 1
				horizontal, thrust, fire = Bots.plan(w, a, slot)
				fire = Bots.trigger(w, a, slot, fire, dt)
			end
			local amount = thrust == true and 1 or (type(thrust) == "number" and math.clamp(thrust, 0, 1) or 0)
			a.vx = a.vx + (horizontal * C.speed - a.vx) * math.min(1, dt * 12)
			a.vy = math.clamp(a.vy + (C.gravity - amount * C.thrust) * dt, -C.max_rise, C.max_fall)
			a.thrusting = amount > 0
			Physics.move(a, dt, w.map, w.solids)
			Weapons.tick(w, a, dt)
			if reload then Weapons.reload(w, a) end
			if swap then Spawns.swap(w, a) end
			if pickup then Spawns.pickup(w, a) end
			if bot then
				local near = Spawns.nearby(w, a)
				if near and Bots.better(w, a, near) then Spawns.pickup(w, a, near) end
			end
			if Weapons.pull(w, a, a.team, fire) and bot then
				Bots.fired(w, a, index - humans + 1)
			end
		end
	end
	Spawns.update_pickups(w, dt)
	Weapons.update(w, dt)
end

return M
