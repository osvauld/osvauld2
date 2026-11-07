-- The E-2000 rover from models/e2000.glb (an OBJ converted by scripts/obj_to_glb.py). The model
-- says where its parts are; this file says what they mean: BASE sizes the chassis body, each
-- WHEEL.* becomes a physics wheel whose visual spins and steers natively, *ARM* parts swing to
-- follow their wheel, everything else rides the chassis. The model faces -X and is 8 m long;
-- it is turned to face +Z and scaled down.
local Glb = require("glb")

local SCALE = 0.45
local YAW = {0,math.sin(math.pi/4),0,math.cos(math.pi/4)} -- model -X becomes rover +Z
local TUNE = {mass=600,rest=0.4,travel=0.35,stiffness=1400,damping=380,grip=1.1,
	torque=110,brake_torque=170,speed_limit=6,steer=26}
-- Its materials are near-black metal shaders; these read as the rover was meant to look.
local PALETTE = {body="#d9822b",METAL="#8a8d93",CHROME="#c9ccd1",plastic="#2e3036",
	TIRE="#202022",glass="#26405a",logo="#e8e8e8"}

local function place(p)
	return Glb.qrot(YAW,{p[1]*SCALE,p[2]*SCALE,p[3]*SCALE})
end

local function bounds(model,node)
	local lo,hi = {math.huge,math.huge,math.huge},{-math.huge,-math.huge,-math.huge}
	for _,prim in ipairs(model.meshes[node.mesh]) do
		for i=1,3 do lo[i]=math.min(lo[i],prim.min[i]); hi[i]=math.max(hi[i],prim.max[i]) end
	end
	return lo,hi
end

return function(path)
	local model = gfx.load_glb(path)
	local base
	for _,node in ipairs(model.nodes) do if node.name=="BASE" then base=node end end
	assert(base,"e2000: no BASE node")
	local origin = place(base.translation)
	local function local_point(m) -- a model-space point in the chassis frame
		local p = place(m)
		return {p[1]-origin[1],p[2]-origin[2],p[3]-origin[3]}
	end
	local function local_at(node) return local_point(node.translation) end

	-- Chassis: BASE's bounds, turned with the model (model x/z swap under the yaw).
	local lo,hi = bounds(model,base)
	local size = {(hi[3]-lo[3])*SCALE,(hi[2]-lo[2])*SCALE,(hi[1]-lo[1])*SCALE}

	local R = {objects={},wheels={},front={},arms={}}
	local function visual(node,extra)
		for k,prim in ipairs(model.meshes[node.mesh]) do
			local material = prim.material and model.materials[prim.material]
			local o = {id=node.name..(k>1 and "/"..k or ""),mesh=prim.mesh,scale={SCALE,SCALE,SCALE},
				rotation=YAW,color=material and (PALETTE[material.name] or material.color) or "#c8c8c8",
				texture=material and material.texture}
			for key,v in pairs(extra) do o[key]=v end
			R.objects[#R.objects+1] = o
		end
	end
	local sag = TUNE.mass*1.62/4/TUNE.stiffness
	local n,low,radius = 0,0,0
	local wheel_of = {}
	for _,node in ipairs(model.nodes) do
		if node.mesh and node.name:match("^WHEEL") then
			n = n+1
			local id = "rover-wheel-"..n
			wheel_of[node.name] = id
			local wlo,whi = bounds(model,node)
			radius = (whi[2]-wlo[2])/2*SCALE
			local at = local_at(node)
			low = at[2]-radius
			R.wheels[#R.wheels+1] = {id=id,body="rover",at={at[1],at[2]+TUNE.rest-sag,at[3]},
				radius=radius,width=(whi[3]-wlo[3])*SCALE,rest=TUNE.rest,travel=TUNE.travel,
				stiffness=TUNE.stiffness,damping=TUNE.damping,grip=TUNE.grip}
			if at[3]>0 then R.front[id]=true end
			-- Tyre, rim and hub all ride the wheel: its native pose spins, steers and springs them.
			visual(node,{attach=id,position={0,0,0}})
		end
	end
	-- A part whose name says ARM hinges at its inboard end and keeps pointing at the nearest
	-- wheel's hub, so it follows the suspension. The hinge runs across the rover.
	local across = Glb.qrot(YAW,{0,0,1})
	for _,node in ipairs(model.nodes) do
		if node.mesh and not node.name:match("^WHEEL") then
			local extra = {attach="rover",position=local_at(node)}
			if node.name:match("ARM") then
				local c,best,hub = node.translation,math.huge,nil
				for _,w in ipairs(model.nodes) do
					if w.name:match("^WHEEL") then
						local t = w.translation
						local d = (t[1]-c[1])^2+(t[3]-c[3])^2
						if d<best then best,hub = d,w end
					end
				end
				local h = hub.translation
				local alo,ahi = bounds(model,node)
				local inner = c[1] + (h[1]<c[1] and ahi[1] or alo[1]) -- the end away from the hub
				local k = (inner-h[1])/(c[1]-h[1])
				local first = #R.objects+1
				visual(node,extra)
				local ids = {}
				for i=first,#R.objects do ids[#ids+1] = R.objects[i].id end
				R.arms[#R.arms+1] = {ids=ids,wheel=wheel_of[hub.name],at=extra.position,
					toward=local_at(hub),pivot=local_point({h[1]+(c[1]-h[1])*k,h[2]+(c[2]-h[2])*k,c[3]})}
			else
				visual(node,extra)
			end
		end
	end
	R.recipe = {id="rover",box=size,mass=TUNE.mass,center={0,-size[2]*0.3,0},dynamic=true}
	R.clearance = -low -- chassis centre height above flat ground at the modelled ride height
	R.radius = radius

	-- This frame's arm poses, for world:scene{poses=...}: each arm turns about its hinge (the
	-- `across` axis through `pivot`) by the angle that carries its modelled hub onto the live one.
	local function sub(a,b) return {a[1]-b[1],a[2]-b[2],a[3]-b[3]} end
	local function flat(v) -- v without its component along the hinge
		local d = v[1]*across[1]+v[2]*across[2]+v[3]*across[3]
		return {v[1]-across[1]*d,v[2]-across[2]*d,v[3]-across[3]*d}
	end
	local function angle(v) -- signed angle about the hinge (up x across turns toward up)
		local up = {0,1,0}
		local side = {up[2]*across[3]-up[3]*across[2],up[3]*across[1]-up[1]*across[3],up[1]*across[2]-up[2]*across[1]}
		return math.atan2(v[1]*up[1]+v[2]*up[2]+v[3]*up[3],v[1]*side[1]+v[2]*side[2]+v[3]*side[3])
	end
	function R.poses(world)
		local body = world:get("rover")
		if not body then return nil end
		local inverse = {-body.rotation[1],-body.rotation[2],-body.rotation[3],body.rotation[4]}
		local out = {}
		for _,arm in ipairs(R.arms) do
			local hub = world:get(arm.wheel)
			if hub then
				local live = Glb.qrot(inverse,sub(hub.position,body.position))
				local turn = angle(flat(sub(live,arm.pivot)))-angle(flat(sub(arm.toward,arm.pivot)))
				local swing = {across[1]*math.sin(turn/2),across[2]*math.sin(turn/2),across[3]*math.sin(turn/2),math.cos(turn/2)}
				local r = Glb.qrot(swing,sub(arm.at,arm.pivot))
				local pose = {position={arm.pivot[1]+r[1],arm.pivot[2]+r[2],arm.pivot[3]+r[3]},
					rotation=Glb.qmul(swing,YAW)}
				for _,id in ipairs(arm.ids) do out[id] = pose end
			end
		end
		return out
	end

	function R.apply(world,input)
		local throttle,brake,steering = input.throttle or 0,input.brake or 0,input.steering or 0
		local motor = nil
		if brake==0 and throttle~=0 then
			motor={velocity=(throttle>0 and 1 or -1)*TUNE.speed_limit/radius,max_torque=math.abs(throttle)*TUNE.torque}
		end
		local commands = {}
		for _,w in ipairs(R.wheels) do
			commands[#commands+1]={id=w.id,motor=motor,brake=brake*TUNE.brake_torque,
				steering=R.front[w.id] and -steering*math.rad(TUNE.steer) or 0}
		end
		world:set_wheels(commands)
	end
	return R
end
