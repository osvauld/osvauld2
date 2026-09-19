local geom = {}

local K = 0.5522847498307936

function geom.circle_path(cx, cy, radius)
	local k = radius * K
	return {
		{ "move", cx + radius, cy },
		{ "cubic", cx + radius, cy + k, cx + k, cy + radius, cx, cy + radius },
		{ "cubic", cx - k, cy + radius, cx - radius, cy + k, cx - radius, cy },
		{ "cubic", cx - radius, cy - k, cx - k, cy - radius, cx, cy - radius },
		{ "cubic", cx + k, cy - radius, cx + radius, cy - k, cx + radius, cy },
		{ "close" },
	}
end

local function ellipse(commands, cx, cy, rx, ry)
	local kx, ky = rx * K, ry * K
	table.insert(commands, { "move", cx + rx, cy })
	table.insert(commands, { "cubic", cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry })
	table.insert(commands, { "cubic", cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy })
	table.insert(commands, { "cubic", cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry })
	table.insert(commands, { "cubic", cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy })
	table.insert(commands, { "close" })
end

function geom.ellipse_path(cx, cy, rx, ry)
	local commands = {}
	ellipse(commands, cx, cy, rx, ry)
	return commands
end

function geom.translate(x, y)
	return { 1, 0, 0, 1, x, y }
end

function geom.scale(x, y)
	return { x, 0, 0, y or x, 0, 0 }
end

function geom.rotate(angle)
	local c, s = math.cos(angle), math.sin(angle)
	return { c, s, -s, c, 0, 0 }
end

function geom.mul(a, b)
	return {
		a[1] * b[1] + a[3] * b[2],
		a[2] * b[1] + a[4] * b[2],
		a[1] * b[3] + a[3] * b[4],
		a[2] * b[3] + a[4] * b[4],
		a[1] * b[5] + a[3] * b[6] + a[5],
		a[2] * b[5] + a[4] * b[6] + a[6],
	}
end

function geom.compose(transforms)
	local result = { 1, 0, 0, 1, 0, 0 }
	for _, transform in ipairs(transforms) do result = geom.mul(result, transform) end
	return result
end

return geom
