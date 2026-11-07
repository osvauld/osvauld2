-- Drive on an imported moon surface. The GLB says what is there; this file says what it means:
-- nodes with extras.body == "mesh" collide, the "spawn" node places the rover.
local Glb = require("glb")
local V = require("vehicle")
local Art = require("rover_art")
local Controls = require("rover_controls")
local C = require("theme").mission

local model = gfx.load_glb("models/moon_surface.glb")
local objects = Glb.objects(model)
local bodies = {}
local world_at = Glb.world(model)
local spawn = {0,0,0}
for i,node in ipairs(model.nodes) do
	local extras = node.extras or {}
	if extras.body=="mesh" then
		for _,prim in ipairs(model.meshes[node.mesh]) do
			bodies[#bodies+1] = {id=node.name,mesh=prim.mesh,position=world_at[i].translation}
		end
	elseif extras.role=="spawn" then
		spawn = world_at[i].translation
	end
end

-- Which rover rides: the articulated E-2000 model if present, else Curiosity (static model on
-- the boxy rover's physics), else the boxy rover itself.
local function boxy()
	local r = {objects={},recipe=V.recipe(),wheels=V.wheels(),apply=V.apply,name="boxy",
		clearance=V.radius+V.rest-V.mounts[1][2]}
	r.objects[1] = {id="rover",mesh=Art.body,color=Art.body_color}
	for i=1,4 do r.objects[#r.objects+1]={id="rover-wheel-"..i,mesh=Art.wheel,color=Art.wheel_color} end
	return r
end

local function curiosity_on(r)
	local CURIOSITY = {scale=0.8,offset={0,-0.84,-0.25}}
	local ok,model = pcall(gfx.load_glb,"models/curiosity.glb")
	if not ok then return nil end
	local s = CURIOSITY.scale
	local frame = {translation=CURIOSITY.offset,rotation={0,0,0,1},scale={s,s,s}}
	r.objects,r.name = {},"curiosity"
	for _,part in ipairs(Glb.objects(model)) do
		local t = Glb.compose(frame,{translation=part.position,rotation=part.rotation,scale=part.scale})
		part.id,part.attach = "curiosity:"..part.id,"rover"
		part.position,part.rotation,part.scale = t.translation,t.rotation,t.scale
		r.objects[#r.objects+1] = part
	end
	return r
end

local ok,rover = pcall(require("e2000"),"models/e2000.glb")
if ok then rover.name = "e2000" else
	print("e2000 rover unavailable: "..tostring(rover))
	rover = curiosity_on(boxy()) or boxy()
end

local start = {spawn[1],spawn[2]+0.2+rover.clearance,spawn[3]}
rover.recipe.position = start
bodies[#bodies+1] = rover.recipe
bodies[#bodies+1] = {id="fall-zone",position={0,-1010,0},box={2000,2000,2000},sensor=true}
for _,o in ipairs(rover.objects) do objects[#objects+1] = o end
objects[#objects+1] = {id="earth",mesh=require("sphere")(1),position={120,70,190},scale={12,12,12},color=C.earth}

local scene = gfx.scene3d({camera={eye={0,8,-12},target={0,0,0},far=600},objects=objects})
local S = {
	world=gfx.world3d({id="moon-surface",scene=scene,bodies=bodies,gravity={0,-1.62,0},wheels=rover.wheels}),
	controls=Controls.new(),running=false,status="Ready",start=start,rover=rover.name,
}
local drive_keys = {KeyW=true,KeyA=true,KeyS=true,KeyD=true,Space=true}

function S.pose()
	return S.world:get("rover") or {position=start,rotation={0,0,0,1},velocity={0,0,0}}
end

function S.speed()
	local v = S.pose().velocity
	return math.sqrt(v[1]^2+v[2]^2+v[3]^2)
end

-- The camera keeps above the rover's own height: the ground's heights live in the GLB, not in Lua.
function S.sample()
	return S.pose().position[2]-0.6
end

-- Visual poses the rover's Lua moves each frame (E-2000 arms); nil when it has none.
function S.poses() return rover.poses and rover.poses(S.world) end
function S.send()
	rover.apply(S.world,S.running and S.controls:input() or {brake=1})
end

function S.drive() S.running,S.status = true,"Driving"; S.send() end
function S.pause() S.running,S.status = false,"Paused"; S.controls:clear(); S.send() end
function S.reset()
	S.world:reset("rover")
	S.controls:clear()
	S.running,S.status = false,"Ready"
	S.send()
end

function S.zone(e)
	if e.who=="rover" and e.id=="fall-zone" and e.phase=="enter" then
		S.reset(); S.status = "Recovered at spawn"
	end
end

function S.key(e)
	if e.cancelled then S.pause(); return true end
	if drive_keys[e.code] then
		if not S.running and e.down then S.drive() end
		S.controls:key(e); S.send(); return true
	end
	if e.down and not e.repeated then
		if e.code=="KeyR" then S.reset(); return true end
		if e.code=="KeyP" then if S.running then S.pause() else S.drive() end; return true end
	end
	return false
end

function S.pedal(name,e)
	if not S.running and e.down and e.phase~="leave" then S.drive() end
	S.controls:hover(name,e)
	S.send()
end

return S
