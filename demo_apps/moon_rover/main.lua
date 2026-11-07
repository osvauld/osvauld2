local C = require("theme")
local M = require("model")
local Camera = require("camera")
local MissionView = require("mission_view")
local Mission = require("mission")
-- Worlds exist only from module initialization; a missing surface model leaves the rest working.
local surface_ok,SurfaceView = pcall(require,"surface_view")
local surface_error = not surface_ok and tostring(SurfaceView) or nil
local in_mission = false
local on_surface = false
local drive_keys = {KeyW=true,KeyS=true,KeyA=true,KeyD=true,Space=true}

local function overview()
	M.inspect()
	Camera.overview()
end

local function drive()
	M.drive()
	Camera.follow(function()
		return M.world:get("rover") or {position=M.spawn,rotation={0,0,0,1}}
	end)
end

local function inspect(action)
	return function() M.inspect(); action() end
end

local function key(e)
	if M.driving then
		if e.cancelled or drive_keys[e.code] then M.key(e); return end
		if e.down and not e.repeated then
			if e.code == "KeyF" then overview(); return end
			if e.code == "KeyP" then
				if M.running then M.pause() else M.release() end
			elseif e.code == "KeyR" then M.reset() end
		end
		Camera.key(e)
		return
	end
	Camera.key(e)
	if e.cancelled or e.repeated or not e.down then return end
	if e.code == "Space" then
		if M.running then M.pause() else M.release() end
	elseif e.code == "KeyR" then M.reset() end
end

local function pedal(name,label)
	return ui.button({id="pedal-"..name,h=42,px=16,radius=8,fill=C.panel,hover_fill=C.hover,
		ui.text({label,color=C.text,font_size=14,no_wrap=true}),
		on_hover=function(e) M.pedal(name,e) end})
end

local function button(id, label, action)
	return ui.button({
		id = id, h = 38, px = 18, radius = 8, fill = C.panel, hover_fill = C.hover,
		ui.text({label, color = C.text, font_size = 14, no_wrap = true}), on_click = action,
	})
end

return function()
	if on_surface then
		return SurfaceView.view(function() require("surface").pause(); on_surface=false; Camera.ground() end)
	end
	if in_mission then
		return MissionView.view(function() Mission.pause(); in_mission=false; Camera.ground() end)
	end
	local rover = M.world:get("rover")
	local speed = rover and math.sqrt(rover.velocity[1]^2+rover.velocity[3]^2) or 0
	return ui.col({
		id = "course-root", full = true, stretch = true, fill = C.bg, pad = 12, gap = 8,
		on_key = key, on_frame = Camera.moving() and Camera.tick or nil,
		ui.row({align_center = true, gap = 16,
			ui.text({M.driving and "Moon rover" or "Lunar test course", font_size = 24, color = C.text, no_wrap = true}),
			ui.text({id="rover-speed",string.format("%.1f m/s",speed),color=C.muted,font_size=14,no_wrap=true}),
			ui.col({grow = true}),
			ui.text({id = "course-status", M.status, color = C.accent, font_size = 14, no_wrap = true}),
		}),
		ui.text({M.driving and "Explore the course. Slow down before the crater and the edges." or
			"Gold sphere rolls down the ramp. Mint block lands on the flat.", color = C.muted, font_size = 13}),
		ui.row({gap=12,align_center=true,
			button("mission-enter","Crater expedition →",function() M.pause(); in_mission=true; MissionView.enter() end),
			ui.text({"Drive to the crater, collect a sample, return to the lander",font_size=13,color=C.muted}),
			surface_ok and button("surface-enter","Moon surface →",function()
				M.pause(); on_surface=true; SurfaceView.enter()
			end) or false,
			surface_error and ui.text({id="surface-error",surface_error,font_size=12,color=C.accent}) or false,
		}),
		not M.driving and ui.text({"Gold lines: 0.2 m height steps · Dark grid: 2 m squares", color = C.contours, font_size = 12}) or false,
		ui.scene3d({
			id = "course-view", grow = true, min_h = 160,
			scene = M.world:scene(Camera.describe(), {running = M.running}),
			on_click = Camera.focus, on_drag = Camera.drag, on_wheel = Camera.wheel,
		}),
		ui.row({gap = 8, align_center = true,
			button("course-release", "Release", M.release),
			button("course-pause", "Pause", M.pause),
			button("course-reset", "Reset", M.reset),
			button("course-overview", "Overview", overview),
			button("rover-drive", "Drive rover", drive),
			ui.col({grow = true}),
			ui.text({M.driving and "P pause · R reset" or "Space pause/resume · R reset", color = C.muted, font_size = 12}),
		}),
		M.driving and ui.row({gap=8,
			pedal("left","← Steer"),pedal("gas","Throttle"),pedal("reverse","Reverse"),
			pedal("brake","Brake"),pedal("right","Steer →"),
		}) or false,
		not M.driving and ui.row({gap = 8, align_center = true,
			ui.text({"Views", color = C.muted, font_size = 12}),
			button("course-view-ground", "Ground", inspect(Camera.ground)),
			button("course-view-flat", "Flat", inspect(function() Camera.preset("flat") end)),
			button("course-view-ramp", "Ramp", inspect(function() Camera.preset("ramp") end)),
			button("course-view-bumps", "Bumps", inspect(function() Camera.preset("bumps") end)),
			button("course-view-crater", "Crater", inspect(function() Camera.preset("crater") end)),
		}) or false,
		ui.text({M.driving and "Mouse: drag to orbit rover · Scroll to zoom" or
			Camera.is_grounded() and "Mouse: drag to look · Scroll to move closer · Click to look at a point" or
			"Mouse: click to focus · Drag to orbit · Scroll to zoom", color = C.muted, font_size = 12}),
		ui.text({M.driving and "W/S throttle/reverse · A/D steer · Space brake · Hold mouse pedals to drive" or
			(Camera.is_grounded() and "Ground: WASD walk · Arrows look · Q/E lower/raise · F overview" or
			"Camera: WASD move · Arrow keys orbit · Q/E zoom · F overview"), color = C.muted, font_size = 12}),
	})
end
