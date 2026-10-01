-- Stage edit: every entity of a scene drawn as a named instance in one frame, so a drag reports
-- the entity it grabbed. A drag moves it live; one write lands on release.
local C = require("theme")
local M = require("model")

local S = {}
local rests = {} -- asset name → { frame = its rest pose, asset = its mirror }, built once
local drag = nil -- { id, x0, y0, from, at, released } while an entity is moved
local floor = nil -- the stage's backdrop, built on first use

local function rest(name)
	if not rests[name] then
		local a = M.load(name)
		local parts = M.parts(a)
		if not parts then
			return nil -- seeded this frame; it shows from the next
		end
		local frame = gfx.drawing({ size = a.size, parts = parts }):pose({})
		rests[name] = { frame = frame, asset = a }
	end
	return rests[name]
end

-- Where each entity's box goes: a drag in flight, its own position, or — mounted — its host's
-- socket less its own plug. A drag outlives its release until the mirror shows the write, so
-- the entity doesn't snap back for the frame the mirror is behind.
local function placements(entities)
	local at, by_id = {}, {}
	for _, e in ipairs(entities) do
		by_id[e.id] = e
	end
	local function place(e)
		if at[e.id] ~= nil then
			return at[e.id] or nil
		end
		at[e.id] = false -- a mount cycle ends here instead of recursing forever
		if drag and drag.id == e.id then
			local landed = drag.released and e.pos[1] == drag.at[1] and e.pos[2] == drag.at[2]
			if landed then
				drag = nil
			else
				at[e.id] = drag.at
				return drag.at
			end
		end
		if e.pos then
			at[e.id] = { e.pos[1], e.pos[2] }
		elseif e.mount and by_id[e.mount.to] then
			local host, mine = rest(by_id[e.mount.to].asset), rest(e.asset)
			local socket = host and host.asset.sockets and host.asset.sockets[e.mount.socket]
			local plug = mine and mine.asset.plug
			local h = place(by_id[e.mount.to])
			if socket and plug and h then
				at[e.id] = { h[1] + socket.at[1] - plug.at[1], h[2] + socket.at[2] - plug.at[2] }
			end
		end
		return at[e.id] or nil
	end
	for _, e in ipairs(entities) do
		place(e)
	end
	return at
end

local function backdrop(w, h)
	if not floor then
		local rect = gfx.path({
			{ "move", 0, 0 }, { "line", w, 0 }, { "line", w, h }, { "line", 0, h }, { "close" },
		})
		floor = gfx.fill({ path = rect, brush = gfx.solid(C.floor) })
	end
	return floor
end

local function on_drag(e, entities)
	if e.phase == "start" then
		for _, ent in ipairs(entities) do
			if ent.id == e.shape and ent.pos then -- a mounted entity rides its host
				local from = { ent.pos[1], ent.pos[2] }
				drag = { id = ent.id, x0 = e.x, y0 = e.y, from = from, at = from }
			end
		end
	elseif drag and not drag.released then
		drag.at = { drag.from[1] + e.x - drag.x0, drag.from[2] + e.y - drag.y0 }
		if e.phase == "end" then
			M.asset:set({ "scene", "entities", drag.id, "pos" }, doc.list({ drag.at[1], drag.at[2] }))
			drag.released = true
		end
	end
end

function S.view()
	local scene = M.asset.scene
	if not scene then
		return ui.text({ "opening the stage…", color = C.muted, no_wrap = true })
	end
	local w, h = scene.size[1], scene.size[2]
	local at = placements(scene.entities)
	local items = { backdrop(w, h) }
	for _, e in ipairs(scene.entities) do
		local r, p = rest(e.asset), at[e.id]
		if r and p then
			local place = { 1, 0, 0, 1, p[1], p[2] }
			table.insert(items, gfx.instance({ id = e.id, visual = r.frame, transform = place }))
		end
	end
	return ui.frame({
		id = "stage-canvas",
		visual = gfx.frame({ width = w, height = h, table.unpack(items) }),
		on_drag = function(e)
			on_drag(e, scene.entities)
		end,
	})
end

-- The scene's entities, for the side panel: id, the asset it shows, and what it is mounted on.
function S.rows()
	local scene = M.asset.scene
	local rows = {}
	for _, e in ipairs(scene and scene.entities or {}) do
		local where = e.mount and ("on " .. e.mount.to .. "." .. e.mount.socket) or e.asset
		table.insert(rows, { id = e.id, note = where })
	end
	return rows
end

return S
