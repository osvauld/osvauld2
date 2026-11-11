-- The robot's own clips, by numbers (plan: docs/design/brawl-movement.md §0, P-A3 and P-A4).
--   lua scripts/test_brawl_clips.lua
-- P-A3: author.lua's clip builder. P-A4: feet on the floor for the grounded acts' clips and a hand
-- or boot on the wall line for `wall`, by forward kinematics over the robot's own shapes.
package.path = "scripts/?.lua;" .. package.path
local U = require("brawl_test_util")
local Look = require("look")
local A = require("characters/robot/author")
local setup = require("characters/paka/setup")
local scout = require("characters/robot/scout")
local T = U.tune

local function fails(f, want)
	local ok, err = pcall(f)
	assert(not ok, "should fail: " .. want)
	assert(tostring(err):find(want, 1, true), "error says " .. tostring(err) .. ", want " .. want)
end

-- P-A3 ------------------------------------------------------------------------------------------
do
	local empty = A.clip({ length = 0.5 })
	for _, t in ipairs({ 0, 0.1, 0.25, 0.5 }) do
		local pose = A.at(empty, t)
		for id, tr in pairs(setup.tracks) do
			for prop, keys in pairs(tr) do
				assert(pose[id] and pose[id][prop] == keys[1][2], "P-A3: empty clip " .. id .. "." .. prop .. " at " .. t)
			end
		end
	end
	local p0 = A.at(empty, 0)
	assert(p0["slot:sword_forward"].show == false, "P-A3: the sword stays sheathed")
	assert(p0["slot:blink_front"].show == false and p0["slot:blink_back"].show == false, "P-A3: eyes open")

	local keyed = A.clip({ length = 1, keys = {
		leg_front = { rot = { { 0, 0 }, { 1, 10 } }, scale_x = { { 0, 1.5 } } },
		body = { y = { { 0, 0 }, { 0.5, 8, "in_out" } } },
	} })
	local base = setup.tracks.leg_front.rot[1][2]
	local p = A.at(keyed, 0.5)
	assert(math.abs(p.leg_front.rot - (base + 5)) < 1e-9, "P-A3: a rot key adds to the setup's")
	assert(p.leg_front.scale_x == 1.5, "P-A3: a scale key multiplies the setup's")
	assert(p.body.y == 8, "P-A3: a part setup.lua doesn't track starts at rest")
	assert(A.at(keyed, 1).leg_front.rot == base + 10)
	fails(function() A.clip({ length = 1, keys = { body = { y = { { 0, 0 }, { 0, 1 } } } } }) end, "rise strictly")
	fails(function() A.clip({ length = 1, keys = { body = { y = { { 0.5, 0 }, { 0.2, 1 } } } } }) end, "rise strictly")
	fails(function() A.clip({ length = 1, keys = { body = { y = { { 2, 0 } } } } }) end, "outside")
	fails(function() A.clip({ length = 1, keys = { nose = { y = { { 0, 0 } } } } }) end, "the rig lacks: nose")
	fails(function() A.clip({ length = 1, keys = { body = { spin = { { 0, 0 } } } } }) end, "unknown property")
	fails(function() A.clip({ length = 1, keys = { leg_front = { rot = { { 0, 1 } } } }, ik = { leg_front = { { 0, 180, 230 } } } }) end,
		"which the clip keys too")
	-- the IK helpers land where they are told
	local ik = A.clip({ length = 0.5, ik = { arm_front = { bend = 1, { 0, 140, 150 }, { 0.5, 135, 120 } },
		leg_back = { { 0, 225, 226 } } } })
	for _, t in ipairs({ 0, 0.25, 0.5 }) do
		local pose = A.at(ik, t)
		local hx, hy = A.pivot(pose, "palm_front")
		local wx, wy = 140 + (135 - 140) * t / 0.5, 150 + (120 - 150) * t / 0.5
		assert(math.abs(hx - wx) < 0.01 and math.abs(hy - wy) < 0.01, "P-A3: two-bone IK at " .. t .. ": " .. hx .. "," .. hy)
		local ax, ay = A.pivot(pose, "hoof-back")
		assert(math.abs(ax - 225) < 0.05 and math.abs(ay - 226) < 0.05, "P-A3: piston IK: " .. ax .. "," .. ay)
	end
end

-- P-A4 ------------------------------------------------------------------------------------------

local byid = {}
for _, p in ipairs(scout.parts) do byid[p.id] = p end
local ROOT = byid.root.pivot -- the feet line is y = ROOT[2]; the wall line, facing right, x = ROOT[1] + body_w / 2

-- Every path point of `id`'s shapes in the drawing, under `pose`: min x, max x, max y.
local function extent(pose, world, id)
	local m = world(id)
	local x0, x1, y1 = math.huge, -math.huge, -math.huge
	for _, s in ipairs(byid[id].shapes) do
		for _, cmd in ipairs(s.path) do
			for i = 2, #cmd, 2 do
				local x, y = m[1] * cmd[i] + m[3] * cmd[i + 1] + m[5], m[2] * cmd[i] + m[4] * cmd[i + 1] + m[6]
				if x < x0 then x0 = x end
				if x > x1 then x1 = x end
				if y > y1 then y1 = y end
			end
		end
	end
	return x0, x1, y1
end

local function times(c)
	local out = {}
	local n = math.floor(c.length * 60 + 1e-9)
	for i = 0, n do out[#out + 1] = i / 60 end
	if not c.loop and out[#out] < c.length then out[#out + 1] = c.length end
	return out
end

local clips = {}
for _, name in ipairs(Look.all_clips()) do clips[name] = require(Look.path(name)) end

-- Feet: for idle, land and turn each boot's lowest point is within 6 of the feet line (both feet
-- down); for run the lowest of the two is (one foot is up).
local SOLES = { "slot:hoof_front", "slot:hoof-back" }
for _, act in ipairs({ "idle", "run", "land", "turn" }) do
	local c = clips[Look.ACTS[act].clip]
	local worst = 0
	for _, t in ipairs(times(c)) do
		local pose = A.at(c, t)
		local w = A.world(pose)
		local lows = {}
		for i, id in ipairs(SOLES) do
			local _, _, y = extent(pose, w, id)
			lows[i] = y - ROOT[2]
		end
		local check = act == "run" and { math.max(lows[1], lows[2]) } or lows
		for _, d in ipairs(check) do
			if math.abs(d) > math.abs(worst) then worst = d end
			assert(math.abs(d) <= 6, string.format("P-A4: %s at %.4f: a boot is %.2f off the floor", act, t, d))
		end
	end
	print(string.format("P-A4 %s: boots within %.2f of the floor", act, worst))
end

-- Wall: facing the wall (a wall slide holds toward it), the most wall-ward point of the hands and
-- boots is within 6 of the wall line, and none is through it by more.
do
	local c = clips.wall
	local line = ROOT[1] + T.body_w / 2
	local worst = 0
	for _, t in ipairs(times(c)) do
		local pose = A.at(c, t)
		local w = A.world(pose)
		local far = -math.huge
		for _, id in ipairs({ "slot:palm_front", "slot:palm_back", "slot:hoof_front", "slot:hoof-back" }) do
			local _, x1 = extent(pose, w, id)
			if x1 > far then far = x1 end
		end
		local d = far - line
		if math.abs(d) > math.abs(worst) then worst = d end
		assert(math.abs(d) <= 6, string.format("P-A4: wall at %.4f: hand or boot %.2f from the wall line", t, d))
	end
	print(string.format("P-A4 wall: a hand or boot within %.2f of the wall line", worst))
end

-- Every robot clip keeps the sword sheathed on the back, at every frame.
for _, name in ipairs(Look.OWN) do
	for _, t in ipairs(times(clips[name])) do
		local pose = A.at(clips[name], t)
		assert(pose["slot:sword_forward"].show == false and (pose["slot:sword"] == nil or pose["slot:sword"].show),
			"the sword is drawn in " .. name .. " at " .. t)
	end
end

print("brawl clips ok")
