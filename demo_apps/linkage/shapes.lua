local C = require("theme")

local S = {}

local KAPPA = 0.5523

function S.circle(cx, cy, r)
	local k = r * KAPPA
	return gfx.path({
		{ "move", cx + r, cy },
		{ "cubic", cx + r, cy + k, cx + k, cy + r, cx, cy + r },
		{ "cubic", cx - k, cy + r, cx - r, cy + k, cx - r, cy },
		{ "cubic", cx - r, cy - k, cx - k, cy - r, cx, cy - r },
		{ "cubic", cx + k, cy - r, cx + r, cy - k, cx + r, cy },
		{ "close" },
	})
end

-- A limb runs along its own +x axis from 0 to len, so shape-local sx is distance along it.
function S.limb(len, w0, w1)
	return gfx.path({
		{ "move", 0, -w0 },
		{ "line", len, -w1 },
		{ "quad", len + w1 * 0.7, 0, len, w1 },
		{ "line", 0, w0 },
		{ "quad", -w0 * 0.7, 0, 0, -w0 },
		{ "close" },
	})
end

function S.palm(len, w)
	return gfx.path({
		{ "move", 0, -w },
		{ "line", len * 0.62, -w * 1.15 },
		{ "quad", len * 0.78, 0, len * 0.62, w * 1.15 },
		{ "line", 0, w },
		{ "close" },
	})
end

function S.finger(y0, y1)
	local x0 = C.hand_len * 0.58
	return gfx.path({
		{ "move", x0, y0 },
		{ "quad", C.hand_len * 0.86, y0, C.hand_len, y1 },
	})
end

return S
