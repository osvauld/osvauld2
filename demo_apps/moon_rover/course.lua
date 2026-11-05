local C = require("theme")
local height_scale = 1
local columns, rows = C.width / C.step + 1, C.depth / C.step + 1
local heights, vertices, indices = {}, {}, {}

local function height(x, z)
	local y = -0.2 * math.clamp(x + 6, 0, 4)
	if x > 2 and x < 6 then y = y + 0.16 * math.sin((x - 2) * math.pi / 2) * math.cos(z * 0.4) end
	local radius = math.sqrt((x - 9)^2 + (z - 2)^2)
	if radius < 3.5 then y = y - 0.9 * (1 - (radius / 3.5)^2)^2 end
	return y * height_scale
end

for row = 0, rows - 1 do
	heights[row + 1] = {}
	for col = 0, columns - 1 do
		heights[row + 1][col + 1] = height(col * C.step - C.width / 2, row * C.step - C.depth / 2)
	end
end

for row = 0, rows - 1 do
	for col = 0, columns - 1 do
		local left, right = math.max(col - 1, 0), math.min(col + 1, columns - 1)
		local back, front = math.max(row - 1, 0), math.min(row + 1, rows - 1)
		local dx = (heights[row + 1][right + 1] - heights[row + 1][left + 1]) / ((right - left) * C.step)
		local dz = (heights[front + 1][col + 1] - heights[back + 1][col + 1]) / ((front - back) * C.step)
		vertices[#vertices + 1] = {
			position = {col * C.step - C.width / 2, heights[row + 1][col + 1], row * C.step - C.depth / 2},
			normal = {-dx, 1, -dz},
		}
		if row < rows - 1 and col < columns - 1 then
			local a = row * columns + col + 1
			local b, c, d = a + 1, a + columns, a + columns + 1
			for _, index in ipairs({a, c, b, b, c, d}) do indices[#indices + 1] = index end
		end
	end
end

local function sample(x, z)
	local u, v = (x + C.width / 2) / C.step, (z + C.depth / 2) / C.step
	if u < 0 or v < 0 or u > columns - 1 or v > rows - 1 then return nil end
	local col, row = math.min(math.floor(u), columns - 2), math.min(math.floor(v), rows - 2)
	local fx, fz = u - col, v - row
	local a, b = heights[row + 1][col + 1], heights[row + 1][col + 2]
	local c, d = heights[row + 2][col + 1], heights[row + 2][col + 2]
	if fx + fz <= 1 then return a + fx * (b - a) + fz * (c - a) end
	return d + (1 - fx) * (c - d) + (1 - fz) * (b - d)
end

local function ribbon(out, ix, x1, z1, x2, z2, width, level)
	local dx, dz = x2 - x1, z2 - z1
	local length = math.sqrt(dx * dx + dz * dz)
	if length < 0.001 then return end
	local nx, nz = -dz / length * width / 2, dx / length * width / 2
	local base = #out
	for _, point in ipairs({{x1+nx,z1+nz},{x1-nx,z1-nz},{x2+nx,z2+nz},{x2-nx,z2-nz}}) do
		local y = level or sample(point[1], point[2]) or height(point[1], point[2])
		out[#out + 1] = {position = {point[1], y + C.guide_lift, point[2]}, normal = {0,1,0}}
	end
	for _, i in ipairs({1,3,2,2,3,4}) do ix[#ix + 1] = base + i end
end

local grid, grid_ix, contours, contour_ix = {}, {}, {}, {}
for x = -C.width/2+2, C.width/2-2, 2 do
	for z = -C.depth/2, C.depth/2-C.step, C.step do ribbon(grid, grid_ix, x, z, x, z+C.step, C.grid_width) end
end
for z = -C.depth/2+2, C.depth/2-2, 2 do
	for x = -C.width/2, C.width/2-C.step, C.step do ribbon(grid, grid_ix, x, z, x+C.step, z, C.grid_width) end
end
-- A level equal to a flat plateau has no unique contour and traces arbitrary cell edges.
for band = 1, 8 do
	local level = 0.1 - band * 0.2
	for i = 1, #indices, 3 do
		local points, hits = {}, {}
		for j = 0, 2 do points[j+1] = vertices[indices[i+j]].position end
		for j = 1, 3 do
			local a, b = points[j], points[j%3+1]
			if (a[2] < level and b[2] >= level) or (b[2] < level and a[2] >= level) then
				local t = (level - a[2]) / (b[2] - a[2])
				hits[#hits + 1] = {a[1] + t*(b[1]-a[1]), a[3] + t*(b[3]-a[3])}
			end
		end
		if #hits == 2 then
			ribbon(contours, contour_ix, hits[1][1], hits[1][2], hits[2][1], hits[2][2], C.contour_width, level)
		end
	end
end

return {
	mesh = gfx.mesh({vertices = vertices, indices = indices}), sample = sample,
	grid = gfx.mesh({vertices = grid, indices = grid_ix}),
	contours = gfx.mesh({vertices = contours, indices = contour_ix}),
}
