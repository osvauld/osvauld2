local glb = require("glb")

local model = gfx.load_glb("models/hierarchy.glb")
-- A textured sign, stood to the left of the hierarchy wherever the file put it.
local sign = gfx.load_glb("models/textured.glb")
local SIGN_AT = { -2.2, 1.2, 0.5 }

local function describe(node)
	local parts = {}
	for k, v in pairs(node.extras or {}) do
		parts[#parts + 1] = k .. "=" .. (type(v) == "table" and table.concat(v, "+") or tostring(v))
	end
	table.sort(parts)
	local parent = node.parent and model.nodes[node.parent].name or "-"
	return (node.name or "?") .. " <" .. parent .. "> " .. table.concat(parts, " ")
end

local objects = (function()
	local objects = glb.objects(model)
	local placed = glb.objects(sign, {
		place = function(_, t)
			return { translation = SIGN_AT, rotation = t.rotation, scale = t.scale }
		end,
	})
	for _, o in ipairs(placed) do
		objects[#objects + 1] = o
	end
	return objects
end)()

-- Orbit about the target; drag to turn. Starts at eye {4, 3.5, 6}.
local TARGET = { 0, 1, 0 }
local RADIUS = math.sqrt(4 * 4 + 2.5 * 2.5 + 6 * 6)
local yaw, pitch = math.atan2(4, 6), math.asin(2.5 / RADIUS)
local grab

local function scene()
	local flat = RADIUS * math.cos(pitch)
	return gfx.scene3d({
		camera = {
			eye = { TARGET[1] + flat * math.sin(yaw), TARGET[2] + RADIUS * math.sin(pitch), TARGET[3] + flat * math.cos(yaw) },
			target = TARGET,
			fov_y = 40,
		},
		objects = objects,
	})
end

return function()
	local rows = {}
	for i, node in ipairs(model.nodes) do
		rows[#rows + 1] = ui.text({ id = "node-" .. i, describe(node) })
	end
	return ui.col({
		full = true,
		stretch = true,
		gap = 8,
		ui.scene3d({
			id = "view",
			grow = true,
			scene = scene(),
			on_drag = function(e)
				if e.phase == "start" then
					grab = { yaw, pitch }
				end
				if grab then
					yaw = grab[1] - e.dx * 0.008
					pitch = math.clamp(grab[2] + e.dy * 0.008, -1.2, 1.4)
				end
				if e.phase == "end" then
					grab = nil
				end
			end,
		}),
		ui.col({ gap = 4, table.unpack(rows) }),
	})
end
