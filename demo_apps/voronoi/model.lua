local C = require("theme")

local M = {
	sites = {},
	paused = false,
	sel = nil,
	hit = nil,
	drag = nil,
}

local seeds = {
	{ 110, 92, 1, 0.4 },
	{ 302, 58, -0.6, 1 },
	{ 524, 124, 0.3, -1 },
	{ 688, 74, -1, 0.5 },
	{ 158, 302, 0.8, -0.7 },
	{ 382, 248, -0.4, -1 },
	{ 598, 340, 1, 0.2 },
	{ 700, 398, -0.9, -0.6 },
}

for i, s in ipairs(seeds) do
	local len = math.sqrt(s[3] * s[3] + s[4] * s[4])
	M.sites[i] = { x = s[1], y = s[2], dx = s[3] / len, dy = s[4] / len }
end

function M.step(e)
	local dt, elapsed = e.dt, e.elapsed
	local hi_x, hi_y = C.w - C.margin, C.h - C.margin
	local held = M.drag and M.drag.i
	for i = 1, #M.sites do
		local s = M.sites[i]
		if i ~= held then
			local turn = math.sin(elapsed * 0.27 + i * 1.7) * C.swirl * dt
			local c, n = math.cos(turn), math.sin(turn)
			s.dx, s.dy = c * s.dx - n * s.dy, n * s.dx + c * s.dy
			s.x = s.x + s.dx * C.speed * dt
			s.y = s.y + s.dy * C.speed * dt
			if s.x < C.margin then
				s.x, s.dx = C.margin, -s.dx
			elseif s.x > hi_x then
				s.x, s.dx = hi_x, -s.dx
			end
			if s.y < C.margin then
				s.y, s.dy = C.margin, -s.dy
			elseif s.y > hi_y then
				s.y, s.dy = hi_y, -s.dy
			end
		end
	end
end

-- Both kinds of shape are drawn in their site's own frame, so the press's `sx, sy` is the grab
-- offset from that site either way. Keeping it from the press is the whole trick: the live
-- `sx, sy` is measured against the transform the shape had when it was grabbed, so it drifts by
-- exactly the distance dragged and cannot be used as an offset a second time.
function M.grab(shape, sx, sy)
	local i = M.index_of(shape)
	M.drag = i and { i = i, shape = shape, sx = sx, sy = sy } or nil
end

function M.drag_to(x, y)
	local d = M.drag
	if not d then
		return
	end
	local s = M.sites[d.i]
	s.x = math.max(C.margin, math.min(C.w - C.margin, x - d.sx))
	s.y = math.max(C.margin, math.min(C.h - C.margin, y - d.sy))
end

function M.index_of(shape)
	return shape and tonumber(shape:match(":(%d+)$"))
end

return M
