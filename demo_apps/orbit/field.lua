local C = require("theme")

local TAU = math.pi * 2

local M = {
	sprites = {},
	by_id = {},
	hot = nil,
	sx = 0,
	sy = 0,
	pin = nil,
	grab = nil,
	gx = 0,
	gy = 0,
	drifting = 0,
	t = 0,
}

-- Lehmer LCG: the field must look the same on every open, and math.random has no seed here.
local function rng(seed)
	local s = seed % 2147483647
	return function()
		s = (16807 * s) % 2147483647
		return s / 2147483647
	end
end

local function clamp(v, lo, hi)
	if lo > hi then
		return (lo + hi) * 0.5
	end
	return v < lo and lo or (v > hi and hi or v)
end

-- The whole orbit has to stay inside the field: the centre can go no closer to an edge than the
-- orbit's own radius, which for a rotated ellipse is its major radius.
local function keep_in(s)
	local m = C.margin + s.a
	s.cx = clamp(s.cx, m, C.field_w - m)
	s.cy = clamp(s.cy, m, C.field_h - m)
end

local function build(n)
	local r = rng(20260918)
	for i = 1, n do
		local tilt = r() * TAU
		local a = C.orbit_min + r() * C.orbit_span
		local m = C.margin + a
		local sprite = {
			id = "orbit:" .. i,
			cx = m + r() * math.max(0, C.field_w - m * 2),
			cy = m + r() * math.max(0, C.field_h - m * 2),
			a = a,
			b = a * (0.2 + r() * 0.75),
			ct = math.cos(tilt),
			st = math.sin(tilt),
			w = (0.12 + r() * 0.5) * (r() < 0.5 and -1 or 1),
			phase = r() * TAU,
			k = 0.55 + r() * 0.7,
			vx = 0,
			vy = 0,
			held = false,
			x = 0,
			y = 0,
		}
		M.sprites[i] = sprite
		M.by_id[sprite.id] = sprite
	end
end

-- A flung sprite carries its whole orbit with it, bouncing off the field and losing speed.
local function drift(s, dt)
	s.cx = s.cx + s.vx * dt
	s.cy = s.cy + s.vy * dt

	local m = C.margin + s.a
	local hx, hy = C.field_w - m, C.field_h - m
	if s.cx < m then
		s.cx, s.vx = m, -s.vx * C.bounce
	elseif s.cx > hx then
		s.cx, s.vx = hx, -s.vx * C.bounce
	end
	if s.cy < m then
		s.cy, s.vy = m, -s.vy * C.bounce
	elseif s.cy > hy then
		s.cy, s.vy = hy, -s.vy * C.bounce
	end

	local d = math.exp(-C.fling_damp * dt)
	s.vx, s.vy = s.vx * d, s.vy * d
	if s.vx * s.vx + s.vy * s.vy < C.rest * C.rest then
		s.vx, s.vy = 0, 0
	end
end

function M.step(dt)
	M.t = M.t + dt
	local t = M.t
	local sprites = M.sprites
	local drifting = 0

	for i = 1, #sprites do
		local s = sprites[i]
		if s.held then
			-- Rewinding the phase by exactly this tick freezes the angle, so a held sprite sits
			-- still in your hand instead of orbiting out of it.
			s.phase = s.phase - s.w * dt
		elseif s.vx ~= 0 or s.vy ~= 0 then
			drift(s, dt)
			drifting = drifting + 1
		end
		local ang = s.phase + s.w * t
		local u = math.cos(ang) * s.a
		local v = math.sin(ang) * s.b
		s.x = s.cx + u * s.ct - v * s.st
		s.y = s.cy + u * s.st + v * s.ct
	end
	M.drifting = drifting

end

function M.grab_at(s, sx, sy, t)
	M.release()
	if not s then
		return
	end
	s.held = true
	s.vx, s.vy = 0, 0
	M.grab =
		{ s = s, cx = s.cx, cy = s.cy, px = s.cx, py = s.cy, pt = t, vx = 0, vy = 0, sx = sx, sy = sy }
	M.gx, M.gy = sx, sy
end

function M.hold_to(dx, dy, t)
	local g = M.grab
	if not g then
		return
	end
	g.s.cx, g.s.cy = g.cx + dx, g.cy + dy
	keep_in(g.s)
	-- Throw speed off the pointer's own clock, so this no longer needs on_frame to hold a
	-- stopwatch. Measured over at least `C.fling_window` rather than between consecutive events:
	-- a mouse can report twice in a millisecond, and dividing by that turns a 1px jitter into
	-- hundreds of px/s.
	local d = t - g.pt
	if d >= C.fling_window then
		g.vx = (g.s.cx - g.px) / d
		g.vy = (g.s.cy - g.py) / d
		g.px, g.py, g.pt = g.s.cx, g.s.cy, t
	end
end

function M.release()
	local g = M.grab
	if not g then
		return
	end
	local s = g.s
	s.held = false
	local vx, vy = g.vx, g.vy
	local sp = math.sqrt(vx * vx + vy * vy)
	if sp < C.rest then
		vx, vy = 0, 0
	elseif sp > C.max_fling then
		vx, vy = vx * C.max_fling / sp, vy * C.max_fling / sp
	end
	s.vx, s.vy = vx, vy
	M.grab = nil
end

build(C.count)
M.step(0)

return M
