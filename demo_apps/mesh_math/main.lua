local C = require("theme")
local amplitude, yaw, pitch, distance = 0.6, 0.5, 0.4, 13
local selected, grab = nil, nil
local angle = 0

-- A heightfield computed in Lua, including its analytic surface normals. Rust knows only triangles.
local function wave(a)
	local vertices, indices = {}, {}
	local nx, nz = 24, 18
	for z = 0, nz do
		for x = 0, nx do
			local px, pz = x / nx * 4 - 2, z / nz * 3 - 1.5
			local y = a * math.sin(px * 1.5) * math.cos(pz * 1.5)
			local dx = a * 1.5 * math.cos(px * 1.5) * math.cos(pz * 1.5)
			local dz = -a * 1.5 * math.sin(px * 1.5) * math.sin(pz * 1.5)
			table.insert(vertices, { position = { px, y, pz }, normal = { -dx, 1, -dz } })
		end
	end
	for z = 0, nz - 1 do
		for x = 0, nx - 1 do
			local v = z * (nx + 1) + x + 1
			for _, i in ipairs({ v, v + nx + 1, v + 1, v + 1, v + nx + 1, v + nx + 2 }) do
				table.insert(indices, i)
			end
		end
	end
	return gfx.mesh({ vertices = vertices, indices = indices })
end
local mesh = wave(amplitude)

local function button(id, label, action)
	return ui.button({
		id = id, w = 120, h = 34, center = true, radius = 6, fill = C.button,
		ui.text({ label, color = C.text, font_size = 13, no_wrap = true }),
		on_click = action,
	})
end

return function()
	local objects = {}
	for i, id in ipairs({ "coral-wave", "blue-wave" }) do
		table.insert(objects, {
			id = id, mesh = mesh, position = { (i - 1.5) * 5, 0, 0 },
			rotation = { 0, math.sin(angle / 2), 0, math.cos(angle / 2) },
			color = selected == id and C.selected or (i == 1 and C.left or C.right),
		})
	end
	local scene = gfx.scene3d({
		camera = { eye = { distance * math.sin(yaw) * math.cos(pitch),
			distance * math.sin(pitch), distance * math.cos(yaw) * math.cos(pitch) },
			target = { 0, 0, 0 }, far = 100 },
		objects = objects,
	})
	return ui.col({
		grow = true, stretch = true, pad = 20, gap = 12, fill = C.bg,
		ui.text({ "Mesh math — authored in Lua", color = C.text, font_size = 23, no_wrap = true }),
		ui.row({ gap = 8,
			button("wave-more", "Wave +", function() amplitude = math.min(1.5, amplitude + 0.2); mesh = wave(amplitude) end),
			button("wave-flat", "Flatten", function() amplitude = 0; mesh = wave(amplitude) end),
			button("wave-rotate", "Rotate", function() angle += 0.3 end),
		}),
		ui.scene3d({
			id = "mesh-view", scene = scene, grow = true, min_h = 260,
			on_click = function(e) selected = e.object end,
			on_drag = function(e)
				if e.phase == "start" then grab = { yaw, pitch } end
				if grab then
					yaw = grab[1] + e.dx * 0.008
					pitch = math.clamp(grab[2] + e.dy * 0.008, 0.1, 1.2)
				end
				if e.phase == "end" then grab = nil end
			end,
			on_wheel = function(e) distance = math.clamp(distance + e.dy * 0.02, 7, 25) end,
		}),
		ui.text({ "475 vertices / 864 triangles, shared twice. Drag to orbit; click to pick. Selected: " .. (selected or "none"),
			id = "mesh-status", color = C.muted, font_size = 12, no_wrap = true }),
	})
end
