-- Clips for the robots, written as changes from the setup pose (docs/design/brawl-movement.md
-- decision 5). `A.clip(spec)` returns a plain clip module that `gfx.clip` takes:
--
--   return A.clip({ length = 0.4, loop = false,
--     keys = { body = { y = { {0, 0}, {0.1, 12, "in_out"} } } },   -- a change from the setup pose
--     ik = { leg_front = { {0, x, y}, {0.4, x, y} },                -- the ankle to (x, y)
--            arm_back = { bend = -1, {0, x, y} } },                 -- the hand to (x, y)
--   })
--
-- Every part setup.lua tracks starts at its setup value; a key on x, y or rot adds to it, a key on
-- scale, scale_x or scale_y multiplies it. The sword stays sheathed (`slot:sword_forward` hidden)
-- and the blinks shut unless a clip keys their `show`. `ik` is solved here, at load time, at every
-- 1/60 s: Lua setting rotations (2d-characters.md decision 11), the clip stays data.
--
-- Pure Lua: no gfx. `A.at` samples a clip as the engine does (world/src/clip.rs) and `A.world`
-- composes a pose as runtime/src/drawing.rs does, so tests can measure a clip with numbers.
local rig = require("characters/paka/rig")
local setup = require("characters/paka/setup")

local A = {}

local FPS = 60
local PROPS = { x = 0, y = 0, rot = 0, scale = 1, scale_x = 1, scale_y = 1 }
local SCALES = { scale = true, scale_x = true, scale_y = true }
local SHUT = { ["slot:sword_forward"] = false, ["slot:blink_front"] = false, ["slot:blink_back"] = false }

local byid = {}
for _, p in ipairs(rig.parts) do byid[p.id] = p end
A.parts = byid

-- The chains `ik` can name. A two-bone chain turns `upper` and `lower` to put `tip`'s pivot on the
-- target (the elbow bends to `bend`'s side). A piston (the robot's one-segment legs) turns `leg` to
-- the target and stretches it along its axis, and keeps the boot's world angle and size as set up.
A.CHAINS = {
	arm_front = { upper = "shoulder_front", lower = "arm_front", tip = "palm_front" },
	arm_back = { upper = "shoulder_back", lower = "arm_back", tip = "palm_back" },
	leg_front = { leg = "leg_front", foot = "hoof_front" },
	leg_back = { leg = "leg_back", foot = "hoof-back" },
}

local function setup_value(id, prop)
	local tr = setup.tracks[id]
	local keys = tr and tr[prop]
	if keys then return keys[1][2] end
	return PROPS[prop]
end
A.setup_value = setup_value

-- Sampling, as world/src/clip.rs samples ---------------------------------------------------------

local function cubic(p1, p2, s)
	local c = 3 * p1
	local b = 3 * (p2 - p1) - c
	local a = 1 - c - b
	return ((a * s + b) * s + c) * s
end

local function ease(e, u)
	if e == nil or e == "linear" then return u end
	if e == "in_out" then return u * u * (3 - 2 * u) end
	if u <= 0 then return 0 end
	local lo, hi = 0, 1
	for _ = 1, 60 do
		local mid = (lo + hi) * 0.5
		if cubic(e[1], e[3], mid) < u then lo = mid else hi = mid end
	end
	return cubic(e[2], e[4], (lo + hi) * 0.5)
end

local function value_at(keys, t)
	local n = 0
	while n < #keys and keys[n + 1][1] <= t do n = n + 1 end
	if n == 0 then return keys[1][2] end
	if n == #keys then return keys[n][2] end
	local a, b = keys[n], keys[n + 1]
	if b[3] == "step" then return a[2] end
	return a[2] + (b[2] - a[2]) * ease(b[3], (t - a[1]) / (b[1] - a[1]))
end

-- `clip:at(t)` in Lua: each tracked part's whole pose.
function A.at(clip, t)
	if clip.loop then t = t % clip.length else t = math.max(0, math.min(t, clip.length)) end
	local out = {}
	for id, tr in pairs(clip.tracks) do
		local p = { x = 0, y = 0, rot = 0, scale = 1, scale_x = 1, scale_y = 1, show = true }
		for prop, keys in pairs(tr) do
			if prop == "show" then
				local v = keys[1][2]
				for _, k in ipairs(keys) do if k[1] <= t then v = k[2] end end
				p.show = v
			else
				p[prop] = value_at(keys, t)
			end
		end
		out[id] = p
	end
	return out
end

-- Forward kinematics ----------------------------------------------------------------------------

local function mul(q, m)
	return { q[1] * m[1] + q[3] * m[2], q[2] * m[1] + q[4] * m[2], q[1] * m[3] + q[3] * m[4],
		q[2] * m[3] + q[4] * m[4], q[1] * m[5] + q[3] * m[6] + q[5], q[2] * m[5] + q[4] * m[6] + q[6] }
end

-- A function id -> the part's matrix from its own space to the drawing's, under `pose`.
function A.world(pose)
	local cache = {}
	local function at(id)
		local m = cache[id]
		if m then return m end
		local part, o = byid[id], pose[id]
		local px, py = part.pivot[1], part.pivot[2]
		local ox, oy = o and o.x or 0, o and o.y or 0
		local r = math.rad(o and o.rot or 0)
		local sc = o and o.scale or 1
		local sx, sy = sc * (o and o.scale_x or 1), sc * (o and o.scale_y or 1)
		local c, n = math.cos(r), math.sin(r)
		local a, b, cc, d = c * sx, n * sx, -n * sy, c * sy
		m = { a, b, cc, d, ox + px - (a * px + cc * py), oy + py - (b * px + d * py) }
		if part.parent then m = mul(at(part.parent), m) end
		cache[id] = m
		return m
	end
	return at
end

local function apply(m, x, y) return m[1] * x + m[3] * y + m[5], m[2] * x + m[4] * y + m[6] end

-- Where part `id`'s pivot is, in the drawing, under `pose`.
function A.pivot(pose, id, world)
	local p = byid[id].pivot
	return apply((world or A.world(pose))(id), p[1], p[2])
end

-- The setup pose, as `A.at` gives a pose.
function A.setup_pose()
	local pose = {}
	for id, tr in pairs(setup.tracks) do
		local p = { x = 0, y = 0, rot = 0, scale = 1, scale_x = 1, scale_y = 1, show = true }
		for prop, keys in pairs(tr) do p[prop] = keys[1][2] end
		pose[id] = p
	end
	return pose
end

-- Where part `id`'s pivot is in the setup pose: the place to aim a hand or plant an ankle from.
function A.home(id)
	local x, y = A.pivot(A.setup_pose(), id)
	return { x, y }
end

-- IK ---------------------------------------------------------------------------------------------

local atan2 = math.atan2 or math.atan -- Luau has atan2; stock Lua 5.4's atan takes two
local function angle(x, y) return math.deg(atan2(y, x)) end
local function dist(x, y) return math.sqrt(x * x + y * y) end
local function wrap(d)
	d = d % 360
	if d >= 180 then d = d - 360 end
	return d
end

-- Two bones: the new local rotations of upper and lower that put tip's pivot on (tx, ty).
local function two_bone(pose, ch, tx, ty, bend)
	local w = A.world(pose)
	local x0, y0 = A.pivot(pose, ch.upper, w)
	local x1, y1 = A.pivot(pose, ch.lower, w)
	local x2, y2 = A.pivot(pose, ch.tip, w)
	local l1, l2, d = dist(x1 - x0, y1 - y0), dist(x2 - x1, y2 - y1), dist(tx - x0, ty - y0)
	d = math.max(math.abs(l1 - l2) + 1e-6, math.min(d, l1 + l2 - 1e-6))
	local inner = math.deg(math.acos(math.max(-1, math.min(1, (l1 * l1 + d * d - l2 * l2) / (2 * l1 * d)))))
	local d1 = wrap(angle(tx - x0, ty - y0) + bend * inner - angle(x1 - x0, y1 - y0))
	local up = pose[ch.upper]
	up.rot = up.rot + d1
	w = A.world(pose)
	x1, y1 = A.pivot(pose, ch.lower, w)
	x2, y2 = A.pivot(pose, ch.tip, w)
	local d2 = wrap(angle(tx - x1, ty - y1) - angle(x2 - x1, y2 - y1))
	local lo = pose[ch.lower]
	lo.rot = lo.rot + d2
	return { [ch.upper] = { rot = up.rot }, [ch.lower] = { rot = lo.rot } }
end

-- A piston leg: turned and stretched so the foot's pivot (the ankle) is on (tx, ty); the boot keeps
-- its setup world angle and is unstretched.
local function piston(pose, ch, tx, ty, foot_angle)
	local w = A.world(pose)
	local hx, hy = A.pivot(pose, ch.leg, w)
	local ax, ay = A.pivot(pose, ch.foot, w)
	local stretch = dist(tx - hx, ty - hy) / dist(ax - hx, ay - hy)
	local leg, foot = pose[ch.leg], pose[ch.foot]
	leg.rot = leg.rot + wrap(angle(tx - hx, ty - hy) - angle(ax - hx, ay - hy))
	leg.scale_x = leg.scale_x * stretch
	foot.scale_x = foot.scale_x / stretch
	w = A.world(pose)
	local m = w(ch.foot)
	foot.rot = foot.rot + wrap(foot_angle - angle(m[1], m[2]))
	return { [ch.leg] = { rot = leg.rot, scale_x = leg.scale_x }, [ch.foot] = { rot = foot.rot, scale_x = foot.scale_x } }
end

-- Building a clip ------------------------------------------------------------------------------

local function check_keys(where, keys, length, bool)
	assert(type(keys) == "table" and #keys > 0, where .. ": needs keys")
	local last = -math.huge
	for i, k in ipairs(keys) do
		local t = k[1]
		assert(type(t) == "number" and t >= 0 and t <= length, where .. " key " .. i .. ": time outside 0.." .. length)
		assert(t > last, where .. " key " .. i .. ": key times must rise strictly")
		last = t
		if bool then
			assert(type(k[2]) == "boolean", where .. " key " .. i .. ": show is a boolean")
		else
			assert(type(k[2]) == "number", where .. " key " .. i .. ": not a number")
		end
	end
end

function A.clip(spec)
	local length, loop = spec.length, spec.loop == true
	assert(type(length) == "number" and length > 0, "clip needs a length")
	for k in pairs(spec) do
		assert(k == "length" or k == "loop" or k == "keys" or k == "ik", "clip: unknown field " .. tostring(k))
	end
	local tracks = {}
	for id, tr in pairs(setup.tracks) do
		local t = {}
		for prop, keys in pairs(tr) do t[prop] = { { 0, keys[1][2] } } end
		tracks[id] = t
	end
	for id, show in pairs(SHUT) do
		tracks[id] = tracks[id] or {}
		if tracks[id].show == nil then tracks[id].show = { { 0, show } } end
	end
	local keyed = {}
	for id, tr in pairs(spec.keys or {}) do
		assert(byid[id], "clip keys a part the rig lacks: " .. tostring(id))
		tracks[id] = tracks[id] or {}
		for prop, keys in pairs(tr) do
			local where = id .. "." .. tostring(prop)
			if prop == "show" then
				check_keys(where, keys, length, true)
				tracks[id].show = keys
			else
				assert(PROPS[prop] ~= nil, where .. ": unknown property")
				check_keys(where, keys, length)
				local base = setup_value(id, prop)
				local out = {}
				for i, k in ipairs(keys) do
					local v = SCALES[prop] and base * k[2] or base + k[2]
					out[i] = { k[1], v, k[3] }
				end
				tracks[id][prop] = out
				keyed[where] = true
			end
		end
	end
	local clip = { length = length, loop = loop, tracks = tracks }
	if not spec.ik then return clip end

	-- IK: solved at every 1/60 s over the base clip, the targets eased between their keys.
	local order = {}
	for name, targets in pairs(spec.ik) do
		local ch = A.CHAINS[name]
		assert(ch, "ik names no chain: " .. tostring(name) .. " (arm_front, arm_back, leg_front, leg_back)")
		check_keys("ik " .. name, targets, length)
		for _, id in ipairs(ch.leg and { ch.leg, ch.foot } or { ch.upper, ch.lower }) do
			for _, prop in ipairs({ "rot", "scale_x" }) do
				assert(not keyed[id .. "." .. prop], "ik " .. name .. " sets " .. id .. "." .. prop .. ", which the clip keys too")
			end
		end
		order[#order + 1] = name
	end
	table.sort(order)
	local foot_angle = {}
	do
		local w = A.world(A.setup_pose())
		for _, name in ipairs(order) do
			local ch = A.CHAINS[name]
			if ch.foot then
				local m = w(ch.foot)
				foot_angle[name] = angle(m[1], m[2])
			end
		end
	end
	local times = {}
	local n = math.floor(length * FPS + 1e-9)
	for i = 0, n do times[#times + 1] = i / FPS end
	if times[#times] < length - 1e-9 then times[#times + 1] = length end
	local solved = {}
	for _, t in ipairs(times) do
		local pose = A.at(clip, t)
		for _, name in ipairs(order) do
			local ch, targets = A.CHAINS[name], spec.ik[name]
			local xs, ys = {}, {}
			for i, k in ipairs(targets) do
				xs[i] = { k[1], k[2], k[4] }
				ys[i] = { k[1], k[3], k[4] }
			end
			local tx, ty = value_at(xs, t), value_at(ys, t)
			local out = ch.leg and piston(pose, ch, tx, ty, foot_angle[name]) or two_bone(pose, ch, tx, ty, targets.bend or 1)
			for id, props in pairs(out) do
				for prop, v in pairs(props) do
					local key = id .. "." .. prop
					solved[key] = solved[key] or { id = id, prop = prop, keys = {} }
					table.insert(solved[key].keys, { t, v })
				end
			end
		end
	end
	for _, s in pairs(solved) do
		local keys, prev = s.keys, nil
		-- keep a turning angle continuous from one frame to the next
		if s.prop == "rot" then
			for _, k in ipairs(keys) do
				if prev then k[2] = prev + wrap(k[2] - prev) end
				prev = k[2]
			end
		end
		tracks[s.id][s.prop] = keys
	end
	return clip
end

return A
