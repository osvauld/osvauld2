-- Brawl's picture in stock Lua (plan: docs/design/brawl-movement.md §0, P-A1, P-A2, P-X1, and step
-- 4's `body` measured from the robot). P-A3 and P-A4 are in test_brawl_clips.lua.
--   lua scripts/test_brawl_look.lua
package.path = "scripts/?.lua;" .. package.path
local U = require("brawl_test_util")
local Look = require("look")
local Script = require("script")
local T = U.tune

local function read(path)
	local f = io.open(path, "rb")
	if not f then return nil end
	local s = f:read("a")
	f:close()
	return s
end

-- P-X1: the character files are copies of character_lab's, byte for byte (as Jetpack's shared
-- modules are checked in test_jetpack_format.lua). No atlas: the robots name no picture.
do
	local files = { "characters/paka/rig.lua", "characters/paka/setup.lua",
		"characters/robot/build.lua", "characters/robot/scout.lua", "characters/robot/brute.lua",
		"characters/robot/volt.lua", "characters/robot/author.lua", "characters/robot/pack.lua" }
	for _, name in ipairs(Look.all_clips()) do files[#files + 1] = Look.path(name) .. ".lua" end
	for _, file in ipairs(files) do
		local canonical = read("demo_apps/character_lab/" .. file)
		assert(canonical, "P-X1: no character_lab/" .. file)
		assert(read("demo_apps/brawl/" .. file) == canonical, "P-X1: brawl/" .. file .. " differs from character_lab/" .. file)
	end
	assert(read("demo_apps/brawl/characters/paka/atlas.png") == nil, "P-X1: brawl carries no atlas")
	assert(#Look.CLIPS == 2 and #Look.OWN == 8 and #Look.FIGHT == 17, "P-X1: two of Paka's clips, eight movement and 17 fighting clips of the robot's")
	-- character_lab lists the robot's clips (cast.lua), so they are authored where the picker is
	local cast = read("demo_apps/character_lab/cast.lua")
	for _, name in ipairs(Look.all_clips()) do
		assert(cast:find('"' .. name .. '"', 1, true), "P-X1: character_lab's cast does not list " .. name)
	end
end

-- The clips as data: length and loop, which gfx.clip reads back as clip.length and clip.loop.
local info = {}
for _, name in ipairs(Look.all_clips()) do
	local c = require(Look.path(name))
	info[name] = { length = c.length, loop = c.loop == true }
end

-- Every act fighter.lua can produce: the literals it sets (set_act, place's ground-or-air line) and
-- tests against, read from the source, then confirmed by running the scripted timeline.
local produced = {}
do
	local src = read("demo_apps/brawl/fighter.lua")
	for args in src:gmatch("set_act%(f,([^\n]*)%)") do
		for name in args:gmatch('"(%l[%w_]*)"') do produced[name] = true end
	end
	for line in src:gmatch("[^\n]*f%.act, f%.act_t = [^\n]*") do
		for name in line:gmatch('"(%l[%w_]*)"') do produced[name] = true end
	end
	for name in src:gmatch('f%.act == "(%l[%w_]*)"') do produced[name] = true end
	local n = 0
	for _ in pairs(produced) do n = n + 1 end
	assert(n >= 10, "P-A1: read " .. n .. " acts from fighter.lua")
	local s, rules = U.fresh()
	for t = 1, Script.length do
		U.F.step(s, { Script.at(t) }, rules)
		local act = s.fighters[1].act
		assert(produced[act], "P-A1: the script reached act " .. act .. ", which the source scan missed")
	end
end

-- P-A1: the table is total over those acts; each names one of the clips.
for act in pairs(produced) do
	local a = Look.ACTS[act]
	assert(a, "P-A1: no look for act " .. act)
	assert(info[a.clip], "P-A1: act " .. act .. " names clip " .. tostring(a.clip) .. ", not one of Look's clips")
end
for act, a in pairs(Look.ACTS) do
	assert(info[a.clip], "P-A1: act " .. act .. " names an unknown clip")
end
assert(Look.ACTS.out.hidden and Look.ACTS.ko.hidden and not Look.ACTS.idle.hidden, "P-A1: out and ko are not drawn")

-- P-A1: the time is act_t / 120, wrapped for a looped clip and held for a once clip; the frame
-- index is that time quantized to 1/60 s (two ticks a frame), and the frame's time never runs
-- past the exact time by a frame.
local function close(a, b) return math.abs(a - b) < 1e-9 end
for name, c in pairs(info) do
	local frames = Look.frames(c)
	local ticks = c.length * 120
	if c.loop then
		assert(ticks == math.floor(ticks) and frames * 2 == ticks, "P-A1: looped " .. name .. " is a whole number of frames")
	end
	for act_t = 0, 1000 do
		local p = Look.at(c, act_t)
		local t = act_t / 120
		local want = c.loop and (act_t % ticks) / 120 or math.min(t, c.length)
		assert(close(p.t, want), "P-A1: " .. name .. " at act_t " .. act_t .. ": t " .. p.t .. ", want " .. want)
		assert(p.index >= 0 and p.index < frames and p.index == math.floor(p.index), "P-A1: frame index " .. p.index)
		if c.loop then
			assert(p.index < frames, "P-A1: a looped clip's index wraps")
			assert(close(p.time, p.index / 60), "P-A1: quantized time")
			-- the same frame one loop later
			local later = Look.at(c, act_t + frames * 2)
			assert(later.index == p.index, "P-A1: " .. name .. " wraps after one length")
		else
			assert(close(p.time, math.min(p.index / 60, c.length)), "P-A1: quantized time, held")
		end
		assert(p.time <= p.t + 1e-9 and p.t - p.time < 1 / 60 + 1e-9, "P-A1: quantized within a frame of t")
		if act_t % 2 == 0 and (c.loop or t <= c.length) then
			assert(close(p.time, p.t), "P-A1: on a frame boundary the quantized time is exact")
		end
	end
	if not c.loop then
		local held = Look.at(c, 100000)
		assert(close(held.t, c.length) and close(held.time, c.length), "P-A1: a once clip holds its end")
	end
end

-- Look.pick: act + act_t → clip name, times and the cache key; the same state gives the same key.
do
	local p = Look.pick("run", 7, info)
	assert(p.clip == "run" and p.index == 3 and close(p.t, 7 / 120), "P-A1: pick")
	assert(Look.pick("run", 7, info).key == p.key, "P-A1: the key is a function of the state")
	assert(Look.pick("idle", 7, info).key ~= p.key, "P-A1: another clip, another key")
	assert(Look.pick("run", 7 + 96, info).key == p.key, "P-A1: a loop later, the same key")
end

-- Step 4: the box is measured from the robot. Forward kinematics over the setup pose, composed as
-- the engine composes a pose (runtime/src/drawing.rs): the chassis and boots set the width, the
-- helmet's top (not the antennae, fins or dome) the height, both above the root's pivot (the feet).
do
	local setup = require("characters/paka/setup")
	local scout = require("characters/robot/scout")
	local function mul(m, n)
		return { m[1] * n[1] + m[3] * n[2], m[2] * n[1] + m[4] * n[2], m[1] * n[3] + m[3] * n[4],
			m[2] * n[3] + m[4] * n[4], m[1] * n[5] + m[3] * n[6] + m[5], m[2] * n[5] + m[4] * n[6] + m[6] }
	end
	local byid, world = {}, {}
	for _, p in ipairs(scout.parts) do byid[p.id] = p end
	local function key(tr, k, d) return tr and tr[k] and tr[k][1][2] or d end
	local function at(id)
		if world[id] then return world[id] end
		local p, tr = byid[id], setup.tracks[id]
		local px, py = p.pivot[1], p.pivot[2]
		local r = math.rad(key(tr, "rot", 0))
		local s = key(tr, "scale", 1)
		local sx, sy = s * key(tr, "scale_x", 1), s * key(tr, "scale_y", 1)
		local c, n = math.cos(r), math.sin(r)
		local m = mul(mul({ 1, 0, 0, 1, key(tr, "x", 0) + px, key(tr, "y", 0) + py }, { c * sx, n * sx, -n * sy, c * sy, 0, 0 }),
			{ 1, 0, 0, 1, -px, -py })
		if p.parent then m = mul(at(p.parent), m) end
		world[id] = m
		return m
	end
	local function bounds(ids)
		local x0, y0, x1, y1 = math.huge, math.huge, -math.huge, -math.huge
		for _, id in ipairs(ids) do
			local m = at(id)
			for _, s in ipairs(byid[id].shapes) do
				for _, cmd in ipairs(s.path) do
					for i = 2, #cmd, 2 do
						local x, y = m[1] * cmd[i] + m[3] * cmd[i + 1] + m[5], m[2] * cmd[i] + m[4] * cmd[i + 1] + m[6]
						x0, y0, x1, y1 = math.min(x0, x), math.min(y0, y), math.max(x1, x), math.max(y1, y)
					end
				end
			end
		end
		return x0, y0, x1, y1
	end
	local root = byid.root.pivot
	local bx0, _, bx1, by1 = bounds({ "slot:body", "slot:hoof_front", "slot:hoof-back" })
	local _, hy0 = bounds({ "slot:head_main" })
	local w, h = (bx1 - bx0) * Look.SCALE, (root[2] - hy0) * Look.SCALE
	print(string.format("robot measured (setup pose): chassis and boots %.1f wide, helmet top %.1f above the feet, boots' soles %.1f below the root", w, h, by1 - root[2]))
	assert(math.abs(T.body_w - w) <= 4, "body_w " .. T.body_w .. " is not the robot's width " .. w)
	assert(math.abs(T.body_h - h) <= 4, "body_h " .. T.body_h .. " is not the robot's height " .. h)
	assert(math.abs(by1 - root[2]) <= 6, "the root pivot is at the soles")
	-- Look.sole (the planting in the view) agrees with this FK on the setup pose, given as clip:at
	-- gives a pose (each tracked part's x, y, rot, scale, scale_x, scale_y).
	local pose = {}
	for id, tr in pairs(setup.tracks) do
		pose[id] = { x = key(tr, "x", 0), y = key(tr, "y", 0), rot = key(tr, "rot", 0), scale = key(tr, "scale", 1),
			scale_x = key(tr, "scale_x", 1), scale_y = key(tr, "scale_y", 1), show = true }
	end
	local sole = Look.sole(scout, pose)
	assert(math.abs(sole - by1) < 1e-9, "Look.sole " .. sole .. " vs FK " .. by1)
	-- and on every frame of the planted clips it is a real number near the floor (Paka's clips
	-- move the root and legs, so not exactly the setup's)
	assert(math.abs(Look.sole(scout, nil) - Look.sole(scout, {})) < 1e-12, "no pose is the rest pose")
	assert(root[1] * 2 == scout.size[1], "the root pivot is the drawing's centre line, so mirroring about it keeps the box")
end

-- The fighting clips are built a view at a time (L:warm, after the robots): until one is, its act draws
-- its stand-in, and the pick says so; the same state gives the same key once it is.
do
	local partial = {}
	for name, c in pairs(info) do partial[name] = c end
	partial.sheavy = nil
	local p = Look.pick("sheavy", 30, partial)
	assert(p.clip == "idle" and p.plant, "stand-in while unwarmed: " .. p.clip)
	local q = Look.pick("sheavy", 30, info)
	assert(q.clip == "sheavy" and q.index == 15 and q.key == "sheavy@15", "the move's own frame: " .. q.key)
	assert(Look.pick("nair", 9, partial).clip == "nair" and Look.pick("nair", 9, { apex = info.apex }).clip == "apex", "an aerial stands in on apex")
	assert(Look.blend_of({ act = "sheavy", act_t = 1, prev_act = "nlight", prev_t = 3 }, { idle = info.idle }) == nil,
		"two stand-ins on one clip don't blend")
	local all = {}
	for _, n in ipairs(Look.all_clips()) do all[n] = true end
	for _, n in ipairs(Look.FIGHT) do assert(all[n] and not Look.STARTUP[n], "fight clip " .. n .. " is warmed, not built at load") end
end

-- P-A2: the blend. Computed from state alone (prev_act, prev_t, act_t); at the switch tick the pose
-- is the old clip's, at blend_ticks the new one's; rot takes the short way round.
do
	local A = require("characters/robot/author")
	local clips = {}
	for _, name in ipairs(Look.all_clips()) do clips[name] = require(Look.path(name)) end
	local function pose_of(f)
		local p = Look.pick(f.act, f.act_t, info)
		local b = Look.blend_of(f, info)
		local now = A.at(clips[p.clip], p.time)
		if not b then return now, nil end
		return Look.blend(A.at(clips[b.from.clip], b.from.time), now, b.w), b
	end
	-- a part one pose doesn't track is at rest in it, as the engine draws an untracked part
	local REST = { x = 0, y = 0, rot = 0, scale = 1, scale_x = 1, scale_y = 1, show = true }
	local function same(a, b, why)
		local ids = {}
		for id in pairs(a) do ids[id] = true end
		for id in pairs(b) do ids[id] = true end
		for id in pairs(ids) do
			local p, q = a[id] or REST, b[id] or REST
			for _, k in ipairs({ "x", "y", "rot", "scale", "scale_x", "scale_y" }) do
				local d = math.abs(p[k] - q[k])
				if k == "rot" then d = math.abs((p[k] - q[k] + 180) % 360 - 180) end
				assert(d < 1e-9, why .. ": " .. id .. "." .. k .. " " .. p[k] .. " vs " .. q[k])
			end
			assert(p.show == q.show, why .. ": " .. id .. ".show")
		end
	end
	assert(Look.BLEND_TICKS == 4, "blend_ticks")
	-- fall (12 ticks in) switching to land
	local f = { act = "land", act_t = 0, prev_act = "fall", prev_t = 13, face = 1 }
	local at0, b0 = pose_of(f)
	assert(b0 and b0.w == 0, "P-A2: a blend at the switch tick")
	local fall_now = Look.pick("fall", 13, info)
	same(at0, A.at(clips.fall, fall_now.time), "P-A2: at the switch tick the old clip's pose")
	for t = 1, Look.BLEND_TICKS - 1 do
		f.act_t = t
		local _, b = pose_of(f)
		assert(b and b.w == t / Look.BLEND_TICKS, "P-A2: w is act_t / blend_ticks")
		assert(b.from.clip == "fall" and b.from.index == Look.pick("fall", 13 + t, info).index, "P-A2: the old clip runs on")
	end
	f.act_t = Look.BLEND_TICKS
	local at4, b4 = pose_of(f)
	assert(b4 == nil, "P-A2: no blend from blend_ticks on")
	same(at4, A.at(clips.land, Look.pick("land", Look.BLEND_TICKS, info).time), "P-A2: at blend_ticks the new clip's pose")
	-- the same state (as after a rewind) gives the same blend
	f.act_t = 2
	local x1 = pose_of(f)
	local x2 = pose_of({ act = "land", act_t = 2, prev_act = "fall", prev_t = 13, face = 1 })
	same(x1, x2, "P-A2: a function of the state")
	-- the short way round, and halfway
	local m = Look.blend({ root = { x = 0, y = 0, rot = 170, scale = 1, scale_x = 1, scale_y = 1, show = true } },
		{ root = { x = 10, y = 0, rot = -170, scale = 1, scale_x = 1, scale_y = 1, show = false } }, 0.5)
	assert(math.abs(m.root.rot - 180) < 1e-9 and m.root.x == 5 and m.root.show == false, "P-A2: short way: " .. m.root.rot)
	local q = Look.blend({ a = { x = 0, y = 0, rot = 350, scale = 1, scale_x = 1, scale_y = 1, show = true } },
		{ a = { x = 0, y = 0, rot = 10, scale = 1, scale_x = 1, scale_y = 1, show = true } }, 0.25)
	assert(math.abs(q.a.rot - 355) < 1e-9, "P-A2: 350 to 10 goes up through 360: " .. q.a.rot)
	-- no blend into or out of hidden acts, on the act's own first tick after a respawn, or same act
	assert(Look.blend_of({ act = "fall", act_t = 0, prev_act = "out", prev_t = 120 }, info) == nil, "P-A2: out of hidden")
	assert(Look.blend_of({ act = "fall", act_t = 0, prev_act = "fall", prev_t = 0 }, info) == nil, "P-A2: place()")
	-- every switch the script makes is blendable: both acts have clips
	local s, rules = U.fresh()
	local n = 0
	for t = 1, Script.length do
		U.F.step(s, { Script.at(t) }, rules)
		local g = s.fighters[1]
		local _, b = pose_of(g)
		if not Look.ACTS[g.act].hidden and b then n = n + 1 end
	end
	assert(n > 50, "P-A2: the script blends " .. n .. " frames")
	print("P-A2: the scripted run is in a blend on " .. n .. " of " .. Script.length .. " ticks")
end

-- P-A7: the flame is a view override from state. Hidden at jet_pow 0; otherwise its scale_y is the
-- clip's (the flicker) times FLAME[jet_pow]. The frame's cache key carries jet_pow, so one state gives
-- one frame; the number of distinct frames the fight script draws is printed.
do
	local A = require("characters/robot/author")
	assert(#Look.FLAME == 3 and Look.FLAME[1] < Look.FLAME[2] and Look.FLAME[2] < Look.FLAME[3], "P-A7: three flame sizes, growing")
	local hover = require(Look.path("jet_hover"))
	for pow = 0, 3 do
		for _, t in ipairs({ 0, 1 / 60, 3 / 60 }) do
			local clip_pose = A.at(hover, t)
			local pose = Look.flame(A.at(hover, t), pow)
			if pow == 0 then
				assert(pose.flame.show == false, "P-A7: hidden at jet_pow 0")
			else
				assert(pose.flame.show == true and pose.flame.scale_y == clip_pose.flame.scale_y * Look.FLAME[pow],
					"P-A7: scale_y is the flicker x FLAME[" .. pow .. "]")
			end
		end
	end
	-- a clip that doesn't track the flame (Paka's idle): the override adds it
	local idle = Look.flame({}, 0)
	assert(idle.flame and idle.flame.show == false, "P-A7: an untracked flame is hidden at 0")
	assert(Look.flame({}, 2).flame.scale_y == Look.FLAME[2], "P-A7: and sized from rest otherwise")
	local k0, k1 = Look.pick("jet_hover", 5, info, 1).key, Look.pick("jet_hover", 5, info, 2).key
	assert(k0 ~= k1, "P-A7: the key carries jet_pow")
	assert(Look.pick("jet_hover", 5, info, 1).key == k0, "P-A7: one state, one key")
	assert(Look.pick("idle", 5, info).key == Look.pick("idle", 5, info, 0).key, "P-A7: no jet_pow is 0")
	-- the fight script's distinct frames (robot, key, face), both fighters, blends aside
	local Fight = require("script_fight")
	local s, rules = U.fresh({ "human", "dummy" })
	local seen, n, lit = {}, 0, 0
	for t = 1, Fight.length do
		U.F.step(s, { Fight.at(t) }, rules)
		for i, g in ipairs(s.fighters) do
			if not Look.ACTS[g.act].hidden and not Look.blend_of(g, info) then
				local key = i .. ":" .. Look.pick(g.act, g.act_t, info, g.jet_pow).key .. ":" .. g.face
				if not seen[key] then
					seen[key], n = true, n + 1
					if g.jet_pow > 0 then lit = lit + 1 end
				end
			end
		end
	end
	print(string.format("P-A7: the fight script draws %d distinct cached frames, %d of them with the flame", n, lit))
	assert(lit > 0, "P-A7: the fight script lights the flame")
end

print("brawl look ok")
