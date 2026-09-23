local C = require("theme")

local angle = 0.55
local selected = "none"
local camera = { yaw = 0.55, pitch = 0.32, distance = 6.6 }
local orbit_start = nil
local coral_label = gfx.text_surface({
	text = "OSVAULD 3D",
	font_size = 34,
	color = "#ffffff",
	background = "#831f35",
})
local blue_label = gfx.text_surface({
	text = "CLICK ME",
	font_size = 38,
	color = "#ffffff",
	background = "#173787",
})

local function rotation_y(radians)
	return { 0, math.sin(radians / 2), 0, math.cos(radians / 2) }
end

local function scene()
	local cp = math.cos(camera.pitch)
	local eye = {
		camera.distance * cp * math.sin(camera.yaw),
		camera.distance * math.sin(camera.pitch),
		camera.distance * cp * math.cos(camera.yaw),
	}
	return gfx.scene3d({
		camera = { eye = eye, target = { 0, 0, 0 }, fov_y = 42 },
		objects = {
			{
				id = "coral-cube",
				position = { -0.38, 0, 0.18 },
				rotation = rotation_y(angle),
				scale = { 2.2, 1.35, 1.15 },
				color = selected == "coral-cube" and "#ff9aae" or "#ff526f",
				surface = coral_label,
			},
			{
				id = "blue-cube",
				position = { 0.38, 0, -0.18 },
				rotation = rotation_y(-angle * 0.8),
				scale = { 1.15, 2.15, 1.35 },
				color = selected == "blue-cube" and "#9db1ff" or "#4f7cff",
				surface = blue_label,
			},
		},
	})
end

local function button(id, label, fn)
	return ui.button({
		id = id,
		px = 14,
		py = 9,
		radius = 7,
		fill = C.accent,
		hover_fill = C.accent_hover,
		on_click = fn,
		ui.text({ label, color = "#ffffff", no_wrap = true }),
	})
end

return function()
	return ui.col({
		id = "model-viewer",
		full = true,
		stretch = true,
		gap = 14,
		pad = 18,
		fill = C.bg,
		ui.text({ "Lua-first 3D model viewer", color = C.text, font_size = 24, no_wrap = true }),
		ui.row({
			grow = true,
			h_full = true,
			gap = 14,
			stretch = true,
			ui.col({
				w = 220,
				no_shrink = true,
				gap = 12,
				pad = 16,
				radius = 10,
				fill = C.panel,
				ui.text({ "Selected object", color = C.muted, no_wrap = true }),
				ui.text({ selected, color = C.text, font_size = 18, no_wrap = true }),
				button("rotate-left", "Rotate left", function()
					angle = angle - 0.18
				end),
				button("rotate-right", "Rotate right", function()
					angle = angle + 0.18
				end),
				ui.text({
					"Click to select. Drag to orbit. Wheel to zoom. Text is GPU-rendered on each cube.",
					color = C.muted,
					font_size = 13,
				}),
			}),
			ui.scene3d({
				id = "scene-viewport",
				scene = scene(),
				grow = true,
				h_full = true,
				min_w = 320,
				min_h = 320,
				radius = 10,
				fill = C.viewport,
				stroke = { 1, C.line },
				on_click = function(e)
					selected = e.object or "none"
				end,
				on_wheel = function(e)
					camera.distance = math.max(3.2, math.min(12, camera.distance - e.dy * 0.025))
				end,
				on_drag = function(e)
					if e.phase == "start" then
						orbit_start = { yaw = camera.yaw, pitch = camera.pitch }
					elseif orbit_start then
						camera.yaw = orbit_start.yaw - e.dx * 0.008
						camera.pitch = math.max(-1.2, math.min(1.2, orbit_start.pitch + e.dy * 0.008))
						if e.phase == "end" then orbit_start = nil end
					end
				end,
			}),
		}),
	})
end
