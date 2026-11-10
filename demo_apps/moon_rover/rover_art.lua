local C = require("theme")
local V = require("vehicle")
local function builder()
	return {vertices = {}, indices = {}}
end

local function quad(mesh, points, normals)
	local base = #mesh.vertices
	for i, point in ipairs(points) do mesh.vertices[#mesh.vertices+1] = {position=point,normal=normals[i]} end
	for _, index in ipairs({1,2,3,1,3,4}) do mesh.indices[#mesh.indices+1] = base+index end
end

local faces = {
	{{0,0,1},{{-1,-1,1},{1,-1,1},{1,1,1},{-1,1,1}}},
	{{0,0,-1},{{1,-1,-1},{-1,-1,-1},{-1,1,-1},{1,1,-1}}},
	{{1,0,0},{{1,-1,1},{1,-1,-1},{1,1,-1},{1,1,1}}},
	{{-1,0,0},{{-1,-1,-1},{-1,-1,1},{-1,1,1},{-1,1,-1}}},
	{{0,1,0},{{-1,1,1},{1,1,1},{1,1,-1},{-1,1,-1}}},
	{{0,-1,0},{{-1,-1,-1},{1,-1,-1},{1,-1,1},{-1,-1,1}}},
}

local function box(mesh, x,y,z, w,h,d, angle)
	local c,s = math.cos(angle or 0), math.sin(angle or 0)
	for _, face in ipairs(faces) do
		local points, normals = {}, {}
		local n = face[1]
		for i,p in ipairs(face[2]) do
			local py,pz = y+p[2]*h/2,z+p[3]*d/2
			points[i] = {x+p[1]*w/2,py*c-pz*s,py*s+pz*c}
			normals[i] = {n[1],n[2]*c-n[3]*s,n[2]*s+n[3]*c}
		end
		quad(mesh,points,normals)
	end
end

local body = builder()
box(body,0,0,0,V.size[1],V.size[2],V.size[3])
for _, x in ipairs({-0.72,0.72}) do
	box(body,x,0.27,0,0.08,0.12,2.4)
	box(body,x,0.7,-0.8,0.06,1.2,0.06)
end
for _, x in ipairs({-0.38,0.38}) do
	box(body,x,0.28,-0.25,0.55,0.14,0.6)
	box(body,x,0.55,-0.55,0.55,0.6,0.1)
end
box(body,0,1.28,-0.8,1.5,0.06,0.06)
box(body,0,0.42,0.6,0.65,0.4,0.2)
box(body,0.5,0.9,0.7,0.04,1.4,0.04)
box(body,0,0.18,-1.02,0.9,0.3,0.3)

local wheel = builder()
local radius, width, sides = V.radius-0.03,V.width,32
for i=0,sides-1 do
	local a,b = 2*math.pi*i/sides,2*math.pi*(i+1)/sides
	local ya,za,yb,zb = radius*math.cos(a),radius*math.sin(a),radius*math.cos(b),radius*math.sin(b)
	quad(wheel,{{-width/2,ya,za},{-width/2,yb,zb},{width/2,yb,zb},{width/2,ya,za}},
		{{0,math.cos(a),math.sin(a)},{0,math.cos(b),math.sin(b)},{0,math.cos(b),math.sin(b)},{0,math.cos(a),math.sin(a)}})
	for _, side in ipairs({-1,1}) do
		local base=#wheel.vertices
		local points=side>0 and {{width/2,0,0},{width/2,ya,za},{width/2,yb,zb}} or
			{{-width/2,0,0},{-width/2,yb,zb},{-width/2,ya,za}}
		for _,p in ipairs(points) do wheel.vertices[#wheel.vertices+1]={position=p,normal={side,0,0}} end
		for j=1,3 do wheel.indices[#wheel.indices+1]=base+j end
	end
end
for i=0,15 do box(wheel,0,V.radius-0.015,0,width+0.02,0.03,0.09,2*math.pi*i/16) end

return {body = gfx.mesh(body), wheel = gfx.mesh(wheel), body_color = C.rover, wheel_color = C.tire}
