-- Siege: a side-on gravity world with a chain of causes. A slingshot fires a stone (dotted arc
-- previews the shot under the world's gravity) at a castle of stone, wood and glass; the collapse
-- topples a run of dominoes, the last pushes a marble down a ramp into a bucket that lights up.
-- Pigs score when they fall to the ground. Also a toy: a swinging wrecking ball on a chain
-- (rope joints), a seesaw (pin joint), bomb (`blast`), moon/flip gravity, a gravity well, and any
-- loose body can be dragged and thrown (`grab` / `release`). Nothing runs per frame: Lua
-- describes bodies and hears zones; Rapier does the moving. Lua cannot read a body's position, so
-- nothing here depends on one.
local W, H, FY = 960, 520, 480 -- the world, and the floor's top
local C = {
	bg = "#0b0d16", panel = "#1a1e2e", text = "#e8ebf5", muted = "#8f96b0", accent = "#ffd166",
	button = "#2a3050", button_hover = "#3a4270", lit = "#ffe066",
}
local G_NORMAL, G_MOON, G_FLIP = { 0, 900 }, { 0, 280 }, { 0, -900 }

-- ── drawing ───────────────────────────────────────────────────────────────────────────────────
local function rectp(x, y, w, h)
	return { { "move", x, y }, { "line", x + w, y }, { "line", x + w, y + h }, { "line", x, y + h }, { "close" } }
end
local function disc(cx, cy, r)
	local k = r * 0.5523
	return { { "move", cx, cy - r }, { "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r }, { "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r }, { "close" } }
end
local function ellipse(cx, cy, rx, ry)
	local kx, ky = rx * 0.5523, ry * 0.5523
	return { { "move", cx, cy - ry }, { "cubic", cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy },
		{ "cubic", cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry }, { "cubic", cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy },
		{ "cubic", cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry }, { "close" } }
end
local function drawing(w, h, shapes)
	return gfx.drawing({ size = { w, h }, parts = { { id = "it", pivot = { w / 2, h / 2 }, shapes = shapes } } })
end
local cache = {}
local function memo(key, make)
	if not cache[key] then cache[key] = make() end
	return cache[key]
end

-- Interpolates two #rrggbb colours: stacked bands make a gradient, since a drawing's fill is solid.
local function mix(a, b, t)
	local function ch(s, i) return tonumber(s:sub(i, i + 1), 16) end
	local out = "#"
	for i = 2, 6, 2 do out = out .. string.format("%02x", math.floor(ch(a, i) + (ch(b, i) - ch(a, i)) * t + 0.5)) end
	return out
end
local function bands(x, y, w, h, stops, n)
	local shapes = {}
	for i = 0, n - 1 do
		local t = i / (n - 1) * (#stops - 1)
		local s = math.min(#stops - 2, math.floor(t))
		shapes[#shapes + 1] = { path = rectp(x, y + h * i / n, w, h / n + 1), fill = mix(stops[s + 1], stops[s + 2], t - s) }
	end
	return shapes
end

local function sky()
	return memo("sky", function()
		local s = bands(0, 0, W, FY, { "#0e1432", "#2c2457", "#7a3a6e", "#e0667a", "#ffb56b" }, 30)
		for i = 1, 40 do -- stars, thinning toward the horizon
			local x, y = (i * 137) % W, (i * 71) % 200
			s[#s + 1] = { path = disc(x, y, 0.8 + (i % 3) * 0.4), fill = "#ffffff" .. (i % 2 == 0 and "cc" or "77") }
		end
		s[#s + 1] = { path = disc(780, 90, 34), fill = "#fff1c9" }
		s[#s + 1] = { path = disc(792, 84, 30), fill = "#ffe29a44" }
		-- far hills
		s[#s + 1] = { path = { { "move", 0, FY }, { "line", 0, 380 }, { "quad", 140, 330, 260, 372 }, { "quad", 380, 410, 520, 360 },
			{ "quad", 660, 310, 800, 372 }, { "quad", 900, 410, W, 380 }, { "line", W, FY }, { "close" } }, fill = "#3b2a5acc" }
		s[#s + 1] = { path = { { "move", 0, FY }, { "line", 0, 430 }, { "quad", 200, 390, 360, 440 }, { "quad", 560, 480, 700, 430 },
			{ "quad", 840, 390, W, 440 }, { "line", W, FY }, { "close" } }, fill = "#2a1f45" }
		return drawing(W, FY, s)
	end)
end

local function ground()
	return memo("ground", function()
		local s = bands(0, 0, W, H - FY, { "#4a7a3a", "#2f5a2c", "#2a2018", "#18120d" }, 14)
		s[#s + 1] = { path = rectp(0, 0, W, 3), fill = "#8fd45a" }
		return drawing(W, H - FY, s)
	end)
end

-- A lit block: body, bright top edge, dark bottom edge, outline.
local function slab(w, h, base, hi, lo, edge)
	return memo(table.concat({ "slab", w, h, base }, ":"), function()
		return drawing(w, h, {
			{ path = rectp(0, 0, w, h), fill = base }, { path = rectp(0, 0, w, math.min(3, h / 3)), fill = hi },
			{ path = rectp(0, h - math.min(3, h / 3), w, math.min(3, h / 3)), fill = lo },
			{ path = rectp(0.5, 0.5, w - 1, h - 1), stroke = { 1, edge or "#00000066" } },
		})
	end)
end
local function ball(r, base, hi, lo, mark)
	return memo(table.concat({ "ball", r, base }, ":"), function()
		local s = { { path = disc(r, r, r), fill = lo }, { path = disc(r - 1, r - 1, r - 2), fill = base },
			{ path = disc(r * 0.7, r * 0.65, r * 0.38), fill = hi } }
		if mark then s[#s + 1] = { path = rectp(r - 1.5, 2, 3, r * 0.6), fill = mark } end
		s[#s + 1] = { path = disc(r, r, r - 0.5), stroke = { 1, "#00000077" } }
		return drawing(2 * r, 2 * r, s)
	end)
end
local function pig(r)
	return memo("pig" .. r, function()
		local e = r * 0.42
		return drawing(2 * r, 2 * r, {
			{ path = disc(r, r, r), fill = "#4f9d3a" }, { path = disc(r - 1, r - 1, r - 2), fill = "#7bd357" },
			{ path = disc(r, r * 1.2, r * 0.42), fill = "#5aa840" }, -- snout
			{ path = disc(r - 3, r * 1.2, 1.6), fill = "#2d5a1f" }, { path = disc(r + 3, r * 1.2, 1.6), fill = "#2d5a1f" },
			{ path = disc(r - e, r * 0.75, 3.2), fill = "#ffffff" }, { path = disc(r + e, r * 0.75, 3.2), fill = "#ffffff" },
			{ path = disc(r - e, r * 0.78, 1.5), fill = "#102010" }, { path = disc(r + e, r * 0.78, 1.5), fill = "#102010" },
			{ path = disc(r, r, r - 0.5), stroke = { 1, "#1f4a14" } },
		})
	end)
end
local function invisible(w, h)
	return memo("inv" .. w .. "x" .. h, function() return drawing(w, h, { { path = rectp(0, 0, w, h), fill = "#00000000" } }) end)
end

local KIND = { -- how each block is drawn and made
	stone = { base = "#8d93a6", hi = "#c5cad9", lo = "#5b6075", mat = { bounce = 0.05, friction = 0.02, grip = 0.9, density = 3 } },
	wood = { base = "#c58b4e", hi = "#e8b27a", lo = "#8a5a2c", mat = { bounce = 0.3, friction = 0.02, grip = 0.7, density = 0.7 } },
	glass = { base = "#8fe3f8aa", hi = "#ffffffcc", lo = "#4aa7c4aa", edge = "#e8fbffcc",
		mat = { bounce = 0.5, friction = 0.02, grip = 0.4, density = 0.8 } },
	domino = { base = "#e0505a", hi = "#ff9aa0", lo = "#9a2a35", mat = { bounce = 0.2, friction = 0.02, grip = 0.9, density = 1.5 } },
}

-- ── the scene as data ─────────────────────────────────────────────────────────────────────────
local gen = 0 -- bumps on reset: fresh ids respawn everything where it began
local function N(name) return gen == 0 and name or (name .. "~" .. gen) end

local function castle()
	local b = {}
	local function add(name, kind, x, y, w, h) b[#b + 1] = { name = name, kind = kind, x = x, y = y, w = w, h = h } end
	for i = 0, 4 do -- two stone pillars of five
		add("sl" .. i, "stone", 440, FY - 28 * (i + 1), 28, 28)
		add("sr" .. i, "stone", 516, FY - 28 * (i + 1), 28, 28)
	end
	add("beam1", "wood", 432, 326, 120, 14)
	add("wl", "wood", 448, 282, 16, 44)
	add("wr", "wood", 524, 282, 16, 44)
	add("beam2", "wood", 432, 270, 120, 12)
	add("g1", "glass", 440, 254, 16, 16)
	add("g2", "glass", 536, 254, 16, 16)
	add("g3", "glass", 466, 300, 10, 26)
	add("g4", "glass", 512, 300, 10, 26)
	-- dominoes grow taller along the run, so each hits harder than the last; the last is a hammer
	local x = 600
	for i, h in ipairs({ 30, 34, 38, 43, 48, 54, 60 }) do
		add("d" .. (i - 1), "domino", x, 440 - h, 8, h)
		x = x + 8 + 0.45 * h
	end
	add("crate1", "wood", 478, FY - 24, 24, 24)
	add("seesaw_c1", "wood", 196, FY - 58, 22, 24)
	return b
end
local blocks = castle()

local state -- everything the game remembers; a reset replaces it
local function fresh()
	return { score = 0, pigs = { pig_roof = true, pig_mid = true }, goal = false, shots = {}, nshot = 0, aim = nil,
		g = G_NORMAL, gname = "normal", well = false, status = "Pull the stone back and let go", pending = nil }
end
state = fresh()

local SLING = { 96, 392 } -- where the stone rests; pulled back, up to PULL away
local PULL, POWER, PIPS, BAND = 72, 11, 16, 6

local grabbable = {} -- ids a drag may take; rebuilt each view

local function loose(id, x, y, drawing_, collider, mat, extra)
	grabbable[id] = true
	local e = { id = id, pos = { x, y }, drawing = drawing_, collider = collider, loose = mat }
	for k, v in pairs(extra or {}) do e[k] = v end
	return e
end

-- ── entities ──────────────────────────────────────────────────────────────────────────────────
local function build()
	grabbable = {}
	local out = {}
	local function add(e) out[#out + 1] = e end
	add({ id = "sky", pos = { 0, 0 }, drawing = sky() })
	-- contact shadows: soft dark ellipses on the ground under the structures
	add({ id = "shadow", pos = { 405, FY - 7 }, drawing = memo("shadow", function()
		return drawing(190, 14, { { path = ellipse(95, 7, 95, 6), fill = "#00000030" }, { path = ellipse(95, 7, 70, 4.5), fill = "#00000040" } })
	end) })
	add({ id = "shadow2", pos = { 580, FY - 41 }, drawing = memo("shadow2", function()
		return drawing(250, 5, { { path = ellipse(125, 2.5, 125, 2.5), fill = "#00000040" } })
	end) })
	-- floor, plinth, shelf and ramp: fixed colliders
	add({ id = "floor", pos = { 0, FY }, drawing = ground(), collider = { rect = { W, H - FY } } })
	add({ id = "wall:t", pos = { 0, -20 }, drawing = invisible(W, 20), collider = { rect = { W, 20 } } }) -- flipped gravity piles up here
	add({ id = "wall:l", pos = { -20, 0 }, drawing = invisible(20, H), collider = { rect = { 20, H } } })
	add({ id = "wall:r", pos = { W, 0 }, drawing = invisible(20, H), collider = { rect = { 20, H } } })
	add({ id = "plinth", pos = { 82, 424 }, drawing = slab(28, 56, "#6b5b4a", "#9a8670", "#3f3428"), collider = { rect = { 28, 56 } } })
	add({ id = "shelf", pos = { 580, 440 }, drawing = slab(240, 40, "#5b6075", "#8d93a6", "#3a3e50"), collider = { rect = { 240, 40 } } })
	local RS, RW = 18, 4.5
	for i = 0, RS - 1 do -- a slope in fine steps, the box colliders being axis-aligned
		local top = 440 + (i + 1) * (FY - 440 - 2) / RS
		add({ id = "ramp" .. i, pos = { 820 + RW * i, top }, drawing = slab(RW + 0.5, FY - top, "#5b6075", "#8d93a6", "#3a3e50", "#5b6075"),
			collider = { rect = { RW, FY - top } } })
	end
	add({ id = "bucket:wall", pos = { 948, 420 }, drawing = slab(10, 60, "#7a6a55", "#a89478", "#4a3f30"), collider = { rect = { 10, 60 } } })
	-- the bucket's inside lights up when the marble lands
	add({ id = "bucket", pos = { 900, 452 }, drawing = memo("bucket" .. tostring(state.goal), function()
		local c = state.goal and C.lit or "#4a4033"
		return drawing(48, 28, { { path = rectp(0, 0, 48, 28), fill = c .. (state.goal and "" or "55") },
			{ path = rectp(0, 24, 48, 4), fill = state.goal and "#fff6b0" or "#2a2118" } })
	end), sensor = { rect = { 48, 28 } } })
	if state.goal then
		add({ id = "glow", pos = { 870, 400 }, drawing = memo("glow", function()
			return drawing(110, 90, { { path = ellipse(55, 45, 55, 45), fill = "#ffe06622" }, { path = ellipse(55, 50, 38, 32), fill = "#ffe06633" },
				{ path = ellipse(55, 56, 22, 18), fill = "#fff6b055" } })
		end) })
	end
	add({ id = "goal", pos = { 906, 452 }, drawing = invisible(40, 28), sensor = { rect = { 40, 28 } } })
	-- the kill zone along the ground where the castle stands: a pig that reaches it is down
	add({ id = "killzone", pos = { 380, FY - 18 }, drawing = invisible(240, 18), sensor = { rect = { 240, 18 } } })

	if state.flash then add({ id = "flash", pos = { 0, 0 }, drawing = memo("flash", function() return drawing(W, H, { { path = rectp(0, 0, W, H), fill = "#ffffff1c" } }) end) }) end
	-- the sling: two posts and the stone in it (not a body: it is moved by hand with `set`)
	add({ id = "sling", pos = { 84, 372 }, drawing = memo("sling", function()
		return drawing(30, 52, { { path = rectp(10, 18, 8, 34), fill = "#5a3b22" }, { path = rectp(2, 6, 7, 20), fill = "#7a5230" },
			{ path = rectp(19, 6, 7, 20), fill = "#7a5230" } })
	end) })
	for i = 1, BAND do add({ id = "band" .. i, pos = { -40, -40 }, drawing = memo("bandpip", function() return drawing(5, 5, { { path = disc(2.5, 2.5, 2.5), fill = "#d9b07a" } }) end) }) end
	for i = 1, PIPS do add({ id = "arc" .. i, pos = { -40, -40 }, drawing = memo("arcpip" .. (i % 3), function()
		return drawing(6, 6, { { path = disc(3, 3, 3 - (i % 3) * 0.4), fill = "#ffffffcc" } })
	end) }) end
	add({ id = "ammo", pos = { SLING[1] - 11, SLING[2] - 11 }, drawing = ball(11, "#9aa0b3", "#d9deee", "#5b6075") })

	-- the castle and dominoes: loose bodies of mixed material
	for _, k in ipairs(blocks) do
		local kd = KIND[k.kind]
		add(loose(N(k.name), k.x, k.y, slab(k.w, k.h, kd.base, kd.hi, kd.lo, kd.edge), { rect = { k.w, k.h } }, kd.mat))
	end
	-- pigs: scored when they fall
	local PIGM = { bounce = 0.35, friction = 0.02, grip = 0.5, density = 0.9 }
	if state.pigs.pig_roof then add(loose(N("pig_roof"), 478, 256 - 14, pig(14), { circle = 14, at = { 14, 14 } }, PIGM)) end
	if state.pigs.pig_mid then add(loose(N("pig_mid"), 482, 326 - 24, pig(12), { circle = 12, at = { 12, 12 } }, PIGM)) end
	-- the marble waiting on a post at the shelf's edge, where the hammer will strike it
	add({ id = "post", pos = { 788, 400 }, drawing = slab(12, 40, "#7a6a55", "#a89478", "#4a3f30"), collider = { rect = { 12, 40 } } })
	add(loose(N("marble"), 784, 380, ball(10, "#6fa8ff", "#d6e6ff", "#2a4a8a"), { circle = 10, at = { 10, 10 } },
		{ bounce = 0.3, friction = 0.01, grip = 0.6, density = 0.5 }))
	-- the pendulum: a chain of small links on rope joints, ending in an iron ball
	local ax, ay, LINKS = 390, 24, 9
	for i = 0, LINKS - 1 do
		add(loose(N("link" .. i), ax - 7, ay + 26 * (i + 1) - 7, ball(7, "#7d8497", "#dfe3f0", "#3d4152"), { circle = 7, at = { 7, 7 } },
			{ bounce = 0.1, friction = 0.05, grip = 0.1, density = 3 }))
	end
	add(loose(N("bob"), ax - 22, ay + 26 * (LINKS + 1) - 22, ball(22, "#4a4f63", "#9aa0b8", "#1f2230"), { circle = 22, at = { 22, 22 } },
		{ bounce = 0.2, friction = 0.03, grip = 0.3, density = 4 }))
	add({ id = "anchor", pos = { ax - 8, ay - 8 }, drawing = ball(8, "#d9b07a", "#fff3d0", "#7a5230") })
	-- the seesaw: a plank on a pin joint, centred on a fulcrum
	add({ id = "fulcrum", pos = { 250, FY - 22 }, drawing = memo("fulcrum", function()
		return drawing(40, 22, { { path = { { "move", 20, 0 }, { "line", 40, 22 }, { "line", 0, 22 }, { "close" } }, fill = "#7a6a55" } })
	end) })
	add(loose(N("seesaw"), 190, FY - 22 - 12, slab(160, 10, "#c58b4e", "#e8b27a", "#8a5a2c"), { rect = { 160, 10 } },
		{ bounce = 0.1, friction = 0.02, grip = 0.8, density = 1 }))
	-- the shots fired so far
	for _, s in ipairs(state.shots) do
		add(loose(s, state[s].x - 11, state[s].y - 11, ball(11, "#9aa0b3", "#d9deee", "#5b6075"), { circle = 11, at = { 11, 11 } },
			{ bounce = 0.1, friction = 0.02, grip = 0.6, density = 14 }))
	end
	return out
end

local function joints()
	local j, LINKS = {}, 9
	local function rope(id, a, b, b_at, length)
		j[#j + 1] = { id = id, kind = "rope", a = a, a_at = { 7, 7 }, b = b, b_at = b_at, length = length }
	end
	rope("chain0", N("link0"), nil, { 390, 24 }, 26)
	for i = 1, LINKS - 1 do rope("chain" .. i, N("link" .. i), N("link" .. (i - 1)), { 7, 7 }, 26) end
	j[#j + 1] = { id = "chainbob", kind = "rope", a = N("bob"), a_at = { 22, 22 }, b = N("link" .. (LINKS - 1)), b_at = { 7, 7 }, length = 26 }
	j[#j + 1] = { id = "seesaw", kind = "pin", a = N("seesaw"), a_at = { 80, 5 }, b_at = { 270, FY - 22 - 7 }, limits = { -12, 12 } }
	return j
end

-- ── interaction ───────────────────────────────────────────────────────────────────────────────
local function arc_from(px, py, vx, vy)
	local g = state.g[2]
	local pts = {}
	for i = 1, PIPS do
		local t = 0.07 * i
		pts[i] = { px + vx * t, py + vy * t + 0.5 * g * t * t }
	end
	return pts
end

local function show_aim(px, py)
	local w = world("lab")
	local vx, vy = (SLING[1] - px) * POWER, (SLING[2] - py) * POWER
	w:set("ammo", { pos = { px - 11, py - 11 } })
	for i = 1, BAND do
		local t = i / (BAND + 1)
		w:set("band" .. i, { pos = { SLING[1] + (px - SLING[1]) * t - 2.5, SLING[2] + (py - SLING[2]) * t - 2.5 } })
	end
	for i, p in ipairs(arc_from(px, py, vx, vy)) do w:set("arc" .. i, { pos = { p[1] - 3, p[2] - 3 } }) end
end

local function hide_aim()
	local w = world("lab")
	state.aim = nil
	w:set("ammo", { pos = { SLING[1] - 11, SLING[2] - 11 } })
	for i = 1, BAND do w:set("band" .. i, { pos = { -40, -40 } }) end
	for i = 1, PIPS do w:set("arc" .. i, { pos = { -40, -40 } }) end
end

local function clamp_pull(x, y)
	local dx, dy = x - SLING[1], y - SLING[2]
	local d = math.sqrt(dx * dx + dy * dy)
	if d > PULL then dx, dy = dx / d * PULL, dy / d * PULL end
	return SLING[1] + dx, SLING[2] + dy
end

local function fire(px, py)
	local vx, vy = (SLING[1] - px) * POWER, (SLING[2] - py) * POWER
	state.nshot = state.nshot + 1
	local id = N("shot" .. state.nshot)
	state[id] = { x = px, y = py }
	state.shots[#state.shots + 1] = id
	if #state.shots > 3 then table.remove(state.shots, 1) end
	state.pending = { id = id, vx = vx, vy = vy }
	state.status = "Fire!"
	world("lab"):after(0.03, "launch")
end

local function on_drag(e)
	local w = world("lab")
	if e.phase == "start" then
		state.aim, state.held = nil, nil
		if e.shape == "ammo" then
			state.aim = true
		elseif grabbable[e.shape] then
			state.held = e.shape
		end
		return
	end
	if state.aim then
		local px, py = clamp_pull(e.x, e.y)
		if e.phase == "move" then
			show_aim(px, py)
		else
			local dx, dy = SLING[1] - px, SLING[2] - py
			hide_aim()
			if dx * dx + dy * dy > 144 then fire(px, py) end
		end
	elseif state.held then
		if e.phase == "end" then w:release(state.held) state.held = nil else w:grab(state.held, { e.x, e.y }) end
	end
end

local function on_zone(e)
	if e.phase ~= "enter" then return end
	local who = e.who
	if e.id == "killzone" then
		for _, name in ipairs({ "pig_roof", "pig_mid" }) do
			if who == N(name) and state.pigs[name] then
				state.pigs[name] = nil
				state.score = state.score + 100
				state.status = "Pig down!  +100"
			end
		end
	elseif e.id == "goal" and who == N("marble") and not state.goal then
		state.goal = true
		state.score = state.score + 500
		state.status = "GOAL!  +500"
	end
end

-- Lua cannot read where a hit landed, so a hard one flashes the whole sky instead.
local function on_hit(e)
	if e.speed > 450 and not state.flash then
		state.flash = true
		world("lab"):after(0.09, "unflash")
	end
end

local function on_timer(e)
	if e.name == "unflash" then state.flash = nil end
	if e.name == "launch" and state.pending then
		local p = state.pending
		state.pending = nil
		world("lab"):set(p.id, { velocity = { p.vx, p.vy } })
	end
end

local function reset()
	gen = gen + 1
	state = fresh()
	state.status = "Reset"
	world("lab"):cancel("launch")
end

local function cycle_gravity()
	if state.gname == "normal" then state.g, state.gname = G_MOON, "moon"
	elseif state.gname == "moon" then state.g, state.gname = G_FLIP, "flip"
	else state.g, state.gname = G_NORMAL, "normal" end
	state.status = "Gravity: " .. state.gname
end

local function button(id, label, on_click)
	return ui.button({
		id = id, px = 16, py = 8, radius = 8, fill = C.button, hover_fill = C.button_hover, press_scale = 0.97,
		ui.text({ label, color = C.text, font_size = 14, no_wrap = true }),
		on_click = on_click,
	})
end

return function()
	local lab = {
		id = "lab", width = W, height = H, radius = 10,
		gravity = state.g,
		wells = state.well and { { at = { 490, 330 }, radius = 200, pull = 1700 } } or {},
		joints = joints(),
		on_drag = on_drag, on_zone = on_zone, on_timer = on_timer, on_hit = on_hit,
	}
	for _, e in ipairs(build()) do lab[#lab + 1] = e end

	return ui.col({
		full = true, center = true, gap = 10, fill = C.bg,
		ui.row({
			gap = 28, align_center = true,
			ui.text({ "SIEGE", color = C.accent, font_size = 20, no_wrap = true }),
			ui.text({ "score " .. state.score, id = "score", color = C.text, font_size = 16, no_wrap = true }),
			ui.text({ state.status, id = "status", color = C.muted, font_size = 14, no_wrap = true }),
		}),
		ui.world(lab),
		ui.row({
			gap = 10,
			button("swing", "Swing", function() world("lab"):push(N("bob"), { 520, -60 }) state.status = "Wrecking ball!" end),
			button("bomb", "Bomb", function() world("lab"):blast({ 486, 400 }, { speed = 620, radius = 200 }) state.status = "BOOM" end),
			button("gravity", "Gravity: " .. state.gname, cycle_gravity),
			button("well", state.well and "Well: on" or "Well: off", function()
				state.well = not state.well
				state.status = state.well and "Gravity well pulls on the castle" or "Well closed"
			end),
			button("reset", "Reset", reset),
		}),
	})
end
