-- Stage edit: every entity of a scene drawn as a named instance in one frame, so a drag reports
-- the entity it grabbed. A drag moves it live; one write lands on release.
local C = require("theme")
local M = require("model")

local S = {}
local rests = {} -- asset name → { frame = its rest pose, asset = its mirror }, built once
local drag = nil -- { id, x0, y0, from, at, released } while an entity is moved
local floor = nil -- the stage's backdrop, built on first use
local play = nil -- { name, clips = { entity id → clip handle } } while a stage plays; nil is edit

local function rest(name)
	if not rests[name] then
		local a = M.load(name)
		local parts = M.parts(a)
		if not parts then
			return nil -- seeded this frame; it shows from the next
		end
		local drawing = gfx.drawing({ size = a.size, vars = a.vars, parts = parts })
		rests[name] = { frame = drawing:pose({}), drawing = drawing, asset = a }
	end
	return rests[name]
end

local variants = {} -- entity id → its look when it sets `vars`, rebuilt when they change

-- An entity's look: its asset's rest, or — when the entity sets `vars` — a variant of it.
local function look(e)
	local r = rest(e.asset)
	if not r or not e.vars then
		return r
	end
	local said = {}
	for name, v in pairs(e.vars) do
		table.insert(said, name .. "=" .. tostring(v))
	end
	table.sort(said)
	local key = e.asset .. ":" .. table.concat(said, ",")
	if not variants[e.id] or variants[e.id].key ~= key then
		local a = r.asset
		local module = { size = a.size, vars = a.vars, parts = M.parts(a) }
		local drawing = gfx.drawing(module, { vars = e.vars })
		variants[e.id] = { key = key, drawing = drawing, frame = drawing:pose({}), asset = a }
	end
	return variants[e.id]
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

local function ring(x, y, r)
	local k = 0.5523 * r
	return gfx.path({
		{ "move", x + r, y }, { "cubic", x + r, y + k, x + k, y + r, x, y + r },
		{ "cubic", x - k, y + r, x - r, y + k, x - r, y },
		{ "cubic", x - r, y - k, x - k, y - r, x, y - r },
		{ "cubic", x + k, y - r, x + r, y - k, x + r, y }, { "close" },
	})
end

-- The world fields an entity plays with: its asset's `world` defaults, each replaced whole by
-- the entity's own, where `false` takes one away. Keys are the room's: only an entity has them.
local FIELDS = { "collider", "sensor", "loose", "group", "blocks" }
function S.fields(e, a)
	local out = { controller = e.controller or nil }
	for _, k in ipairs(FIELDS) do
		local v = e[k]
		if v == nil then
			v = a.world and a.world[k]
		end
		out[k] = v or nil
	end
	return out
end

-- Each collider as a line and each sensor dashed, where the entity stands. A mounted entity
-- has neither while it rides.
local function body_marks(entities)
	local marks = {}
	for _, e in ipairs(entities) do
		local r, p = rest(e.asset), last[e.id]
		local f = r and p and not e.mount and S.fields(e, r.asset) or {}
		for _, k in ipairs({ "collider", "sensor" }) do
			local s = f[k]
			if s then
				local at = s.at or { 0, 0 }
				local x, y = p[1] + at[1], p[2] + at[2]
				local path = s.rect and gfx.path({
					{ "move", x, y }, { "line", x + s.rect[1], y }, { "line", x + s.rect[1], y + s.rect[2] },
					{ "line", x, y + s.rect[2] }, { "close" },
				}) or ring(x, y, s.circle)
				local brush = gfx.solid(k == "sensor" and C.muted or C.accent)
				table.insert(marks, gfx.stroke({
					path = path, brush = brush, width = 1.5, dashes = k == "sensor" and { 6, 4 } or nil,
				}))
			end
		end
	end
	return marks
end

-- A ring at every socket the dragged plug fits, filled where it would snap.
local function socket_marks()
	local marks = {}
	for _, s in ipairs(drag and drag.sockets or {}) do
		local circle = ring(s.x, s.y, 9)
		local on = drag.snap and drag.snap.to == s.to and drag.snap.socket == s.socket
		local brush = gfx.solid(on and C.accent or C.muted)
		table.insert(marks, on and gfx.fill({ path = circle, brush = brush })
			or gfx.stroke({ path = circle, brush = brush, width = 2 }))
	end
	return marks
end

-- Clip handles are made once per press of play, so each press starts every clip from its top.
local function start(entities)
	play = { name = M.name, clips = {} }
	for _, e in ipairs(entities) do
		local r = rest(e.asset)
		local clip = e.clip and r and r.asset.clips and r.asset.clips[e.clip]
		if clip then
			play.clips[e.id] = gfx.clip(clip)
		end
	end
end

-- One world entity. A mounted one rides its socket's part with `turn`, pivoting on its own plug,
-- so a hat nods with the head; the world carries only by an entity that is not itself carried.
local function world_entity(e, by_id, at)
	local r = look(e)
	if not r or not at[e.id] then
		return false
	end
	local out = { id = e.id, pos = at[e.id], drawing = r.drawing, clip = play.clips[e.id] }
	local host = e.mount and by_id[e.mount.to]
	local h = host and not host.mount and rest(host.asset)
	local socket = h and h.asset.sockets and h.asset.sockets[e.mount.socket]
	local plug = r.asset.plug
	if socket and plug then
		out.attach = { to = host.id, part = socket.part, at = socket.at, pivot = plug.at, turn = true }
	end
	for k, v in pairs(S.fields(e, r.asset)) do
		out[k] = v
	end
	return out
end

-- Stage play: the scene as a world, at drawing scale — the viewport's zoom does the rest.
local function world(scene)
	local at, by_id = placements(scene.entities), {}
	for _, e in ipairs(scene.entities) do
		by_id[e.id] = e
	end
	local w, h = scene.size[1], scene.size[2]
	-- Entities are the world's own positional entries: a nested list would read as one entity.
	local spec = { id = "stage-play", width = w, height = h, fill = C.floor }
	for i, e in ipairs(scene.entities) do
		spec[i] = world_entity(e, by_id, at)
	end
	return ui.world(spec)
end

-- "edit" and "play" for a stage, made with the editor's own chip.
function S.bar(chip)
	local playing = play ~= nil and play.name == M.name
	local scene = M.asset.scene
	return ui.row({
		gap = 6,
		chip("stage:edit", "edit", not playing, function()
			play = nil
		end),
		chip("stage:play", "play", playing, function()
			if scene then
				start(scene.entities)
			end
		end),
	})
end

function S.view()
	local scene = M.asset.scene
	if not scene then
		return ui.text({ "opening the stage…", color = C.muted, no_wrap = true })
	end
	if play and play.name == M.name then
		return world(scene)
	end
	local w, h = scene.size[1], scene.size[2]
	last = placements(scene.entities)
	local items = { backdrop(w, h) }
	for _, e in ipairs(scene.entities) do
		local r, p = look(e), last[e.id]
		if r and p then
			local place = { 1, 0, 0, 1, p[1], p[2] }
			table.insert(items, gfx.instance({ id = e.id, visual = r.frame, transform = place }))
		end
	end
	for _, mark in ipairs(body_marks(scene.entities)) do
		table.insert(items, mark)
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
