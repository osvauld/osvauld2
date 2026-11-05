local C = require("theme")
local Course = require("course")
local yaw, pitch, distance, grab = -0.58, -0.12, 42, nil
local target, held, last_time = {0,-0.5,0}, {}, nil
local grounded, gx, gz, height, look_distance = true, C.ground_start[1], C.ground_start[2], C.eye_height, 10
local follow = nil
local follow_sample = Course.sample
local orbit_max = C.camera_max
local keys = {KeyW=true, KeyA=true, KeyS=true, KeyD=true, KeyQ=true, KeyE=true,
	ArrowLeft=true, ArrowRight=true, ArrowUp=true, ArrowDown=true}
local Camera = {}

function Camera.overview(view)
	follow = nil
	yaw, pitch, distance, grab = 0.7, 0.55, view and view.distance or 42, nil
	local at = view and view.target or {0,-0.5,0}
	target, held, last_time = {at[1],at[2],at[3]}, {}, nil
	orbit_max = view and math.max(C.camera_max,view.distance*2) or C.camera_max
	grounded = false
end

function Camera.ground()
	follow = nil
	grounded, gx, gz, height, look_distance = true, C.ground_start[1], C.ground_start[2], C.eye_height, 10
	yaw, pitch, grab, held, last_time = -0.58, -0.12, nil, {}, nil
end

function Camera.is_grounded()
	return grounded
end

function Camera.is_following()
	return follow ~= nil
end

function Camera.follow(get_pose,sample)
	follow, grounded = get_pose, false
	follow_sample = sample or Course.sample
	yaw, pitch, distance, grab, held, last_time = 0, 0.22, 4.8, nil, {}, nil
end

local function rotate(q,v)
	local tx,ty,tz = 2*(q[2]*v[3]-q[3]*v[2]),2*(q[3]*v[1]-q[1]*v[3]),2*(q[1]*v[2]-q[2]*v[1])
	return {v[1]+q[4]*tx+q[2]*tz-q[3]*ty,v[2]+q[4]*ty+q[3]*tx-q[1]*tz,v[3]+q[4]*tz+q[1]*ty-q[2]*tx}
end

function Camera.preset(name)
	Camera.overview()
	local view = C.views[name]
	target = {view.target[1], view.target[2], view.target[3]}
	distance = view.distance
end

function Camera.describe()
	if follow then
		local pose = follow()
		local at, q = pose.position, pose.rotation
		local offset = rotate(q,{math.sin(yaw)*distance*math.cos(pitch),math.sin(pitch)*distance,-math.cos(yaw)*distance*math.cos(pitch)})
		local ahead = rotate(q,{0,0.3,0.8})
		local x,z = at[1]+offset[1],at[3]+offset[3]
		local y = math.max(at[2]+offset[2],(follow_sample(x,z) or 0)+0.8)
		return {eye={x,y,z},target={at[1]+ahead[1],at[2]+ahead[2],at[3]+ahead[3]},near=0.03,far=200,fov_y=60}
	end
	if grounded then
		local y = (Course.sample(gx, gz) or 0) + height
		return {
			eye = {gx,y,gz},
			target = {gx - math.sin(yaw)*math.cos(pitch)*look_distance,
				y + math.sin(pitch)*look_distance, gz - math.cos(yaw)*math.cos(pitch)*look_distance},
			fov_y = 60, near = 0.03, far = 200,
		}
	end
	return {
		eye = {target[1] + distance * math.sin(yaw) * math.cos(pitch),
			target[2] + distance * math.sin(pitch), target[3] + distance * math.cos(yaw) * math.cos(pitch)},
		target = target, near = 0.03, far = math.max(200,distance*3),
	}
end

function Camera.focus(e)
	if follow then return end
	if not e.object then return end
	if grounded then
		local y = (Course.sample(gx, gz) or 0) + height
		local dx, dy, dz = e.world_x - gx, e.world_y - y, e.world_z - gz
		look_distance = math.max(0.1, math.sqrt(dx*dx + dy*dy + dz*dz))
		yaw, pitch = math.atan2(-dx,-dz), math.asin(math.clamp(dy/look_distance,-1,1))
	else target = {e.world_x, e.world_y, e.world_z} end
end

function Camera.drag(e)
	if e.phase == "start" then grab = {yaw, pitch} end
	if grab then
		if grounded then
			yaw = grab[1] - e.dx * 0.006
			pitch = math.clamp(grab[2] - e.dy * 0.006, -1.2, 0.8)
		else
			yaw = grab[1] + e.dx * 0.008
			pitch = math.clamp(grab[2] + e.dy * 0.008, 0.15, 1.4)
		end
	end
	if e.phase == "end" then grab = nil end
end

local function walk(forward, right)
	gx = math.clamp(gx + right * math.cos(yaw) - forward * math.sin(yaw), -C.width/2+0.5, C.width/2-0.5)
	gz = math.clamp(gz - right * math.sin(yaw) - forward * math.cos(yaw), -C.depth/2+0.5, C.depth/2-0.5)
end

function Camera.wheel(e)
	if follow then distance = math.clamp(distance*math.exp(e.dy*C.zoom_rate),2,12)
	elseif grounded then walk(-e.dy * C.ground_scroll, 0)
	else distance = math.clamp(distance * math.exp(e.dy * C.zoom_rate), C.camera_min, orbit_max) end
end

function Camera.key(e)
	if e.cancelled then held, last_time = {}, nil; return end
	if e.repeated then return end
	if e.code == "KeyF" and e.down then Camera.overview(); return end
	if keys[e.code] then
		if not next(held) then last_time = nil end
		held[e.code] = e.down or nil
	end
end

function Camera.moving()
	return next(held) ~= nil
end

local function axis(positive, negative)
	return (held[positive] and 1 or 0) - (held[negative] and 1 or 0)
end

function Camera.tick(e)
	if e.elapsed == last_time then return end
	local dt = last_time and math.clamp(e.elapsed - last_time, 0, 0.1) or 0
	last_time = e.elapsed
	if follow then
		yaw = yaw - axis("ArrowRight","ArrowLeft")*C.camera_turn*dt
		pitch = math.clamp(pitch+axis("ArrowUp","ArrowDown")*C.camera_turn*dt,0.1,0.8)
		distance = math.clamp(distance*math.exp(axis("KeyQ","KeyE")*C.camera_zoom*dt),2,12)
		return
	end
	local forward, right = axis("KeyW", "KeyS"), axis("KeyD", "KeyA")
	local length = math.max(1, math.sqrt(forward * forward + right * right))
	if grounded then
		yaw -= axis("ArrowRight", "ArrowLeft") * C.camera_turn * dt
		pitch = math.clamp(pitch + axis("ArrowUp", "ArrowDown") * C.camera_turn * dt, -1.2, 0.8)
		walk(forward * C.ground_speed * dt / length, right * C.ground_speed * dt / length)
		height = math.clamp(height + axis("KeyE", "KeyQ") * dt, 0.6, 3.5)
		return
	end
	yaw += axis("ArrowRight", "ArrowLeft") * C.camera_turn * dt
	pitch = math.clamp(pitch + axis("ArrowUp", "ArrowDown") * C.camera_turn * dt, 0.15, 1.4)
	local step = C.camera_speed * dt / length
	target[1] += (right * math.cos(yaw) - forward * math.sin(yaw)) * step
	target[3] += (-right * math.sin(yaw) - forward * math.cos(yaw)) * step
	distance = math.clamp(distance * math.exp(axis("KeyQ", "KeyE") * C.camera_zoom * dt), C.camera_min, orbit_max)
end

return Camera
