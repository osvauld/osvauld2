local C = require("theme")
local P = {}

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- The map's platforms plus four walls just outside it, so nothing leaves the map.
function P.solids(map)
	local out = {
		{ x = 0, y = -30, w = map.w, h = 30 }, { x = 0, y = map.h, w = map.w, h = 30 },
		{ x = -30, y = 0, w = 30, h = map.h }, { x = map.w, y = 0, w = 30, h = map.h },
	}
	for _, p in ipairs(map.platforms) do table.insert(out, p) end
	return out
end

-- Earliest segment intersection. Expansion also makes this a swept actor/cover query.
function P.segment(x, y, dx, dy, rect, rx, ry)
	local lo, hi = 0, 1
	local function axis(p, d, a, b)
		if math.abs(d) < 1e-9 then return p >= a and p <= b end
		local t, u = (a - p) / d, (b - p) / d
		lo, hi = math.max(lo, math.min(t, u)), math.min(hi, math.max(t, u))
		return lo <= hi
	end
	if axis(x, dx, rect.x - rx, rect.x + rect.w + rx)
		and axis(y, dy, rect.y - ry, rect.y + rect.h + ry) then return lo end
	return nil
end

function P.blocked(solid, x, y, tx, ty)
	for _, r in ipairs(solid) do
		if P.segment(x, y, tx - x, ty - y, r, 0, 0) then return true end
	end
	return false
end

function P.move(a, dt, map, solid)
	local dx, dy = a.vx * dt, a.vy * dt
	local nx = a.x + dx
	for _, r in ipairs(solid) do
		if a.y + C.ry > r.y + 0.01 and a.y - C.ry < r.y + r.h - 0.01 then
			if dx > 0 and a.x + C.rx <= r.x + 0.01 and nx + C.rx >= r.x then
				nx, a.vx = math.min(nx, r.x - C.rx), 0
			elseif dx < 0 and a.x - C.rx >= r.x + r.w - 0.01 and nx - C.rx <= r.x + r.w then
				nx, a.vx = math.max(nx, r.x + r.w + C.rx), 0
			end
		end
	end
	a.x = clamp(nx, C.rx, map.w - C.rx)
	local ny = a.y + dy
	a.grounded = false
	for _, r in ipairs(solid) do
		if a.x + C.rx > r.x + 0.01 and a.x - C.rx < r.x + r.w - 0.01 then
			if dy >= 0 and a.y + C.ry <= r.y + 0.01 and ny + C.ry >= r.y then
				ny, a.vy, a.grounded = math.min(ny, r.y - C.ry), 0, true
			elseif dy < 0 and a.y - C.ry >= r.y + r.h - 0.01 and ny - C.ry <= r.y + r.h then
				ny, a.vy = math.max(ny, r.y + r.h + C.ry), 0
			end
		end
	end
	a.y = clamp(ny, C.ry, map.h - C.ry)
end

return P
