local C = require("theme")

-- The selected part's path points as canvas dots. Each dot is its own part, so a press on it
-- names it: `pt:<shape>:<command>:<k>`, k counting the command's points — its last is the end
-- point (filled), any before it curve controls (hollow, tied to their end by a guide line).
local H = {}

-- Sizes are on screen: `s`, the viewport's zoom, divides them out so they never grow with it.
local R = 4 -- a dot's radius

local function dot(id, x, y, control, s)
	local r = R / s
	local k = r * 0.5523 -- a circle's cubic handle length
	local path = {
		{ "move", x + r, y },
		{ "cubic", x + r, y + k, x + k, y + r, x, y + r },
		{ "cubic", x - k, y + r, x - r, y + k, x - r, y },
		{ "cubic", x - r, y - k, x - k, y - r, x, y - r },
		{ "cubic", x + k, y - r, x + r, y - k, x + r, y },
		{ "close" },
	}
	local fill = control and C.canvas or C.accent
	local ring = control and C.accent or C.canvas
	return { id = id, pivot = { x, y }, shapes = { { path = path, fill = fill, stroke = { 1.5 / s, ring } } } }
end

-- A curve's first control leans on where the curve starts; every later one, on where it ends.
local function guide(lines, from, to, s)
	local path = { { "move", from[1], from[2] }, { "line", to[1], to[2] } }
	table.insert(lines, { path = path, stroke = { 1 / s, C.accent } })
end

-- Parts to append to the canvas drawing for `part`, at canvas scale `z`, seen at zoom `s`.
function H.parts(part, z, s)
	local lines, dots = {}, {}
	for i, shape in ipairs(part.shapes or {}) do
		local last = nil -- the previous command's end: this one's start
		for j, cmd in ipairs(shape.path or {}) do -- a shadow has no path
			local pts = {}
			for k = 2, #cmd - 1, 2 do
				table.insert(pts, { cmd[k] * z, cmd[k + 1] * z })
			end
			local stop = pts[#pts]
			for k, p in ipairs(pts) do
				if k < #pts then
					guide(lines, k == 1 and last or stop, p, s)
				end
				table.insert(dots, dot("pt:" .. i .. ":" .. j .. ":" .. k, p[1], p[2], k < #pts, s))
			end
			last = stop or last
		end
	end
	-- The guides go first, under the dots; a press on one is no point.
	local out = #lines > 0 and { { id = "pt:guides", pivot = { 0, 0 }, shapes = lines } } or {}
	for _, d in ipairs(dots) do
		table.insert(out, d)
	end
	return out
end

return H
