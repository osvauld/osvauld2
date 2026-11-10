local C = require("theme")
local course = require("course")
local V = require("vehicle")
local Art = require("rover_art")
local Controls = require("rover_controls")
local objects = {
	{id = "course", mesh = course.mesh, color = C.ground},
	{id = "course-grid", mesh = course.grid, color = C.grid},
	{id = "course-contours", mesh = course.contours, color = C.contours},
	{id = "ball", mesh = require("sphere")(C.ball_radius), position = C.ball_spawn, color = C.ball},
	{id = "box", position = C.box_spawn, scale = C.box_size, color = C.box},
	{id = "rover", mesh = Art.body, position = V.spawn, color = Art.body_color},
}
for i=1,4 do objects[#objects+1]={id="rover-wheel-"..i,mesh=Art.wheel,color=Art.wheel_color} end
local scene = gfx.scene3d({
	camera = {eye = {18,17,23}, target = {0,-0.5,0}, far = 200},
	objects = objects,
})

local M = {status = "Ready", running = false, driving = false, controls = Controls.new(), spawn = V.spawn}
M.world = gfx.world3d({
	id = "lunar-course", scene = scene, gravity = {0,-1.62,0},
	bodies = {
		{id = "course", position = {0,0,0}, mesh = course.mesh},
		{id = "ball", position = C.ball_spawn, sphere = C.ball_radius, dynamic = true},
		{id = "box", position = C.box_spawn, box = C.box_size, dynamic = true},
		V.recipe(),
	},
	wheels = V.wheels(),
})

function M.send()
	V.apply(M.world, M.driving and M.running and M.controls:input() or {brake=1})
end

function M.drive()
	M.controls:clear()
	M.driving, M.running, M.status = true, true, "Driving"
	M.send()
end

function M.inspect()
	M.controls:clear()
	M.driving = false
	M.status = M.running and "Running" or "Paused"
	M.send()
end

function M.key(e)
	if e.cancelled then M.pause(); return end
	M.controls:key(e)
	M.send()
end

function M.pedal(name,e)
	M.controls:hover(name,e)
	M.send()
end

function M.release()
	M.running, M.status = true, M.driving and "Driving" or "Running"
	M.send()
end

function M.pause()
	M.running, M.status = false, "Paused"
	M.controls:clear()
	M.send()
end

function M.reset()
	M.world:reset("ball")
	M.world:reset("box")
	M.world:reset("rover")
	M.controls:clear()
	M.running, M.status = false, "Ready"
	M.send()
end

return M
