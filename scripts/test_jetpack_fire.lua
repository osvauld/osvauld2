-- The firing timeline in plain Lua (fire_fx.lua): recoil along the barrel and muzzle climb that ease back,
-- the trigger finger, the muzzle flash per gun kind, and the casings (eject, fall, one bounce, fade, cap).
-- Run from the repo root with stock Lua 5.4.
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local F = require("format")
local FX = require("fire_fx")
local function check(ok, why) assert(ok, why) end
local function near(a, b, e) return math.abs(a - b) <= (e or 1e-9) end

local guns, gerr = F.guns(require("baked/guns"))
check(#guns >= 6 and not gerr, "baked guns must validate: " .. tostring(gerr))
local by = {}
for _, g in ipairs(guns) do by[g.id] = g end

-- Recoil: at the shot the gun is kicked back by its `recoil` (drawing units x scale), and is home by
-- min(fire_interval, 0.15), easing (never growing) on the way.
for _, g in ipairs(guns) do
	local t = math.min(g.stats.fire_interval, 0.15)
	check(near(FX.kick_time(g), t), g.id .. ": kick time")
	local back, climb = FX.recoil(g, 0)
	check(near(back, g.stats.recoil * g.scale), g.id .. ": recoil at the shot is the gun's recoil, got " .. back)
	check(climb > math.rad(1) - 1e-9 and climb <= math.rad(8) + 1e-9, g.id .. ": a few degrees of climb")
	local prev, pc = back, climb
	for i = 1, 40 do
		local b, c = FX.recoil(g, t * i / 40 - 1e-9)
		check(b <= prev + 1e-12 and c <= pc + 1e-12 and b >= 0 and c >= 0, g.id .. ": recoil only eases back")
		prev, pc = b, c
	end
	local b0, c0 = FX.recoil(g, t)
	check(b0 == 0 and c0 == 0, g.id .. ": back to 0 at the return time")
	check(select(1, FX.recoil(g, t * 3)) == 0, g.id .. ": stays home")
	check(select(1, FX.recoil(g, -0.01)) == 0, g.id .. ": no recoil before the shot")
	-- Trigger: squeezed at the shot, released by the return time.
	check(FX.trigger(g, 0) == 1, g.id .. ": trigger squeezed at the shot")
	check(FX.trigger(g, t) == 0 and FX.trigger(g, 5) == 0, g.id .. ": trigger released")
	local tp = 1
	for i = 1, 40 do
		local v = FX.trigger(g, t * i / 40 - 1e-9)
		check(v <= tp + 1e-12 and v >= 0 and v <= 1, g.id .. ": trigger releases monotonically")
		tp = v
	end
end
check(FX.kick_time(by.pistol) == 0.15 and FX.kick_time(by.smg) == 0.07, "kick time is min(interval, 0.15)")
for _, heavy in ipairs({ "shotgun", "sniper", "rocket" }) do
	for _, light in ipairs({ "pistol", "smg", "rifle" }) do
		check(FX.climb_degrees(by[heavy]) > FX.climb_degrees(by[light]), heavy .. " climbs more than " .. light)
	end
end
local st = FX.state(by.rifle, 0.01)
check(st.back > 0 and st.climb > 0 and st.trigger > 0 and st.flash, "state at 10 ms: everything on")
st = FX.state(by.rifle, nil)
check(st.back == 0 and st.climb == 0 and st.trigger == 0 and not st.flash, "never fired: nothing")

-- Flash: shows for its time and no longer; style by gun kind.
check(FX.flash_style(by.pistol) == "small" and FX.flash_style(by.smg) == "small", "pistol/SMG: small flash")
check(FX.flash_style(by.rifle) == "long" and FX.flash_style(by.sniper) == "long", "rifle/sniper: long flash")
check(FX.flash_style(by.shotgun) == "wide", "shotgun: wide flash")
check(FX.flash_style(by.rocket) == "rocket", "rocket: back-blast flash")
for _, g in ipairs(guns) do
	local d = FX.flash_time[FX.flash_style(g)]
	check(FX.flash(g, 0) and FX.flash(g, 0).t == 0, g.id .. ": flash at the shot")
	check(FX.flash(g, d * 0.99), g.id .. ": flash until its time")
	check(FX.flash(g, d) == nil and FX.flash(g, 1) == nil and FX.flash(g, -0.001) == nil, g.id .. ": flash gone")
	check(d <= FX.kick_time(g) + 1e-9 or g.id == "rocket", g.id .. ": flash no longer than the kick")
end

-- Casings: leave the eject point up and back (mirrored facing left), fall, bounce once, fade, expire.
local seq = { 0.5, 0.5, 0.5, 0.5 }
local function rnd() return 0.5 end
check(FX.eject(by.rocket, { 0, 0 }, 0, 1) == nil, "a rocket ejects nothing")
local shell = FX.eject(by.shotgun, { 0, 0 }, 0, 1, nil, rnd)
check(shell.color == "#d8392f" and shell.kind == "shell", "a shotgun ejects a red shell")
check(FX.eject(by.rifle, { 0, 0 }, 0, 1, nil, rnd).kind == "brass", "a rifle ejects brass")

local right = FX.eject(by.rifle, { 100, 50 }, 0, 1, nil, rnd)
local left = FX.eject(by.rifle, { 100, 50 }, math.pi, -1, nil, rnd)
check(right.x == 100 and right.y == 50 and left.x == 100 and left.y == 50, "starts at the eject point")
check(right.vx < 0 and right.vy < 0, "facing right: back (left) and up")
check(left.vx > 0 and left.vy < 0, "facing left: mirrored, back (right) and up")
check(near(right.vx, -left.vx, 1e-9) and near(right.vy, left.vy, 1e-9), "left is the mirror of right")
check(right.spin == -left.spin and right.spin ~= 0, "spins, the other way when mirrored")
local slow = FX.eject(by.rifle, { 0, 0 }, 0, 1, { vx = 100, vy = 0 }, rnd)
check(slow.vx > right.vx, "inherits the shooter's motion")
-- Aimed up 45 degrees, the pop is still up-and-back of the gun's frame
local aimed = FX.eject(by.rifle, { 0, 0 }, -math.pi / 4, 1, nil, rnd)
check(aimed.vy < 0, "aimed up it still goes up")

-- Fall onto a floor at y = 100: starts at y = 50.
local floor = { { x = -1000, y = 100, w = 2000, h = 50 } }
local c = FX.eject(by.rifle, { 100, 50 }, 0, 1, nil, rnd)
local dt = 1 / 120
local apex, first_bounce, rest_at, steps = c.y, nil, nil, 0
local maxy_before_bounce = -1
while true do
	local alive = FX.step(c, dt, floor)
	steps = steps + 1
	if not alive then break end
	apex = math.min(apex, c.y)
	check(c.y <= 100.001, "never inside the floor")
	if c.bounces == 0 then maxy_before_bounce = math.max(maxy_before_bounce, c.y) end
	if c.bounces == 1 and not first_bounce then first_bounce = steps * dt; check(c.vy < 0, "the bounce goes up") end
	if c.rest and not rest_at then rest_at = steps * dt end
	if steps == 40 then check(c.y > apex or c.vy > 0 or c.bounces == 0, "falls under gravity") end
end
check(near(steps * dt, FX.casing_life, dt), "expires at about 1.5 s, got " .. steps * dt)
check(apex < 50, "rises first")
check(first_bounce and first_bounce < 0.8, "lands and bounces (soon)")
check(rest_at and rest_at > first_bounce, "then rests after the second contact")
check(c.bounces == 2 and c.rest and c.vx == 0 and c.vy == 0, "bounces once, then rests")
check(near(c.rot % math.pi, 0, 1e-9) or near(c.rot % math.pi, math.pi, 1e-9), "lies flat")
check(near(c.y, 100, 0.05), "resting on the floor")

-- Fade: opaque early, gone at the end.
local f = FX.eject(by.rifle, { 0, 0 }, 0, 1, nil, rnd)
check(FX.alpha(f) == 1, "opaque at first")
f.age = FX.casing_life - FX.casing_fade / 2
check(near(FX.alpha(f), 0.5, 1e-9), "half faded")
f.age = FX.casing_life
check(FX.alpha(f) == 0, "invisible at the end")

-- A wall: the casing bounces off horizontally and does not pass through.
local wall = { { x = 110, y = -1000, w = 50, h = 2000 } }
local w = FX.eject(by.rifle, { 100, 50 }, math.pi, -1, nil, rnd) -- flung right
for _ = 1, 120 do FX.step(w, dt, wall); check(w.x <= 110.001, "does not pass through a wall") end

-- Cap: oldest dropped.
local list = {}
for i = 1, FX.max_casings + 25 do
	local k = FX.eject(by.rifle, { i, 0 }, 0, 1, nil, rnd)
	FX.add(list, k)
end
check(#list == FX.max_casings, "casings are capped")
check(list[1].x == 26 and list[#list].x == FX.max_casings + 25, "the oldest go first")
FX.step_all(list, FX.casing_life, floor)
check(#list == 0, "step_all removes the expired")

-- Explosion: the ring grows to the radius, thins and fades; debris flies out and falls; deterministic.
local b0 = FX.blast(70, 0, 300, 200)
local b1 = FX.blast(70, FX.blast_time * 0.5, 300, 200)
local b2 = FX.blast(70, FX.blast_time, 300, 200)
check(b0.ring == 0 and b1.ring > 40 and near(b2.ring, 70, 1e-9), "the ring grows to the radius")
check(b1.thickness < b0.thickness and b2.ring_alpha == 0, "thins and fades")
check(#b1.debris == FX.debris_count and b1.debris[1].alpha > 0, "debris")
local d0, d1 = b0.debris[1], b1.debris[1]
check(math.abs(d1.x - 300) + math.abs(d1.y - 200) > 10 and d0.x == 300, "debris flies out from the centre")
check(FX.blast(70, 0.2, 300, 200).debris[3].x == FX.blast(70, 0.2, 300, 200).debris[3].x, "deterministic")

print("jetpack fire ok")
