-- The monster truck's look, cut out of one SVG: "Monster Truck" by Eli Miller, CC0
-- (opengameart.org/content/monster-truck), unchanged in art/monster_truck.svg.
--
-- The artist layered it by material (Struts, Truck, then Tire = Wheel/Tread/Hub with both wheels
-- in each), not by object, so the cut is here: the body is the strut and truck parts, and each
-- wheel is the tyre shapes on its side of the middle. It faces left and we drive right, so
-- everything is mirrored (x → -x), drawn at SCALE and moved into its body's own box — the frame
-- vehicle.lua's V.truck measures from.
local T = {}

local SCALE = 0.5
local BODY = { g5322 = true, g5333 = true, g5347 = true, g5361 = true, g5369 = true, g5377 = true, layer1 = true }
local TYRE = { layer7 = true, layer6 = true, layer9 = true }
local MIDDLE = 315                                  -- between the wheels, in the SVG's own units
local BOX = { 283, 82.5 }                           -- the chassis box's top-right, scaled units
-- Wheel centres, scaled units, unmirrored; mirrored, the SVG's right wheel is our rear one.
local WHEELS = { { 225.75, 151 }, { 97.5, 151 } }

-- A shape mirrored and moved by (x, y) → (-x·s + dx, y·s + dy): path, gradient geometry, stroke.
local function moved(shape, s, dx, dy)
	local out = { rule = shape.rule }
	local path = {}
	for _, c in ipairs(shape.path) do
		local m = { c[1] }
		for i = 2, #c, 2 do
			m[i], m[i + 1] = -c[i] * s + dx, c[i + 1] * s + dy
		end
		table.insert(path, m)
	end
	out.path = path
	local function point(p) return { -p[1] * s + dx, p[2] * s + dy } end
	local f = shape.fill
	if type(f) == "table" and f.linear then
		local g = f.linear
		out.fill = { linear = { from = point(g.from), to = point(g.to), stops = g.stops, extend = g.extend } }
	elseif type(f) == "table" and f.radial then
		local g = f.radial
		out.fill = { radial = {
			center = point(g.center), radius = g.radius * s, stops = g.stops, extend = g.extend,
			focus = g.focus and point(g.focus), focus_radius = g.focus_radius and g.focus_radius * s,
		} }
	else
		out.fill = f
	end
	if shape.stroke then
		local st = shape.stroke
		local dashes
		if st.dashes then
			dashes = {}
			for i, d in ipairs(st.dashes) do dashes[i] = d * s end
		end
		out.stroke = { st[1] * s, st[2], cap = st.cap, join = st.join, miter_limit = st.miter_limit,
			dashes = dashes, dash_offset = st.dash_offset and st.dash_offset * s }
	end
	return out
end

local function centre_x(shape)
	local lo, hi = math.huge, -math.huge
	for _, c in ipairs(shape.path) do
		for i = 2, #c, 2 do lo, hi = math.min(lo, c[i]), math.max(hi, c[i]) end
	end
	return (lo + hi) / 2
end

-- One part, every shape `keep` accepts, in draw order, moved into a w×h box.
local function cut(spec, keep, w, h, dx, dy)
	local shapes = {}
	for _, part in ipairs(spec.parts) do
		for _, shape in ipairs(part.shapes or {}) do
			if keep(part.id, shape) then table.insert(shapes, moved(shape, SCALE, dx, dy)) end
		end
	end
	assert(#shapes > 0, "monster_truck.svg: a cut found no shapes")
	return gfx.drawing({ size = { w, h }, parts = { { id = "it", pivot = { w / 2, h / 2 }, shapes = shapes } } })
end

-- Drawings by entity id, for bodies of `sizes` (setup.lua's boxes, units).
function T.looks(sizes)
	local spec = gfx.svg("art/monster_truck.svg")
	local out = {}
	local cw, ch = sizes.chassis[1], sizes.chassis[2]
	out.chassis = cut(spec, function(id) return BODY[id] end, cw, ch, BOX[1], -BOX[2])
	for i, at in ipairs(WHEELS) do
		local d = sizes["wheel" .. i][1]
		local left = at[1] * 2 < MIDDLE -- which side of the SVG this wheel's shapes are on
		out["wheel" .. i] = cut(spec, function(id, shape)
			return TYRE[id] and ((centre_x(shape) < MIDDLE) == left)
		end, d, d, d / 2 + at[1], d / 2 - at[2])
	end
	-- The cab is the head: drawn by the chassis, so the sensor body itself shows nothing.
	local hd = sizes.head[1]
	out.head = gfx.drawing({ size = { hd, hd }, parts = { { id = "it", pivot = { hd / 2, hd / 2 } } } })
	return out
end

return T
