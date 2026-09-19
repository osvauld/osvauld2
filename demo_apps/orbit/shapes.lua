local K = 0.5522847498307936

local M = {}

function M.circle(cx, cy, r)
	local k = r * K
	return gfx.path({
		{ "move", cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
		{ "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
		{ "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "close" },
	})
end

-- Ellipse with radii a/b, rotated by the unit vector (ct, st), centred on (cx, cy).
function M.ellipse(cx, cy, a, b, ct, st)
	local ka, kb = a * K, b * K
	local function at(u, v)
		return cx + u * ct - v * st, cy + u * st + v * ct
	end
	local x0, y0 = at(a, 0)
	local x1, y1 = at(0, b)
	local x2, y2 = at(-a, 0)
	local x3, y3 = at(0, -b)
	local a1x, a1y = at(a, kb)
	local a2x, a2y = at(ka, b)
	local b1x, b1y = at(-ka, b)
	local b2x, b2y = at(-a, kb)
	local c1x, c1y = at(-a, -kb)
	local c2x, c2y = at(-ka, -b)
	local d1x, d1y = at(ka, -b)
	local d2x, d2y = at(a, -kb)
	return gfx.path({
		{ "move", x0, y0 },
		{ "cubic", a1x, a1y, a2x, a2y, x1, y1 },
		{ "cubic", b1x, b1y, b2x, b2y, x2, y2 },
		{ "cubic", c1x, c1y, c2x, c2y, x3, y3 },
		{ "cubic", d1x, d1y, d2x, d2y, x0, y0 },
		{ "close" },
	})
end

return M
