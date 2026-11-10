-- Gun stats in the model (design/jetpack-militia.md step 4, tests 4 and 6): every baked gun's shots to
-- kill, rate, mag, reload, semi/auto, pellets and cone, falloff, rocket splash and knockback, bot rate.
math.clamp = math.clamp or function(v, lo, hi) return math.max(lo, math.min(hi, v)) end
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
package.path = "demo_apps/jetpack/?.lua;scripts/?.lua;" .. package.path
local World = require("jetpack_solo")
local Weapons = require("weapons")
local Bots = require("bots")
local F = require("format")
local C = require("theme")
local DT = C.step
local function check(ok, why) assert(ok, why) end
local function near(a, b, tol, why) assert(math.abs(a - b) <= tol, why .. " (" .. tostring(a) .. " vs " .. tostring(b) .. ")") end

local baked = F.guns(require("baked/guns"))
local by_id = {}
for _, g in ipairs(baked) do by_id[g.id] = g end
check(#baked == 6, "six baked guns")

-- A flat open arena, no cover: the player at (100, 500), the first bot as the target, other bots out.
local function arena(guns, platforms)
	local map = { w = 6000, h = 1000, platforms = platforms or {},
		spawns = { { team = "player", x = 100, y = 500 }, { team = "bot", x = 400, y = 500 }, { team = "bot", x = 500, y = 500 } } }
	local w = World.new(map, guns)
	w:start(); w:advance(0)
	for i, b in ipairs(w.bots) do if i > 1 then b.hp, b.respawn, b.dropped = 0, 1e9, true end end -- step 5: a dead bot would drop a gun for the others
	return w, w.player, w.bots[1]
end
-- A copy of the guns with one gun's stats overridden.
local function tuned(id, over)
	local out = {}
	for _, g in ipairs(baked) do
		local c = g
		if g.id == id then
			c = {}
			for k, v in pairs(g) do c[k] = v end
			c.stats = {}
			for k, v in pairs(g.stats) do c.stats[k] = v end
			for k, v in pairs(over) do c.stats[k] = v end
		end
		table.insert(out, c)
	end
	return out
end
local function place(a, x, y) a.x, a.y, a.vx, a.vy, a.shield = x, y, 0, 0, 0 end
-- The weapons half of Model.step, with nothing moving.
local function tick(w, trigger, dt)
	local p = w.player
	Weapons.tick(w, p, dt or DT)
	Weapons.pull(w, p, "player", trigger)
	Weapons.update(w, dt or DT)
end

-- 1. Defaults: the baked rifle, a custom list used when given.
local w = arena()
check(w.player.gun == "rifle" and #w.guns == 6, "default guns are the baked six, the player holds the rifle")
w = arena(tuned("rifle", { damage = 33 }))
check(Weapons.gun(w, w.player).stats.damage == 33, "World.new(map, guns) uses the given guns")
w = arena()
local ammo, mag, reload_left = w:ammo(w.player)
check(ammo == 30 and mag == 30 and not reload_left, "a fresh rifle holds a full mag")

-- 2. Shots to kill a 100-hp target at close range (40 units), time to kill, per baked gun.
local stk = { pistol = 5, smg = 10, rifle = 7, shotgun = 1, sniper = 2, rocket = 1 }
for _, gun in ipairs(baked) do
	local s = gun.stats
	w = arena()
	local p, t = w.player, w.bots[1]
	p.gun, p.aim = gun.id, 0
	place(p, 100, 500); place(t, 140, 500); t.hp = 100
	local n, first_t, kill_t, time, held, just = 0, nil, nil, 0, true, false
	local last_fired = 0
	for _ = 1, math.floor(30 / DT) do
		-- semi guns need a fresh press per shot: release for a step after each one
		held = s.auto or not just
		local before = w.fired
		tick(w, held)
		just = w.fired > before
		time = time + DT
		if w.fired > last_fired then
			n = n + 1; last_fired = w.fired
			first_t = first_t or time
		end
		if t.hp <= 0 then kill_t = time; break end
	end
	check(t.hp <= 0, gun.id .. " never killed the target")
	check(n == stk[gun.id], gun.id .. " shots to kill: " .. n .. " vs " .. stk[gun.id])
	near(kill_t - first_t, (stk[gun.id] - 1) * s.fire_interval, 0.06, gun.id .. " time to kill")
	-- the table's STK follows from the stats
	local per = s.damage * s.pellets
	if s.kind == "rocket" then per = s.damage * 2 - 40 end
	check(math.ceil(100 / per) <= stk[gun.id] + 1, gun.id .. " stats disagree with the STK table")
end

-- 3. Rate, mag, reload, per gun: a full mag at the interval, then RELOADING for reload_time, then full.
for _, gun in ipairs(baked) do
	local s = gun.stats
	w = arena()
	local p, t = w.player, w.bots[1]
	p.gun = gun.id
	place(p, 100, 500); place(t, 5000, 100) -- out of the way
	local shots, times, time, held, just = 0, {}, 0, true, false
	local last = 0
	local empty_at
	for _ = 1, math.floor(40 / DT) do
		held = (s.auto or not just) and not empty_at
		local before = w.fired
		tick(w, held)
		just = w.fired > before
		time = time + DT
		if w.fired > last then last = w.fired; table.insert(times, time) end
		if p.ammo == 0 and not empty_at then empty_at = time end
		if #times == s.mag_size and p.reload_left == nil and p.ammo == s.mag_size then break end
		if empty_at and p.reload_left == nil then break end
	end
	check(#times == s.mag_size, gun.id .. " fired " .. #times .. " not a mag of " .. s.mag_size)
	near(times[#times] - times[1], (s.mag_size - 1) * s.fire_interval, 2 * DT, gun.id .. " rate over the mag")
	check(empty_at ~= nil, gun.id .. " mag never emptied")
	near(time - times[#times], s.reload_time, 2 * DT, gun.id .. " reload time")
	check(p.ammo == s.mag_size and not p.reload_left, gun.id .. " mag did not refill")
	-- no shot while reloading
	w = arena()
	p = w.player; p.gun = gun.id; place(p, 100, 500); place(w.bots[1], 5000, 100)
	w:ammo(p); p.ammo = 1
	tick(w, true)
	check(p.ammo == 0 and p.reload_left, gun.id .. " did not auto reload when empty")
	local fired = w.fired
	for _ = 1, math.floor(s.reload_time / DT) - 3 do tick(w, true) end
	check(w.fired == fired, gun.id .. " fired during the reload")
end

-- 4. Semi fires once per press; auto fires for as long as it is held.
w = arena(); place(w.player, 100, 500); place(w.bots[1], 5000, 100)
w.player.gun = "pistol"
for _ = 1, math.floor(2 / DT) do tick(w, true) end
check(w.fired == 1, "a held semi trigger fired " .. w.fired .. " times")
tick(w, false); tick(w, true) -- long after the interval: a new press fires
check(w.fired == 2, "a fresh press fired")
tick(w, false); tick(w, true)
check(w.fired == 2, "a re-press inside the interval does not fire at once")
for _ = 1, math.floor(0.4 / DT) do tick(w, true) end
check(w.fired == 3, "the re-press fired once the interval ended, and only once")
w = arena(); place(w.player, 100, 500); place(w.bots[1], 5000, 100)
w.player.gun = "smg"
for _ = 1, math.floor(1 / DT) do tick(w, true) end
near(w.fired, 1 / 0.07, 1.5, "a held auto trigger fires at its interval")

-- 5. Shotgun: eight pellets per pull, inside the cone, deterministic, not all alike.
w = arena(); place(w.player, 100, 500); place(w.bots[1], 5000, 100)
w.player.gun, w.player.aim = "shotgun", 0.3
Weapons.shoot(w, w.player, "player")
check(#w.shots == 8 and w.fired == 1, "shotgun spawns 8 pellets for one pull")
local spread, angles = by_id.shotgun.stats.spread, {}
local distinct = {}
for _, b in ipairs(w.shots) do
	local ang = math.atan2(b.vy, b.vx)
	check(math.abs(ang - 0.3) <= spread + 1e-9, "pellet outside the cone")
	near(math.sqrt(b.vx ^ 2 + b.vy ^ 2), by_id.shotgun.stats.shot_speed, 1e-6, "pellet speed")
	check(b.damage == 14, "pellet damage is the gun's damage")
	table.insert(angles, ang); distinct[string.format("%.9f", ang)] = true
end
local n_distinct = 0
for _ in pairs(distinct) do n_distinct = n_distinct + 1 end
check(n_distinct >= 6, "pellets are not independent")
local w2 = arena(); place(w2.player, 100, 500); w2.player.gun, w2.player.aim = "shotgun", 0.3
Weapons.shoot(w2, w2.player, "player")
for i, b in ipairs(w2.shots) do near(math.atan2(b.vy, b.vx), angles[i], 1e-12, "same world seed, same pellets") end
-- a perfectly accurate gun sends everything along the aim
w = arena(tuned("sniper", { spread = 0 })); place(w.player, 100, 500); w.player.gun, w.player.aim = "sniper", 0.5
Weapons.shoot(w, w.player, "player")
near(math.atan2(w.shots[1].vy, w.shots[1].vx), 0.5, 1e-12, "zero spread is exact")
-- the cone is used: an SMG's shots do not all go straight
w = arena(); place(w.player, 100, 500); w.player.gun = "smg"
local min_a, max_a = 9, -9
for _ = 1, 40 do
	w.shots = {}; Weapons.shoot(w, w.player, "player")
	local ang = math.atan2(w.shots[1].vy, w.shots[1].vx)
	min_a, max_a = math.min(min_a, ang), math.max(max_a, ang)
end
check(min_a >= -0.09 - 1e-9 and max_a <= 0.09 + 1e-9 and max_a - min_a > 0.1, "smg cone is used and bounded")

-- 6. Range falloff: full damage to `range`, linear to 40% at 1.5 range, gone after. Measured to the
-- target's near face. Rifle without spread.
local function rifle_hit(distance)
	w = arena(tuned("rifle", { spread = 0 }))
	local p, t = w.player, w.bots[1]
	place(p, 100, 500); place(t, 100 + distance + C.rx, 500); t.hp = 100
	local mag = 0
	w.shots = {}; Weapons.shoot(w, p, "player")
	for _ = 1, math.floor(3 / DT) do Weapons.update(w, DT); if #w.shots == 0 then break end end
	check(#w.shots == 0, "shot outlived its range")
	return 100 - t.hp
end
local R = by_id.rifle.stats.range
check(rifle_hit(R * 0.5) == 16, "full damage inside range")
check(rifle_hit(R - 1) == 16, "full damage at range")
near(rifle_hit(R * 1.25), math.floor(16 * (1 - 0.6 * 0.5) + 0.5), 1, "half way through falloff")
check(rifle_hit(R * 1.25) < 16 and rifle_hit(R * 1.45) < rifle_hit(R * 1.25), "damage falls with distance")
check(rifle_hit(R * 1.6) == 0, "no damage past the falloff")
near(Weapons.falloff(R * 1.5, R), 0.4, 1e-9, "falloff floor")
near(Weapons.falloff(R, R), 1, 1e-9, "falloff starts at range")

-- 7. Rocket: slow, explodes on hit / solid / range end; splash linear; knockback; self-splash.
local rs = by_id.rocket.stats
local function dmg_at(distance)
	w = arena(); local t = w.bots[1]
	place(w.player, 3000, 100) -- far away
	place(t, 1000 + distance, 500); t.hp = 100
	Weapons.explode(w, 1000, 500, rs.splash, rs.damage, w.player.id, "player")
	return 100 - t.hp, t
end
check(dmg_at(0) == 80, "splash at the centre is the gun damage")
near(dmg_at(rs.splash / 2), 40, 1, "splash at r/2")
check(dmg_at(rs.splash) == 0, "splash at r is nothing")
check(dmg_at(rs.splash + 20) == 0, "splash beyond r is nothing")
check(dmg_at(rs.splash * 0.25) > dmg_at(rs.splash * 0.75), "splash falls linearly with distance")
-- knockback: away from the centre, weaker farther out
w = arena(); place(w.player, 3000, 100)
local t = w.bots[1]; place(t, 960, 540); t.hp = 100
Weapons.explode(w, 1000, 500, rs.splash, rs.damage, w.player.id, "player")
check(t.vx < 0 and t.vy > 0, "knockback pushes away from the blast")
local near_push = math.sqrt(t.vx ^ 2 + t.vy ^ 2)
place(t, 1000 + 20, 500 - 5); t.hp = 100
Weapons.explode(w, 1000, 500, rs.splash, rs.damage, w.player.id, "player")
check(t.vx > 0 and t.vy < 0, "knockback direction follows the side of the blast")
place(t, 1000 + 60, 500); t.hp = 100
Weapons.explode(w, 1000, 500, rs.splash, rs.damage, w.player.id, "player")
check(math.sqrt(t.vx ^ 2 + t.vy ^ 2) < near_push, "farther targets are pushed less")
-- self-splash is allowed; a bot's rocket spares other bots
w = arena(); local p, b = w.player, w.bots[1]
place(p, 1000, 500); p.hp = 100; place(b, 1000, 500); b.hp = 100
Weapons.explode(w, 1000, 500, rs.splash, rs.damage, p.id, "player")
check(p.hp == 20 and b.hp == 20, "the owner takes their own splash")
w.bots[2].hp, w.bots[2].respawn = 100, 0
if w.bots[2] then
	place(p, 3000, 100); p.hp = 100; place(b, 1000, 500); b.hp = 100; place(w.bots[2], 1000, 500)
	Weapons.explode(w, 1000, 500, rs.splash, rs.damage, b.id, "bot")
	check(w.bots[2].hp == 100 and b.hp == 20, "bot splash hurts its owner, not its team")
end
-- through the real shot path: slow shell, explodes against a wall, splash hits the neighbour
w = arena(nil, { { x = 1500, y = 0, w = 100, h = 1000 } })
p, t = w.player, w.bots[1]
place(p, 1000, 500); p.gun, p.aim = "rocket", 0
place(t, 1480, 540); t.hp = 100
tick(w, true)
check(#w.shots == 1 and w.shots[1].rocket, "rocket spawns a shell")
near(w.shots[1].vx, rs.shot_speed * math.cos(w.shots[1].vy / rs.shot_speed), 1, "rocket is slow")
check(rs.shot_speed < 600, "rocket shell is slower than bullets")
for _ = 1, math.floor(3 / DT) do tick(w, false); if #w.shots == 0 then break end end
check(#w.shots == 0, "rocket exploded on the wall")
check(t.hp < 100 and t.hp >= 20, "wall blast splashed the target (hp " .. t.hp .. ")")
check(p.hp == 100, "the owner was far enough")
-- direct hit + splash
w = arena(); p, t = w.player, w.bots[1]
place(p, 1000, 500); p.gun = "rocket"; place(t, 1200, 500); t.hp = 100
tick(w, true)
for _ = 1, math.floor(3 / DT) do tick(w, false); if #w.shots == 0 then break end end
check(t.hp == 0, "a direct rocket hit plus its splash kills")
-- range end: no wall, no target; it flies `range`, then bursts
w = arena(); p, t = w.player, w.bots[1]
place(p, 100, 500); p.gun = "rocket"; place(t, 5000, 100)
tick(w, true)
local bursts = 0
local flown = 0
while #w.shots > 0 and flown < 600 do
	tick(w, false); flown = flown + 1
	for _, e in ipairs(w.effects) do if e.kind == "burst" and e.age <= DT + 1e-9 then bursts = bursts + 1 end end
end
near(flown * DT, rs.range / rs.shot_speed, 3 * DT, "rocket flew its range then exploded")
check(bursts >= 1, "rocket range end made an explosion")
-- the rocket's splash at the end of its range reaches a nearby target
w = arena(); p, t = w.player, w.bots[1]
place(p, 100, 500); p.gun = "rocket"; place(t, 100 + rs.range, 500 + 60); t.hp = 100
tick(w, true)
for _ = 1, math.floor(4 / DT) do tick(w, false); if #w.shots == 0 then break end end
check(t.hp < 100, "range-end blast hurt a target near the burst")

-- 8. Reload input and ammo through the model (Model:step).
w = arena(); p = w.player
place(p, 100, 500); place(w.bots[1], 5000, 100)
w.bots[1].hp = 0; w.bots[1].respawn = 1e9
w.input.reload = true
w:step(DT)
check(not p.reload_left, "reload does nothing with a full mag")
p.ammo = 10
w.input.reload = true
w:step(DT)
check(p.reload_left ~= nil and not w.input.reload, "manual reload starts and is consumed")
for _ = 1, math.floor(by_id.rifle.stats.reload_time / DT) + 2 do w:step(DT) end
check(p.ammo == 30 and not p.reload_left, "manual reload refills the mag")
-- fire through input
w.input.fire = true
for _ = 1, 60 do w:step(DT) end
check(w.fired > 3 and p.ammo < 30, "player input.fire fires the rifle")
-- changing the gun resets the mag to the new gun's
p.gun = "pistol"
local a2, m2 = w:ammo(p)
check(a2 == 12 and m2 == 12, "a new gun starts with its own full mag")
-- death resets ammo
p.ammo = 3
Weapons.damage(w, p, 1000)
for _ = 1, 400 do w.input.fire = false; w:step(DT) end
check(p.hp > 0 and p.gun == "rifle" and w:ammo(p) == 30, "respawn re-arms with the loadout, full mag") -- step 5: the loadout, not the last gun

-- 9. A bot with a sniper fires at the sniper's interval (after its reaction delay).
w = arena(); p = w.player
p.shield = 1e9
place(p, 100, 500)
local bot = w.bots[1]
bot.gun = "sniper"; place(bot, 500, 500); bot.shield = 0
local times, last, time = {}, w.enemy_fired, 0
for _ = 1, math.floor(9 / DT) do
	w:step(DT); time = time + DT
	if w.enemy_fired > last then last = w.enemy_fired; table.insert(times, time) end
end
check(#times >= 4, "sniper bot fired " .. #times)
near(times[1], Bots.reaction(2), 3 * DT, "bot waits its reaction delay before the first shot")
for i = 2, 4 do near(times[i] - times[i - 1], by_id.sniper.stats.fire_interval, 2 * DT, "bot sniper interval") end
-- and a rifle bot shoots in bursts, not a stream
w = arena(); w.player.shield = 1e9; place(w.player, 100, 500)
bot = w.bots[1]; place(bot, 500, 500)
times, last, time = {}, w.enemy_fired, 0
for _ = 1, math.floor(6 / DT) do
	w:step(DT); time = time + DT
	if w.enemy_fired > last then last = w.enemy_fired; table.insert(times, time) end
end
local longest = 0
for i = 2, #times do longest = math.max(longest, times[i] - times[i - 1]) end
check(#times >= 6 and longest > 0.5, "rifle bot pauses between bursts")
check(not Bots.interval, "the constant bot interval is gone")

print("jetpack weapons: stk, rate, mag, reload, semi/auto, pellets, falloff, rocket splash, knockback and bots passed")
