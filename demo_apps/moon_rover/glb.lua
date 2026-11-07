-- Turn a gfx.load_glb model into world-space scene3d objects. Plain Lua: what a node *means*
-- (collider, wheel, prop) is the app's rule, not this file's. Copy it into an app that loads
-- models; apps do not share modules.
local M = {}

local function qmul(a, b)
	return {
		a[4] * b[1] + a[1] * b[4] + a[2] * b[3] - a[3] * b[2],
		a[4] * b[2] - a[1] * b[3] + a[2] * b[4] + a[3] * b[1],
		a[4] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[4],
		a[4] * b[4] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3],
	}
end
M.qmul = qmul

-- Rotate v by unit quaternion q.
local function qrot(q, v)
	local x, y, z, w = q[1], q[2], q[3], q[4]
	local tx = 2 * (y * v[3] - z * v[2])
	local ty = 2 * (z * v[1] - x * v[3])
	local tz = 2 * (x * v[2] - y * v[1])
	return {
		v[1] + w * tx + (y * tz - z * ty),
		v[2] + w * ty + (z * tx - x * tz),
		v[3] + w * tz + (x * ty - y * tx),
	}
end
M.qrot = qrot

-- Compose parent then child. Scale multiplies per axis, which is exact unless a rotated child
-- sits under a non-uniformly scaled parent (glTF would shear; apply scale in Blender to avoid).
function M.compose(parent, child)
	local s = parent.scale
	local p = qrot(parent.rotation, {
		s[1] * child.translation[1],
		s[2] * child.translation[2],
		s[3] * child.translation[3],
	})
	return {
		translation = { parent.translation[1] + p[1], parent.translation[2] + p[2], parent.translation[3] + p[3] },
		rotation = qmul(parent.rotation, child.rotation),
		scale = { s[1] * child.scale[1], s[2] * child.scale[2], s[3] * child.scale[3] },
	}
end

-- World transform of every node, by index.
function M.world(model)
	local out = {}
	local function at(i)
		if out[i] then
			return out[i]
		end
		local n = model.nodes[i]
		out[i] = n.parent and M.compose(at(n.parent), n) or {
			translation = n.translation,
			rotation = n.rotation,
			scale = n.scale,
		}
		return out[i]
	end
	for i = 1, #model.nodes do
		at(i)
	end
	return out
end

-- One scene3d object per drawn primitive. `id` is the node name (`name/2` for a second
-- primitive); `opts.keep(node)` may skip nodes (colliders the app hides), `opts.place(i, t)`
-- may replace a world transform (a part Lua animates).
function M.objects(model, opts)
	opts = opts or {}
	local world = M.world(model)
	local objects = {}
	for i, node in ipairs(model.nodes) do
		if node.mesh and (not opts.keep or opts.keep(node)) then
			local t = opts.place and opts.place(i, world[i]) or world[i]
			for k, prim in ipairs(model.meshes[node.mesh]) do
				local material = prim.material and model.materials[prim.material]
				objects[#objects + 1] = {
					id = (node.name or ("node" .. i)) .. (k > 1 and ("/" .. k) or ""),
					mesh = prim.mesh,
					position = t.translation,
					rotation = t.rotation,
					scale = t.scale,
					color = material and material.color or "#c8c8c8",
					texture = material and material.texture,
				}
			end
		end
	end
	return objects
end

return M
