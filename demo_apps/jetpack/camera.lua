-- Pure camera maths (no gfx). The view fills the playfield; its size in world units comes from the
-- short side, so a soldier (52 units) is ~90-140 px tall on a 780x360 to 960x540 view. The camera follows the
-- player on both axes, leads toward the aim, eases, and is held to the map: it never shows anything
-- past the floor, the ceiling or a wall -- except horizontally, when the soldier would otherwise stand
-- under a control. The one hard rule is "the soldier's screen box never overlaps a control zone"
-- (the two stick boxes, the reload, swap, pickup and zoom slots); the camera slides sideways, overscrolling a wall if it must.
-- A loose band keeps the soldier from drifting off-screen while the easing catches up.
local Sticks = require("sticks")
local Camera = {}
Camera.__index = Camera

Camera.anchor = { x = 0.5, y = 0.55 }              -- where the soldier rests: low, so the ground is near the bottom
Camera.band = { l = 0.12, r = 0.88, t = 0.14, b = 0.86 } -- loose follow limits (the map and the zones win over it)
Camera.units = 210                                 -- world units along the short side at zoom 1 (the game is landscape-only)
Camera.lead = { x = 0.16, y = 0.12 }               -- look-ahead, as a share of the view
Camera.rate = 8                                    -- easing, 1/s
Camera.zoom_rate = 12                              -- zoom easing, 1/s: ~95 % of a level change in a quarter second
Camera.lead_gain = 0.35                            -- look-ahead grows by this per unit of zoom past 1 (the extra view is in front)
Camera.min_px = 30                                 -- a soldier is never drawn smaller than this tall (px); caps the zoom
Camera.half = { w = 26, h = 26 }                   -- half the soldier's square frame, in units
Camera.pad = 6                                     -- px kept between the soldier and a control
Camera.ahead = 0.3                                 -- the target starts avoiding a zone this far (of h) above it

local function clamp(v, lo, hi) return math.max(lo, math.min(hi, v)) end

function Camera.new(map)
	return setmetatable({ map = map, x = 0, y = 0, zoom = 1 }, Camera)
end

-- The widest view factor a w x h playfield allows: the soldier (2 x Camera.half.h units) stays Camera.min_px tall.
-- About 3.8 at 960x460, 3.0 on a 780x360 phone.
function Camera.max_zoom(w, h)
	return math.max(1, math.min(w, h) * 2 * Camera.half.h / (Camera.min_px * Camera.units))
end

-- Scale (px per unit), the view's size in units, and the zoom actually used (capped) for a playfield of
-- w x h px at `zoom` (> 1 widens).
local function metrics(w, h, zoom)
	zoom = math.min(zoom, Camera.max_zoom(w, h))
	local units = Camera.units * zoom
	local scale = math.min(w, h) / units
	return scale, w / scale, h / scale, zoom
end

-- The control zones (px, playfield-local) the soldier keeps clear of, `side` = which edge they hug.
function Camera.zones(w, h)
	local size = Sticks.size(math.min(w, h))
	local slots = Sticks.slots(w, h)
	local function zone(slot) return { x0 = slot.x, x1 = slot.x + slot.w, y0 = slot.y, y1 = slot.y + slot.h, side = "r" } end
	return {
		{ x0 = 10, x1 = 10 + size, y0 = h - 10 - size, y1 = h - 10, side = "l" },
		{ x0 = w - 10 - size, x1 = w - 10, y0 = h - 10 - size, y1 = h - 10, side = "r" },
		zone(slots.reload), zone(slots.swap), zone(slots.pickup), zone(slots.zoom),
	}
end

-- Slide x sideways until the soldier's box is off every zone; `ahead` (share of h) starts the slide
-- early for a soldier approaching a zone from above (so the camera eases there instead of lurching).
local function avoid(x, vw, w, h, px, py, y, scale, ahead)
	local hw, hh = Camera.half.w * scale, Camera.half.h * scale
	local sy = (py - y) * scale
	for _, z in ipairs(Camera.zones(w, h)) do
		local sx = (px - x) * scale
		local l, r = z.x0 - hw - Camera.pad, z.x1 + hw + Camera.pad
		if sy >= z.y0 - hh - ahead * h and sy <= z.y1 + hh and sx > l and sx < r then
			sx = z.side == "l" and r or l
			x = px - sx / scale
		end
	end
	return x
end

-- Loose band, then the map (which wins), then -- unless `soft` -- the zones (which win over the map).
local function constrain(self, w, h, vw, vh, scale, px, py, x, y, ahead)
	local b, map = Camera.band, self.map
	x, y = clamp(x, px - b.r * vw, px - b.l * vw), clamp(y, py - b.b * vh, py - b.t * vh)
	x = map.w <= vw and (map.w - vw) / 2 or clamp(x, 0, map.w - vw)
	y = map.h <= vh and (map.h - vh) / 2 or clamp(y, 0, map.h - vh)
	return avoid(x, vw, w, h, px, py, y, scale, ahead), y
end

-- The look-ahead for a mouse: a vector (length <= 1) from the soldier's resting spot to the cursor's screen position,
-- as a share of half the view. It depends on the cursor alone, never on the aim, so the aim cannot feed back through
-- the camera (aim -> look-ahead -> view moves -> aim). Pass it as `lead`; sticks and keys pass none and lead by the aim.
function Camera.cursor_lead(w, h, cx, cy)
	local lx, ly = (cx - Camera.anchor.x * w) / (0.5 * w), (cy - Camera.anchor.y * h) / (0.5 * h)
	local len = math.sqrt(lx * lx + ly * ly)
	if len > 1 then lx, ly = lx / len, ly / len end
	return { lx, ly }
end

-- Where the camera wants to be: the soldier at the anchor, shifted toward the aim (or `lead`, a vector of
-- length <= 1) -- further as the view widens, so the extra view lies in front -- then constrained.
function Camera:target(w, h, vw, vh, scale, px, py, aim, zoom, lead)
	local gain = 1 + Camera.lead_gain * ((zoom or 1) - 1)
	local lx, ly = math.cos(aim), math.sin(aim)
	if lead then lx, ly = lead[1], lead[2] end
	local x = px + lx * Camera.lead.x * gain * vw - Camera.anchor.x * vw
	local y = py + ly * Camera.lead.y * gain * vh - Camera.anchor.y * vh
	return constrain(self, w, h, vw, vh, scale, px, py, x, y, Camera.ahead)
end

-- Place the camera at its target at once (a new match, a respawn).
function Camera:snap(w, h, px, py, aim, zoom, lead)
	self.zoom = zoom or 1
	local scale, vw, vh, z = metrics(w, h, self.zoom)
	self.x, self.y = self:target(w, h, vw, vh, scale, px, py, aim or 0, z, lead)
end

-- One step of `dt` seconds: ease the zoom (a level change takes ~0.25 s) and the position, then hold the constraints.
function Camera:update(dt, w, h, px, py, aim, zoom, lead)
	local k = 1 - math.exp(-Camera.rate * math.max(0, dt))
	self.zoom = self.zoom + ((zoom or 1) - self.zoom) * (1 - math.exp(-Camera.zoom_rate * math.max(0, dt)))
	local scale, vw, vh, z = metrics(w, h, self.zoom)
	local tx, ty = self:target(w, h, vw, vh, scale, px, py, aim or 0, z, lead)
	self.x, self.y = constrain(self, w, h, vw, vh, scale, px, py, self.x + (tx - self.x) * k, self.y + (ty - self.y) * k, 0)
end

-- What to draw and project with for a w x h playfield: scale, view size, and the camera's origin
-- constrained for this size (so a differently sized capture still frames the soldier off the controls).
-- `ox`/`oy` are zero (the view fills the playfield); they stay so callers can add an inset.
function Camera:geometry(w, h, px, py)
	local scale, vw, vh = metrics(w, h, self.zoom)
	local x, y = constrain(self, w, h, vw, vh, scale, px, py, self.x, self.y, 0)
	return { scale = scale, vw = vw, vh = vh, x = x, y = y, ox = 0, oy = 0 }
end

-- World units to screen px (playfield-local).
function Camera.to_screen(geo, x, y)
	return (x - geo.x) * geo.scale + geo.ox, (y - geo.y) * geo.scale + geo.oy
end

-- Screen px (playfield-local) to world units.
function Camera.to_world(geo, x, y)
	return (x - geo.ox) / geo.scale + geo.x, (y - geo.oy) / geo.scale + geo.y
end

return Camera
