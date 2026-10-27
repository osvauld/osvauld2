-- Pure camera tests (stock lua, from the worktree root):  lua scripts/test_jetpack_camera.lua
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local Camera = require("camera")
local F = require("format")

local function check(ok, why) if not ok then error(why, 2) end end
local function near(a, b, eps) return math.abs(a - b) <= (eps or 0.001) end

local maps = F.maps(dofile("demo_apps/jetpack/baked/maps.lua"))
local function map(id) return assert(F.find(maps, id)) end
local outpost = map("outpost")
local W, H = 960, 460 -- a landscape playfield (a 960x540 window less the bar)
local SOLDIER = 52    -- a drawn soldier's height in world units (pilot 104 drawing units at 0.5)

local function settle(c, px, py, aim, zoom, w, h, seconds)
	for _ = 1, math.floor((seconds or 4) * 60) do c:update(1 / 60, w or W, h or H, px, py, aim or 0, zoom or 1) end
	return c:geometry(w or W, h or H, px, py)
end
local function screen(g, px, py) return (px - g.x) * g.scale + g.ox, (py - g.y) * g.scale + g.oy end
-- The soldier's screen box is off every control zone (sticks, reload) and on the playfield.
local function clear(g, w, h, px, py)
	local sx, sy = screen(g, px, py)
	local hw, hh = Camera.half.w * g.scale, Camera.half.h * g.scale
	if sx < 0 or sx > w or sy < 0 or sy > h then return false, "off screen" end
	for i, z in ipairs(Camera.zones(w, h)) do
		if sx + hw > z.x0 and sx - hw < z.x1 and sy + hh > z.y0 and sy - hh < z.y1 then return false, "under zone " .. i end
	end
	return true
end
local function in_band(g, w, h, px, py) return clear(g, w, h, px, py) end
-- Nothing is drawn past the map's top or bottom.
local function on_map_vertically(g, m) return g.y >= -0.001 and g.y + g.vh <= m.h + 0.001 end

-- zoom: a soldier is 110-140 px tall on a 960x540 view; the short side sets the scale
local c = Camera.new(outpost)
local g = c:geometry(960, 540, 2000, 1000)
check(SOLDIER * g.scale >= 110 and SOLDIER * g.scale <= 140, "soldier is " .. SOLDIER * g.scale .. " px on 960x540")
check(near(g.vw * g.scale, 960) and near(g.vh * g.scale, 540), "view fills the playfield")
local ph = c:geometry(780, 360, 2000, 1000) -- a landscape phone (the game is landscape-only)
check(SOLDIER * ph.scale >= 80 and SOLDIER * ph.scale <= 120, "landscape-phone soldier " .. SOLDIER * ph.scale .. " px")
check(near(ph.vw * ph.scale, 780) and near(ph.vh * ph.scale, 360), "landscape-phone view fills the playfield")
check(type(Camera.units) == "number", "one view size: the game is landscape-only")
check(outpost.w > 3 * g.vw and outpost.h > 3 * g.vh, "maps are far larger than the view")

-- follow: both axes, smooth (not a snap), converging on the anchor
c = Camera.new(outpost)
c:snap(W, H, 2000, 1500, 0)
local x0, y0 = c.x, c.y
c:update(1 / 60, W, H, 2400, 1000, 0, 1)
check(c.x > x0 and c.x < x0 + 400, "horizontal follow is smooth")
check(c.y < y0 and c.y > y0 - 500, "vertical follow is smooth")
g = settle(c, 2400, 1000, 0)
local sx, sy = screen(g, 2400, 1000)
check(near(sx, (Camera.anchor.x - Camera.lead.x) * W, 2), "settled on the anchor, led toward the aim: " .. sx)
check(near(sy, Camera.anchor.y * H, 2), "settled on the vertical anchor: " .. sy)

-- look-ahead: the camera leads toward the aim, in all four directions
local function settled_at(aim)
	local k = Camera.new(outpost)
	k:snap(W, H, 2000, 1000, aim)
	local gg = settle(k, 2000, 1000, aim)
	return gg
end
local right, left, up, down = settled_at(0), settled_at(math.pi), settled_at(-math.pi / 2), settled_at(math.pi / 2)
check(right.x - left.x > 0.2 * right.vw, "camera leads right/left by a fifth of the view or more")
check(down.y - up.y > 0.15 * down.vh, "camera leads up/down")
check(right.x > 2000 - Camera.anchor.x * right.vw, "aiming right moves the camera right of the anchor")
check(in_band(right, W, H, 2000, 1000) and in_band(left, W, H, 2000, 1000)
	and in_band(up, W, H, 2000, 1000) and in_band(down, W, H, 2000, 1000), "look-ahead never puts the soldier under a control")

-- edges: strict to the map on both axes wherever no control is in the way
c = Camera.new(outpost)
g = settle(c, 0.4 * g.vw, 1000, math.pi, 1)
check(near(g.x, 0), "x holds the map edge when the soldier is clear of the sticks: " .. g.x)
c = Camera.new(outpost)
c:snap(W, H, 40, 1000, math.pi) -- mid-height at the left wall: no stick in the way
g = settle(c, 40, 1000, math.pi)
check(near(g.x, 0) and clear(g, W, H, 40, 1000), "left wall mid-height: no overscroll")
c:snap(W, H, 40, 1985, math.pi) -- on the floor in the left corner: the stick zone forces an overscroll
g = settle(c, 40, 1985, math.pi)
check(clear(g, W, H, 40, 1985), "left corner floor is clear of the stick")
check(g.x < 0 and g.x > -0.4 * g.vw, "overscroll is no more than the stick needs: " .. g.x)
c:snap(W, H, 4380, 1985, 0)
g = settle(c, 4380, 1985, 0)
check(clear(g, W, H, 4380, 1985) and g.x + g.vw > outpost.w, "right corner floor overscrolls only sideways")
check(on_map_vertically(g, outpost), "right corner floor: no vertical overscroll")
c:snap(W, H, 2000, 100, 0)
g = settle(c, 2000, 100, 0)
check(near(g.y, 0) and clear(g, W, H, 2000, 100), "ceiling: the view stops at the top edge")
c:snap(W, H, 2000, outpost.h - 30, 0)
g = settle(c, 2000, outpost.h - 30, 0)
check(near(g.y + g.vh, outpost.h) and clear(g, W, H, 2000, outpost.h - 30), "floor: the view stops at the bottom edge")
-- mid-map the camera never leaves the map
c = Camera.new(outpost)
c:snap(W, H, 1200, 800, 0)
g = settle(c, 1200, 800, 0)
check(g.x >= 0 and g.y >= 0 and g.x + g.vw <= outpost.w and g.y + g.vh <= outpost.h, "mid-map view stays on the map")

-- standing on the floor of each big map, at several x, in landscape (desktop, phone): no area below the map,
-- the ground sits near the bottom (not 45% up), and the soldier is clear of every control
for _, id in ipairs({ "outpost", "catacombs", "rooftops" }) do
	local m = map(id)
	for _, size in ipairs({ { 960, 460 }, { 780, 360 }, { 1200, 800 } }) do
		local w, h = size[1], size[2]
		for _, px in ipairs({ 30, 200, m.w / 2, m.w - 200, m.w - 30 }) do
			local py = m.h - 30
			local k = Camera.new(m)
			k:snap(w, h, px, py, 0)
			local gg = settle(k, px, py, 0, 1, w, h)
			check(on_map_vertically(gg, m), string.format("%s %dx%d floor x=%d shows area below the map (y %.1f + %.1f > %d)", id, w, h, px, gg.y, gg.vh, m.h))
			local ok, why = clear(gg, w, h, px, py)
			check(ok, string.format("%s %dx%d floor x=%d: %s", id, w, h, px, why))
			local _, sy = screen(gg, px, py)
			check(sy > 0.7 * h, string.format("%s %dx%d: soldier on the floor is at %.0f%% of the height", id, w, h, 100 * sy / h))
		end
	end
end

-- a sweep of positions over each map (corners included): the soldier is never under a control
for _, id in ipairs({ "outpost", "catacombs", "rooftops" }) do
	local m = map(id)
	for _, size in ipairs({ { 960, 460 }, { 780, 360 } }) do
		local w, h = size[1], size[2]
		for fx = 0, 10 do
			for fy = 0, 10 do
				local px, py = 15 + (m.w - 30) * fx / 10, 20 + (m.h - 40) * fy / 10
				local k = Camera.new(m)
				k:snap(w, h, px, py, 0.4)
				local gg = settle(k, px, py, 0.4, 1, w, h, 2)
				local ok, why = clear(gg, w, h, px, py)
				check(ok, string.format("%s %dx%d at (%.0f, %.0f): %s", id, w, h, px, py, why))
				-- the geometry of an unsettled camera is held clear too
				local fresh = Camera.new(m)
				ok, why = clear(fresh:geometry(w, h, px, py), w, h, px, py)
				check(ok, string.format("%s %dx%d fresh camera at (%.0f, %.0f): %s", id, w, h, px, py, why))
			end
		end
	end
end

-- the framing holds through a fast scripted flight (fall at 440, run at 230) on every big map
for _, id in ipairs({ "outpost", "catacombs", "rooftops" }) do
	local m = map(id)
	for _, size in ipairs({ { 960, 460 }, { 780, 360 } }) do
		local w, h = size[1], size[2]
		local k = Camera.new(m)
		local px, py = 200, m.h - 100
		k:snap(w, h, px, py, 0)
		for i = 1, 60 * 40 do
			local t = i / 60
			px = math.max(15, math.min(m.w - 15, px + 230 * (math.sin(t * 0.3) > 0 and 1 or -1) / 60))
			py = math.max(20, math.min(m.h - 20, py + (math.sin(t * 0.7) > 0 and -190 or 440) / 60))
			local aim = math.sin(t * 1.3) * math.pi
			k:update(1 / 60, w, h, px, py, aim, 1)
			local gg = k:geometry(w, h, px, py)
			local ok, why = clear(gg, w, h, px, py)
			check(ok, string.format("%s %dx%d: %s at t=%.2f (%.0f, %.0f)", id, w, h, why, t, px, py))
			check(on_map_vertically(gg, m), string.format("%s %dx%d overscrolled vertically at t=%.2f", id, w, h, t))
		end
	end
end

-- zoom hook: a sniper (zoom 1.6) widens the view smoothly; 1 restores it
c = Camera.new(outpost)
c:snap(W, H, 2000, 1000, 0)
local base = c:geometry(W, H, 2000, 1000)
c:update(1 / 60, W, H, 2000, 1000, 0, 1.6)
local mid = c:geometry(W, H, 2000, 1000)
check(mid.vw > base.vw and mid.vw < base.vw * 1.6, "zoom eases in")
g = settle(c, 2000, 1000, 0, 1.6)
check(near(g.vw, base.vw * 1.6, 0.5) and near(g.scale, base.scale / 1.6, 0.01), "sniper view is 1.6 times wider")
g = settle(c, 2000, 1000, 0, 1)
check(near(g.vw, base.vw, 0.5), "zoom returns")

-- zoom levels (a gun's `zooms` list): each widens the view by its factor, up to the cap that keeps a soldier legible
local LEVELS = { 1, 1.25, 1.5, 2, 3, 4 }
local base_view = Camera.new(outpost):geometry(W, H, 2000, 1000)
for _, z in ipairs(LEVELS) do
	local k = Camera.new(outpost)
	k:snap(W, H, 2000, 1000, 0, 1)
	local gg = settle(k, 2000, 1000, 0, z)
	local eff = math.min(z, Camera.max_zoom(W, H))
	check(near(gg.vw, base_view.vw * eff, 0.5), string.format("level %gx widens the view by %g (vw %.1f vs %.1f)", z, eff, gg.vw, base_view.vw * eff))
	check(near(gg.vw * gg.scale, W, 0.5), "the widened view still fills the playfield")
end
check(Camera.max_zoom(960, 460) >= 3.5 and Camera.max_zoom(780, 360) >= 2.5, "the cap leaves most of the sniper's range on a phone")
-- eases between levels in about a quarter second (not a snap, not a crawl)
do
	local k = Camera.new(outpost)
	k:snap(W, H, 2000, 1000, 0, 1)
	local w1 = k:geometry(W, H, 2000, 1000).vw
	local target = w1 * 2
	k:update(1 / 60, W, H, 2000, 1000, 0, 2)
	local first = (k:geometry(W, H, 2000, 1000).vw - w1) / (target - w1)
	check(first > 0.03 and first < 0.35, "one frame moves the zoom a little: " .. first)
	for _ = 2, 15 do k:update(1 / 60, W, H, 2000, 1000, 0, 2) end -- 0.25 s
	local done = (k:geometry(W, H, 2000, 1000).vw - w1) / (target - w1)
	check(done > 0.9 and done < 1.0001, "0.25 s covers 90 percent of a level change: " .. done)
end
-- a soldier stays at least 30 px tall at the widest level, on phones and desktop
for _, size in ipairs({ { 960, 460 }, { 780, 360 }, { 1200, 800 } }) do
	local w, h = size[1], size[2]
	local k = Camera.new(outpost)
	k:snap(w, h, 2000, 1000, 0, 4)
	local gg = settle(k, 2000, 1000, 0, 4, w, h)
	check(SOLDIER * gg.scale >= 29.5, string.format("%dx%d at 4x: soldier is %.1f px", w, h, SOLDIER * gg.scale))
end
-- the soldier is never under a control at any level across the map sweep (settled, fresh and mid-ease)
for _, id in ipairs({ "outpost", "catacombs", "rooftops" }) do
	local m = map(id)
	for _, size in ipairs({ { 960, 460 }, { 780, 360 } }) do
		local w, h = size[1], size[2]
		for _, z in ipairs(LEVELS) do
			for fx = 0, 4 do
				for fy = 0, 4 do
					local px, py = 15 + (m.w - 30) * fx / 4, 20 + (m.h - 40) * fy / 4
					local k = Camera.new(m)
					k:snap(w, h, px, py, 0.4, z)
					local gg = settle(k, px, py, 0.4, z, w, h, 2)
					local ok, why = clear(gg, w, h, px, py)
					check(ok, string.format("%s %dx%d %gx at (%.0f, %.0f): %s", id, w, h, z, px, py, why))
					check(on_map_vertically(gg, m) or m.h <= gg.vh, string.format("%s %dx%d %gx overscrolled vertically", id, w, h, z))
					-- mid-ease: a camera that was at 1 and is heading for z
					k = Camera.new(m)
					k:snap(w, h, px, py, 0.4, 1)
					for _ = 1, 8 do
						k:update(1 / 60, w, h, px, py, 0.4, z)
						ok, why = clear(k:geometry(w, h, px, py), w, h, px, py)
						check(ok, string.format("%s %dx%d easing to %gx at (%.0f, %.0f): %s", id, w, h, z, px, py, why))
					end
				end
			end
		end
	end
end
-- look-ahead grows with zoom: the soldier sits further back from the centre, in share of the (wider) view
do
	local function share(z, aim)
		local k = Camera.new(outpost)
		k:snap(W, H, 2200, 1000, aim, z)
		local gg = settle(k, 2200, 1000, aim, z)
		local sx = screen(gg, 2200, 1000)
		return (sx / W - Camera.anchor.x) * (aim == 0 and 1 or -1), gg
	end
	local s1, g1 = share(1, 0)
	local s2, g2 = share(2, 0)
	local s4, g4 = share(4, 0)
	check(s1 < 0 and s2 < s1 and s4 < s2, string.format("look-ahead share grows with zoom: %.3f %.3f %.3f", s1, s2, s4))
	-- in world units the view ahead of the soldier more than doubles when the view doubles
	local ahead1 = g1.x + g1.vw - 2200
	local ahead2 = g2.x + g2.vw - 2200
	check(ahead2 > 2 * ahead1, string.format("world view ahead at 2x (%.0f) is over twice 1x (%.0f)", ahead2, ahead1))
	local l, lg = share(2, math.pi)
	check(l < 0 and lg, "aiming left leads left at 2x")
end

-- to_world inverts the screen mapping (aim by pointer)
c = Camera.new(outpost)
c:snap(W, H, 1500, 900, 0)
g = c:geometry(W, H, 1500, 900)
local wx, wy = Camera.to_world(g, 600, 200)
sx, sy = screen(g, wx, wy)
check(near(sx, 600) and near(sy, 200), "to_world is the inverse of the projection")

-- a map smaller than the view is centred, not negative-clamped into nonsense
local tiny = F.map({ id = "t", w = 640, h = 360, platforms = {}, spawns = {
	{ id = "p", team = "player", x = 50, y = 50 }, { id = "b", team = "bot", x = 90, y = 50 } } })
c = Camera.new(tiny)
g = settle(c, 320, 180, 0, 1, 1200, 900)
check(g.x == g.x and g.y == g.y, "no NaN on a map smaller than the view")
print("jetpack camera ok: zoom, follow, look-ahead, edges, floor framing, corner sweep, flight, sniper zoom, zoom levels")
