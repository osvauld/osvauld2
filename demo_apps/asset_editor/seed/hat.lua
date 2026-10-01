-- A top hat. Its plug is the middle of the brim: the point that sits on a `hat` socket.
local K = 0.5523 -- a quarter ellipse's cubic handle, as a fraction of the radius

local function ellipse(cx, cy, rx, ry)
	local kx, ky = K * rx, K * ry
	return {
		{ "move", cx + rx, cy },
		{ "cubic", cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry },
		{ "cubic", cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy },
		{ "cubic", cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry },
		{ "cubic", cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy },
		{ "close" },
	}
end

local ink = { 2, "#11121c" }

return {
	size = { 64, 50 },
	plug = { kind = "hat", at = { 32, 44 } },
	parts = {
		{ id = "hat", pivot = { 32, 44 }, shapes = {
			{ shadow = { rect = { 6, 40, 52, 10 }, radius = 5, blur = 2.5, color = "#00000044" } },
			{ path = ellipse(32, 43, 30, 6), fill = "#1d1e2c", stroke = ink },
			-- The crown: slightly wider at the top, lit from the left.
			{ path = { { "move", 15, 42 }, { "line", 12, 6 }, { "quad", 32, 0, 52, 6 }, { "line", 49, 42 },
				{ "quad", 32, 46, 15, 42 }, { "close" } },
				fill = { linear = { from = { 12, 0 }, to = { 52, 0 },
					stops = { { 0, "#4a4e6e" }, { 0.35, "#2b2d42" }, { 1, "#15161f" } } } },
				stroke = ink },
			{ path = { { "move", 14, 31 }, { "quad", 32, 35, 50, 31 }, { "line", 49, 39 }, { "quad", 32, 43, 15, 39 },
				{ "close" } },
				fill = { linear = { from = { 0, 31 }, to = { 0, 43 },
					stops = { { 0, "#ff5d6c" }, { 1, "#b81f2e" } } } } },
		} },
	},
}
