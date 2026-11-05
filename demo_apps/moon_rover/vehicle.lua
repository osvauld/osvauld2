-- Rover assembly and driving policy; physical quantities use SI units.
local V = {
	spawn = {-10,1.2,-8}, size = {1.6,0.35,2.6}, mass = 400, center = {0,-0.1,0},
	radius = 0.35, width = 0.26, rest = 0.28, travel = 0.18,
	stiffness = 2500, damping = 500, grip = 1.1,
	torque = 42, brake_torque = 105, speed_limit = 6, steer = 28,
	mounts = {{-0.9,-0.15,0.9},{0.9,-0.15,0.9},{-0.9,-0.15,-0.9},{0.9,-0.15,-0.9}},
}

function V.recipe()
	return {id = "rover", position = V.spawn, box = V.size, mass = V.mass,
		center = V.center, dynamic = true}
end

function V.wheels()
	local wheels = {}
	for i,at in ipairs(V.mounts) do
		wheels[i]={id="rover-wheel-"..i,body="rover",at=at,radius=V.radius,width=V.width,
			rest=V.rest,travel=V.travel,stiffness=V.stiffness,damping=V.damping,grip=V.grip}
	end
	return wheels
end

-- Driver intent and brake priority are app rules; native wheels receive physical commands.
function V.apply(world,input)
	local throttle,brake,steering = input.throttle or 0,input.brake or 0,input.steering or 0
	local motor = nil
	if brake==0 and throttle~=0 then
		motor={velocity=(throttle>0 and 1 or -1)*V.speed_limit/V.radius,max_torque=math.abs(throttle)*V.torque}
	end
	local commands = {}
	for i=1,#V.mounts do
		commands[i]={id="rover-wheel-"..i,motor=motor,brake=brake*V.brake_torque,
			steering=i<=2 and -steering*math.rad(V.steer) or 0}
	end
	world:set_wheels(commands)
end

return V
