local C = require("theme")

local held = {}
local x, y = C.width / 2, C.height / 2
local aim_x, aim_y, angle = nil, nil, 0
local shots = {}
local fire_clock, spawn_clock = 0, 0
local score, spawned = 0, 3
local targets = {
	{ x = 560, y = 240 }, { x = 360, y = 90 }, { x = 170, y = 360 },
}
local sites = {
	{ 80, 80 }, { 640, 80 }, { 80, 400 }, { 640, 400 },
	{ 560, 240 }, { 360, 90 }, { 170, 360 },
}
local next_site = 1

local controls = {
	KeyW = true, KeyA = true, KeyS = true, KeyD = true,
	ArrowUp = true, ArrowLeft = true, ArrowDown = true, ArrowRight = true,
}

local field_path = gfx.path({
	{ "move", 0, 0 }, { "line", C.width, 0 },
	{ "line", C.width, C.height }, { "line", 0, C.height }, { "close" },
})
local hull_path = gfx.path({
	{ "move", 10, 3 }, { "line", 34, 3 }, { "line", 39, 10 },
	{ "line", 39, 41 }, { "line", 5, 41 }, { "line", 5, 10 }, { "close" },
})
local track_path = gfx.path({
	{ "move", 4, 12 }, { "line", 4, 36 },
	{ "move", 40, 12 }, { "line", 40, 36 },
})
local tank = gfx.frame({
	width = 44, height = 44,
	gfx.fill({ path = hull_path, brush = gfx.solid(C.hull) }),
	gfx.stroke({ path = hull_path, brush = gfx.solid(C.outline), width = 2 }),
	gfx.stroke({ path = track_path, brush = gfx.solid(C.outline), width = 5 }),
})
local barrel_path = gfx.path({ { "move", 22, 22 }, { "line", 22, 2 } })
local hub_path = gfx.path({
	{ "move", 22, 14 }, { "line", 29, 18 }, { "line", 29, 27 },
	{ "line", 22, 31 }, { "line", 15, 27 }, { "line", 15, 18 }, { "close" },
})
local turret_brush = gfx.solid(C.turret)
local turret = gfx.frame({
	width = 44, height = 44,
	gfx.stroke({ path = barrel_path, brush = gfx.solid(C.outline), width = 8, cap = "round" }),
	gfx.stroke({ path = barrel_path, brush = turret_brush, width = 5, cap = "round" }),
	gfx.fill({ path = hub_path, brush = turret_brush }),
})
local shot_path = gfx.path({
	{ "move", 4, 0 }, { "line", 8, 4 }, { "line", 4, 8 },
	{ "line", 0, 4 }, { "close" },
})
local shot_visual = gfx.frame({ width = 8, height = 8,
	gfx.fill({ path = shot_path, brush = gfx.solid(C.shot) }),
})
local target_path = gfx.path({
	{ "move", 0, 0 }, { "line", 32, 0 }, { "line", 32, 32 },
	{ "line", 0, 32 }, { "close" },
})
local target_visual = gfx.frame({ width = 32, height = 32,
	gfx.fill({ path = target_path, brush = gfx.solid(C.target) }),
	gfx.stroke({ path = target_path, brush = gfx.solid(C.outline), width = 2 }),
})
local field_brush = gfx.solid(C.arena)
local border_brush = gfx.solid(C.border)

local function key(e)
	if e.cancelled then held = {}; return end
	if controls[e.code] then held[e.code] = e.down or nil end
end

local function face_target()
	if not aim_x then return end
	local dx, dy = aim_x - x, aim_y - y
	if dx * dx + dy * dy > 1 then angle = math.atan2(dy, dx) + math.pi / 2 end
end

local function hover(e)
	if e.phase == "leave" then aim_x, aim_y = nil, nil; return end
	aim_x, aim_y = e.x, e.y
	face_target()
end

local function fire()
	if #shots >= C.max_shots then return end
	local sx, sy = math.sin(angle), -math.cos(angle)
	table.insert(shots, {
		x = x + sx * 26, y = y + sy * 26,
		vx = sx * C.shot_speed, vy = sy * C.shot_speed,
	})
end

local function slab(p, d, center, lo, hi)
	if d == 0 then
		if p < center - 16 or p > center + 16 then return nil end
		return lo, hi
	end
	local a, b = (center - 16 - p) / d, (center + 16 - p) / d
	return math.max(lo, math.min(a, b)), math.min(hi, math.max(a, b))
end

local function hit_time(ox, oy, nx, ny, target)
	local rx, ry = ox - target.px, oy - target.py
	local lo, hi = slab(rx, nx - target.x - rx, 0, 0, 1)
	if not lo or lo > hi then return nil end
	lo, hi = slab(ry, ny - target.y - ry, 0, lo, hi)
	if lo and lo <= hi then return lo end
end

local function spawn_enemy()
	if #targets >= C.max_enemies then return end
	for _ = 1, #sites do
		local site = sites[next_site]
		next_site = next_site % #sites + 1
		local dx, dy = site[1] - x, site[2] - y
		local clear = dx * dx + dy * dy > 96 * 96
		for _, other in ipairs(targets) do
			local ox, oy = site[1] - other.x, site[2] - other.y
			if ox * ox + oy * oy < 48 * 48 then clear = false; break end
		end
		if clear then
			table.insert(targets, { x = site[1], y = site[2] })
			spawned += 1
			return
		end
	end
end

local function tick(e)
	local dx = ((held.KeyD or held.ArrowRight) and 1 or 0)
		- ((held.KeyA or held.ArrowLeft) and 1 or 0)
	local dy = ((held.KeyS or held.ArrowDown) and 1 or 0)
		- ((held.KeyW or held.ArrowUp) and 1 or 0)
	if dx ~= 0 or dy ~= 0 then
		local magnitude = math.sqrt(dx * dx + dy * dy)
		local step = C.speed * e.dt / magnitude
		x = math.clamp(x + dx * step, C.radius, C.width - C.radius)
		y = math.clamp(y + dy * step, C.radius, C.height - C.radius)
		face_target()
	end
	for _, target in ipairs(targets) do
		target.px, target.py = target.x, target.y
		local tx, ty = x - target.x, y - target.y
		local distance = math.sqrt(tx * tx + ty * ty)
		if distance > 30 then
			local step = math.min(C.enemy_speed * e.dt, distance - 30) / distance
			target.x += tx * step
			target.y += ty * step
		end
	end
	for i = #shots, 1, -1 do
		local shot = shots[i]
		local ox, oy = shot.x, shot.y
		shot.x += shot.vx * e.dt
		shot.y += shot.vy * e.dt
		local victim, earliest = nil, 2
		for index, target in ipairs(targets) do
			local t = hit_time(ox, oy, shot.x, shot.y, target)
			if t and t < earliest then victim, earliest = index, t end
		end
		if victim then
			table.remove(targets, victim)
			score += 1
			table.remove(shots, i)
		elseif shot.x < 0 or shot.x > C.width or shot.y < 0 or shot.y > C.height then
			table.remove(shots, i)
		end
	end
	fire_clock += e.dt
	if fire_clock >= C.fire_interval then
		fire_clock -= C.fire_interval
		fire()
	end
	spawn_clock += e.dt
	if spawn_clock >= C.spawn_interval then
		spawn_clock -= C.spawn_interval
		spawn_enemy()
	end
end

local function picture()
	local c, s = math.cos(angle), math.sin(angle)
	local markers = {}
	for _, target in ipairs(targets) do
		table.insert(markers, gfx.instance({ visual = target_visual,
			transform = { 1, 0, 0, 1, target.x - 16, target.y - 16 } }))
	end
	local bullets = {}
	for _, shot in ipairs(shots) do
		table.insert(bullets, gfx.instance({ visual = shot_visual,
			transform = { 1, 0, 0, 1, shot.x - 4, shot.y - 4 } }))
	end
	return gfx.frame({
		width = C.width, height = C.height,
		gfx.fill({ path = field_path, brush = field_brush }),
		gfx.stroke({ path = field_path, brush = border_brush, width = 3 }),
		gfx.group(markers),
		gfx.group(bullets),
		gfx.instance({ visual = tank, transform = { 1, 0, 0, 1, x - 22, y - 22 } }),
		gfx.instance({ visual = turret,
			transform = { c, s, -s, c, x - 22 * c + 22 * s, y - 22 * s - 22 * c } }),
	})
end

return function()
	return ui.col({
		full = true, center = true, gap = 12, fill = C.bg,
		ui.text({ "Tank · WASD or arrows · auto-fire", color = C.text,
			font_size = 22, no_wrap = true }),
		ui.frame({ id = "arena", visual = picture(), on_key = key, on_frame = tick,
			on_hover = hover }),
		ui.text({ "score: " .. score, color = C.text, no_wrap = true }),
		ui.text({ "enemies: " .. #targets .. "/" .. C.max_enemies .. " · spawned: " .. spawned,
			color = C.text, no_wrap = true }),
		ui.text({ "enemy: " .. (targets[1] and (math.floor(targets[1].x) .. "," ..
			math.floor(targets[1].y)) or "none"), color = C.text, no_wrap = true }),
		ui.text({ "shots: " .. #shots .. "/" .. C.max_shots,
			color = C.text, no_wrap = true }),
		ui.text({ "aim: " .. math.floor(math.deg(angle) + 0.5), color = C.text, no_wrap = true }),
		ui.text({ "tank: " .. math.floor(x) .. "," .. math.floor(y),
			color = C.text, no_wrap = true }),
	})
end
