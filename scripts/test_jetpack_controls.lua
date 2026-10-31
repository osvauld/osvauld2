-- Pure stick maths (stock lua, from the worktree root):  lua scripts/test_jetpack_controls.lua
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local Sticks = require("sticks")
local Camera = require("camera")
local T = require("theme")
local function check(ok, why) if not ok then error(why, 2) end end

local ring = Sticks.ring(Sticks.size(460))
-- thrust: nothing inside the deadzone, proportional above it, 1 at the rim, never past it
check(Sticks.thrust(0, ring) == false and Sticks.thrust(-0.1 * ring, ring) == false, "deadzone is idle")
check(Sticks.thrust(ring, ring) == false, "pushing down does not fly")
check(math.abs(Sticks.thrust(-ring, ring) - 1) < 1e-9 and Sticks.thrust(-3 * ring, ring) == 1, "full push is full thrust")
local last = 0
check(Sticks.thrust(-0.13 * ring, ring) >= Sticks.floor, "throttle starts at the floor")
for i = 13, 100 do
	local t = Sticks.thrust(-ring * i / 100, ring)
	check(t and t > last and t <= 1, "thrust rises with stick height at " .. i)
	last = t
end
-- hovering is easy: the hover height holds gravity/thrust, and +-10% of stick moves the throttle by < 12%
local hover = T.gravity / T.thrust
check(math.abs(Sticks.thrust(-ring * Sticks.hover, ring) - hover) < 0.02, "S.hover is the hover height: " .. Sticks.thrust(-ring * Sticks.hover, ring))
check(Sticks.thrust(-ring * (Sticks.hover + 0.1), ring) - hover < 0.12 and hover - Sticks.thrust(-ring * (Sticks.hover - 0.1), ring) < 0.12,
	"a tenth of stick travel moves the throttle gently around the hover")
-- a hover band wide enough for a thumb: the stick may wander 6 px and the throttle stays within +-0.1 of hover
check(6 / ring < 0.2, "ring is big enough for a thumb")

-- walking: deadzone, symmetric, full at walk_full of the ring
check(Sticks.walk(0.05 * ring, ring) == 0, "walk deadzone")
check(Sticks.walk(ring, ring) == 1 and Sticks.walk(-ring, ring) == -1, "full walk")
check(math.abs(Sticks.walk(3, ring) + Sticks.walk(-3, ring)) < 1e-9, "walk is symmetric")

-- aim: angle from the right stick, fires past the ring
local a, fire = Sticks.aim(0, 0, ring)
check(a == nil and fire == false, "resting stick neither aims nor fires")
a, fire = Sticks.aim(0.3 * ring, 0, ring)
check(math.abs(a) < 1e-9 and fire == false, "a gentle push aims without firing")
a, fire = Sticks.aim(0, -0.9 * ring, ring)
check(math.abs(a + math.pi / 2) < 1e-9 and fire, "a hard push up aims up and fires")

-- sizes follow the short side and fit two to a phone
for _, short in ipairs({ 360, 400, 460, 540, 780 }) do
	local size = Sticks.size(short)
	check(size >= 110 and size <= 150, "stick size " .. size)
end
check(2 * Sticks.size(360) + 40 <= 780 and Sticks.size(360) + 20 <= 360, "two sticks fit a 780x360 landscape phone")
-- the playfield is never wide enough to be a problem, but the sticks leave a gap to play in
check(780 - 2 * (Sticks.size(360) + 10) >= 400, "a landscape phone keeps 400 px between the sticks")

-- the button slots hug the right edge and the reload slot (the one drawn) clears the aim stick
for _, view in ipairs({ { 960, 460 }, { 780, 360 }, { 844, 390 } }) do
	local w, h = view[1], view[2]
	local size = Sticks.size(math.min(w, h))
	local stick_top = h - 10 - size
	for name, s in pairs(Sticks.slots(w, h)) do
		-- the pickup slot is the one outside the column: just left of swap, same row
		check(name == "pickup" or s.x + s.w >= w - 12, name .. " slot hugs the right edge at " .. w)
		check(s.x + s.w <= w and s.y >= 0, name .. " slot is on screen")
		if name ~= "pickup" then check(s.y + s.h <= stick_top, name .. " slot clears the aim stick at " .. w .. "x" .. h) end
		if name == "reload" then check(s.y + s.h <= stick_top, name .. " slot clears the aim stick at " .. w .. "x" .. h) end
	end
end
-- no slot overlaps another (pickup beside swap, the column below)
for _, view in ipairs({ { 960, 460 }, { 780, 360 } }) do
	local slots = Sticks.slots(view[1], view[2])
	for a, sa in pairs(slots) do
		for b, sb in pairs(slots) do
			if a < b then
				check(sa.x + sa.w <= sb.x or sb.x + sb.w <= sa.x or sa.y + sa.h <= sb.y or sb.y + sb.h <= sa.y,
					a .. " and " .. b .. " slots overlap at " .. view[1])
			end
		end
	end
	check(slots.pickup.y == slots.swap.y and slots.pickup.x + slots.pickup.w < slots.swap.x, "pickup sits left of swap")
	check(slots.pickup.x >= 0, "pickup slot is on screen")
	for name, s in pairs(slots) do check(s.w >= 36 and s.h >= 36, name .. " slot is a thumb-sized target at " .. view[1]) end
end
-- zoom: step through a gun's levels, wrap to 1, label, reset on a gun change
local levels = { 1, 2, 3, 4 }
check(Sticks.zoom_step(levels, 1) == 2 and Sticks.zoom_step(levels, 3) == 4, "a tap steps to the next level")
check(Sticks.zoom_step(levels, 4) == 1, "a tap on the last level wraps to 1")
check(Sticks.zoom_step({ 1 }, 1) == 1, "a one-level gun stays at 1")
check(Sticks.zoom_step(levels, 2, 1) == 3 and Sticks.zoom_step(levels, 4, 1) == 4, "wheel out steps up and stops at the top")
check(Sticks.zoom_step(levels, 2, -1) == 1 and Sticks.zoom_step(levels, 1, -1) == 1, "wheel in steps down and stops at 1")
check(Sticks.zoom_label(1) == "1x" and Sticks.zoom_label(2) == "2x" and Sticks.zoom_label(1.25) == "1.25x" and Sticks.zoom_label(1.5) == "1.5x", "labels")
local gun = { id = "sniper", stats = { zooms = levels } }
check(Sticks.zoom_levels(gun) == levels and Sticks.zoom_levels(nil)[1] == 1 and #Sticks.zoom_levels({ id = "x", stats = {} }) == 1, "levels of a gun")
local z = Sticks.zoom_new()
check(z.i == 1, "zoom starts at 1")
Sticks.zoom_track(z, "sniper", 0, true)
z.i = 3
Sticks.zoom_track(z, "sniper", 0, true)
check(z.i == 3, "tracking the same gun keeps the level")
Sticks.zoom_track(z, "sniper", 0, true)
Sticks.zoom_track(z, "rifle", 0, true)
check(z.i == 1, "a different gun resets to 1")
Sticks.zoom_track(z, "rifle", 0.3, true)
z.i = 2
Sticks.zoom_track(z, "rifle", 0.2, true)
Sticks.zoom_track(z, "rifle", 0.0, true)
check(z.i == 2, "a swap clock running down keeps the level")
Sticks.zoom_track(z, "rifle", 0.3, true)
check(z.i == 1, "a fresh equip (swap clock restarts) resets to 1 even for the same gun id")
z.i = 3
Sticks.zoom_track(z, "rifle", 0, false)
check(z.i == 1, "dying resets to 1")
-- the zoom slot is the one under reload, clear of the aim stick
for _, view in ipairs({ { 960, 460 }, { 780, 360 } }) do
	local slots = Sticks.slots(view[1], view[2])
	local size = Sticks.size(math.min(view[1], view[2]))
	check(slots.zoom and slots.zoom.y > slots.reload.y and slots.zoom.y + slots.zoom.h <= view[2] - 10 - size, "zoom slot sits under reload, above the aim stick at " .. view[1])
end
local zoned = false
for _, zone in ipairs(Camera.zones(960, 460)) do
	local s = Sticks.slots(960, 460).zoom
	if zone.x0 == s.x and zone.y0 == s.y then zoned = true end
end
check(zoned, "the camera keeps the soldier off the zoom button")

-- mouse aim (2026-10-27): from the cursor to the soldier's SCREEN position, a dead radius, light smoothing, and a camera
-- look-ahead driven by the cursor's screen position (not by the aim) so nothing feeds back.
math.atan2 = math.atan2 or function(y, x) return math.atan(y, x) end
local function deg(r) return r * 180 / math.pi end
local function wrap(a) return (a + math.pi) % (2 * math.pi) - math.pi end
-- smoothing: a 90-degree step of the cursor reaches 90 % within ~0.1 s, never overshoots, and lands
local m = Sticks.mouse_new()
check(Sticks.mouse_step(m, 1 / 60, 600, 300, 400, 300, 50, 0) == 0, "cursor dead right aims 0")
local a
for i = 1, 6 do a = Sticks.mouse_step(m, 1 / 60, 400, 100, 400, 300, 50, 0) end -- cursor straight up: -90 deg, 0.1 s
check(a < 0 and a <= -0.9 * math.pi / 2, "reaches 90% of a 90-degree step in 0.1 s: " .. deg(a))
check(a >= -math.pi / 2 - 1e-9, "no overshoot")
for i = 1, 60 do a = Sticks.mouse_step(m, 1 / 60, 400, 100, 400, 300, 50, 0) end
check(math.abs(a + math.pi / 2) < 1e-3, "settles on the target")
-- the short way round across +-180
m = Sticks.mouse_new()
Sticks.mouse_step(m, 1 / 60, 100, 299, 400, 300, 50, 0)
local b = Sticks.mouse_step(m, 1 / 60, 100, 301, 400, 300, 50, 0)
check(math.abs(wrap(b - math.pi)) < 0.2, "smoothing crosses +-180 the short way: " .. deg(b))
-- dead radius: the cursor over the soldier keeps the last direction
m = Sticks.mouse_new()
for i = 1, 30 do a = Sticks.mouse_step(m, 1 / 60, 500, 200, 400, 300, 50, 0) end
local held = a
for i = 1, 30 do a = Sticks.mouse_step(m, 1 / 60, 400 + (i % 5) - 2, 300 + (i % 7) - 3, 400, 300, 50, 0) end
check(math.abs(a - held) < 1e-6, "inside the dead radius the aim holds: " .. deg(a - held))
m = Sticks.mouse_new()
check(Sticks.mouse_step(m, 1 / 60, 401, 300, 400, 300, 50, 1.25) == 1.25, "inside the dead radius from the start keeps the current aim")
check(Sticks.mouse_dead(2) == 2 * 52 and Sticks.mouse_dead(0.5) == 26, "the dead radius is one soldier tall")

-- the loop: a fixed cursor on screen, the camera easing toward its look-ahead for 2 s. The aim settles.
local map = { w = 1600, h = 900 }
local function settle(w, h, cx, cy, px, py)
	local cam = Camera.new(map)
	cam:snap(w, h, px, py, 0, 1)
	local mm, hist = Sticks.mouse_new(), {}
	for i = 1, 120 do
		local geo = cam:geometry(w, h, px, py)
		local sx, sy = Camera.to_screen(geo, px, py - 4)
		local aim = Sticks.mouse_step(mm, 1 / 60, cx, cy, sx, sy, Sticks.mouse_dead(geo.scale), 0)
		cam:update(1 / 60, w, h, px, py, aim, 1, Camera.cursor_lead(w, h, cx, cy))
		hist[i] = aim
	end
	return deg(math.abs(hist[120] - hist[90]))
end
for _, c in ipairs({ { 960, 480, 700, 300 }, { 960, 480, 560, 240 }, { 960, 480, 480, 120 }, { 960, 480, 900, 400 }, { 780, 360, 300, 200 }, { 960, 480, 560, 330 } }) do
	local drift = settle(c[1], c[2], c[3], c[4], 800, 500)
	check(drift < 1, string.format("aim settles with a resting cursor at (%d,%d) in %dx%d: drift %.4f deg over the last 0.5 s", c[3], c[4], c[1], c[2], drift))
end
-- the look-ahead is a function of the cursor's screen position alone, and bounded
local lead = Camera.cursor_lead(960, 480, 480, 264)
check(math.abs(lead[1]) < 1e-9 and math.abs(lead[2]) < 1e-9, "no look-ahead with the cursor on the anchor")
lead = Camera.cursor_lead(960, 480, 960, 264)
check(lead[1] > 0.9 and lead[1] <= 1 and math.abs(lead[2]) < 1e-9, "cursor at the right edge leads right")
for _, p in ipairs({ { 0, 0 }, { 960, 480 }, { 0, 480 }, { 960, 0 } }) do
	local l = Camera.cursor_lead(960, 480, p[1], p[2])
	check(math.sqrt(l[1] ^ 2 + l[2] ^ 2) <= 1 + 1e-9, "look-ahead never exceeds the stick/aim lead")
end
-- the cursor lead moves the camera toward the cursor like an aim does
local cam = Camera.new(map)
cam:snap(960, 480, 800, 500, 0, 1)
local x0 = cam.x
for i = 1, 120 do cam:update(1 / 60, 960, 480, 800, 500, 0, 1, { -1, 0 }) end
check(cam.x < x0 - 20, "an explicit lead overrides the aim and shifts the camera")
print("jetpack controls ok: walk, proportional thrust, aim and fire, slots, zoom, mouse aim")
