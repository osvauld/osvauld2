local C = require("theme")
local Physics = require("physics")
local FX = require("fire_fx")
local W = {}

-- Rules (design/jetpack-militia.md step 4, dated note): a gun's `range` is where its shots start to
-- lose damage; damage falls linearly to `falloff_floor` of full at `falloff_end` x range, where the
-- shot expires. Rockets never fall off; they explode at `range`. Splash does the gun's `damage` at
-- the centre, falling linearly to 0 at the `splash` radius (measured to the soldier's centre), and
-- pushes away from the centre by `knockback` * the same factor. The owner takes their own splash.
W.falloff_end, W.falloff_floor, W.knockback = 1.5, 0.4, 560
W.seed = 20240611

local function round(v) return math.max(1, math.floor(v + 0.5)) end

-- Deterministic per world (minstd: exact in doubles): the world's `rng` is its whole state.
function W.random(w)
	w.rng = ((w.rng or W.seed) * 16807) % 2147483647
	return w.rng / 2147483647
end

-- The gun table (format.lua) a soldier holds: its id, else the first gun.
function W.gun(w, a) return w.gun_by_id[a.gun] or w.guns[1] end

-- Gun stats for `a`; a gun change (or a respawn clearing `ammo_gun`) refills the mag.
function W.sync(w, a)
	local g = W.gun(w, a)
	if a.ammo_gun ~= g.id then a.ammo_gun, a.ammo, a.reload_left = g.id, g.stats.mag_size, nil end
	return g.stats
end

-- ammo, mag size, seconds of reload left (nil when not reloading).
function W.ammo(w, a)
	local s = W.sync(w, a)
	return a.ammo, s.mag_size, a.reload_left
end

-- Damage multiplier by distance flown.
function W.falloff(dist, range)
	if dist <= range then return 1 end
	local t = math.min(1, (dist - range) / (range * (W.falloff_end - 1)))
	return 1 - (1 - W.falloff_floor) * t
end

-- `radius` (explosions): the blast ring's size; such an effect lives longer (fire_fx.blast_time).
function W.effect(w, x, y, kind, radius)
	if #w.effects < 40 then
		table.insert(w.effects, {x = x, y = y, age = 0, kind = kind, radius = radius, life = radius and FX.blast_time or nil})
	end
end

function W.team(w, a) return a == w.player and "player" or "bot" end

-- Spawn the shot(s) of one trigger pull from the soldier's gun now: no ammo or interval rules.
function W.shoot(w, a, team)
	if a.hp <= 0 then return end
	local s = W.sync(w, a)
	if #w.shots + s.pellets > 96 then return end
	local rocket = s.kind == "rocket"
	local maxd = rocket and s.range or s.range * W.falloff_end
	for _ = 1, s.pellets do
		local ang = a.aim + (W.random(w) * 2 - 1) * s.spread
		-- Start at the actor, so cover cannot be bypassed by putting a muzzle through it.
		table.insert(w.shots, {x = a.x, y = a.y - 4, vx = math.cos(ang) * s.shot_speed,
			vy = math.sin(ang) * s.shot_speed, team = team, owner = a, dist = 0, maxd = maxd,
			life = maxd / s.shot_speed + 1, range = s.range, rocket = rocket, splash = s.splash,
			damage = s.damage * (a.damage_scale or 1)})
	end
	a.flash = 0.055
	a.fired_at, a.shot_n = w.time or 0, (a.shot_n or 0) + 1 -- the firing animation (fx_art.lua) keys off these
	if team == "player" then w.fired = w.fired + 1 else w.enemy_fired = w.enemy_fired + 1 end
end

-- Per step, before pulling the trigger: cool down and finish a reload.
function W.tick(w, a, dt)
	local s = W.sync(w, a)
	a.fire_clock = math.max(-dt, a.fire_clock - dt) -- a little credit, so the rate does not drift
	if a.reload_left then
		a.reload_left = a.reload_left - dt
		if a.reload_left <= 1e-9 then a.ammo, a.reload_left = s.mag_size, nil end
	end
end

function W.reload(w, a)
	local s = W.sync(w, a)
	if a.reload_left or a.ammo >= s.mag_size then return false end
	a.reload_left = s.reload_time
	return true
end

-- The trigger state for this step. Auto guns fire while held; semi guns once per press (a press
-- inside the interval waits for it). Bots re-press every step. True when a shot left.
function W.pull(w, a, team, trigger)
	local s = W.sync(w, a)
	local fresh = trigger and (team == "bot" or not a.trigger_prev)
	a.trigger_prev = trigger
	if not trigger then a.pending = false; return false end
	if fresh then a.pending = true end
	if a.reload_left or (a.swap_left or 0) > 0 then return false end -- step 5: no firing mid-swap
	if a.ammo <= 0 then W.reload(w, a); return false end
	if a.fire_clock > 1e-9 or not (s.auto or a.pending) then return false end
	a.pending = false
	a.ammo = a.ammo - 1
	a.fire_clock = a.fire_clock + s.fire_interval
	W.shoot(w, a, team)
	if a.ammo <= 0 then W.reload(w, a) end
	return true
end

function W.damage(w, a, amount)
	if a.hp <= 0 or a.shield > 0 then return end
	a.hp = math.max(0, a.hp - amount)
	W.effect(w, a.x, a.y, "hit")
	if a.hp == 0 then
		a.respawn, a.thrusting = 2.2, false
		W.effect(w, a.x, a.y, "burst")
		if a == w.player then w.deaths = w.deaths + 1; w:clear_input()
		else w.kills = w.kills + 1 end
	end
end

-- Splash at (x, y): `damage` at the centre to 0 at `radius`, plus the push. Spares the owner's
-- teammates (not the owner).
function W.explode(w, x, y, radius, damage, owner, team)
	W.effect(w, x, y, "burst", radius)
	local all = {w.player}
	for _, b in ipairs(w.bots) do table.insert(all, b) end
	for _, a in ipairs(all) do
		if a.hp > 0 and (a == owner or W.team(w, a) ~= team) then
			local dx, dy = a.x - x, a.y - y
			local d = math.sqrt(dx * dx + dy * dy)
			if d < radius then
				local f = 1 - d / radius
				local amount = math.floor(damage * f + 0.5)
				if amount >= 1 then W.damage(w, a, amount) end
				if a.hp > 0 then
					local nx, ny = 0, -1
					if d > 1e-6 then nx, ny = dx / d, dy / d end
					a.vx, a.vy = a.vx + nx * W.knockback * f, a.vy + ny * W.knockback * f
				end
			end
		end
	end
end

-- Advance shots (cover first, then actors, swept) and age out effects.
function W.update(w, dt)
	local solid, p = w.solids, w.player
	for i = #w.shots, 1, -1 do
		local b = w.shots[i]
		local dx, dy = b.vx * dt, b.vy * dt
		local len = math.sqrt(dx * dx + dy * dy)
		local dist = b.dist or 0
		if b.maxd and len > b.maxd - dist and len > 0 then
			local k = math.max(0, b.maxd - dist) / len
			dx, dy, len = dx * k, dy * k, len * k
		end
		local first, victim = 2, nil
		for _, r in ipairs(solid) do
			local t = Physics.segment(b.x, b.y, dx, dy, r, 0, 0)
			if t and t < first then first, victim = t, nil end
		end
		local targets = b.team == "player" and w.bots or {p}
		for _, a in ipairs(targets) do
			if a.hp > 0 then
				local t = Physics.segment(b.x, b.y, dx, dy,
					{x = a.x - C.rx, y = a.y - C.ry, w = C.rx * 2, h = C.ry * 2}, 0, 0)
				if t and t < first then first, victim = t, a end
			end
		end
		b.life = b.life - dt
		if first <= 1 then
			local hx, hy = b.x + dx * first, b.y + dy * first
			local base = b.damage or (b.team == "player" and 20 or 12)
			if b.rocket then
				if victim then W.damage(w, victim, round(base)) end
				W.explode(w, hx, hy, b.splash, base, b.owner, b.team)
			else
				W.effect(w, hx, hy, "spark")
				if victim then
					local f = b.range and W.falloff(dist + len * first, b.range) or 1
					W.damage(w, victim, round(base * f))
				end
			end
			table.remove(w.shots, i)
		else
			b.x, b.y, b.dist = b.x + dx, b.y + dy, dist + len
			if b.life <= 0 or (b.maxd and b.dist >= b.maxd - 1e-9) then
				if b.rocket then W.explode(w, b.x, b.y, b.splash, b.damage, b.owner, b.team) end
				table.remove(w.shots, i)
			end
		end
	end
	for i = #w.effects, 1, -1 do
		w.effects[i].age = w.effects[i].age + dt
		if w.effects[i].age > (w.effects[i].life or 0.35) then table.remove(w.effects, i) end
	end
	if w.casings then FX.step_all(w.casings, dt, solid) end
end

return W
