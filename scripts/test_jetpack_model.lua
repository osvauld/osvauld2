-- Plain Lua can exercise the pure Luau model without a renderer; bridge smoke proves the app.
math.clamp = math.clamp or function(v, lo, hi) return math.max(lo, math.min(hi, v)) end
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local Model = require("model")
local C = require("theme")
local function check(ok, why) assert(ok, why) end
local function step(m, frames)
	for _ = 1, frames do m:advance(m.clock + 1 / 60) end
end
local function fresh()
	local m = Model.new()
	m:start()
	m:advance(0)
	return m
end

local m = fresh()
local y = m.player.y
m.input.thrust = true
step(m, 30)
check(m.player.y < y - 40 and m.player.fuel < 100, "thrust did not lift/consume fuel")
check(y - m.player.y < 65, "half-second thrust is too abrupt")
check(m.player.vy >= -190, "ascent speed is too fast")
m.input.thrust = false
step(m, 90)
check(m.player.y <= (450 - C.ry) + 0.01, "fell through the floor")
check(m.player.fuel > 50, "fuel did not recover")

-- Proportional thrust: input.thrust is a boolean or 0..1; force and fuel drain scale with it.
local function burn(amount)
	local t = fresh()
	for _, b in ipairs(t.bots) do b.hp, b.respawn = 0, 1000 end
	local y0 = t.player.y
	t.input.thrust = amount
	step(t, 30)
	return y0 - t.player.y, 100 - t.player.fuel
end
local full_rise, full_burn = burn(true)
local num_rise, num_burn = burn(1)
local three_rise, three_burn = burn(0.75)
local half_rise, half_burn = burn(0.5)
local none_rise, none_burn = burn(0)
check(math.abs(full_rise - num_rise) < 1e-9 and math.abs(full_burn - num_burn) < 1e-9, "thrust 1 differs from true")
check(three_rise > 0 and three_rise < full_rise, "three-quarter thrust must rise, but slower than full")
check(half_rise <= 0.01, "half thrust cannot beat gravity, so the pilot stays down")
check(none_rise <= 0.01 and none_burn <= 0, "no thrust neither rises nor drains")
check(math.abs(half_burn - full_burn / 2) < 0.01, "half thrust drains half the fuel")
check(math.abs(three_burn - full_burn * 0.75) < 0.01, "three-quarter thrust drains three quarters")

m = fresh()
for _, b in ipairs(m.bots) do b.hp, b.respawn = 0, 1000 end
m.input.thrust = true
step(m, 400)
for _ = 1, 60 do
	step(m, 1)
	check(not m.player.thrusting, "empty jetpack pulsed thrust without releasing the control")
end
check(m.player.y >= (450 - C.ry) - 0.01, "empty jetpack did not land")
m.input.thrust = false
step(m, 60)
check(m.player.fuel > 20, "released jetpack did not refill")
m.input.thrust = true
step(m, 20)
check(m.player.y < (450 - C.ry) - 20, "refilled jetpack did not restart")

-- Swept bullets hit cover before actors, even when they cross both in one step.
m = fresh()
m.bots[1].x, m.bots[1].y = 520, 420
m.shots = {{ x = 400, y = 420, vx = 20000, vy = 0, team = "player", life = 1 }}
m:step(1 / 120)
check(m.bots[1].hp == 60 and #m.shots == 0, "shot passed through cover")

m = fresh()
m.bots[1].x, m.bots[1].y = 300, 150
for _ = 1, 3 do
	m.shots = {{x = 250, y = 150, vx = 10000, vy = 0, team = "player", life = 1}}
	m:step(1 / 120)
end
check(m.kills == 1 and m.bots[1].hp == 0, "bot kill was not scored once")
step(m, 180)
check(m.bots[1].hp == 60, "bot did not respawn")

m = fresh()
m.player.x, m.player.y, m.player.vx = 430, 420, 300
m.input.move = 1
step(m, 20)
check(m.player.x <= 456 - C.rx + 0.01, "player crossed the central cover")

m = fresh()
step(m, 2400)
check(m.deaths > 0, "bots did not damage/defeat idle player")

m = fresh()
m.input.move, m.input.fire, m.input.thrust = 1, true, true
m:pause()
local x, time = m.player.x, m.time
step(m, 100)
check(m.player.x == x and m.time == time, "pause advanced simulation")
m:resume()
check(not m.input.fire and not m.input.thrust and m.input.move == 0, "resume retained input")
local time = m.time
m:advance(m.clock)
check(m.time == time, "same-clock observation advanced simulation")
m:advance(m.clock + 100)
check(m.time - time <= 8 / 120 + 0.00001, "stall debt was unbounded")

local a, b = fresh(), fresh()
for _, v in ipairs({a, b}) do v.input.move = 1; v.input.thrust = true end
for i = 1, 60 do a:advance(i / 60) end
for i = 1, 120 do b:advance(i / 120) end
check(math.abs(a.player.x - b.player.x) < 0.001 and math.abs(a.player.y - b.player.y) < 0.001,
	"fixed-step simulation depends on presentation rate")
print("jetpack model: flight, collision, cover, combat, respawn, pause and clocks passed")

-- The camera is pure and has its own test: scripts/test_jetpack_camera.lua.
