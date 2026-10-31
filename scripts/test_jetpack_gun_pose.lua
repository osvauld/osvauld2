-- The gun in the soldier's hands, pure maths (rig.lua, no gfx): for every baked gun, at an aim sweep and
-- both facings, the gun palm sits on the gun's grip, the free palm on its support (two-handed), and the
-- gun's points follow the aim and the mirror. Run from the repo root with stock Lua 5.4.
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local F = require("format")
local R = require("rig")
local function check(ok, why) assert(ok, why) end

local pilots, perr = F.pilots(require("baked/pilots"))
check(#pilots == 1, "baked pilots must validate: " .. tostring(perr))
local scout = pilots[1]
local guns, gerr = F.guns(require("baked/guns"))
check(#guns >= 6 and not gerr, "baked guns must validate: " .. tostring(gerr))
local s = scout.scale

local function dist(a, b) return math.sqrt((a[1] - b[1]) ^ 2 + (a[2] - b[2]) ^ 2) end
local function game(p) return { p[1] * s, p[2] * s } end

local checked, free_checked = 0, 0
for _, gun in ipairs(guns) do
	local hold = R.gun_hold(gun, scout)
	check((hold.support ~= nil) == (gun.points.support ~= nil), gun.id .. ": support only when the gun has one")
	for _, facing in ipairs({ 1, -1 }) do
		for deg = -85, 85, 5 do
			local own = math.rad(deg)
			local aim = facing > 0 and own or math.pi - own
			local at = gun.id .. " at " .. deg .. " facing " .. facing
			local pose = R.pose(scout, aim, hold)
			local mount = game({ pose.gun.x, pose.gun.y })
			checked = checked + 1

			check(pose.reach.gun, at .. ": gun arm reaches the grip")
			check(dist(game(pose.points.palm_gun), mount) < 0.5, at .. ": gun palm off the grip")
			local grip = R.gun_point(gun, "grip", mount, aim, facing)
			check(dist(grip, mount) < 1e-9, at .. ": the grip is the mount")
			if gun.points.support then
				free_checked = free_checked + 1
				local support = R.gun_point(gun, "support", mount, aim, facing)
				check(pose.reach.free, at .. ": free arm reaches the support")
				check(dist(game(pose.points.palm_free), support) < 0.5,
					string.format("%s: free palm off the support by %.2f", at, dist(game(pose.points.palm_free), support)))
			else
				check(R.gun_point(gun, "support", mount, aim, facing) == nil, at .. ": no support point")
			end

			-- Every point keeps its distance from the grip (scaled), and the muzzle is along the barrel.
			local ux, uy = math.cos(aim), math.sin(aim)
			for _, name in ipairs({ "muzzle", "eject", "mag" }) do
				local p, q = R.gun_point(gun, name, mount, aim, facing), gun.points[name]
				local d = math.sqrt((q[1] - gun.points.grip[1]) ^ 2 + (q[2] - gun.points.grip[2]) ^ 2) * gun.scale
				check(math.abs(dist(p, mount) - d) < 1e-9, at .. ": " .. name .. " keeps its distance")
			end
			local m = R.gun_point(gun, "muzzle", mount, aim, facing)
			local along = (m[1] - mount[1]) * ux + (m[2] - mount[2]) * uy
			local across = -(m[1] - mount[1]) * uy + (m[2] - mount[2]) * ux
			local g = gun.points
			check(math.abs(along - (g.muzzle[1] - g.grip[1]) * gun.scale) < 1e-9, at .. ": muzzle along the barrel")
			check(math.abs(across - (g.muzzle[2] - g.grip[2]) * gun.scale * facing) < 1e-9, at .. ": muzzle across the barrel")
			check(along > 0 and math.abs(across) < along * 0.5, at .. ": the muzzle is ahead of the grip, near the line")

			-- The drawing's transform puts every point where gun_point says.
			local t = R.gun_transform(gun, mount, aim, facing)
			for _, name in ipairs({ "grip", "muzzle", "mag" }) do
				local q = gun.points[name]
				local x, y = t[1] * q[1] + t[3] * q[2] + t[5], t[2] * q[1] + t[4] * q[2] + t[6]
				check(dist({ x, y }, R.gun_point(gun, name, mount, aim, facing)) < 1e-9, at .. ": transform agrees on " .. name)
			end
		end
	end
	-- Aiming flat left is aiming flat right in a mirror: the same heights, the x offsets reversed.
	local right, left = R.pose(scout, 0, hold), R.pose(scout, math.pi, hold)
	local rm, lm = game({ right.gun.x, right.gun.y }), game({ left.gun.x, left.gun.y })
	for _, name in ipairs({ "muzzle", "eject", "mag" }) do
		local p, q = R.gun_point(gun, name, rm, 0, 1), R.gun_point(gun, name, lm, math.pi, -1)
		check(math.abs((p[2] - rm[2]) - (q[2] - lm[2])) < 1e-9, gun.id .. ": " .. name .. " keeps its height facing left")
		check(math.abs((p[1] - rm[1]) + (q[1] - lm[1])) < 1e-9, gun.id .. ": " .. name .. " mirrors in x")
	end
	check(R.gun_point(gun, "muzzle", lm, math.pi, -1)[1] < lm[1], gun.id .. ": the muzzle points left")
end
check(checked == #guns * 2 * 35, "swept every gun both ways: " .. checked)
check(free_checked == (#guns - 1) * 2 * 35, "every gun but the pistol is two-handed")

-- Scale decision (revised 2026-10-27, step 2b): every gun is drawn at (nearly) the drafted 0.5 game units per
-- drawing unit, pistol included, and the scout's arms are long enough that the two hands of a two-handed
-- gun sit visibly apart: at a level aim the palms are at least MIN_GAP game units apart (a glove is ~8 across,
-- so 12 reads as two hands, not one fist), and the gun elbow is never folded deeper than MAX_FOLD degrees.
local MIN_SCALE, MIN_GAP, MAX_FOLD = 0.45, 12, 115
local function bend(pose, side)
	local sh, el, wr = pose.points["shoulder_" .. side], pose.points["elbow_" .. side], pose.points["wrist_" .. side]
	local ux, uy, vx, vy = sh[1] - el[1], sh[2] - el[2], wr[1] - el[1], wr[2] - el[2]
	local c = (ux * vx + uy * vy) / math.sqrt((ux * ux + uy * uy) * (vx * vx + vy * vy))
	return 180 - math.deg(math.acos(math.max(-1, math.min(1, c)))) -- 0 = straight arm
end
local min_gap, worst_fold = math.huge, 0
for _, gun in ipairs(guns) do
	check(gun.scale >= MIN_SCALE and gun.scale <= scout.scale, gun.id .. ": scale " .. gun.scale .. " is full size")
	local hold = R.gun_hold(gun, scout)
	for _, facing in ipairs({ 1, -1 }) do
		local aim = facing > 0 and 0 or math.pi
		local pose = R.pose(scout, aim, hold)
		local fold = bend(pose, "gun")
		worst_fold = math.max(worst_fold, fold)
		check(fold <= MAX_FOLD, string.format("%s facing %d: gun elbow folded %.0f degrees", gun.id, facing, fold))
		if hold.support then
			local gap = dist(game(pose.points.palm_gun), game(pose.points.palm_free))
			min_gap = math.min(min_gap, gap)
			check(gap >= MIN_GAP, string.format("%s facing %d: palms only %.1f game units apart", gun.id, facing, gap))
		end
	end
end
local torso_h = 0
for _, part in ipairs(scout.parts) do
	if part.id == "torso" then
		local lo, hi = math.huge, -math.huge
		for _, sh in ipairs(part.shapes) do
			for _, c in ipairs(sh.path) do
				if c[3] then lo, hi = math.min(lo, c[3]), math.max(hi, c[3]) end
			end
		end
		torso_h = (hi - lo) * s
	end
end
local rifle = F.find(guns, "rifle")
local rifle_len = rifle.size[1] * rifle.scale
check(rifle_len > torso_h * 1.4 and rifle_len < torso_h * 3.2, string.format("rifle %.1f long beside a torso %.1f tall", rifle_len, torso_h))
-- Recoil (step 3): at the shot, the gun is pushed back along its barrel by the gun's `recoil` and its muzzle
-- climbs; both palms stay on the grip and support, and the body (shoulders, head) does not move.
local FX = require("fire_fx")
local kicked = 0
for _, gun in ipairs(guns) do
	local hold = R.gun_hold(gun, scout)
	for _, facing in ipairs({ 1, -1 }) do
		for deg = -60, 60, 20 do
			local own = math.rad(deg)
			local aim = facing > 0 and own or math.pi - own
			local at = gun.id .. " at " .. deg .. " facing " .. facing
			local base = R.pose(scout, aim, hold)
			local bm = game({ base.gun.x, base.gun.y })
			for _, age in ipairs({ 0, 0.3, 0.7 }) do
				local t = FX.kick_time(gun) * age
				local back, climb = FX.recoil(gun, t)
				local k = {}
				for key, v in pairs(hold) do k[key] = v end
				k.kick, k.climb = back / s, climb
				local pose = R.pose(scout, aim, k)
				kicked = kicked + 1
				local mount = game({ pose.gun.x, pose.gun.y })
				check(pose.facing == facing, at .. ": the climb never flips the facing")
				check(pose.reach.gun and (not gun.points.support or pose.reach.free), at .. ": arms still reach under recoil")
				check(dist(game(pose.points.palm_gun), mount) < 0.5, at .. ": gun palm follows the kicked grip")
				if gun.points.support then
					local support = R.gun_point(gun, "support", mount, pose.gun.angle, facing)
					check(dist(game(pose.points.palm_free), support) < 0.5, string.format("%s age %.1f: free palm off the kicked support by %.2f",
						at, age, dist(game(pose.points.palm_free), support)))
				end
				-- the body stays: shoulders and the head's turn are the same as at rest
				check(dist(pose.points.shoulder_gun, base.points.shoulder_gun) < 1e-9 and dist(pose.points.shoulder_free, base.points.shoulder_free) < 1e-9, at .. ": shoulders stay")
				check(pose.parts.head.rot == base.parts.head.rot and pose.parts.torso.rot == 0, at .. ": head and torso stay")
				check(pose.parts.finger_gun.rot == 0, at .. ": no trigger asked, no squeeze")
				-- the mount moved: back along the (raised) barrel by `back`, plus the arm lifting with the climb
				local travelled = dist(mount, bm)
				check(back == 0 or travelled > 0, at .. ": the gun moved when kicked")
				if t == 0 then
					-- Muzzle climbs: at a level aim it sits higher relative to the grip than at rest.
					if deg == 0 then
						local m0 = R.gun_point(gun, "muzzle", bm, base.gun.angle, facing)
						local m1 = R.gun_point(gun, "muzzle", mount, pose.gun.angle, facing)
						check((m1[2] - mount[2]) < (m0[2] - bm[2]) - 0.1, at .. ": the muzzle climbs")
						local along = (m1[1] - m0[1]) * facing
						check(along < -(back * 0.8), string.format("%s: the muzzle kicks back by about the recoil (%.2f vs %.2f)", at, along, back))
					end
				end
			end
			-- trigger squeeze moves only the finger
			local squeezed = R.pose(scout, aim, { support = hold.support, drop = hold.drop, trigger = 1 })
			check(squeezed.parts.finger_gun.rot == R.trigger_travel, at .. ": a squeeze curls the finger")
		end
	end
end
check(kicked == #guns * 2 * 7 * 3, "recoil poses swept: " .. kicked)

print(string.format("palm gap >= %.1f game units, worst gun-elbow bend %.0f degrees", min_gap, worst_fold))
print("jetpack gun pose ok: " .. checked .. " poses, " .. free_checked .. " two-handed")
