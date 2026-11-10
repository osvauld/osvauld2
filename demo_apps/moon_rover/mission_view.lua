local C = require("theme")
local M = require("mission")
local Camera = require("camera")
local View = {}

local function follow()
	Camera.follow(M.pose,M.level.terrain.sample)
end

function View.enter()
	M.enter()
	follow()
end

local function map()
	if Camera.is_following() then Camera.overview(C.mission.map) else follow() end
end

local function key(e)
	if M.key(e) then
		if e.cancelled then Camera.key(e) end
		return
	end
	if e.code=="KeyF" then
		if e.down and not e.repeated then map() end
		return
	end
	if e.code=="KeyZ" then
		Camera.key({code="KeyE",down=e.down,repeated=e.repeated})
	else Camera.key(e) end
end

local function button(id,label,action)
	return ui.button({id=id,h=38,px=16,radius=8,fill=C.panel,hover_fill=C.hover,
		ui.text({label,color=C.text,font_size=14,no_wrap=true}),on_click=action})
end

local function pedal(name,label)
	return ui.button({id="mission-pedal-"..name,h=40,px=14,radius=8,fill=C.panel,hover_fill=C.hover,
		ui.text({label,color=C.text,font_size=14,no_wrap=true}),on_hover=function(e) M.pedal(name,e) end})
end

function View.view(leave)
	local pose = M.pose()
	local objective = M.finished and "Sample delivered. Reset run to explore again." or
		(M.cargo and "Return the sample to the lander" or "Collect a sample at the crater beacon")
	return ui.col({id="mission-root",full=true,stretch=true,fill=C.bg,pad=12,gap=8,
		on_key=key,on_frame=Camera.moving() and Camera.tick or nil,
		ui.row({align_center=true,gap=16,
			ui.text({"Crater expedition",font_size=24,color=C.text,no_wrap=true}),
			ui.text({id="mission-speed",string.format("%.1f m/s",M.speed(pose)),font_size=14,color=C.muted,no_wrap=true}),
			ui.col({grow=true}),
			ui.text({id="mission-history","Returns: "..M.history(),font_size=13,color=C.muted,no_wrap=true}),
		}),
		ui.text({id="mission-objective",objective..string.format(" · %.0f m",M.distance(pose)),font_size=14,color=C.accent}),
		ui.row({gap=16,
			ui.text({id="mission-cargo",M.cargo and "Cargo: sample secured" or "Cargo: empty",font_size=13,color=C.mission.sample}),
			ui.text({id="mission-status",M.status,font_size=13,color=C.muted}),
		}),
		ui.scene3d({id="mission-view",grow=true,min_h=160,scene=M.world:scene(Camera.describe(),{running=M.running}),
			on_zone=M.zone,on_drag=Camera.drag,on_wheel=Camera.wheel,on_click=Camera.focus}),
		ui.row({gap=8,
			button("mission-drive",M.running and "Driving" or "Drive / Resume",M.drive),
			button("mission-pause","Pause",M.pause),
			button("mission-reset", "Reset run", M.reset),
			button("mission-camera",Camera.is_following() and "Map" or "Follow rover",map),
		}),
		ui.row({gap=8,
			button("mission-interact",M.cargo and "Deliver sample (E)" or "Collect sample (E)",M.interact),
			button("mission-course","Test course",leave),
		}),
		ui.row({gap=8,pedal("left","← Steer"),pedal("gas","Throttle"),pedal("reverse","Reverse"),pedal("brake","Brake"),pedal("right","Steer →")}),
		ui.text({"W/S drive · A/D steer · Space brake · E interact · P pause · R reset",font_size=12,color=C.muted}),
		ui.text({"Drag to orbit · Scroll or Q/Z to zoom · Arrows orbit · F map · Falls return to lander",font_size=12,color=C.muted}),
	})
end

return View
