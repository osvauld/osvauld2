-- pie.lua — pure functions of their arguments. Handlers are passed in, never reached for.
local C = require("theme")
local D = require("data")

local M = {}

local R = C.radius
local CX, CY = C.size / 2, C.size / 2
local TAU = math.pi * 2

-- Where each slice begins and how far it sweeps. Angles run clockwise from twelve o'clock,
-- which is also the direction screen y grows, so a positive rotation is the one you expect.
function M.layout()
	local out, a = {}, -math.pi / 2
	for i, s in ipairs(D.slices) do
		local sweep = TAU * s.value / D.total
		out[i] = { slice = s, start = a, sweep = sweep }
		a = a + sweep
	end
	return out
end

-- One wedge, built at the origin opening from angle zero. Every slice is this same shape rotated
-- into place, which is the point: `sx, sy` come back in *these* coordinates, upright, whatever
-- angle the slice ended up at. There are no arc commands, so the rim is a fan of short lines.
local function wedge(sweep)
	local cmds = { { "move", 0, 0 }, { "line", R, 0 } }
	local steps = math.max(2, math.ceil(sweep / 0.06))
	for i = 1, steps do
		local a = sweep * i / steps
		cmds[#cmds + 1] = { "line", R * math.cos(a), R * math.sin(a) }
	end
	cmds[#cmds + 1] = { "close" }
	return gfx.path(cmds)
end

-- Rotate by `a`, then move to the pie's centre, nudged `out` along the given bisector.
local function place(a, out, bisector)
	local dx = CX + math.cos(bisector) * out
	local dy = CY + math.sin(bisector) * out
	return { math.cos(a), math.sin(a), -math.sin(a), math.cos(a), dx, dy }
end

local function disc(r)
	local cmds = { { "move", CX + r, CY } }
	for i = 1, 48 do
		local a = TAU * i / 48
		cmds[#cmds + 1] = { "line", CX + r * math.cos(a), CY + r * math.sin(a) }
	end
	cmds[#cmds + 1] = { "close" }
	return gfx.path(cmds)
end

-- `hot` and `sel` are shape ids — the very strings the runtime hands back to on_hover/on_click.
function M.visual(hot, sel)
	local items = {}
	for _, w in ipairs(M.layout()) do
		local id = "slice:" .. w.slice.key
		local out = (id == hot and C.pop or 0) + (id == sel and C.pop / 2 or 0)
		items[#items + 1] = gfx.group({
			-- The name goes on the group, so the whole wedge answers as one shape and the fill
			-- inside it needs no name of its own.
			id = id,
			transform = place(w.start, out, w.start + w.sweep / 2),
			gfx.fill({ path = wedge(w.sweep), brush = gfx.solid(w.slice.color) }),
		})
	end

	-- Unnamed, and drawn last: the pointer falls straight through it to the slice beneath.
	items[#items + 1] = gfx.fill({ path = disc(R * 0.42), brush = gfx.solid(C.hub) })

	items.width = C.size
	items.height = C.size
	return gfx.frame(items)
end

-- What the pointer is standing on, in the slice's own upright coordinates. None of this is
-- inverted by hand: `sx, sy` arrive already rotated back by the runtime.
function M.reading(shape, sx, sy)
	local s = D.find(shape and shape:sub(7) or "")
	if not s then return nil end
	local into = math.deg(math.atan2(sy, sx))
	local depth = math.sqrt(sx * sx + sy * sy) / R
	return s, into, depth
end

return M
