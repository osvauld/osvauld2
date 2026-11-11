-- Brawl's depth placement and backgrounds in stock Lua (plan: docs/design/brawl-maps.md §0, P-V1..P-V3).
--   lua scripts/test_brawl_depth.lua
package.path = "scripts/?.lua;" .. package.path
local U = require("brawl_test_util")
local Depth = require("depth")
local Camera = require("camera")
local Map = require("map")
local Library = require("library")
local lib = Library.load()
local BODY = { w = U.tune.body_w, h = U.tune.body_h }
local EPS = 1e-9

local function near(a, b, eps) return math.abs(a - b) <= (eps or EPS) * math.max(1, math.abs(a), math.abs(b)) end

-- P-V1: depth zoom. f = 1 is the stage's own mapping exactly, f = 0 ignores the camera, and in
-- between the scale and the offset are monotone in f. Camera.parallax is the flat case of it.
do
	local home = { x = 0, y = -100 }
	local k1 = 900 / Camera.params.ref_w
	local cams = {
		{ x = -650, y = -600, w = 1300, h = 1300 / 1.5 },
		{ x = -1500, y = -1100, w = 2700, h = 1800 },
		{ x = 200, y = -300, w = 1300, h = 1300 / (16 / 9) },
	}
	for _, cam in ipairs(cams) do cam.zoom = Camera.params.ref_w / cam.w end
	local points = { { 0, 0 }, { -900, -520 }, { 1200, 300 }, { -3000, -1600 } }
	for ci, cam in ipairs(cams) do
		local k = k1 * cam.zoom
		for _, p in ipairs(points) do
			local sx, sy, s = Depth.place(cam, 1, home, p[1], p[2], k1)
			assert(sx == (p[1] - cam.x) * k and sy == (p[2] - cam.y) * k and s == k, "P-V1: f = 1 is the stage's mapping, camera " .. ci)
			-- f = 0: the same point lands in the same place whatever the camera (moved and zoomed, the
			-- playfield's shape kept).
			local other = { x = cam.x + 777, y = cam.y - 333, w = cam.w * 1.7, h = cam.h * 1.7 }
			other.zoom = Camera.params.ref_w / other.w
			local ax, ay, as = Depth.place(cam, 0, home, p[1], p[2], k1)
			local bx, by, bs = Depth.place(other, 0, home, p[1], p[2], k1)
			assert(near(ax, bx) and near(ay, by) and as == k1 and bs == k1, "P-V1: f = 0 doesn't follow the camera " .. ci)
			-- Monotone in f: the scale, and the parallax offset (how far the layer follows the camera).
			local last_s, last_dx, last_dy
			for i = 0, 20 do
				local f = i / 20
				local _, _, sf = Depth.place(cam, f, home, p[1], p[2], k1)
				local dx, dy = Depth.shift(cam, f, home)
				if last_s then
					local up = cam.zoom >= 1
					assert((up and sf >= last_s - EPS) or (not up and sf <= last_s + EPS), "P-V1: scale monotone in f")
					local cx, cy = cam.x + cam.w / 2, cam.y + cam.h / 2
					assert((cx >= home.x and dx <= last_dx + EPS) or (cx < home.x and dx >= last_dx - EPS), "P-V1: x offset monotone in f")
					assert((cy >= home.y and dy <= last_dy + EPS) or (cy < home.y and dy >= last_dy - EPS), "P-V1: y offset monotone in f")
				end
				last_s, last_dx, last_dy = sf, dx, dy
			end
		end
		-- The flat case (the layer drawn at the stage's scale) is the old parallax.
		for _, f in ipairs({ 0, 0.1, 0.6, 1 }) do
			local dx, dy = Camera.parallax(cam, f, home)
			for _, p in ipairs(points) do
				local sx, sy = Depth.place(cam, f, home, p[1], p[2], k1, k)
				assert(near(sx, (p[1] + dx - cam.x) * k, 1e-9) and near(sy, (p[2] + dy - cam.y) * k, 1e-9), "P-V1: parallax is place at the stage's scale")
			end
		end
	end
end

-- The maps, every one in the index, compiled as the app compiles them.
local maps = {}
for _, id in ipairs(require("maps/index")) do
	maps[#maps + 1] = { id = id, map = require("maps/" .. id), compiled = Map.compile(require("maps/" .. id), lib, { body = BODY }) }
end

-- The views P-V2 tries: the camera's bounds' 4 corners x zoom_min, zoom_max x these aspects (the
-- plan's 900/600 and 16/9, and the playfield shapes around them).
local ASPECTS = { 900 / 600, 16 / 9, 1, 4 / 3, 900 / 662, 2, 2.4 }

-- P-V2: no layer shows its edge. Every backdrop layer (tiled picture, haze, ridge) covers the view's
-- width, everything with a body reaches the view's bottom, and the ground's body does too.
do
	local checked = 0
	for _, m in ipairs(maps) do
		local scene, stage = m.compiled.scene, m.compiled.stage
		local cases = Depth.cases(stage.camera, ASPECTS, Camera)
		assert(#cases == #ASPECTS * 2 * 4, "P-V2: 4 corners x 2 zooms x aspects")
		local wide = 0
		for _, cam in ipairs(cases) do
			local k1 = 1
			local W, H = cam.w * cam.zoom * k1, cam.h * cam.zoom * k1
			for i, it in ipairs(scene.under) do
				local where = string.format("P-V2: %s under[%d] %s (%s) at view x=%g y=%g w=%g", m.id, i, it.asset or "?", it.kind, cam.x, cam.y, cam.w)
				if it.kind == "layer" or it.kind == "haze" or it.kind == "ridge" then
					local x0, _, s = Depth.place(cam, it.f, scene.home, it.x, it.y, k1)
					local x1 = Depth.place(cam, it.f, scene.home, it.x + it.w, it.y, k1)
					assert(x0 <= 0 and x1 >= W, where .. ": covers " .. x0 .. ".." .. x1 .. " of 0.." .. W)
					local _, y1 = Depth.place(cam, it.f, scene.home, it.x, it.bottom, k1)
					assert(y1 >= H, where .. ": its body ends at " .. y1 .. " above the view's bottom " .. H)
					assert(s > 0, where)
					checked = checked + 1
					if cam.limit == "min" then wide = wide + 1 end
				elseif it.kind == "ground" then
					local _, y1 = Depth.place(cam, 1, scene.home, it.x, it.bottom, k1)
					assert(y1 >= H - 1e-6, where .. ": the ground's body ends above the view's bottom")
				end
			end
		end
		assert(wide > 0, "P-V2: zoomed-out views were tried")
	end
	print("P-V2 layer x view checks: " .. checked)
end

-- P-V3: drift is a pure function of render time: drift speed, wraps inside the span without a jump
-- (the wrap happens out of view), and the same total time gives the same x whatever the frame steps.
do
	local x, v, span = 100, 6, 3000
	assert(Depth.drift(x, v, span, 0) == x, "P-V3: no time, no drift")
	assert(near(Depth.drift(x, v, span, 10) - Depth.drift(x, v, span, 0), 60), "P-V3: drift speed")
	assert(near(Depth.drift(x, -v, span, 10) - x, -60), "P-V3: it can drift left")
	-- Wraps inside [x - span/2, x + span/2).
	for t = 0, 2000, 7.3 do
		local at = Depth.drift(x, v, span, t)
		assert(at >= x - span / 2 - EPS and at < x + span / 2 + EPS, "P-V3: inside its span at t=" .. t)
	end
	-- Frame steps: 1/60 and 1/144 steps to the same total time land in the same place.
	local t60 = 0
	for _ = 1, 600 do t60 = t60 + 1 / 60 end
	local t144 = 0
	for _ = 1, 1440 do t144 = t144 + 1 / 144 end
	assert(near(Depth.drift(x, v, span, t60), Depth.drift(x, v, span, t144), 1e-9), "P-V3: 10 s is 10 s at any frame rate")
	-- Every drifting cloud of every map wraps out of view: just after the wrap it is wholly left of
	-- the furthest-left view, just before it wholly right of the furthest-right one.
	local clouds = 0
	for _, m in ipairs(maps) do
		local scene, stage = m.compiled.scene, m.compiled.stage
		local cases = Depth.cases(stage.camera, ASPECTS, Camera)
		for i, it in ipairs(scene.under) do
			if it.drift then
				clouds = clouds + 1
				local vis = Depth.cover(cases, it.f, scene.home)
				local centre = it.x + it.w / 2
				local lo, hi = centre - it.span / 2, centre + it.span / 2
				local where = string.format("P-V3: %s under[%d] %s", m.id, i, it.asset)
				assert(lo + it.w / 2 <= vis.x0, where .. ": wraps in view on the left (" .. lo + it.w / 2 .. " > " .. vis.x0 .. ")")
				assert(hi - it.w / 2 >= vis.x1, where .. ": wraps in view on the right (" .. hi - it.w / 2 .. " < " .. vis.x1 .. ")")
				-- Continuity: across a step, the move is the speed (bar the out-of-view wrap).
				local a, b = Depth.drift(centre, it.drift, it.span, 100), Depth.drift(centre, it.drift, it.span, 100 + 1 / 60)
				assert(near(b - a, it.drift / 60, 1e-6) or math.abs(b - a) > it.span / 2, where .. ": a jump mid-span")
			end
		end
	end
	assert(clouds >= 3, "P-V3: the yard's clouds drift, saw " .. clouds)
	print("P-V3 drifting clouds: " .. clouds)
end

print("brawl depth ok")
