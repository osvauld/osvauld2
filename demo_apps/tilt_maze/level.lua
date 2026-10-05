local C = require("theme")

local vertices = {}
for _, p in ipairs({ {0,1,0}, {0,0,1}, {1,0,0}, {0,0,-1}, {-1,0,0}, {0,-1,0} }) do
	table.insert(vertices, { position = p, normal = p })
end
local marble = gfx.mesh({
	vertices = vertices,
	indices = { 1,2,3, 1,3,4, 1,4,5, 1,5,2, 6,3,2, 6,4,3, 6,5,4, 6,2,5 },
})

local parts = {
	{ id = "floor", position = {0,-0.5,0}, box = {8,1,8}, color = C.floor },
	{ id = "rail-west", position = {-4,0.35,0}, box = {0.25,1,8}, color = C.wall },
	{ id = "rail-east", position = {4,0.35,0}, box = {0.25,1,8}, color = C.wall },
	{ id = "rail-north", position = {0,0.35,-4}, box = {8,1,0.25}, color = C.wall },
	{ id = "wall-one", position = {-1.4,0.4,-1}, box = {5.2,1,0.3}, color = C.wall },
	{ id = "wall-two", position = {1.4,0.4,1.2}, box = {5.2,1,0.3}, color = C.wall },
	{ id = "checkpoint", position = {2.6,0.05,0}, box = {1.2,1.8,1.2}, sensor = true,
		scale = {1.2,0.06,1.2}, color = C.checkpoint },
	{ id = "goal", position = {-2.6,0.05,2.8}, box = {1.4,1.8,1.2}, sensor = true,
		scale = {1.4,0.06,1.2}, color = C.goal },
	{ id = "hazard", position = {2.8,0.05,-2.7}, box = {1.4,1.8,1.0}, sensor = true,
		scale = {1.4,0.06,1.0}, color = C.hazard },
}
local objects, bodies = {}, {}
for _, p in ipairs(parts) do
	table.insert(objects, { id = p.id, position = p.position, scale = p.scale or p.box, color = p.color })
	table.insert(bodies, { id = p.id, position = p.position, box = p.box, sensor = p.sensor })
end
table.insert(objects, { id = "ball", mesh = marble, position = C.start,
	scale = {C.radius,C.radius,C.radius}, color = C.ball })
table.insert(bodies, { id = "ball", position = C.start, sphere = C.radius, dynamic = true })
-- The open south edge can drop the marble; this stationary zone catches it below the tray.
table.insert(bodies, { id = "fall", position = {0,-3,0}, box = {30,1,30}, sensor = true })

local camera = { eye = {0,11,8.5}, target = {0,0,0}, fov_y = 48, near = 0.1, far = 50 }
local game = gfx.world3d({ id = "tilt-maze", scene = gfx.scene3d({camera = camera, objects = objects}), bodies = bodies })
local S = { status = "Ready", attempt = 0, checkpoint = false, paused = false, tilt = "Level", events = {} }

local function rotate(q, p)
	local x,y,z,w = q[1],q[2],q[3],q[4]
	local cx,cy,cz = y*p[3]-z*p[2], z*p[1]-x*p[3], x*p[2]-y*p[1]
	return {
		p[1] + 2*w*cx + 2*(y*cz-z*cy),
		p[2] + 2*w*cy + 2*(z*cx-x*cz),
		p[3] + 2*w*cz + 2*(x*cy-y*cx),
	}
end

local function tilt(name, pitch, roll)
	local a,b = math.rad(pitch)/2, math.rad(roll)/2
	local q = { math.sin(a)*math.cos(b), -math.sin(a)*math.sin(b), math.cos(a)*math.sin(b), math.cos(a)*math.cos(b) }
	for _, p in ipairs(parts) do
		game:set(p.id, {pos = rotate(q, p.position), rotation = q})
	end
	S.tilt = name
end

local function begin(fresh)
	if fresh then S.checkpoint = false end
	S.attempt = S.attempt + 1
	S.status, S.paused, S.events = "Playing", false, {}
	tilt("Level", 0, 0)
	game:reset("ball")
	if S.checkpoint then game:set("ball", {pos = C.checkpoint_spawn}) end
end

local M = { state = S, game = game }
function M.release() begin(true) end
function M.retry() begin(false) end
function M.pause()
	if S.status == "Playing" then S.paused = not S.paused end
end
function M.tilt(name)
	if S.status ~= "Playing" or S.paused then return end
	if name == "North" then tilt(name, -C.tilt, 0)
	elseif name == "South" then tilt(name, C.tilt, 0)
	elseif name == "East" then tilt(name, 0, -C.tilt)
	elseif name == "West" then tilt(name, 0, C.tilt)
	else
		tilt("Level", 0, 0)
		-- A player-operated magnetic brake clears momentum; it never edits marble position.
		game:set("ball", {velocity = {0,0,0}, spin = {0,0,0}})
	end
end
function M.zone(e)
	if e.who ~= "ball" or S.status ~= "Playing" then return end
	table.insert(S.events, e.id .. ": " .. e.phase .. " @" .. tostring(e.tick))
	if e.phase ~= "enter" then return end
	if e.id == "checkpoint" then S.checkpoint = true
	elseif e.id == "goal" and S.checkpoint then S.status = "Won"
	elseif e.id == "hazard" or e.id == "fall" then S.status = "Lost" end
end
return M
