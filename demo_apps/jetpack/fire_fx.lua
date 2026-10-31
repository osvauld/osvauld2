-- The firing timeline, pure maths (no gfx): from a gun and the time since its shot, how far the gun is
-- kicked back along the barrel and up, how far the trigger finger is squeezed and which muzzle flash
-- shows; and the ejected casings, which are small bodies stepped here against the map's solids.
-- fx_art.lua / pilot_art.lua only draw what this says. scripts/test_jetpack_fire.lua tests it under stock lua.
-- Design: design/jetpack-militia.md step 3. Everything here is cosmetic: nothing reads it back into the rules.
local FX = {}

FX.return_cap = 0.15 -- the gun is back by min(fire_interval, this) seconds
FX.climb_per_recoil = 0.7 -- degrees the muzzle climbs per drawing unit of recoil, between the limits
FX.climb_limits = { 1, 8 }
FX.flash_time = { small = 0.05, long = 0.06, wide = 0.07, rocket = 0.12 }
FX.smoke_time = 0.7 -- the rocket's back-blast smoke, a world effect the view spawns
FX.casing_life, FX.casing_fade = 1.5, 0.4
FX.max_casings = 40
FX.gravity = 720
FX.restitution = 0.45
FX.blast_time = 0.55

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- Recoil and trigger ------------------------------------------------------------------------------
-- Seconds the kick takes to return.
function FX.kick_time(gun) return math.min(gun.stats.fire_interval, FX.return_cap) end

-- Degrees the muzzle climbs at the instant of the shot.
function FX.climb_degrees(gun)
	return clamp(gun.stats.recoil * FX.climb_per_recoil, FX.climb_limits[1], FX.climb_limits[2])
end

-- (back, climb) `age` seconds after a shot: `back` game units along the barrel away from the target
-- (the gun's `recoil` drawing units x its scale at age 0), `climb` radians the muzzle is raised. Both ease
-- out to 0 at kick_time and stay there.
function FX.recoil(gun, age)
	local t = FX.kick_time(gun)
	if age < 0 or age >= t then return 0, 0 end
	local k = (1 - age / t) ^ 2
	return gun.stats.recoil * gun.scale * k, math.rad(FX.climb_degrees(gun)) * k
end

-- 0..1 squeeze of the trigger finger: full for the first quarter of the kick time, then released.
function FX.trigger(gun, age)
	local t = FX.kick_time(gun)
	if age < 0 or age >= t then return 0 end
	local hold = t * 0.25
	if age <= hold then return 1 end
	return 1 - (age - hold) / (t - hold)
end

-- Muzzle flash ------------------------------------------------------------------------------------
-- small: pistol / SMG; long: fast bullets (rifle, sniper); wide: a pellet gun; rocket: front puff + back-blast.
function FX.flash_style(gun)
	local s = gun.stats
	if s.kind == "rocket" then return "rocket" end
	if s.pellets > 1 then return "wide" end
	if s.shot_speed >= 1200 then return "long" end
	return "small"
end

-- The flash showing `age` seconds after a shot, or nil: { style, t = 0..1 through its life }.
function FX.flash(gun, age)
	local style = FX.flash_style(gun)
	local d = FX.flash_time[style]
	if age < 0 or age >= d then return nil end
	return { style = style, t = age / d }
end

-- Everything the actor's art needs `age` seconds after a shot (nil age: never fired).
function FX.state(gun, age)
	if not age then return { back = 0, climb = 0, trigger = 0 } end
	local back, climb = FX.recoil(gun, age)
	return { back = back, climb = climb, trigger = FX.trigger(gun, age), flash = FX.flash(gun, age) }
end

-- Casings ----------------------------------------------------------------------------------------
-- nil for a rocket (it has a back-blast instead); a shotgun ejects a red shell, the rest brass.
function FX.casing_style(gun)
	local s = gun.stats
	if s.kind == "rocket" then return nil end
	if s.pellets > 1 then return { kind = "shell", len = 6.5, w = 3.4, color = "#d8392f" } end
	local len = 3.2 + math.min(3, s.damage / 30)
	return { kind = "brass", len = len, w = len * 0.5, color = "#f0c030" }
end

-- A cheap deterministic generator for cosmetics (the world's rng is the rules', and stays untouched).
local noise = 12345
function FX.rand()
	noise = (noise * 16807) % 2147483647
	return noise / 2147483647
end

-- A casing leaving `at` ({x, y} world, the gun's eject point) with the gun at `aim` facing `facing`
-- (+-1), the shooter moving at `vel` ({vx, vy} or nil). It pops up and back in the gun's own frame, so
-- facing left mirrors it. `rnd` (default FX.rand) gives 0..1.
function FX.eject(gun, at, aim, facing, vel, rnd)
	local style = FX.casing_style(gun)
	if not style then return nil end
	rnd = rnd or FX.rand
	local lx, ly = -(35 + 35 * rnd()), -(120 + 70 * rnd()) -- gun frame: back, up
	local dy = ly * facing
	local c, s = math.cos(aim), math.sin(aim)
	vel = vel or { vx = 0, vy = 0 }
	return { x = at[1], y = at[2], vx = lx * c - dy * s + (vel.vx or 0) * 0.6, vy = lx * s + dy * c + (vel.vy or 0) * 0.6,
		rot = aim, spin = -facing * (10 + 12 * rnd()), age = 0, bounces = 0, rest = false,
		kind = style.kind, len = style.len, w = style.w, color = style.color }
end

local function inside(solids, x, y)
	for _, r in ipairs(solids) do
		if x > r.x and x < r.x + r.w and y > r.y and y < r.y + r.h then return r end
	end
end

-- Advance one casing by `dt` against `solids` ({x,y,w,h} rects). The first contact bounces it (losing
-- energy), the second lays it flat at rest. False once it has outlived casing_life.
function FX.step(c, dt, solids, gravity)
	c.age = c.age + dt
	if c.age >= FX.casing_life then return false end
	if c.rest then return true end
	c.vy = c.vy + (gravity or FX.gravity) * dt
	local nx, ny = c.x + c.vx * dt, c.y + c.vy * dt
	local r = inside(solids, nx, ny)
	if r then
		c.bounces = c.bounces + 1
		local vertical = inside(solids, c.x, ny)
		local horizontal = inside(solids, nx, c.y)
		if vertical or not horizontal then
			ny = c.vy > 0 and r.y - 0.01 or r.y + r.h + 0.01
			if not vertical then nx = c.x end
			c.vy = -c.vy * FX.restitution
			c.vx = c.vx * 0.6
		else
			nx = c.vx > 0 and r.x - 0.01 or r.x + r.w + 0.01
			c.vx = -c.vx * FX.restitution
		end
		c.spin = c.spin * 0.4
		if c.bounces >= 2 then
			c.rest, c.vx, c.vy, c.spin = true, 0, 0, 0
			c.rot = math.floor(c.rot / math.pi + 0.5) * math.pi -- lies flat
		end
	end
	c.x, c.y = nx, ny
	c.rot = c.rot + c.spin * dt
	return true
end

-- 1 until the fade starts, then down to 0 at the end of its life.
function FX.alpha(c)
	local left = FX.casing_life - c.age
	return clamp(left / FX.casing_fade, 0, 1)
end

-- Add to a list, dropping the oldest past max_casings.
function FX.add(list, c)
	table.insert(list, c)
	while #list > FX.max_casings do table.remove(list, 1) end
end

function FX.step_all(list, dt, solids, gravity)
	for i = #list, 1, -1 do
		if not FX.step(list[i], dt, solids, gravity) then table.remove(list, i) end
	end
end

-- Explosions -------------------------------------------------------------------------------------
-- A rocket blast of `radius` units, `age` seconds on: the ring's radius and thickness (fraction of the
-- radius), the fireball's radius and alpha, and the debris (a fixed fan from the centre, flung out and
-- pulled down). Deterministic in (x, y), so a blast looks the same every frame.
FX.debris_count = 12
function FX.blast(radius, age, x, y)
	local u = clamp(age / FX.blast_time, 0, 1)
	local ease = 1 - (1 - u) ^ 3
	local out = {
		ring = radius * ease, thickness = 0.28 - 0.2 * u, ring_alpha = 1 - u,
		core = radius * (0.35 + 0.5 * math.min(1, u * 3)), core_alpha = clamp(1 - u * 2.5, 0, 1),
		debris = {},
	}
	local seed = math.floor((x or 0) * 7 + (y or 0) * 13) % 1000
	for i = 1, FX.debris_count do
		local h = ((seed + i * 97) * 16807) % 2147483647 / 2147483647
		local ang = (i / FX.debris_count) * 2 * math.pi + h * 0.5
		local speed = radius * (1.6 + 1.4 * h)
		table.insert(out.debris, { x = (x or 0) + math.cos(ang) * speed * age,
			y = (y or 0) + math.sin(ang) * speed * age + 0.5 * 500 * age * age,
			alpha = clamp(1.25 - u * 1.25, 0, 1), size = 1.6 + 1.6 * h })
	end
	return out
end

return FX
