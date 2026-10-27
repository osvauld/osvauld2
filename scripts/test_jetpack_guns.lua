-- The baked guns (demo_apps/jetpack/baked/guns.lua) through format.lua's gun validation, plus what
-- only the baked content promises: six guns, strict point placement, the design table's shape.
-- Run from the repo root with stock Lua 5.4:  lua scripts/test_jetpack_guns.lua
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local F = require("format")
local data = require("baked/guns")

local fails = 0
local function check(ok, msg)
	if not ok then fails = fails + 1; print("FAIL " .. msg) end
end
local num = function(v, lo, hi) return type(v) == "number" and v == v and v >= lo and v <= hi end

local guns, err = F.guns(data)
check(err == nil, "every baked gun validates: " .. tostring(err))
check(#guns >= 6 and #guns == #data.guns, "at least six guns, none dropped")

local ids = {}
for _, g in ipairs(guns) do
	local at = "gun " .. g.id
	-- zoom levels: first 1, ascending, at most four steps; the long guns zoom further than the short ones
	local zs = g.stats.zooms
	check(zs and zs[1] == 1 and #zs >= 1 and #zs <= 4, at .. ": zoom levels start at 1")
	for i = 2, #zs do check(zs[i] > zs[i - 1], at .. ": zoom levels ascend") end
	if g.id == "sniper" then check(zs[#zs] == 4 and #zs == 4, at .. ": the sniper scope goes to 4x") end
	if g.id == "pistol" or g.id == "smg" or g.id == "shotgun" then check(zs[#zs] <= 1.25, at .. ": close guns barely zoom") end
	ids[g.id] = true
	local w, h = g.size[1], g.size[2]
	check(g.scale >= 0.2 and g.scale <= 0.5, at .. ": scale in proportion to the 0.5 pilot")
	check(g.vars.ink and g.vars.metal and g.vars.accent and (g.vars.wood or g.vars.grip), at .. ": the usual palette")
	-- Baked art stays inside its box (the format only forbids wild numbers).
	for _, p in ipairs(g.parts) do
		check(#p.shapes > 0, at .. " " .. p.id .. ": has shapes")
		for _, s in ipairs(p.shapes) do
			for i, c in ipairs(s.path) do
				for k = 2, #c, 2 do
					check(num(c[k], 0, w) and num(c[k + 1], 0, h), at .. " shape " .. s.id .. " cmd " .. i .. ": inside the box")
				end
			end
		end
	end
	check((g.id == "pistol") == (g.points.support == nil), at .. ": only the pistol is one-handed")
	check(g.points.muzzle[1] > g.points.grip[1], at .. ": muzzle is forward of the grip")
end
for _, id in ipairs({ "pistol", "smg", "rifle", "shotgun", "sniper", "rocket" }) do
	check(ids[id], "gun " .. id .. " is baked")
end

if fails > 0 then print(fails .. " failure(s)"); os.exit(1) end
print("jetpack guns ok: " .. #guns .. " guns")
