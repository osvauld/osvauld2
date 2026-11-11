-- The camera: presentation, never state. Pure Lua (no gfx, ui or net), for any number of fighters.
-- A view is { x, y, w, h, zoom } in stage units, x and y its top-left; zoom is ref_w / w, so zoom 1
-- shows 1 600 units across. `target` frames the boxes, `ease` walks a camera toward a target as a
-- pure function of (camera, target, dt). Nothing here may feed `step`.
local Depth = require("depth")
local C = {}

C.params = {
	ref_w = 1600, -- view width at zoom 1
	min_view = 1300, -- the closest it gets: one fighter alone, on the ground, with the top platform in view
	margin = 260, -- kept around every fighter in play
	zoom_min = 0.55, -- the furthest out (the stage's bounds can hold it closer)
	half_life = 0.12, -- seconds to close half of the gap
	snap = 0.05, -- closer than this (units) is there, when the screen scale is not known
	snap_px = 0.5, -- closer than this (screen px) is there: below it the view only shimmers
}

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

-- Fighters in play as boxes (feet at the bottom centre); out and ko are left out.
function C.boxes(fighters, body_w, body_h)
	local out = {}
	for _, f in ipairs(fighters) do
		if f.act ~= "out" and f.act ~= "ko" then
			out[#out + 1] = { x = f.x - body_w / 2, y = f.y - body_h, w = body_w, h = body_h }
		end
	end
	return out
end

-- The zoom range for a stage's camera bounds and a playfield aspect (w / h): the bounds can hold
-- the view further in than zoom_min, never further out.
function C.limits(bounds, aspect, p)
	p = p or C.params
	local zmin = math.max(p.zoom_min, p.ref_w / math.min(bounds.w, bounds.h * aspect))
	return zmin, math.max(zmin, p.ref_w / p.min_view)
end

-- The view framing every box with the margin, as close as the limits allow, kept inside `bounds`.
-- nil when no box is in play (the caller keeps the camera where it is).
function C.target(boxes, bounds, aspect, p)
	p = p or C.params
	if #boxes == 0 then return nil end
	local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
	for _, b in ipairs(boxes) do
		x0, y0 = math.min(x0, b.x), math.min(y0, b.y)
		x1, y1 = math.max(x1, b.x + b.w), math.max(y1, b.y + b.h)
	end
	local zmin, zmax = C.limits(bounds, aspect, p)
	local need = math.max(p.min_view, x1 - x0 + 2 * p.margin, (y1 - y0 + 2 * p.margin) * aspect)
	local w = clamp(need, p.ref_w / zmax, p.ref_w / zmin)
	local h = w / aspect
	return {
		x = clamp((x0 + x1) / 2 - w / 2, bounds.x, bounds.x + bounds.w - w),
		y = clamp((y0 + y1) / 2 - h / 2, bounds.y, bounds.y + bounds.h - h),
		w = w, h = h, zoom = p.ref_w / w,
	}
end

-- One step toward `target`: a fixed share of the gap per second, so any frame rate gives the same
-- camera after the same time. Edges interpolate, so a view between two views inside the bounds is
-- inside them too. Within snap_px of the target on every edge, it is the target.
function C.ease(cam, target, dt, p, pxu)
	p = p or C.params
	if cam == nil then return C.copy(target) end
	local k = 1 - 0.5 ^ (math.max(dt, 0) / p.half_life)
	local function lerp(a, b) return a + (b - a) * k end
	local x, y, w, h = lerp(cam.x, target.x), lerp(cam.y, target.y), lerp(cam.w, target.w), lerp(cam.h, target.h)
	local s = pxu and p.snap_px / pxu or p.snap -- `pxu`: screen px per unit now
	if math.abs(x - target.x) < s and math.abs(y - target.y) < s and math.abs(w - target.w) < s and math.abs(h - target.h) < s then
		return C.copy(target)
	end
	return { x = x, y = y, w = w, h = h, zoom = p.ref_w / w }
end

-- Whole screen pixels: the camera's offset, and a stage point's place under it. Everything drawn is
-- placed this way (the offset is rounded once, the point's own px relative to it), so a still scene
-- draws identical frames and the pieces keep their relation while the camera moves.
local function round(v) return math.floor(v + 0.5) end
function C.off(cam, k) return round(cam.x * k), round(cam.y * k) end
function C.at(cam, k, x, y)
	local ox, oy = C.off(cam, k)
	return round(x * k) - ox, round(y * k) - oy
end

-- How far a layer of factor f is shifted so it follows the camera by 1 - f: f = 1 stays on the map,
-- 0 stays on the screen. `home` is the camera centre at which the layer sits where it was placed.
-- The flat case of depth.lua's placement (the layer drawn at the stage's scale, no depth zoom).
function C.parallax(cam, f, home) return Depth.shift(cam, f, home) end

function C.copy(v) return { x = v.x, y = v.y, w = v.w, h = v.h, zoom = v.zoom } end

function C.same(a, b)
	return a.x == b.x and a.y == b.y and a.w == b.w and a.h == b.h and a.zoom == b.zoom
end

return C
