package.path = "demo_apps/moon_rover/?.lua;" .. package.path
math.clamp = math.clamp or function(x,lo,hi) return math.max(lo,math.min(hi,x)) end
gfx = {mesh=function(spec) return spec end}
local build=require("lunar_terrain")
local a,b,c=build(20261105),build(20261105),build(17)
assert(#a.mesh.indices/3==8192)
assert(#a.mesh.vertices==4225)
local different=false
for i,v in ipairs(a.mesh.vertices) do
	local p=v.position
	for j=1,3 do assert(p[j]==b.mesh.vertices[i].position[j],"seed is not repeatable") end
	assert(math.abs(a.sample(p[1],p[3])-p[2])<1e-7,"sampler differs from rendered mesh")
	if math.abs(p[2]-c.mesh.vertices[i].position[2])>0.01 then different=true end
end
assert(different,"seed does not change terrain")
assert(a.sample(49,0)==nil and a.sample(0,-49)==nil)
local peak=0
for _,p in ipairs(a.mesh.vertices) do
	local x,z=p.position[1],p.position[3]
	for _,offset in ipairs({{1.5,0},{0,1.5}}) do
		local y=a.sample(x+offset[1],z+offset[2])
		if y then peak=math.max(peak,math.abs(y-p.position[2])/1.5) end
	end
end
assert(peak<0.45,"terrain too steep: "..peak)
assert(math.abs(a.sample(-21,-24))<0.001,"lander pad not flat")
assert(math.abs(a.sample(12,12)+2)<0.001,"sample site not flat")
print("lunar terrain: repeatable seed, variation, 8192 shared triangles, exact sampling, bounded slopes")
