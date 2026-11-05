package.path = "demo_apps/moon_rover/?.lua;" .. package.path
math.clamp = math.clamp or function(x, lo, hi) return math.max(lo, math.min(hi, x)) end
gfx = {mesh = function(spec) return spec end}

local C = require("theme")
local course = require("course")
local flat = {}
for i = 1, #course.mesh.indices, 3 do
	local a = course.mesh.vertices[course.mesh.indices[i]].position[2]
	local b = course.mesh.vertices[course.mesh.indices[i+1]].position[2]
	local c = course.mesh.vertices[course.mesh.indices[i+2]].position[2]
	if math.abs(a-b) < 1e-10 and math.abs(a-c) < 1e-10 then flat[a] = true end
end
assert(next(flat), "fixture needs flat terrain next to its slopes")
assert(#course.contours.vertices > 0, "fixture needs visible contours")
for _, vertex in ipairs(course.contours.vertices) do
	local level = vertex.position[2] - C.guide_lift
	for height in pairs(flat) do
		assert(math.abs(level-height) > 1e-7,
			"ambiguous contour follows a flat plateau's triangulation at height " .. height)
	end
end
print("rover course: contours avoid ambiguous flat plateaus")
