-- Depth: where a layer behind (or in front of) the stage is drawn (docs/design/brawl-maps.md
-- decisions 6-7). Pure: no gfx, ui or net; stock Lua 5.4 and Luau.
--
-- A layer of depth factor f has its own px per unit, k1 * (1 + (zoom - 1) * f), with k1 the
-- playfield's px per unit at zoom 1 (width / ref_w): far layers barely shrink as the camera pulls
-- back, so their edges stay out of view and they read as far. It follows the camera by f about
-- `home`. f = 1 is the stage itself, f = 0 is fixed to the screen.
local D = {}

-- How far a layer of factor f is shifted, in its units, so it follows the camera by 1 - f (the
-- flat parallax: Camera.parallax).
function D.shift(cam, f, home)
	local cx, cy = cam.x + cam.w / 2, cam.y + cam.h / 2
	return (cx - home.x) * (1 - f), (cy - home.y) * (1 - f)
end

function D.scale(cam, f, k1) return k1 * (1 + (cam.zoom - 1) * f) end

-- The screen point (from the view's top-left) of the layer point (x, y), and the layer's px per
-- unit. `s` forces the px per unit (the stage's k gives the flat parallax); f = 1 is the stage's
-- own mapping, exactly.
function D.place(cam, f, home, x, y, k1, s)
	local k = k1 * cam.zoom
	if f == 1 and (s == nil or s == k) then return (x - cam.x) * k, (y - cam.y) * k, k end
	s = s or D.scale(cam, f, k1)
	local cx, cy = cam.x + cam.w / 2, cam.y + cam.h / 2
	return cam.w * k / 2 + ((x - home.x) - (cx - home.x) * f) * s, cam.h * k / 2 + ((y - home.y) - (cy - home.y) * f) * s, s
end

-- What of a layer of factor f a view shows, in the layer's units: { x0, y0, x1, y1 }.
function D.visible(cam, f, home)
	local z = cam.zoom
	local sc = 1 + (z - 1) * f -- k1 cancels out
	local cx, cy = cam.x + cam.w / 2, cam.y + cam.h / 2
	local hw, hh = cam.w * z / 2 / sc, cam.h * z / 2 / sc
	local ox, oy = home.x + (cx - home.x) * f, home.y + (cy - home.y) * f
	return { x0 = ox - hw, y0 = oy - hh, x1 = ox + hw, y1 = oy + hh }
end

-- The views a map's backdrop must hold up under: the camera bounds' 4 corners at the nearest and
-- furthest zoom, for each playfield aspect. `C` is the camera module (limits, params).
function D.cases(bounds, aspects, C)
	local out = {}
	for _, aspect in ipairs(aspects) do
		local zmin, zmax = C.limits(bounds, aspect)
		for _, which in ipairs({ { zmin, "min" }, { zmax, "max" } }) do
			local z = which[1]
			local w = C.params.ref_w / z
			local h = w / aspect
			for _, x in ipairs({ bounds.x, bounds.x + bounds.w - w }) do
				for _, y in ipairs({ bounds.y, bounds.y + bounds.h - h }) do
					out[#out + 1] = { x = x, y = y, w = w, h = h, zoom = z, limit = which[2] }
				end
			end
		end
	end
	return out
end

-- The union of what every view in `cases` shows of a layer of factor f.
function D.cover(cases, f, home)
	local r = { x0 = math.huge, y0 = math.huge, x1 = -math.huge, y1 = -math.huge }
	for _, cam in ipairs(cases) do
		local v = D.visible(cam, f, home)
		r.x0, r.y0 = math.min(r.x0, v.x0), math.min(r.y0, v.y0)
		r.x1, r.y1 = math.max(r.x1, v.x1), math.max(r.y1, v.y1)
	end
	return r
end

-- Copies of a picture `w` wide, `overlap` apart, so a strip centred on `x` spans [x0, x1].
-- Returns the count and the strip's left edge (all in the same units as the arguments).
function D.tiles(x, w, overlap, x0, x1)
	local half = math.max(x - x0, x1 - x, w / 2)
	local n = math.max(1, math.ceil((2 * half - w) / (w - overlap)) + 1)
	local total = (w - overlap) * (n - 1) + w
	return n, x - total / 2, total
end

-- A drifting thing's x (its centre) at render time t: `speed` units a second, wrapping inside
-- [x - span/2, x + span/2). A pure function of t, so any frame steps give the same place.
function D.drift(x, speed, span, t)
	if speed == 0 or t == 0 then return x end
	local lo = x - span / 2
	return lo + (span / 2 + speed * t) % span
end

return D
