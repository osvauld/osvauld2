-- The firing effects, drawn: a soldier with its recoil, trigger squeeze and muzzle flash (pilot_art.lua, fed by
-- fire_fx.lua's timeline), ejected casings, tracers, rockets with their smoke trails, and explosions (a ring,
-- a fireball and debris). The timeline and the casings' physics are fire_fx.lua's and the rules'
-- (weapons.lua steps `m.casings`); this module owns the gfx handles and spawns a shot's cosmetics when a
-- soldier is first drawn after it (the eject point is known only to the rig).
local C = require("theme")
local P = require("pilot_art")
local FX = require("fire_fx")
local X = {}

local brushes = {}
local function brush(color)
	if not brushes[color] then brushes[color] = gfx.solid(color) end
	return brushes[color]
end
local function line(x, y, tx, ty, color, width)
	return gfx.stroke({ path = gfx.path({ { "move", x, y }, { "line", tx, ty } }), brush = brush(color), width = width, cap = "round" })
end
local function poly(points, color)
	local commands = { { "move", points[1][1], points[1][2] } }
	for i = 2, #points do table.insert(commands, { "line", points[i][1], points[i][2] }) end
	table.insert(commands, { "close" })
	return gfx.fill({ path = gfx.path(commands), brush = brush(color) })
end
local function instance(visual, x, y, angle, scale)
	local c, s = math.cos(angle or 0) * (scale or 1), math.sin(angle or 0) * (scale or 1)
	return gfx.instance({ visual = visual, transform = { c, s, -s, c, x, y } })
end
local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- Colour with an alpha in `levels` steps (compiled frames are cached per step).
local levels = 5
local function step_of(alpha) return clamp(math.ceil(alpha * levels - 1e-9), 1, levels) end
local function tint(color, step) return color .. string.format("%02x", math.floor(255 * step / levels)) end

local cache = {}
local function cached(key, make)
	local v = cache[key]
	if not v then v = make(); cache[key] = v end
	return v
end

-- A circle of radius 1 around the origin as path commands (cubic arcs).
local K = 0.5523
local function circle(r, commands)
	commands = commands or {}
	table.insert(commands, { "move", r, 0 })
	table.insert(commands, { "cubic", r, r * K, r * K, r, 0, r })
	table.insert(commands, { "cubic", -r * K, r, -r, r * K, -r, 0 })
	table.insert(commands, { "cubic", -r, -r * K, -r * K, -r, 0, -r })
	table.insert(commands, { "cubic", r * K, -r, r, -r * K, r, 0 })
	table.insert(commands, { "close" })
	return commands
end
local function disk(color, step)
	return cached("disk/" .. color .. step, function()
		return gfx.frame({ width = 2, height = 2, gfx.fill({ path = gfx.path(circle(1)), brush = brush(tint(color, step)) }) })
	end)
end
-- A ring of outer radius 1 and thickness `k` (a fraction of it).
local function ring(color, k, step)
	return cached("ring/" .. color .. k .. "/" .. step, function()
		local commands = circle(1)
		circle(1 - k, commands)
		return gfx.frame({ width = 2, height = 2,
			gfx.fill({ path = gfx.path(commands), brush = brush(tint(color, step)), rule = "evenodd" }) })
	end)
end
local function chip(color, size, step)
	return cached("chip/" .. color .. size .. step, function()
		return gfx.frame({ width = size, height = size, gfx.fill({
			path = gfx.path({ { "move", -size / 2, -size / 2 }, { "line", size / 2, -size / 2 }, { "line", size / 2, size / 2 },
				{ "line", -size / 2, size / 2 }, { "close" } }), brush = brush(tint(color, step)) }) })
	end)
end

-- Tracers: a bright core on a soft glow, head at the origin, trailing back along -x.
local tracer_colors = { player = { glow = "#ffd36a", core = "#ffffff" }, bot = { glow = "#ff7a5c", core = "#fff0ea" } }
local function tracer(team, len)
	local c = tracer_colors[team]
	return cached("tracer/" .. team .. len, function()
		return gfx.frame({ width = len + 4, height = 6,
			line(-len, 0, -len / 2, 0, tint(c.glow, 2), 3.2), line(-len / 2, 0, 0, 0, tint(c.glow, 4), 3.6),
			line(-len * 0.7, 0, 0, 0, c.core, 1.4) })
	end)
end

local rocket_art = gfx.frame({ width = 24, height = 12,
	poly({ { -8, -2.2 }, { -12, -5.5 }, { -5.5, -2.2 } }, "#6b7a78"), poly({ { -8, 2.2 }, { -12, 5.5 }, { -5.5, 2.2 } }, "#6b7a78"),
	poly({ { -9, -2.3 }, { 3, -2.3 }, { 3, 2.3 }, { -9, 2.3 } }, "#c9d0c8"),
	poly({ { 3, -2.3 }, { 9, 0 }, { 3, 2.3 } }, "#e0523c"),
	poly({ { -9, -2.3 }, { -6, -2.3 }, { -6, 2.3 }, { -9, 2.3 } }, "#8a9794"),
})
local rocket_flame = gfx.frame({ width = 16, height = 8,
	poly({ { -9, -2 }, { -19, 0 }, { -9, 2 } }, "#ff9a2e"), poly({ { -9, -1 }, { -14, 0 }, { -9, 1 } }, "#fffbd6") })

local function casing(c, step)
	return cached("casing/" .. c.color .. c.len .. step, function()
		local l, w = c.len / 2, c.w / 2
		return gfx.frame({ width = c.len, height = c.w, gfx.fill({
			path = gfx.path({ { "move", -l, -w }, { "line", l, -w }, { "line", l, w }, { "line", -l, w }, { "close" } }),
			brush = brush(tint(c.color, step)) }) })
	end)
end

local smoke_color, fire_color, core_color = "#8b9a97", "#ff9a2e", "#fffbd6"
local tail_min = 24 -- shots are born at the soldier: draw them once clear of the gun

local function shots(result, m)
	for _, b in ipairs(m.shots) do
		local angle = math.atan2(b.vy, b.vx)
		if (b.dist or 0) >= tail_min then
			if b.rocket then
				local dx, dy = math.cos(angle), math.sin(angle)
				local n = math.min(9, math.floor((b.dist - tail_min) / 7))
				for k = n, 1, -1 do
					local d = 9 + k * 7
					local r = 1.8 + k * 0.85
					table.insert(result, instance(disk(smoke_color, step_of(0.55 * (1 - k / (n + 1.5)))), b.x - dx * d, b.y - dy * d, 0, r))
				end
				table.insert(result, instance(rocket_flame, b.x, b.y, angle, 0.8 + 0.3 * math.sin((m.time or 0) * 70)))
				table.insert(result, instance(rocket_art, b.x, b.y, angle))
			else
				local speed = math.sqrt(b.vx * b.vx + b.vy * b.vy)
				local len = math.floor(clamp(speed * 0.02, 10, 40) / 4) * 4
				table.insert(result, instance(tracer(b.team == "player" and "player" or "bot", len), b.x, b.y, angle))
			end
		end
	end
end

local function blast(result, e)
	local b = FX.blast(e.radius, e.age, e.x, e.y)
	local u = clamp(e.age / FX.blast_time, 0, 1)
	table.insert(result, instance(disk(smoke_color, step_of(0.4 * (1 - u))), e.x, e.y - u * 8, 0, e.radius * (0.45 + 0.4 * u)))
	if b.ring_alpha > 0 and b.ring > 1 then
		table.insert(result, instance(disk(fire_color, step_of(0.3 * b.ring_alpha)), e.x, e.y, 0, b.ring))
	end
	if b.core_alpha > 0 then
		local r = e.radius * (0.18 + 0.32 * math.min(1, u * 3))
		table.insert(result, instance(disk(fire_color, step_of(b.core_alpha)), e.x, e.y, 0, r))
		table.insert(result, instance(disk(core_color, step_of(b.core_alpha)), e.x, e.y, 0, r * 0.55))
	end
	if b.ring_alpha > 0 and b.ring > 1 then
		local k = clamp(math.floor(b.thickness * 20 + 0.5) / 20, 0.05, 0.3)
		table.insert(result, instance(ring("#ffd27a", k, step_of(b.ring_alpha)), e.x, e.y, 0, b.ring))
	end
	for _, d in ipairs(b.debris) do
		if d.alpha > 0 then table.insert(result, instance(chip(fire_color, math.floor(d.size * 3) / 2, step_of(d.alpha)), d.x, d.y, e.age * 9)) end
	end
end

local spark = gfx.frame({ width = 12, height = 12, line(-5, 0, 5, 0, C.shot, 2), line(0, -5, 0, 5, C.shot, 2) })

local function effects(result, m)
	for _, e in ipairs(m.effects) do
		if e.kind == "smoke" then
			local u = clamp(e.age / FX.smoke_time, 0, 1)
			for j = 1, 3 do
				local x = e.x + e.dx * (5 * j + e.age * 28)
				local y = e.y + e.dy * (5 * j + e.age * 28) - e.age * (10 + 6 * j)
				table.insert(result, instance(disk(smoke_color, step_of(0.55 * (1 - u))), x, y, 0, 3 + j * 1.6 + e.age * 9))
			end
		elseif e.kind == "burst" and e.radius then
			blast(result, e)
		else
			local size = e.kind == "burst" and 4 or 1
			table.insert(result, instance(spark, e.x, e.y, e.age * 10, size * (1 + e.age * 3)))
		end
	end
end

-- The map-sized frame of everything fired: casings, tracers, rockets, effects.
function X.effects(m)
	local result = { width = m.map.w, height = m.map.h }
	for _, c in ipairs(m.casings or {}) do
		table.insert(result, instance(casing(c, step_of(FX.alpha(c))), c.x, c.y, c.rot))
	end
	shots(result, m)
	effects(result, m)
	return gfx.frame(result)
end

-- Cosmetics of a fresh shot: its casing from the eject point, the rocket's back-blast smoke. `lag` is how long ago
-- the shot happened, so a late-noticed one is caught up.
local function spawn(m, a, gun, info, ax, ay, lag)
	local ex, ey = a.x - ax + info.eject[1], a.y - ay + info.eject[2]
	m.casings = m.casings or {}
	local c = FX.eject(gun, { ex, ey }, info.aim, info.facing, a)
	if c then
		for _ = 1, math.floor(lag * 120) do FX.step(c, 1 / 120, m.solids or {}) end
		FX.add(m.casings, c)
	elseif gun.stats.kind == "rocket" and #m.effects < 40 then
		local dx, dy = math.cos(info.aim + math.pi), math.sin(info.aim + math.pi)
		table.insert(m.effects, { x = ex, y = ey, age = lag, kind = "smoke", dx = dx, dy = dy })
	end
end

-- The soldier's visual, with the recoil/trigger/flash of its latest shot (`a.fired_at`, weapons.lua) on the
-- game clock `m.time`; the first draw after a new shot (`a.shot_n`) spawns its casing / smoke.
function X.soldier(m, a, pilot, team, gun)
	local age = a.fired_at and m.time - a.fired_at or nil
	local fx = gun and FX.state(gun, age) or nil
	local visual, info = P.actor(a, m.time, pilot, team, gun, fx)
	if gun and info and a.shot_n then
		m.fx_seen = m.fx_seen or {}
		if m.fx_seen[a.id] ~= a.shot_n then
			m.fx_seen[a.id] = a.shot_n
			if age and age < 0.25 then
				local ax, ay = P.anchor(pilot)
				spawn(m, a, gun, info, ax, ay, age)
			end
		end
	end
	return visual
end

return X
