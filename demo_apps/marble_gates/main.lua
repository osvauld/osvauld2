local C = require("theme")
local sphere = require("sphere")(0.25)
local yaw, pitch, distance, grab = 0.65, 0.45, 10, nil
local visible = true

local game = gfx.world3d({
	id = "marble-game",
	scene = gfx.scene3d({
		camera = { eye = {7,5,7}, target = {0,1,0} },
		objects = {
			{ id = "platform", scale = {6,0.5,4}, color = C.platform },
			{ id = "marble", mesh = sphere, position = {0,3,0}, color = C.marble },
		},
	}),
	bodies = {
		{ id = "platform", box = {6,0.5,4}, position = {0,0,0} },
		{ id = "marble", sphere = 0.25, position = {0,3,0}, dynamic = true },
	},
})

local function button(id, label, action)
	return ui.button({
		id = id, h = 34, px = 18, center = true, radius = 6, fill = C.button,
		ui.text({ label, color = C.text, font_size = 13, no_wrap = true }),
		on_click = action,
	})
end

return function()
	local camera = {
		eye = {distance*math.sin(yaw)*math.cos(pitch),
			1+distance*math.sin(pitch), distance*math.cos(yaw)*math.cos(pitch)},
		target = {0,1,0},
	}
	return ui.col({
		grow = true, stretch = true, pad = 20, gap = 12, fill = C.bg,
		ui.text({ "Marble Gates — first drop", color = C.text, font_size = 23, no_wrap = true }),
		ui.row({ gap = 8,
			button("marble-reset", "Reset marble", function() game:reset("marble") end),
			button("marble-visibility", visible and "Hide / pause" or "Show / resume",
				function() visible = not visible end),
		}),
		visible and ui.scene3d({
			id = "marble-view", scene = game:scene(camera), grow = true, min_h = 260,
			on_drag = function(e)
				if e.phase == "start" then grab = {yaw, pitch} end
				if grab then
					yaw = grab[1] + e.dx * 0.008
					pitch = math.clamp(grab[2] + e.dy * 0.008, 0.1, 1.2)
				end
				if e.phase == "end" then grab = nil end
			end,
			on_wheel = function(e) distance = math.clamp(distance + e.dy * 0.02, 5, 20) end,
		}) or ui.col({ grow = true }),
		ui.text({ "Native gravity, bounce and sleep. Drag to orbit; wheel to zoom. Ramps and gates come next.",
			color = C.muted, font_size = 12, no_wrap = true }),
	})
end
