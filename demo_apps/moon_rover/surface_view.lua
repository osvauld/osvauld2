local C = require("theme")
local S = require("surface")
local Camera = require("camera")
local View = {}

local function follow() Camera.follow(S.pose,S.sample) end

function View.enter() follow() end

local function key(e)
	if S.key(e) then
		if e.cancelled then Camera.key(e) end
		return
	end
	if e.code=="KeyZ" then Camera.key({code="KeyE",down=e.down,repeated=e.repeated}) else Camera.key(e) end
end

local function button(id,label,action)
	return ui.button({id=id,h=38,px=16,radius=8,fill=C.panel,hover_fill=C.hover,
		ui.text({label,color=C.text,font_size=14,no_wrap=true}),on_click=action})
end

local function pedal(name,label)
	return ui.button({id="surface-pedal-"..name,h=40,px=14,radius=8,fill=C.panel,hover_fill=C.hover,
		ui.text({label,color=C.text,font_size=14,no_wrap=true}),on_hover=function(e) S.pedal(name,e) end})
end

function View.view(leave)
	local p = S.pose().position
	return ui.col({id="surface-root",full=true,stretch=true,fill=C.bg,pad=12,gap=8,
		on_key=key,on_frame=Camera.moving() and Camera.tick or nil,
		ui.row({align_center=true,gap=16,
			ui.text({"Moon surface",font_size=24,color=C.text,no_wrap=true}),
			ui.text({id="surface-speed",string.format("%.1f m/s",S.speed()),font_size=14,color=C.muted,no_wrap=true}),
			ui.text({id="surface-position",string.format("x %.0f · z %.0f",p[1],p[3]),font_size=13,color=C.muted,no_wrap=true}),
			ui.col({grow=true}),
			ui.text({id="surface-status",S.status,font_size=14,color=C.accent,no_wrap=true}),
		}),
		ui.scene3d({id="surface-view",grow=true,min_h=160,scene=S.world:scene(Camera.describe(),{running=S.running,poses=S.poses()}),
			on_zone=S.zone,on_drag=Camera.drag,on_wheel=Camera.wheel}),
		ui.row({gap=8,
			button("surface-drive",S.running and "Driving" or "Drive",S.drive),
			button("surface-pause","Pause",S.pause),
			button("surface-reset","Reset",S.reset),
			button("surface-course","Test course",leave),
			pedal("left","← Steer"),pedal("gas","Throttle"),pedal("reverse","Reverse"),pedal("brake","Brake"),pedal("right","Steer →"),
		}),
		ui.text({"W/S drive · A/D steer · Space brake · P pause · R reset · Drag to orbit · Scroll or Q/Z to zoom",font_size=12,color=C.muted}),
	})
end

return View
