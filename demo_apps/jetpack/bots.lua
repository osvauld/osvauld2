local C = require("theme")
local Physics = require("physics")
local Weapons = require("weapons")
local B = {}

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- Gun upgrades (step 5; step 8 refines): a bot wants a loose gun whose damage per second beats the one in
-- hand by 15 %, and leaves slow precision guns (zoom or rockets) alone. It walks to one in sight within
-- `seek_x` x `seek_y` units, and `world.lua` makes it take one in reach.
B.upgrade, B.seek_x, B.seek_y = 1.15, 220, 110
local function dps(s) return s.damage * s.pellets / s.fire_interval end
function B.better(w, a, l)
	local g = w.gun_by_id[l.gun]
	if not g or g.stats.kind == "rocket" or (g.stats.zooms[#g.stats.zooms] > 2) or (l.ammo and l.ammo <= 0) then return false end
	return dps(g.stats) >= B.upgrade * dps(Weapons.gun(w, a).stats)
end

-- The nearest loose gun this bot wants and can walk to, or nil.
function B.seek(w, a)
	local best, score = nil, math.huge
	for _, l in ipairs(w.loose) do
		local dx, dy = math.abs(l.x - a.x), math.abs(l.y - a.y)
		if dx <= B.seek_x and dy <= B.seek_y and dx < score and B.better(w, a, l)
			and not Physics.blocked(w.solids, a.x, a.y, l.x, l.y) then best, score = l, dx end
	end
	return best
end

-- One bot's decision for this step: horizontal in -1..1, thrust, fire. Sets `a.aim`.
-- `index` (>= 2) staggers the orbit side, wobble phase and fire rate.
function B.plan(w, a, index)
	local p, map, solid = w.player, w.map, w.solids
	local orbit = index == 2 and -210 or 210
	local target_x = clamp(p.x + orbit + math.sin(w.time * 0.8 + index) * 65, 55, map.w - 55)
	local target_y = clamp(p.y - 90 + math.sin(w.time * 1.1 + index) * 45, 70, map.h - 140)
	local gun = B.seek(w, a)
	if gun then target_x, target_y = gun.x, gun.y - 24 end
	-- Overfly cover when a low route is obstructed. Fly with bounded acceleration.
	-- Over it, measured from the player: 110 on the yard's ground, not a tall map's ceiling.
	if Physics.blocked(solid, a.x, a.y, target_x, a.y) then target_y = math.min(target_y, math.max(70, p.y - 321)) end
	local horizontal = clamp((target_x - a.x) * 0.014, -0.65, 0.65)
	local desired = clamp((target_y - a.y) * 4, -180, 180)
	local thrust = a.vy > desired
	local error = math.sin(w.time * 2.7 + index * 3) * 0.065
	a.aim = math.atan2(p.y - a.y, p.x - a.x) + error
	local fire = p.hp > 0 and not Physics.blocked(solid, a.x, a.y - 4, p.x, p.y)
	return horizontal, thrust, fire
end

-- Bots shoot at their gun's own interval, but with human-ish limits: a reaction delay after the line
-- clears, then bursts of about half a second of fire with a pause between, and softer hits
-- (`damage_scale` of the gun's damage) so a rifle bot is beatable. Step 8 (bots upgrade) tunes these.
B.damage_scale = 0.5
function B.reaction(index) return 0.3 + index * 0.1 end
function B.pause(index) return 0.9 + index * 0.2 end

-- Whether the bot pulls its trigger this step, given that `fire` (a clear line) holds. Also reloads a
-- half-empty gun while idle.
function B.trigger(w, a, index, fire, dt)
	if not fire then
		a.acquired = false
		local ammo, mag = Weapons.ammo(w, a)
		if ammo < mag / 2 then Weapons.reload(w, a) end
		return false
	end
	if not a.acquired then a.acquired, a.bot_wait, a.bot_burst = true, B.reaction(index), 0 end
	a.bot_wait = a.bot_wait - dt
	return a.bot_wait <= 0
end

-- Called after the bot's shot left: a burst ends, the pause starts.
function B.fired(w, a, index)
	a.bot_burst = a.bot_burst + 1
	local interval = Weapons.gun(w, a).stats.fire_interval
	if a.bot_burst >= math.max(1, math.floor(0.5 / interval + 0.5)) then
		a.bot_burst, a.bot_wait = 0, B.pause(index)
	end
end

return B
