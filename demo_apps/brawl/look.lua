-- The fighter's picture, a function of its state (docs/design/brawl-movement.md decisions 4-7):
-- the act picks a clip, `act_t / 120` is the time in it, quantized to the clips' 1/60 s, and
-- `face` mirrors it. Posed frames are cached per (robot, clip, frame index, face). For
-- `blend_ticks` after a switch the old act's pose is lerped into the new one's, from `prev_act` and
-- `prev_t` in state, so a rewind redraws the same blend; blended frames bypass the cache.
--
-- The table, the time and the blend are pure (scripts/test_brawl_look.lua runs them under stock
-- Lua); `gfx` is touched only inside `Look.new` and its methods.
local Look = {}

Look.CLIP_DIR = "characters/paka/clips/"
Look.CLIPS = { "idle", "run" } -- Paka's, copied from character_lab (P-X1)
Look.OWN_DIR = "characters/robot/clips/"
Look.OWN = { "rise", "apex", "fall", "land", "jump2", "dash", "wall", "turn" } -- authored on the robot
Look.ROBOTS = { "scout", "brute", "volt" }
Look.BLEND_TICKS = 4

-- Every act fighter.lua sets. `hidden` acts are not drawn. `plant`: the lowest boot point of each
-- posed frame is put on the feet line (see `Look.sole`).
Look.ACTS = {
	idle = { clip = "idle", plant = true },
	run = { clip = "run", plant = true },
	turn = { clip = "turn", plant = true },
	dash = { clip = "dash", plant = true },
	land = { clip = "land", plant = true },
	rise = { clip = "rise" },
	jump2 = { clip = "jump2" },
	apex = { clip = "apex" },
	fall = { clip = "fall" },
	wall = { clip = "wall" },
	out = { clip = "idle", hidden = true },
	ko = { clip = "idle", hidden = true },
}

-- Where each clip's file is.
function Look.path(clip)
	for _, name in ipairs(Look.OWN) do
		if name == clip then return Look.OWN_DIR .. name end
	end
	return Look.CLIP_DIR .. clip
end

function Look.all_clips()
	local out = {}
	for _, n in ipairs(Look.CLIPS) do out[#out + 1] = n end
	for _, n in ipairs(Look.OWN) do out[#out + 1] = n end
	return out
end

local TICKS = 120 -- match ticks a second
local FPS = 60 -- the clips' frames a second

-- How many frame indices a clip has: a loop's whole frames; a once clip's frames up to its end,
-- plus one for the held end itself (a clip's length need not be whole frames: Paka's `jump` is 0.6667 s).
function Look.frames(c)
	if c.loop then return math.max(1, math.floor(c.length * FPS + 0.5)) end
	return math.floor(c.length * FPS + 1e-9) + 2
end

-- `c` is { length, loop }. Returns the exact time `t` (act_t / 120, wrapped or held), the frame
-- `index` and that frame's `time`, which is what gets posed.
function Look.at(c, act_t)
	local n = Look.frames(c)
	local index = math.floor(act_t / (TICKS / FPS))
	local t, time
	if c.loop then
		local ticks = c.length * TICKS
		if ticks == math.floor(ticks) then
			t = (act_t % ticks) / TICKS
		else
			t = math.fmod(act_t / TICKS, c.length)
		end
		index = index % n
		time = index / FPS
	else
		t = math.min(act_t / TICKS, c.length)
		index = math.min(index, n - 1)
		time = math.min(index / FPS, c.length)
	end
	return { t = t, index = index, time = time }
end

-- act + act_t → { clip, t, index, time, key, hidden, plant }; `info[clip]` is { length, loop }.
function Look.pick(act, act_t, info)
	local a = Look.ACTS[act]
	local p = Look.at(info[a.clip], act_t)
	p.clip, p.hidden, p.plant = a.clip, a.hidden == true, a.plant == true
	p.key = a.clip .. "@" .. p.index
	return p
end

-- The blend a fighter is in, from its state alone: nil, or { from = the old act's pick at the time
-- it would be at now (`prev_t + act_t`), w = act_t / blend_ticks }. At the switch tick (act_t 0) w is
-- 0, the old pose; from blend_ticks on there is none, the new pose. None from or to a hidden act or
-- between two acts on one clip.
function Look.blend_of(f, info)
	local prev = f.prev_act
	if prev == nil or prev == f.act or f.act_t >= Look.BLEND_TICKS then return nil end
	local a, b = Look.ACTS[prev], Look.ACTS[f.act]
	if a.hidden or b.hidden or a.clip == b.clip then return nil end
	return { from = Look.pick(prev, f.prev_t + f.act_t, info), w = f.act_t / Look.BLEND_TICKS }
end

local function short(a, b)
	local d = (b - a) % 360
	if d > 180 then d = d - 360 end
	return d
end

-- Two poses (clip:at's tables) lerped by w: 0 is `a`, 1 is `b`; `rot` the short way round. A part
-- only one of them tracks is at rest in the other. `show` switches halfway.
local REST = { x = 0, y = 0, rot = 0, scale = 1, scale_x = 1, scale_y = 1, show = true }
function Look.blend(a, b, w)
	local out = {}
	local function one(id)
		if out[id] then return end
		local p, q = a[id] or REST, b[id] or REST
		out[id] = {
			x = p.x + (q.x - p.x) * w, y = p.y + (q.y - p.y) * w,
			rot = p.rot + short(p.rot, q.rot) * w,
			scale = p.scale + (q.scale - p.scale) * w,
			scale_x = p.scale_x + (q.scale_x - p.scale_x) * w,
			scale_y = p.scale_y + (q.scale_y - p.scale_y) * w,
			show = q.show,
		}
		if w < 0.5 then out[id].show = p.show end
	end
	for id in pairs(a) do one(id) end
	for id in pairs(b) do one(id) end
	return out
end

-- The soles: the boots' parts, whose lowest drawn point is where the robot touches the floor.
Look.SOLES = { "slot:hoof_front", "slot:hoof-back" }

-- Where the boots' lowest point is under `pose` (clip:at's table, or nil for the rest pose), in
-- drawing units, +y down: forward kinematics composed as runtime/src/drawing.rs composes a pose,
-- over every point of the boots' paths. Paka's clips were made for Paka's hooves, so a frame's
-- soles sit a few units off the root's pivot, differently each frame; this is how far.
function Look.sole(robot, pose)
	local byid, world = {}, {}
	for _, part in ipairs(robot.parts) do byid[part.id] = part end
	local function at(id)
		if world[id] then return world[id] end
		local part, o = byid[id], pose and pose[id] or nil
		local px, py = part.pivot[1], part.pivot[2]
		local ox, oy = o and o.x or 0, o and o.y or 0
		local r = math.rad(o and o.rot or 0)
		local sc = o and o.scale or 1
		local sx, sy = sc * (o and o.scale_x or 1), sc * (o and o.scale_y or 1)
		local c, n = math.cos(r), math.sin(r)
		-- T(o + p) R S T(-p)
		local a, b, cc, d = c * sx, n * sx, -n * sy, c * sy
		local m = { a, b, cc, d, ox + px - (a * px + cc * py), oy + py - (b * px + d * py) }
		if part.parent then
			local q = at(part.parent)
			m = { q[1] * m[1] + q[3] * m[2], q[2] * m[1] + q[4] * m[2], q[1] * m[3] + q[3] * m[4],
				q[2] * m[3] + q[4] * m[4], q[1] * m[5] + q[3] * m[6] + q[5], q[2] * m[5] + q[4] * m[6] + q[6] }
		end
		world[id] = m
		return m
	end
	local low = -math.huge
	for _, id in ipairs(Look.SOLES) do
		local m = at(id)
		for _, shape in ipairs(byid[id].shapes or {}) do
			for _, cmd in ipairs(shape.path or {}) do
				for i = 2, #cmd, 2 do
					local y = m[2] * cmd[i] + m[4] * cmd[i + 1] + m[6]
					if y > low then low = y end
				end
			end
		end
	end
	return low
end

-- View side ---------------------------------------------------------------------------------------

-- Mirroring is a pose on the root, not an instance transform, so the parts stay reachable to the
-- pointer (an instanced visual's names are not). The root has no parent and its pivot is on the
-- drawing's centre line, so x -> -x, rot -> -rot and scale_x -> -scale_x about it is the mirror
-- of the whole drawing in its own box.
local function mirror(pose)
	local r = pose.root or { x = 0, y = 0, rot = 0, scale = 1, scale_x = 1, scale_y = 1, show = true }
	r.x, r.rot, r.scale_x = -r.x, -r.rot, -r.scale_x
	pose.root = r
	return pose
end

local L = {}
L.__index = L

-- The clips are built at once (ten files). Robots are built one a view (`L:warm`), or when first
-- drawn: a debug build's module load has a 1 s CPU budget, and each robot's build.lua plus
-- gfx.drawing is a large share of it (character_lab's three-robot load has run over it on a busy
-- machine). Posing waits for the view.
function Look.new()
	local self = setmetatable({ clips = {}, info = {}, drawings = {}, cache = {}, soles = {},
		misses = 0, pose_s = 0, max_s = 0, blends = 0, blend_s = 0 }, L)
	for _, name in ipairs(Look.all_clips()) do
		local h = gfx.clip(require(Look.path(name)))
		self.clips[name], self.info[name] = h, { length = h.length, loop = h.loop }
	end
	return self
end

function L:drawing(robot)
	local d = self.drawings[robot]
	if not d then
		d = gfx.drawing(require("characters/robot/" .. robot))
		self.drawings[robot] = d
	end
	return d
end

-- Builds the next robot not built yet, at most one a call; true once all are.
function L:warm()
	for _, robot in ipairs(Look.ROBOTS) do
		if not self.drawings[robot] then
			self:drawing(robot)
			return false
		end
	end
	return true
end

function L:loaded()
	local n = 0
	for _, robot in ipairs(Look.ROBOTS) do
		if self.drawings[robot] then n = n + 1 end
	end
	return string.format("clips=%d robots=%d/%d", #Look.all_clips(), n, #Look.ROBOTS)
end

-- The robot's posed frame for fighter `f`, what was picked, and `p.sole`: the boots' lowest point
-- in that frame (drawing units). nil for an act not drawn. In a blend the frame is posed afresh each
-- call (`p.blend` is its weight); otherwise it is cached. A pose is timed with `now()` (wall clock; a
-- diagnostic only, never state).
function L:frame(robot, f)
	local p = Look.pick(f.act, f.act_t, self.info)
	if p.hidden then return nil, p end
	local b = Look.blend_of(f, self.info)
	if b then
		local d = self:drawing(robot)
		local t0 = now()
		local pose = Look.blend(self.clips[b.from.clip]:at(b.from.time), self.clips[p.clip]:at(p.time), b.w)
		p.sole = Look.sole(require("characters/robot/" .. robot), pose)
		if f.face < 0 then mirror(pose) end
		local fr = d:pose(pose)
		local dt = now() - t0
		self.blends, self.blend_s = self.blends + 1, self.blend_s + dt
		p.blend = b.w
		return fr, p
	end
	local key = robot .. ":" .. p.key .. (f.face < 0 and ":l" or ":r")
	local fr = self.cache[key]
	local soles = self.soles
	if not fr then
		local d = self:drawing(robot)
		local t0 = now()
		local pose = self.clips[p.clip]:at(p.time)
		soles[robot .. ":" .. p.key] = soles[robot .. ":" .. p.key] or Look.sole(require("characters/robot/" .. robot), pose)
		if f.face < 0 then mirror(pose) end
		fr = d:pose(pose)
		local dt = now() - t0
		self.misses, self.pose_s = self.misses + 1, self.pose_s + dt
		if dt > self.max_s then self.max_s = dt end
		self.cache[key] = fr
	end
	p.sole = soles[robot .. ":" .. p.key]
	return fr, p
end

-- The drawing's size and the root's pivot (the feet), for placing the frame.
function L:anchor(robot)
	local m = require("characters/robot/" .. robot)
	for _, part in ipairs(m.parts) do
		if part.id == "root" then return m.size[1], m.size[2], part.pivot[1], part.pivot[2] end
	end
	error("robot " .. robot .. " has no root")
end

function L:cost()
	local mean = self.misses > 0 and self.pose_s / self.misses or 0
	local blend = self.blends > 0 and self.blend_s / self.blends or 0
	return string.format("poses=%d mean_ms=%.3f max_ms=%.3f blends=%d blend_ms=%.3f", self.misses, mean * 1000,
		self.max_s * 1000, self.blends, blend * 1000)
end

return Look
