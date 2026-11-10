local C = require("theme")
local objects = {{id="floor",position=C.floor_at,scale=C.floor_size,color=C.floor},
	{id="carriage",scale=C.size,color=C.accent}}
local wheel_mesh=require("wheel_mesh")(C.radius,C.width,C.segments)
local wheels = {}
for i,at in ipairs(C.mounts) do
	local id="wheel-"..i
	wheels[i]={id=id,body="carriage",at=at,radius=C.radius,width=C.width,
		rest=C.rest,travel=C.travel,stiffness=C.stiffness,damping=C.damping,grip=C.grip}
	objects[#objects+1]={id=id,mesh=wheel_mesh,color=C.wheel}
end
local world=gfx.world3d({id="wheel-bench",gravity={0,-9.81,0},
	scene=gfx.scene3d({camera={eye=C.eye,target=C.target,far=100},objects=objects}),
	bodies={{id="floor",position=C.floor_at,box=C.floor_size},
		{id="carriage",position=C.spawn,box=C.size,dynamic=true,mass=C.mass}},wheels=wheels})
local running,mode=false,"Freewheel"
local function actuate(next_mode)
	mode=next_mode
	local commands={}
	for i=1,#wheels do
		commands[i]={id=wheels[i].id,brake=mode=="Brake" and C.brake_torque or 0,
			motor=mode=="Motor" and {velocity=C.motor_velocity,max_torque=C.motor_torque} or nil}
	end
	world:set_wheels(commands)
end
local function button(id,label,action)
	return ui.button({id=id,h=38,px=16,radius=7,fill=C.panel,on_click=action,
		ui.text({label,color=C.text,font_size=14})})
end
return function()
	local pose=world:get("carriage")
	return ui.col({full=true,stretch=true,pad=12,gap=10,fill=C.bg,
		ui.text({"Wheel and motor bench",font_size=25,color=C.text}),
		ui.text({"Observe acceleration, coasting and braking under Earth gravity.",font_size=14,color=C.muted}),
		ui.text({id="bench-readout",string.format("%.0f kg · %.2f m/s · %s",C.mass,pose.velocity[3],mode),font_size=15,color=C.accent}),
		ui.scene3d({id="bench-view",grow=true,min_h=200,scene=world:scene(nil,{running=running})}),
		ui.row({gap=8,button("bench-run","Run",function() running=true end),
			button("bench-pause","Pause",function() running=false end),
			button("bench-reset","Reset",function() running=false; world:reset("carriage"); actuate("Freewheel") end)}),
		ui.row({gap=8,button("bench-motor",string.format("Motor: %g rad/s, %g N·m",C.motor_velocity,C.motor_torque),function() actuate("Motor") end),
			button("bench-coast","Freewheel",function() actuate("Freewheel") end),
			button("bench-brake",string.format("Brake: %g N·m",C.brake_torque),function() actuate("Brake") end)}),
		ui.text({"The motor targets wheel speed within its torque limit. Freewheel removes motor and brake.",font_size=13,color=C.muted}),
	})
end
