-- CURSOR
--
-- A pointer drawn by the app: it follows the real one, shows the button, and takes no input —
-- its shapes are unnamed, so the pointer falls through them. Mount it on the app's root:
--
--   on_hover = cursor.track,   -- or call cursor.track(e) from your own on_hover
--   system_cursor = false,     -- hide the OS pointer, or a window shows both
--   ...,
--   cursor.view(),             -- the last child, so it paints on top
--
-- What it looks like is up to whatever it points at: an element declares `cursor = "grab"` (or
-- `"text"`, `"crosshair"`), or `cursor = <gfx.frame>` to supply its own drawing, centred on the
-- pointer. Unknown names draw the arrow.
--
-- `require` only reaches the app's own files, so an app carries a copy; this is the canonical one.

local M = {}

local C = {
	up = "#f5f5f5",
	down = "#ffc933",
	edge = "#111318",
	ring = "#ffc933",
}

local x, y, shown, down, look = 0, 0, false, false, nil

-- A rounded rectangle as path commands — gfx has no arcs, so the corners are quads.
local function rrect(px, py, w, h, r)
	return {
		{ "move", px + r, py },
		{ "line", px + w - r, py },
		{ "quad", px + w, py, px + w, py + r },
		{ "line", px + w, py + h - r },
		{ "quad", px + w, py + h, px + w - r, py + h },
		{ "line", px + r, py + h },
		{ "quad", px, py + h, px, py + h - r },
		{ "line", px, py + r },
		{ "quad", px, py, px + r, py },
		{ "close" },
	}
end

-- Filled shapes with a dark edge, so a look reads on light and dark alike.
local function shapes(w, h, fill, outlines)
	local items = { width = w, height = h }
	for _, o in ipairs(outlines) do
		items[#items + 1] = gfx.fill({ path = gfx.path(o), brush = gfx.solid(fill) })
		items[#items + 1] = gfx.stroke({ path = gfx.path(o), brush = gfx.solid(C.edge), width = 1.5, join = "round" })
	end
	return gfx.frame(items)
end

-- Lines with a light halo underneath, for thin looks (I-beam, crosshair).
local function strokes(w, h, lines)
	local items = { width = w, height = h }
	for _, pass in ipairs({ { C.up, 4 }, { C.edge, 1.5 } }) do
		for _, l in ipairs(lines) do
			items[#items + 1] = gfx.stroke({ path = gfx.path(l), brush = gfx.solid(pass[1]), width = pass[2], cap = "round" })
		end
	end
	return gfx.frame(items)
end

local ARROW = {
	{ "move", 1, 1 },
	{ "line", 1, 22 },
	{ "line", 6.5, 17 },
	{ "line", 11, 27 },
	{ "line", 15, 25 },
	{ "line", 10.5, 16 },
	{ "line", 17.5, 16 },
	{ "close" },
}

local function hand(finger)
	local parts = { rrect(3, 9, 16, 13, 5) }
	for i = 0, 3 do
		parts[#parts + 1] = rrect(4 + i * 3.6, 10 - finger, 3.4, finger + 4, 1.7)
	end
	parts[#parts + 1] = rrect(0.8, 11, 4.5, 7, 2.2) -- thumb
	return parts
end

-- Built once: visuals are immutable resources. Each look is a visual and its hotspot.
local LOOKS = {
	arrow = { up = shapes(20, 28, C.up, { ARROW }), down = shapes(20, 28, C.down, { ARROW }), hx = 1, hy = 1 },
	grab = {
		up = shapes(20, 23, C.up, hand(8)),
		down = shapes(20, 23, C.down, hand(2)),
		hx = 11,
		hy = 13,
	},
	text = (function()
		local v = strokes(12, 22, {
			{ { "move", 6, 2 }, { "line", 6, 20 } },
			{ { "move", 2, 2 }, { "line", 10, 2 } },
			{ { "move", 2, 20 }, { "line", 10, 20 } },
		})
		return { up = v, down = v, hx = 6, hy = 11 }
	end)(),
	crosshair = (function()
		local v = strokes(23, 23, {
			{ { "move", 11.5, 2 }, { "line", 11.5, 8 } },
			{ { "move", 11.5, 15 }, { "line", 11.5, 21 } },
			{ { "move", 2, 11.5 }, { "line", 8, 11.5 } },
			{ { "move", 15, 11.5 }, { "line", 21, 11.5 } },
		})
		return { up = v, down = v, hx = 11.5, hy = 11.5 }
	end)(),
}

-- On the app root, `e.x, e.y` are viewport coordinates — what `absolute` places by.
function M.track(e)
	x, y, down, look = e.x, e.y, e.down, e.look
	shown = e.phase ~= "leave"
end

function M.view()
	if not shown then
		return {}
	end
	local name, visual, hx, hy
	if type(look) == "userdata" then
		-- Supplied by the element: its centre is the hotspot.
		name, visual, hx, hy = "visual", look, look.width / 2, look.height / 2
	else
		local l = LOOKS[look] or LOOKS.arrow
		name = LOOKS[look] and look or "arrow"
		visual, hx, hy = down and l.down or l.up, l.hx, l.hy
	end
	return {
		-- One id for the ring across press and release, so the release is a retained fade from
		-- what was showing rather than a new element appearing at zero.
		ui.col({
			id = "cursor-ring",
			absolute = true,
			left = x - 14,
			top = y - 14,
			w = 28,
			h = 28,
			radius = 14,
			stroke = { 2, C.ring },
			fill = "rgba(255,201,51,0.25)",
			fade = down and { 1, 60 } or { 0, 320 },
		}),
		-- A new id per look, so switching looks fades the new one in.
		ui.frame({
			id = "cursor:" .. name,
			visual = visual,
			absolute = true,
			left = x - hx,
			top = y - hy,
			fade_in = 90,
		}),
	}
end

return M
