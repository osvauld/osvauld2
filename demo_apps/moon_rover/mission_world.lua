local Theme = require("theme")
local C = Theme.mission
local V = require("vehicle")
local Art = require("rover_art")
local terrain = require("lunar_terrain")(C.seed)
local objects = {{id="lunar-ground",mesh=terrain.mesh,color=C.ground}}
local bodies = {{id="lunar-ground",position={0,0,0},mesh=terrain.mesh}}

local function box(id,at,size,color,solid)
	objects[#objects+1] = {id=id,position=at,scale=size,color=color}
	if solid then bodies[#bodies+1] = {id=id,position=at,box=size} end
end

local hx,hz = C.home[1],C.home[2]
box("landing-pad",{hx,0.03,hz},C.pad_size,C.pad,true)
box("lander-core",{hx-6,1.7,hz},{2.8,2.2,2.8},C.lander,true)
box("lander-foil",{hx-6,0.8,hz},{3.2,0.6,3.2},C.foil,true)
box("lander-window",{hx-6,2,hz+1.42},{1.6,0.6,0.06},C.window,false)
box("lander-top",{hx-6,3,hz},{2.3,0.5,2.3},C.lander,true)
for i,offset in ipairs({{-1.7,-1.7},{1.7,-1.7},{-1.7,1.7},{1.7,1.7}}) do
	box("lander-leg-"..i,{hx-6+offset[1],0.5,hz+offset[2]},{0.18,1,0.18},C.foil,true)
	box("lander-foot-"..i,{hx-6+offset[1],0.08,hz+offset[2]},{0.7,0.16,0.7},C.pad,true)
end

local sx,sz = C.site[1],C.site[2]
local sy = terrain.sample(sx,sz)
box("sample-plinth",{sx-5,sy+0.25,sz+4},{1,0.5,1},C.pad,true)
box("sample-beacon",{sx-5,sy+2,sz+4},{0.12,3.5,0.12},C.sample,true)
box("sample-flag",{sx-4.2,sy+3.5,sz+4},{1.6,0.7,0.08},C.sample,false)

local home = {hx,0.72+C.pad_size[2],hz}
local site = {sx,sy+0.72,sz}
bodies[#bodies+1] = {id="home-zone",position=home,sphere=C.collect_range,sensor=true}
bodies[#bodies+1] = {id="sample-zone",position=site,sphere=C.collect_range,sensor=true}
bodies[#bodies+1] = {id="fall-zone",position={0,-1006,0},box={2000,2000,2000},sensor=true}

local spawn = {hx+2,C.pad_size[2]+V.radius+V.rest-V.mounts[1][2],hz}
local yaw = math.atan2(sx-spawn[1],sz-spawn[3])
local rotation = {0,math.sin(yaw/2),0,math.cos(yaw/2)}
local recipe = V.recipe()
recipe.position,recipe.rotation = spawn,rotation
bodies[#bodies+1] = recipe
objects[#objects+1] = {id="rover",mesh=Art.body,color=Art.body_color}
for i=1,4 do objects[#objects+1]={id="rover-wheel-"..i,mesh=Art.wheel,color=Art.wheel_color} end

local function segment_distance(x,z)
	local dx,dz = sx-spawn[1],sz-spawn[3]
	local t = math.clamp(((x-spawn[1])*dx+(z-spawn[3])*dz)/(dx*dx+dz*dz),0,1)
	return math.sqrt((x-spawn[1]-t*dx)^2+(z-spawn[3]-t*dz)^2)
end

local random = C.seed
local function next_random()
	random = (random*48271)%2147483647
	return random/2147483647
end
local function rock_mesh(w,h,d)
	local points = {{-w,0,0},{w,0,0},{0,0,-d},{0,0,d},{0,h,0},{0,-h*0.45,0}}
	local vertices,indices = {},{}
	local ring = {1,3,2,4}
	for _,tip in ipairs({5,6}) do
		for i=1,4 do
			local a,b,c = points[tip],points[ring[i]],points[ring[i%4+1]]
			local ux,uy,uz = b[1]-a[1],b[2]-a[2],b[3]-a[3]
			local vx,vy,vz = c[1]-a[1],c[2]-a[2],c[3]-a[3]
			local nx,ny,nz = uy*vz-uz*vy,uz*vx-ux*vz,ux*vy-uy*vx
			if nx*(a[1]+b[1]+c[1])+ny*(a[2]+b[2]+c[2])+nz*(a[3]+b[3]+c[3])<0 then b,c=c,b; nx,ny,nz=-nx,-ny,-nz end
			local length = math.sqrt(nx*nx+ny*ny+nz*nz)
			for _,p in ipairs({a,b,c}) do
				vertices[#vertices+1]={position=p,normal={nx/length,ny/length,nz/length}}
				indices[#indices+1]=#vertices
			end
		end
	end
	return gfx.mesh({vertices=vertices,indices=indices})
end
local count = 0
for _=1,256 do
	if count>=C.rock_count then break end
	local x,z = (next_random()-0.5)*84,(next_random()-0.5)*84
	if segment_distance(x,z)>5 and (x-hx)^2+(z-hz)^2>100 and (x-sx)^2+(z-sz)^2>100 then
		count = count+1
		local w,h,d = 0.5+next_random(),0.5+next_random()*0.8,0.5+next_random()
		local mesh = rock_mesh(w,h,d)
		local at = {x,terrain.sample(x,z)+h*0.45,z}
		local id = "rock-"..count
		objects[#objects+1]={id=id,mesh=mesh,position=at,color=C.rocks[count%#C.rocks+1]}
		bodies[#bodies+1]={id=id,mesh=mesh,position=at}
	end
end
for i=1,12 do
	local t=i/13
	local x,z = spawn[1]+(sx-spawn[1])*t,spawn[3]+(sz-spawn[3])*t
	objects[#objects+1]={id="route-"..i,position={x,terrain.sample(x,z)+0.035,z},
		scale={0.18,0.04,0.8},rotation=rotation,color=C.route}
end
local earth = require("sphere")(1)
objects[#objects+1]={id="earth",mesh=earth,position={56,32,90},scale={7,7,7},color=C.earth}

local scene = gfx.scene3d({camera={eye={0,60,80},target={0,-0.5,0},far=400},objects=objects})
return {
	world=gfx.world3d({id="crater-mission",scene=scene,bodies=bodies,gravity={0,-1.62,0},wheels=V.wheels()}),
	terrain=terrain,spawn=spawn,rotation=rotation,home=home,site=site,
}
