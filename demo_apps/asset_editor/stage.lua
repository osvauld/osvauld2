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
		local frame = gfx.drawing({ size = a.size, vars = a.vars, parts = parts }):pose({})
		rests[name] = { frame = frame, asset = a }
	end
	return rests[name]
end

local SNAP = 24 -- how near a plug must come to a matching socket to equip there
local last = {} -- last view's placements: where a grab starts, where sockets are while dragging

-- Where each entity's box goes: a drag in flight, its own position, or — mounted — its host's
-- socket less its own plug. A released drag is dropped here: this view's mirror already has it.
local function placements(entities)
	if drag and drag.released then
		drag = nil
	end
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
			at[e.id] = drag.snap and drag.snap.at or drag.at
		elseif e.pos then
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

-- Every socket the dragged entity's plug fits, as stage points — the interface is the kind.
-- An entity mounted on the dragged one can't be its host: that would be a cycle.
local function sockets_for(ent, entities)
	local plug = rest(ent.asset) and rest(ent.asset).asset.plug
	local found = {}
	for _, host in ipairs(entities) do
		local r, h = rest(host.asset), last[host.id]
		local riding = host.mount and host.mount.to == ent.id
		if plug and r and h and host.id ~= ent.id and not riding then
			for name, socket in pairs(r.asset.sockets or {}) do
				if socket.kind == plug.kind then
					local x, y = h[1] + socket.at[1], h[2] + socket.at[2]
					table.insert(found, { to = host.id, socket = name, x = x, y = y })
				end
			end
		end
	end
	return found, plug
end

local function on_drag(e, entities)
	if e.phase == "start" then
		for _, ent in ipairs(entities) do
			if ent.id == e.shape and last[ent.id] then
				local from = { last[ent.id][1], last[ent.id][2] }
				local sockets, plug = sockets_for(ent, entities)
				drag = { id = ent.id, ent = ent, x0 = e.x, y0 = e.y, from = from, at = from }
				drag.sockets, drag.plug = sockets, plug
			end
		end
	elseif drag and not drag.released then
		drag.at = { drag.from[1] + e.x - drag.x0, drag.from[2] + e.y - drag.y0 }
		drag.snap = nil
		for _, s in ipairs(drag.sockets) do
			local px, py = drag.at[1] + drag.plug.at[1], drag.at[2] + drag.plug.at[2]
			if (px - s.x) ^ 2 + (py - s.y) ^ 2 <= SNAP * SNAP then
				local at = { s.x - drag.plug.at[1], s.y - drag.plug.at[2] }
				drag.snap = { to = s.to, socket = s.socket, at = at }
			end
		end
		if e.phase == "end" then
			local function field(name)
				return { "scene", "entities", drag.id, name }
			end
			local ent, snap = drag.ent, drag.snap
			if snap then -- equip: it rides the socket and has no place of its own
				M.asset:set(field("mount"), doc.map({ to = snap.to, socket = snap.socket }))
				if ent.pos then
					M.asset:delete(field("pos"))
				end
			else
				M.asset:set(field("pos"), doc.list({ drag.at[1], drag.at[2] }))
				if ent.mount then -- pulled off its socket
					M.asset:delete(field("mount"))
				end
			end
			drag.released = true
		end
	end
end

-- A ring at every socket the dragged plug fits, filled where it would snap.
local function socket_marks()
	local marks = {}
	for _, s in ipairs(drag and drag.sockets or {}) do
		local k, r = 0.5523 * 9, 9
		local x, y = s.x, s.y
		local ring = gfx.path({
			{ "move", x + r, y }, { "cubic", x + r, y + k, x + k, y + r, x, y + r },
			{ "cubic", x - k, y + r, x - r, y + k, x - r, y },
			{ "cubic", x - r, y - k, x - k, y - r, x, y - r },
			{ "cubic", x + k, y - r, x + r, y - k, x + r, y }, { "close" },
		})
		local on = drag.snap and drag.snap.to == s.to and drag.snap.socket == s.socket
		local brush = gfx.solid(on and C.accent or C.muted)
		table.insert(marks, on and gfx.fill({ path = ring, brush = brush })
			or gfx.stroke({ path = ring, brush = brush, width = 2 }))
	end
	return marks
end

function S.view()
	local scene = M.asset.scene
	if not scene then
		return ui.text({ "opening the stage…", color = C.muted, no_wrap = true })
	end
	local w, h = scene.size[1], scene.size[2]
	last = placements(scene.entities)
	local items = { backdrop(w, h) }
	for _, e in ipairs(scene.entities) do
		local r, p = rest(e.asset), last[e.id]
		if r and p then
			local place = { 1, 0, 0, 1, p[1], p[2] }
			table.insert(items, gfx.instance({ id = e.id, visual = r.frame, transform = place }))
		end
	end
	for _, mark in ipairs(socket_marks()) do
		table.insert(items, mark)
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
