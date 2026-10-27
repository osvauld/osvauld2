-- Direct tests of the Jetpack Arena rules modules under stock Lua (no gfx/ui).
math.clamp = math.clamp or function(v, lo, hi) return math.max(lo, math.min(hi, v)) end
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local World = require("world")
local Physics = require("physics")
local Weapons = require("weapons")
local Spawns = require("spawns")
local Bots = require("bots")
local C = require("theme")
local function check(ok, why) assert(ok, why) end
local function fresh() local w = World.new(); w:start(); w:advance(0); return w end

-- physics: a sweep hits the rect at the right fraction, misses past it; walls bound the map.
local rect = {x = 100, y = 0, w = 50, h = 50}
local t = Physics.segment(0, 25, 200, 0, rect, 0, 0)
check(t and math.abs(t - 0.5) < 1e-9, "sweep did not hit the rect at its near face")
check(Physics.segment(0, 100, 200, 0, rect, 0, 0) == nil, "sweep hit a rect it passes below")
check(Physics.blocked({rect}, 0, 25, 200, 25) and not Physics.blocked({rect}, 0, 25, 90, 25),
	"blocked disagreed with the segment")
local w = fresh()
check(#Physics.solids(w.map) == 4 + #w.map.platforms, "solids are four walls plus platforms")
local a = {x = 200, y = 100, vx = 0, vy = 50000, grounded = false}
Physics.move(a, 1 / 60, w.map, w.solids)
check(a.y >= 100 and a.y <= w.map.h - C.ry + 0.01 and a.vy == 0 and a.grounded, "move did not stop a fall at the floor")

-- weapons: a shot damages its target, a shield absorbs it.
w = fresh()
local bot = w.bots[1]
bot.x, bot.y = 300, 150
w.shots = {{x = 250, y = 150, vx = 10000, vy = 0, team = "player", life = 1}}
Weapons.update(w, 1 / 120)
check(bot.hp == 40 and #w.shots == 0, "player shot did not deal 20 damage")
bot.shield = 1
w.shots = {{x = 250, y = 150, vx = 10000, vy = 0, team = "player", life = 1}}
Weapons.update(w, 1 / 120)
check(bot.hp == 40, "shield did not absorb the shot")
w.player.aim = 0
Weapons.shoot(w, w.player, "player")
check(#w.shots == 1 and w.fired == 1, "shoot did not spawn a counted shot")
Weapons.damage(w, bot, 1000); bot.shield = 0
Weapons.damage(w, bot, 1000)
check(bot.hp == 0 and w.kills == 1 and bot.respawn > 0, "lethal damage did not score a kill")
for _ = 1, 100 do Weapons.effect(w, 0, 0, "hit") end
check(#w.effects <= 40, "effects list is unbounded")

-- spawns: a dead soldier waits out the timer, then returns at its spawn with full hp and a shield.
w = fresh()
bot = w.bots[1]
bot.hp, bot.respawn, bot.x, bot.y = 0, 1, 0, 0
check(not Spawns.tick(w, bot, 2, 0.5) and bot.hp == 0, "respawned before the timer")
check(not Spawns.tick(w, bot, 2, 0.6), "respawn tick reports a live soldier")
check(bot.hp == 60 and bot.shield > 0 and bot.x == bot.spawn.x and bot.y == bot.spawn.y,
	"bot did not respawn with full hp and shield")
check(Spawns.tick(w, bot, 2, 0.01), "live soldier ticked as dead")
w.player.hp, w.player.respawn = 0, 0
Spawns.tick(w, w.player, 1, 0.01)
check(w.player.hp == 100, "player did not respawn with 100 hp")

-- bots: a clear line fires, cover in between does not.
w = fresh()
bot = w.bots[1]
w.player.x, w.player.y, bot.x, bot.y = 250, 150, 300, 150
local _, _, fire = Bots.plan(w, bot, 2)
check(fire, "bot with a clear line did not fire")
w.player.x, w.player.y, bot.x, bot.y = 400, 420, 520, 420
_, _, fire = Bots.plan(w, bot, 2)
check(not fire, "bot fired through cover")
print("jetpack modules: physics, weapons, spawns and bots passed")
