-- Brawl's camera in stock Lua (plan: docs/design/brawl-movement.md §0, P-C1..P-C4).
--   lua scripts/test_brawl_camera.lua
package.path = "scripts/?.lua;" .. package.path
local U = require("brawl_test_util")
local Camera = require("camera")
local T, stage = U.tune, U.yard
local P = Camera.params
local bounds = stage.camera
local ASPECT = 1.5

local function box(x, y) return { x = x - T.body_w / 2, y = y - T.body_h, w = T.body_w, h = T.body_h } end
local function fighter(x, y, act) return { x = x, y = y, act = act or "idle" } end
local function inside(view, b, m)
	return view.x <= b.x - m + 1e-9 and view.y <= b.y - m + 1e-9
		and view.x + view.w >= b.x + b.w + m - 1e-9 and view.y + view.h >= b.y + b.h + m - 1e-9
end
local function within(view, r)
	return view.x >= r.x - 1e-9 and view.y >= r.y - 1e-9
		and view.x + view.w <= r.x + r.w + 1e-9 and view.y + view.h <= r.y + r.h + 1e-9
end
local function target(fighters, aspect)
	return Camera.target(Camera.boxes(fighters, T.body_w, T.body_h), bounds, aspect or ASPECT)
end

-- P-C1: one fighter: its box plus the margin, at least min_view wide, inside the bounds.
do
	local zmin, zmax = Camera.limits(bounds, ASPECT)
	for _, at in ipairs({ { 0, 0 }, { 0, -520 }, { -890, 0 }, { 890, 0 }, { -600, -260 }, { 0, -600 }, { 700, 300 } }) do
		local x, y = at[1], at[2]
		local v = target({ fighter(x, y) })
		local b = box(x, y)
		assert(v.w >= P.min_view - 1e-9, "P-C1: at least min_view wide at " .. x .. "," .. y)
		assert(math.abs(v.h - v.w / ASPECT) < 1e-9, "P-C1: the view has the playfield's shape")
		assert(inside(v, b, P.margin), "P-C1: box plus margin inside the view at " .. x .. "," .. y)
		assert(within(v, bounds), "P-C1: inside the camera bounds at " .. x .. "," .. y)
		assert(v.zoom >= zmin - 1e-9 and v.zoom <= zmax + 1e-9, "P-C1: zoom in range")
	end
	-- Past what the bounds allow, the bounds win: still never outside them.
	local v = target({ fighter(1400, 0) })
	assert(within(v, bounds), "P-C1: bounds win over the margin")
	-- A playfield of any shape: the view takes its aspect and stays in the bounds.
	for _, a in ipairs({ 0.8, 1.0, 1.286, 1.78, 2.4 }) do
		local v2 = target({ fighter(0, 0) }, a)
		assert(math.abs(v2.w / v2.h - a) < 1e-9 and within(v2, bounds), "P-C1: aspect " .. a)
		assert(inside(v2, box(0, 0), P.margin), "P-C1: margin at aspect " .. a)
	end
end

-- P-C2: two fighters 2 000 apart, the margin around both; the zoom limit; out fighters left out.
do
	local zmin, zmax = Camera.limits(bounds, ASPECT)
	assert(zmin >= P.zoom_min - 1e-9, "P-C2: the limit is at least zoom_min")
	local v = target({ fighter(-1000, 0), fighter(1000, 0) })
	assert(inside(v, box(-1000, 0), P.margin) and inside(v, box(1000, 0), P.margin), "P-C2: both inside with margin")
	assert(within(v, bounds), "P-C2: inside the bounds")
	assert(v.zoom >= zmin - 1e-9 and v.zoom < 1, "P-C2: zoomed out for two")
	-- Two fighters further apart than any view allows: the zoom stops at the limit, bounds hold.
	local far = target({ fighter(-1400, 0), fighter(1400, -800) })
	assert(far.zoom >= zmin - 1e-9 and far.zoom <= zmax + 1e-9 and within(far, bounds), "P-C2: the zoom never passes the limit")
	assert(math.abs(far.zoom - zmin) < 1e-9, "P-C2: and it sits on it")
	-- Out and ko are left out of the framing.
	local alone = target({ fighter(300, 0) })
	local lone = target({ fighter(300, 0), fighter(-1400, -900, "out") })
	assert(Camera.same(alone, lone), "P-C2: an out fighter is left out")
	local ko = target({ fighter(300, 0), fighter(-1400, -900, "ko") })
	assert(Camera.same(alone, ko), "P-C2: a ko fighter is left out")
	assert(Camera.target(Camera.boxes({ fighter(0, 0, "out") }, T.body_w, T.body_h), bounds, ASPECT) == nil,
		"P-C2: nobody in play is no target")
	-- N fighters: four of them are all in view.
	local four = { fighter(-700, 0), fighter(700, 0), fighter(-460, -260), fighter(0, -520) }
	local v4 = target(four)
	for _, f in ipairs(four) do assert(inside(v4, box(f.x, f.y), P.margin), "P-C2: four fighters, all inside") end
end

-- P-C3: easing is a pure function of (camera, target, dt); it converges on the target.
do
	local a = target({ fighter(0, 0) })
	local b = target({ fighter(-800, -520), fighter(700, 0) })
	local before = Camera.copy(a)
	local run = {}
	local dts = { 1 / 60, 1 / 60, 1 / 30, 1 / 120, 0.1, 1 / 60 }
	local function go()
		local cam = a
		local out = {}
		for i = 1, 400 do
			cam = Camera.ease(cam, b, dts[i % #dts + 1])
			out[i] = cam
			assert(within(cam, bounds), "P-C3: every eased view is inside the bounds")
		end
		return out
	end
	local one, two = go(), go()
	for i = 1, #one do assert(Camera.same(one[i], two[i]), "P-C3: same dts, same camera at step " .. i) end
	assert(Camera.same(a, before), "P-C3: easing doesn't change its input")
	assert(Camera.same(one[#one], b), "P-C3: it converges on the target exactly")
	-- Monotone: each step is no further from the target than the last (in x), and dt 0 stands still.
	local d0 = math.abs(a.x - b.x)
	local mid = Camera.ease(a, b, 1 / 60)
	assert(math.abs(mid.x - b.x) < d0 and math.abs(mid.w - b.w) < math.abs(a.w - b.w), "P-C3: it moves toward the target")
	assert(Camera.same(Camera.ease(a, b, 0), a), "P-C3: no time, no move")
	-- Frame rate doesn't change where it is after a second (two 1/120 steps = one 1/60 step).
	local c1, c2 = a, a
	for _ = 1, 30 do c1 = Camera.ease(c1, b, 1 / 60) end
	for _ = 1, 60 do c2 = Camera.ease(c2, b, 1 / 120) end
	assert(math.abs(c1.x - c2.x) < 1e-6 and math.abs(c1.w - c2.w) < 1e-6, "P-C3: a second is a second at any frame rate")
end

-- P-C4: parallax. A layer of factor f follows the camera by 1 - f: 1 stays on the map, 0 on the screen.
do
	local home = { x = 0, y = -200 }
	local cam = { x = 400 - 550, y = -100 - 430, w = 1100, h = 860 } -- centre (400, 0)
	local function shift(f) return Camera.parallax(cam, f, home) end
	local dx, dy = shift(1)
	assert(dx == 0 and dy == 0, "P-C4: f = 1 moves with the map")
	dx, dy = shift(0)
	assert(dx == 400 and dy == 100, "P-C4: f = 0 moves with the camera")
	dx, dy = shift(0.25)
	assert(dx == 300 and dy == 75, "P-C4: f = 0.25 moves 3/4 of the way with the map")
	dx = Camera.parallax(cam, 0.5, { x = 400, y = -100 })
	assert(dx == 0, "P-C4: nothing at the home centre")
end

-- P-C5: the scene's back layers: depth factors in 0..1, five tiled layers, about twenty pictures.
-- That each layer covers every view is P-V2's (test_brawl_depth.lua): with depth zoom a far layer
-- needs less than the bounds' width. Picture files against their sizes are P-L1's job
-- (test_brawl_library.lua); the pictures a map draws are P-K's (test_brawl_maps.lua).
do
	local scene = U.compiled.scene
	local layers = 0
	for i, it in ipairs(scene.under) do
		assert(it.f >= 0 and it.f <= 1, "P-C5: under[" .. i .. "] f in 0..1")
		if it.kind == "layer" then layers = layers + 1 end
	end
	assert(layers == 5, "P-C5: the yard has five tiled layers, saw " .. layers)
	assert(#scene.images <= 20, "P-C5: about twenty pictures, not " .. #scene.images)
	print("P-C5 pictures: " .. #scene.images)
end

-- P-C6: no shimmer after a stop. Run, stop somewhere new, camera eases (60 Hz, 900 px wide screen): its
-- screen-px offset is whole pixels, reaches its target's exactly within 2 s, then frame to frame is
-- identical; the robot's left/top are integers and constant once it is still and the camera settled.
do
	local SW = 900
	local f, rules = U.fresh()
	local cam, settled_at, last
	local run = { x = 1 }
	local drawn = {}
	for frame = 1, 60 * 6 do
		local running = frame <= 40
		U.step(f, rules, running and U.input(run) or U.input(), 2)
		local fi = f.fighters[1]
		local tg = target({ fi })
		cam = Camera.ease(cam, tg, 1 / 60, nil, cam and SW / cam.w or SW / tg.w)
		local k = SW / cam.w
		local ox, oy = Camera.off(cam, k)
		assert(ox == math.floor(ox) and oy == math.floor(oy), "P-C6: whole-pixel camera offset")
		local l, t = Camera.at(cam, k, fi.x - 40, fi.y - 120)
		assert(l == math.floor(l) and t == math.floor(t), "P-C6: whole-pixel robot position")
		local tx, ty = Camera.off(tg, k)
		if not running and Camera.same(cam, tg) and not settled_at then settled_at = frame end
		if settled_at then
			assert(ox == Camera.off(tg, SW / tg.w), "P-C6: the camera reached its target exactly")
			drawn[#drawn + 1] = { ox, oy, l, t }
		end
	end
	assert(settled_at and settled_at - 40 <= 120, "P-C6: settled within 2 s of the stop, at " .. tostring(settled_at))
	local d1 = drawn[1]
	for i, d in ipairs(drawn) do
		for j = 1, 4 do assert(d[j] == d1[j], "P-C6: frame " .. i .. " of the settled scene differs") end
	end
	-- a long way: still bounded
	local c, n = Camera.copy(target({ fighter(-800, 0) })), 0
	local far = target({ fighter(800, -300) })
	while not Camera.same(c, far) do
		c = Camera.ease(c, far, 1 / 60, nil, SW / c.w)
		n = n + 1
		assert(n < 60 * 3, "P-C6: bounded settle over the longest trip")
	end
end

print("brawl camera ok")
