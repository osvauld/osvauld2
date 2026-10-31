-- The pilot rig in plain Lua: two-bone IK puts the hands on the gun and the support point.
-- Forward kinematics here is written apart from rig.lua's, from the pose rots alone.
package.path = "demo_apps/jetpack/?.lua;" .. package.path
local F = require("format")
local R = require("rig")
local function check(ok, why) assert(ok, why) end

local pilots, err = F.pilots(require("baked/pilots"))
check(#pilots == 1, "baked pilots must validate: " .. tostring(err))
local scout = pilots[1]
local W = scout.size[1]

local function part(id)
	for _, p in ipairs(scout.parts) do if p.id == id then return p end end
end
local function rot(v, a)
	local c, s = math.cos(a), math.sin(a)
	return v[1] * c - v[2] * s, v[1] * s + v[2] * c
end
-- Unmirrored world points of an arm from its three part rots (degrees, each relative to its parent).
local function fk(pose, upper, fore, hand, palm)
	local S, E, Wr, P = part(upper).pivot, part(fore).pivot, part(hand).pivot, scout.points[palm]
	local a1 = math.rad(pose.parts[upper].rot)
	local a2 = a1 + math.rad(pose.parts[fore].rot)
	local a3 = a2 + math.rad(pose.parts[hand].rot)
	local dx, dy = rot({ E[1] - S[1], E[2] - S[2] }, a1)
	local elbow = { S[1] + dx, S[2] + dy }
	dx, dy = rot({ Wr[1] - E[1], Wr[2] - E[2] }, a2)
	local wrist = { elbow[1] + dx, elbow[2] + dy }
	dx, dy = rot({ P[1] - Wr[1], P[2] - Wr[2] }, a3)
	return elbow, wrist, { wrist[1] + dx, wrist[2] + dy }, a3
end
local function mirror(p, facing) return { facing > 0 and p[1] or W - p[1], p[2] } end
local function dist(a, b) return math.sqrt((a[1] - b[1]) ^ 2 + (a[2] - b[2]) ^ 2) end
local function normal(d) d = d % 360; return d > 180 and d - 360 or d end

local gs, fs = part("arm_gun").pivot, part("arm_free").pivot
local gl = dist(gs, part("forearm_gun").pivot) + dist(part("forearm_gun").pivot, part("hand_gun").pivot)
local fl = dist(fs, part("forearm_free").pivot) + dist(part("forearm_free").pivot, part("hand_free").pivot)
local SUPPORT, DROP = 18, 7

local gun_reach, free_reach, free_short, aims = 0, 0, 0, 0
for _, facing in ipairs({ 1, -1 }) do
	for deg = -80, 80, 5 do
		local own = math.rad(deg)
		local aim = facing > 0 and own or math.pi - own
		aims = aims + 1
		local pose = R.pose(scout, aim, { support = SUPPORT, drop = DROP })
		check(pose.facing == facing, "facing follows the aim")

		-- The gun palm lands on the gun's grip, the hand turned along the barrel.
		local elbow, wrist, palm, hand_angle = fk(pose, "arm_gun", "forearm_gun", "hand_gun", "palm_gun")
		local grip = mirror(palm, facing)
		check(pose.gun.x and math.abs(pose.gun.angle - aim) < 1e-9, "the gun points along the aim")
		if pose.reach.gun then
			gun_reach = gun_reach + 1
			check(dist(grip, { pose.gun.x, pose.gun.y }) < 0.5,
				string.format("gun palm off the grip by %.2f at %d° facing %d", dist(grip, { pose.gun.x, pose.gun.y }), deg, facing))
			check(math.abs(normal(math.deg(hand_angle) - deg)) < 0.5, "the gun hand lies along the barrel")
			-- The elbow bends downward: below the shoulder-to-wrist line (clockwise in y-down).
			local cross = (wrist[1] - gs[1]) * (elbow[2] - gs[2]) - (wrist[2] - gs[2]) * (elbow[1] - gs[1])
			check(cross >= -1e-9, "the gun elbow bends down at " .. deg)
		end
		-- Unreachable: the arm is straight, bones in line toward the target.
		if not pose.reach.gun then
			check(math.abs(dist(gs, elbow) + dist(elbow, wrist) - dist(gs, wrist)) < 1e-6, "an unreachable gun arm is straight")
		end

		-- The free palm lands on the support point on the barrel (below it by DROP).
		local fe, fw, fp = fk(pose, "arm_free", "forearm_free", "hand_free", "palm_free")
		local g = { pose.gun.x, pose.gun.y }
		local u = { math.cos(own), math.sin(own) } -- unmirrored barrel direction
		local gu = mirror({ 0, 0 }, 1)
		local gx = facing > 0 and g[1] or W - g[1]
		local target = { gx + SUPPORT * u[1] - DROP * u[2], g[2] + SUPPORT * u[2] + DROP * u[1] }
		if pose.reach.free then
			free_reach = free_reach + 1
			check(dist(fp, target) < 0.5, string.format("free palm off the support by %.2f at %d° facing %d", dist(fp, target), deg, facing))
			local cross = (fw[1] - fs[1]) * (fe[2] - fs[2]) - (fw[2] - fs[2]) * (fe[1] - fs[1])
			check(cross >= -1e-9, "the free elbow bends down at " .. deg)
			local m = mirror(fp, facing)
			check(dist(m, pose.points.palm_free) < 1e-6, "the pose reports the free palm")
		else
			free_short = free_short + 1
			check(math.abs(dist(fs, fe) + dist(fe, fw) - dist(fs, fw)) < 1e-6, "an unreachable free arm is straight")
			local want = math.atan(target[2] - fs[2], target[1] - fs[1])
			local have = math.atan(fe[2] - fs[2], fe[1] - fs[1])
			check(math.abs(normal(math.deg(have - want))) < 1e-6, "the unreachable free arm points at the target")
		end
	end
end
check(gun_reach == aims, "the scout's gun arm reaches its grip across the whole sweep")
check(free_reach >= aims * 0.8, "the free arm reaches the support for most aims: " .. free_reach .. "/" .. aims)

-- A target past the arm's length leaves it straight toward the target: a far support point.
local far = R.pose(scout, 0, { support = 200 })
check(far.reach.free == false and far.reach.gun == true, "a far support is out of reach")
local fe, fw = fk(far, "arm_free", "forearm_free", "hand_free", "palm_free")
check(math.abs(dist(fs, fe) + dist(fe, fw) - fl) < 1e-6 and math.abs(fe[2] - fs[2]) < dist(fs, fe), "the far arm is straight")

-- Mirroring: aiming at pi - a gives the same arm rots as aiming at a, the gun at the mirrored spot.
for _, deg in ipairs({ 0, 30, -50 }) do
	local a = R.pose(scout, math.rad(deg), { support = SUPPORT, drop = DROP, trigger = 0.4, walk = 0.5 })
	local b = R.pose(scout, math.pi - math.rad(deg), { support = SUPPORT, drop = DROP, trigger = 0.4, walk = 0.5 })
	check(a.facing == 1 and b.facing == -1, "opposite facings")
	for id, p in pairs(a.parts) do
		check(math.abs(p.rot - b.parts[id].rot) < 1e-9, "mirrored rot of " .. id)
	end
	check(math.abs((W - b.gun.x) - a.gun.x) < 1e-9 and math.abs(a.gun.y - b.gun.y) < 1e-9, "the gun mirrors")
	check(math.abs((W - b.points.palm_free[1]) - a.points.palm_free[1]) < 1e-9, "the free hand mirrors")
end

-- The head looks toward the aim, at most 35° either way.
check(math.abs(R.pose(scout, math.rad(20), {}).parts.head.rot - 20) < 1e-9, "the head follows a small aim")
check(R.pose(scout, math.rad(80), {}).parts.head.rot == 35, "the head stops at +35")
check(R.pose(scout, math.rad(-80), {}).parts.head.rot == -35, "the head stops at -35")
check(R.pose(scout, math.pi - math.rad(80), {}).parts.head.rot == 35, "the head stops at +35 facing left")

-- One-handed: no support lets the free arm hang down, and swing moves it.
local hung = R.pose(scout, 0, {})
check(hung.reach.free == nil, "a hanging arm reaches for nothing")
local _, hw = fk(hung, "arm_free", "forearm_free", "hand_free", "palm_free")
check(hw[2] > fs[2] + fl * 0.5, "the hanging free hand is well below its shoulder")
local swung = R.pose(scout, 0, { swing = 0.5 })
check(swung.parts.arm_free.rot > hung.parts.arm_free.rot + 20, "swing moves the hanging arm")
check(swung.parts.arm_gun.rot == hung.parts.arm_gun.rot, "the gun arm ignores swing")

-- The trigger finger squeezes; the other parts do not care.
local pulled, released = R.pose(scout, 0, { trigger = 1 }), R.pose(scout, 0, { trigger = 0 })
check(pulled.parts.finger_gun.rot > released.parts.finger_gun.rot + 10, "the trigger finger squeezes")
check(pulled.parts.hand_gun.rot == released.parts.hand_gun.rot, "the hand holds still")

-- Legs follow walk.
local walk = R.pose(scout, 0, { walk = 1 })
check(walk.parts.leg_back.rot > 10 and walk.parts.leg_front.rot == -walk.parts.leg_back.rot, "legs swing in opposition")

for _, id in ipairs(F.parts) do check(hung.parts[id] and type(hung.parts[id].rot) == "number", "a rot for " .. id) end
print("jetpack rig ok")
