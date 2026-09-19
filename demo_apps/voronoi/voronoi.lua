local M = {}

-- Sutherland-Hodgman against one half-plane: keep the part of `poly` at least as close to `a`
-- as to `b`, i.e. where d·p <= d·m for d = b - a and m the midpoint of ab.
local function clip(poly, a, b)
	local dx, dy = b.x - a.x, b.y - a.y
	local c = dx * (a.x + b.x) * 0.5 + dy * (a.y + b.y) * 0.5
	local out, n = {}, #poly
	for i = 1, n do
		local p, q = poly[i], poly[i % n + 1]
		local dp = dx * p[1] + dy * p[2] - c
		local dq = dx * q[1] + dy * q[2] - c
		if dp <= 0 then
			out[#out + 1] = p
		end
		-- only a strict crossing makes a new vertex; a vertex on the line is already in `out`
		if (dp < 0 and dq > 0) or (dp > 0 and dq < 0) then
			local t = dp / (dp - dq)
			out[#out + 1] = { p[1] + t * (q[1] - p[1]), p[2] + t * (q[2] - p[2]) }
		end
	end
	return out
end

function M.cell(sites, i, w, h)
	local a = sites[i]
	local poly = { { 0, 0 }, { w, 0 }, { w, h }, { 0, h } }
	for j = 1, #sites do
		local b = sites[j]
		if j ~= i then
			if b.x == a.x and b.y == a.y then
				-- coincident sites have no bisector; the lower index keeps the cell so the
				-- cells still tile instead of two of them claiming the same ground
				if j < i then
					return {}
				end
			else
				poly = clip(poly, a, b)
				if #poly < 3 then
					return poly
				end
			end
		end
	end
	return poly
end

function M.cells(sites, w, h)
	local out = {}
	for i = 1, #sites do
		out[i] = M.cell(sites, i, w, h)
	end
	return out
end

function M.area(poly)
	local a, n = 0, #poly
	for i = 1, n do
		local p, q = poly[i], poly[i % n + 1]
		a = a + p[1] * q[2] - q[1] * p[2]
	end
	return a * 0.5
end

function M.nearest(sites, x, y)
	local best, d2 = 1, math.huge
	for i = 1, #sites do
		local dx, dy = x - sites[i].x, y - sites[i].y
		local d = dx * dx + dy * dy
		if d < d2 then
			best, d2 = i, d
		end
	end
	return best
end

function M.contains(poly, x, y)
	local n = #poly
	for i = 1, n do
		local p, q = poly[i], poly[i % n + 1]
		if (q[1] - p[1]) * (y - p[2]) - (q[2] - p[2]) * (x - p[1]) < 0 then
			return false
		end
	end
	return true
end

return M
