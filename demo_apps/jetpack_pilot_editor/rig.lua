-- The pilot rig, pure maths (no gfx): from an aim, how every part of a version 2 pilot (format.lua)
-- is posed so the gun hand sits on the gun's grip and the free hand on its support. Shared by the game and the
-- pilot editor's preview (each app ships an identical copy; scripts/test_jetpack_format.lua checks
-- they match); scripts/test_jetpack_rig.lua tests it under stock lua.
local R = {}

-- Luau's math.atan takes one argument and has atan2; stock Lua 5.4 is the reverse.
local atan2 = math.atan2 or math.atan

R.head_limit = 35 -- degrees the head turns toward the aim, either way
R.trigger_travel = 32 -- degrees the trigger finger curls when fully squeezed
R.leg_swing = 0.3 -- radians the legs swing at walk = 1

local function degrees(r)
	local d = math.deg(r) % 360
	return d > 180 and d - 360 or d
end
local function turn(x, y, a)
	local c, s = math.cos(a), math.sin(a)
	return x * c - y * s, x * s + y * c
end
local function dist(a, b) return math.sqrt((a[1] - b[1]) ^ 2 + (a[2] - b[2]) ^ 2) end

local function pivot(pilot, id)
	for _, p in ipairs(pilot.parts) do
		if p.id == id then return p.pivot end
	end
end

-- An arm at rest: shoulder, the bone lengths and the directions they were drawn in, and where the
-- palm sits from the wrist (the hand's own frame is unturned at rest).
local function arm(pilot, upper, fore, hand, palm)
	local s, e, w, p = pivot(pilot, upper), pivot(pilot, fore), pivot(pilot, hand), pilot.points[palm]
	return { s = s, l1 = dist(s, e), l2 = dist(e, w), a1 = atan2(e[2] - s[2], e[1] - s[1]),
		a2 = atan2(w[2] - e[2], w[1] - e[1]), pw = { p[1] - w[1], p[2] - w[2] } }
end

-- Rots (degrees, each relative to its parent) that put the palm on `target` with the hand turned
-- `own` radians; elbows bend down (clockwise on screen). Out of reach the arm is straight toward the
-- target. Also whether it was reached.
local function reach(a, target, own)
	local px, py = turn(a.pw[1], a.pw[2], own)
	local wx, wy = target[1] - px, target[2] - py -- where the wrist must be
	local dx, dy = wx - a.s[1], wy - a.s[2]
	local d = math.sqrt(dx * dx + dy * dy)
	local ang1, ang2, ok
	if d >= a.l1 + a.l2 then
		local tx, ty = target[1] - a.s[1], target[2] - a.s[2]
		ang1 = (tx == 0 and ty == 0) and own or atan2(ty, tx)
		ang2, ok = ang1, false
	else
		d = math.max(d, math.abs(a.l1 - a.l2) + 1e-9, 1e-9)
		local c = (a.l1 * a.l1 + d * d - a.l2 * a.l2) / (2 * a.l1 * d)
		local base = (dx == 0 and dy == 0) and own or atan2(dy, dx)
		ang1 = base + math.acos(math.max(-1, math.min(1, c)))
		local ex, ey = a.s[1] + a.l1 * math.cos(ang1), a.s[2] + a.l1 * math.sin(ang1)
		ang2, ok = atan2(wy - ey, wx - ex), true
	end
	local r1 = ang1 - a.a1
	local r2 = (ang2 - a.a2) - r1
	return degrees(r1), degrees(r2), degrees(own - (ang2 - a.a2)), ok
end

-- Forward kinematics of the rots just solved: the elbow, wrist and palm in the pilot's own space.
local function place(a, r1, r2, r3)
	local c1 = math.rad(r1)
	local c2 = c1 + math.rad(r2)
	local c3 = c2 + math.rad(r3)
	local ex, ey = turn(a.l1 * math.cos(a.a1), a.l1 * math.sin(a.a1), c1)
	local elbow = { a.s[1] + ex, a.s[2] + ey }
	local wx, wy = turn(a.l2 * math.cos(a.a2), a.l2 * math.sin(a.a2), c2)
	local wrist = { elbow[1] + wx, elbow[2] + wy }
	local px, py = turn(a.pw[1], a.pw[2], c3)
	return elbow, wrist, { wrist[1] + px, wrist[2] + py }
end

-- How a pilot stands for a world `aim` (radians, y down). `opts`:
--   support  units along the barrel from the grip where the free hand holds (nil: one-handed, the
--            free arm hangs and swings by `swing` radians)
--   drop     units below the barrel the free palm sits (default 0)
--   trigger  0..1 squeeze of the trigger finger
--   walk     -1..1 leg swing
--   kick     pilot units the gun is pushed back along its barrel (recoil); both hands follow
--   climb    radians the muzzle is raised (recoil); the body and head keep the true aim
-- Returns { facing, parts = { id = { rot } } (degrees, for drawing:pose, in the unmirrored pilot),
-- gun = { x, y, angle } (pilot space, mirrored with facing; angle is the aim, raised by `climb`), points (pilot
-- space, mirrored): shoulder_*, elbow_*, wrist_*, palm_gun, palm_free, muzzle_base, eye;
-- reach = { gun, free } (false: out of reach, arm straight; free is nil when one-handed) }.
function R.pose(pilot, aim, opts)
	opts = opts or {}
	local facing = math.cos(aim) >= 0 and 1 or -1
	local aimed = facing > 0 and aim or math.pi - aim -- the aim seen in the unmirrored pilot
	local climb = opts.climb or 0
	local own = aimed - climb -- the gun's own angle: the muzzle climbs (up is a smaller angle unmirrored)
	local w = pilot.size[1]
	local gun_arm = arm(pilot, "arm_gun", "forearm_gun", "hand_gun", "palm_gun")
	local free_arm = arm(pilot, "arm_free", "forearm_free", "hand_free", "palm_free")
	local hold = pilot.points.hold
	local hx, hy = turn(hold[1] - gun_arm.s[1], hold[2] - gun_arm.s[2], own)
	local kick = opts.kick or 0
	local grip = { gun_arm.s[1] + hx - kick * math.cos(own), gun_arm.s[2] + hy - kick * math.sin(own) }

	local parts, reached = {}, {}
	local g1, g2, g3, gok = reach(gun_arm, grip, own)
	parts.arm_gun, parts.forearm_gun, parts.hand_gun = { rot = g1 }, { rot = g2 }, { rot = g3 }
	reached.gun = gok

	local f1, f2, f3
	if opts.support then
		local ux, uy = math.cos(own), math.sin(own)
		local drop = opts.drop or 0
		local target = { grip[1] + opts.support * ux - drop * uy, grip[2] + opts.support * uy + drop * ux }
		f1, f2, f3, reached.free = reach(free_arm, target, own)
	else
		-- Hanging: down and a little forward, the forearm bent forward, the hand following it.
		local ang1 = math.pi / 2 - 0.35 + (opts.swing or 0)
		f1 = degrees(ang1 - free_arm.a1)
		f2 = degrees(free_arm.a1 - free_arm.a2 - 0.5)
		f3 = 0
	end
	parts.arm_free, parts.forearm_free, parts.hand_free = { rot = f1 }, { rot = f2 }, { rot = f3 }

	local walk = math.deg((opts.walk or 0) * R.leg_swing)
	parts.leg_back, parts.leg_front = { rot = walk }, { rot = -walk }
	parts.jetpack, parts.torso, parts.thumb_gun = { rot = 0 }, { rot = 0 }, { rot = 0 }
	local look = math.max(-R.head_limit, math.min(R.head_limit, degrees(aimed)))
	parts.head = { rot = look }
	parts.finger_gun = { rot = math.max(0, math.min(1, opts.trigger or 0)) * R.trigger_travel }

	local function out(p) return { facing > 0 and p[1] or w - p[1], p[2] } end
	local ge, gw, gp = place(gun_arm, g1, g2, g3)
	local fe, fw, fp = place(free_arm, f1, f2, f3)
	local neck, eye = pivot(pilot, "head"), pilot.points.eye
	local ex, ey = turn(eye[1] - neck[1], eye[2] - neck[2], math.rad(look))
	local mount = out(grip)
	return {
		facing = facing, own = own, parts = parts, reach = reached,
		gun = { x = mount[1], y = mount[2], angle = aim + (facing > 0 and -climb or climb) },
		points = {
			shoulder_gun = out(gun_arm.s), elbow_gun = out(ge), wrist_gun = out(gw), palm_gun = out(gp),
			shoulder_free = out(free_arm.s), elbow_free = out(fe), wrist_free = out(fw), palm_free = out(fp),
			muzzle_base = mount, eye = out({ neck[1] + ex, neck[2] + ey }),
		},
	}
end

-- The gun in the hands ------------------------------------------------------------------------
-- Gun art is drawn pointing right with its handle below; in game space (the actor's frame, game
-- units) a gun sits with its `grip` on `mount` (the rig's gun point, in game units), turned to `aim`,
-- its y flipped when facing left so the handle stays below.

-- The pose options that make the free hand hold `gun`: where its support point is from the grip along
-- the barrel and below it, in the pilot's drawing units. One-handed guns give none.
function R.gun_hold(gun, pilot)
	local s, g = gun.points.support, gun.points.grip
	if not s then return {} end
	local k = gun.scale / pilot.scale
	return { support = (s[1] - g[1]) * k, drop = (s[2] - g[2]) * k }
end

-- A drawing point of the gun (by drawing coordinates) in game space.
local function gun_at(gun, mount, aim, facing, px, py)
	local g = gun.points.grip
	local dx, dy = (px - g[1]) * gun.scale, (py - g[2]) * gun.scale * facing
	local c, s = math.cos(aim), math.sin(aim)
	return { mount[1] + dx * c - dy * s, mount[2] + dx * s + dy * c }
end

-- A named point of the gun (grip, support, muzzle, eject, mag) in game space; nil if it has none.
function R.gun_point(gun, name, mount, aim, facing)
	local p = gun.points[name]
	if not p then return nil end
	return gun_at(gun, mount, aim, facing, p[1], p[2])
end

-- The gfx transform { xx, yx, xy, yy, dx, dy } that draws the gun's drawing there.
function R.gun_transform(gun, mount, aim, facing)
	local o = gun_at(gun, mount, aim, facing, 0, 0)
	local x1, x2 = gun_at(gun, mount, aim, facing, 1, 0), gun_at(gun, mount, aim, facing, 0, 1)
	return { x1[1] - o[1], x1[2] - o[2], x2[1] - o[1], x2[2] - o[2], o[1], o[2] }
end

return R
