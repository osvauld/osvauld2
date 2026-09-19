local graph = require("graph")

local C = {
	bg = "#0e1116",
	bar = "#161b22",
	canvas = "#11151c",
	node = "#1f2630",
	node_drag = "#2a3442",
	node_line = "#3a4556",
	handle = "#6e7b8f",
	accent = "#58a6ff",
	text = "#e6edf3",
	muted = "#8b949e",
	faint = "#5c6673",
}

local brush = {
	edge = gfx.solid("#6e7b8f"),
	edge_on = gfx.solid(C.accent),
}

-- Viewer-local interaction state; slice 1 persists nothing.
-- `connect` is a port drag in flight: source node, pointer, the valid node under it, and
-- `spawn`, where a release on empty canvas would place a new node. `editing` is an open field:
-- node, which field ("label" or "body"), and its draft text. `resize` is a corner-grip drag.
-- `hovered` is the edge a click would select.
local S = { drag = nil, selected = nil, hovered = nil, connect = nil, editing = nil, resize = nil }

-- Closer than this to its own port, a release is a fumble rather than a request for a node.
local SPAWN_MIN = 24
-- How far from a curve, in canvas units, a click still picks the edge.
local EDGE_HIT = 8

-- There is no blur event, so every other action saves an open field first. A blank name keeps
-- the old one; blank notes are allowed.
local function commit_edit()
	local ed = S.editing
	if not ed then return end
	local text = ed.text:match("^%s*(.-)%s*$")
	local n = graph.get(ed.id)
	if ed.field == "body" then
		n.body = text
	elseif text ~= "" then
		n.label = text
	end
	S.editing = nil
end

local function open_edit(n, field)
	commit_edit()
	S.editing = { id = n.id, field = field, text = n[field] }
end

local function editing(n, field)
	return S.editing ~= nil and S.editing.id == n.id and S.editing.field == field
end

-- Nodes paint over edges, so a point on a node picks no edge.
local function pick_edge(x, y)
	if graph.node_at(x, y) then return nil end
	return graph.edge_at(x, y, EDGE_HIT)
end

-- A gesture in flight owns the pointer; hover shouldn't light edges under it.
local function busy()
	return S.drag ~= nil or S.connect ~= nil or S.resize ~= nil
end

local function edge_curve(e)
	return graph.curve(graph.get(e.from), graph.get(e.to))
end

local function edges_visual()
	local items = { width = graph.W, height = graph.H }
	for _, e in ipairs(graph.edges) do
		local on = S.selected == e.id
		local hot = S.hovered == e.id and not busy()
		local x0, y0, x1, y1, x2, y2, x3, y3 = edge_curve(e)
		local b = (on or hot) and brush.edge_on or brush.edge
		table.insert(items, gfx.stroke({
			path = gfx.path({ { "move", x0, y0 }, { "cubic", x1, y1, x2, y2, x3, y3 } }),
			brush = b, width = on and 3 or 2, cap = "round",
		}))
		table.insert(items, gfx.fill({ path = gfx.path(graph.arrow(x2, y2, x3, y3, 10, 5)), brush = b }))
	end
	local c = S.connect
	if c then
		-- Snap to the target's in-port so the preview is exactly the edge a release creates.
		local ex, ey = c.x, c.y
		if c.target then
			ex, ey = graph.in_port(graph.get(c.target))
		elseif c.spawn then
			ex, ey = graph.in_port(c.spawn)
		end
		local x0, y0 = graph.out_port(graph.get(c.from))
		local _, _, x1, y1, x2, y2, x3, y3 = graph.curve_between(x0, y0, ex, ey)
		table.insert(items, gfx.stroke({
			path = gfx.path({ { "move", x0, y0 }, { "cubic", x1, y1, x2, y2, x3, y3 } }),
			brush = brush.edge_on, width = 2, cap = "round", dashes = { 7, 6 },
		}))
		table.insert(items, gfx.fill({
			path = gfx.path(graph.arrow(x2, y2, x3, y3, 10, 5)), brush = brush.edge_on,
		}))
	end
	return gfx.frame(items)
end

local function field_style(t)
	t.w_full, t.px, t.radius = true, 6, 6
	t.fill, t.stroke, t.color = C.bg, { 1, C.accent }, C.text
	t.on_input = function(v) S.editing.text = v end
	t.on_esc = function() S.editing = nil end
	return t
end

-- Each field takes its own click, so a press opens the field it landed on; a real drag of the
-- node cancels it.
local function label_view(n)
	if editing(n, "label") then
		return ui.input(field_style({
			id = "label-input:" .. n.id, value = S.editing.text, autofocus = true,
			h = 26, font_size = 13, on_enter = commit_edit,
		}))
	end
	return ui.row({
		id = "label:" .. n.id,
		w_full = true, h = 26, px = 6, align_center = true,
		ui.text({ n.label, color = C.text, font_size = 13, no_wrap = true }),
		on_click = function() open_edit(n, "label") end,
	})
end

-- Notes wrap to the node's width, so a resize reflows them. Enter in the area is a newline;
-- click outside to save.
local function body_view(n)
	if editing(n, "body") then
		return ui.text_area(field_style({
			id = "body-input:" .. n.id, value = S.editing.text, autofocus = true,
			grow = true, py = 4, font_size = 12,
		}))
	end
	local empty = n.body == ""
	return ui.col({
		id = "body:" .. n.id,
		w_full = true, grow = true, px = 6,
		ui.text({ empty and "Add notes…" or n.body, color = empty and C.faint or C.muted, font_size = 12 }),
		on_click = function() open_edit(n, "body") end,
	})
end

-- `dx, dy` are cumulative from the press, in canvas space, so zoom is already undone.
local function node_view(n)
	local dragging = S.drag ~= nil and S.drag.id == n.id
	local resizing = S.resize ~= nil and S.resize.id == n.id
	local lit = dragging or resizing or (S.connect ~= nil and S.connect.target == n.id)
	local t = {
		id = "node:" .. n.id,
		absolute = true, left = n.x, top = n.y,
		w = n.w, h = n.h, radius = 10, px = 4, py = 6, gap = 2,
		fill = lit and C.node_drag or C.node,
		stroke = { 1, lit and C.accent or C.node_line },
		label_view(n),
		body_view(n),
	}
	if S.editing and S.editing.id == n.id then
		-- No drag while a field is open, so a press in it places the caret. The no-op click
		-- keeps that press from reaching the canvas, whose click would save.
		t.fill, t.stroke = C.node_drag, { 1, C.accent }
		t.on_click = function() end
		return ui.col(t)
	end
	t.hover_stroke = { 1, C.accent }
	t.on_drag = function(e)
		local phase, dx, dy = e.phase, e.dx, e.dy
		if phase == "start" then
			commit_edit()
			S.drag = { id = n.id, from_x = n.x, from_y = n.y }
			return
		end
		if not (S.drag and S.drag.id == n.id) then return end
		n.x, n.y = graph.clamp_pos(S.drag.from_x + dx, S.drag.from_y + dy, n.w, n.h)
		if phase == "end" then S.drag = nil end
	end
	return ui.col(t)
end

local grip_visual = gfx.frame({
	width = 10, height = 10,
	gfx.stroke({ path = gfx.path({ { "move", 9, 2 }, { "line", 2, 9 } }), brush = brush.edge, width = 1.5, cap = "round" }),
	gfx.stroke({ path = gfx.path({ { "move", 9, 6 }, { "line", 6, 9 } }), brush = brush.edge, width = 1.5, cap = "round" }),
})

-- Bottom-right corner. A sibling like the port, painted above the node so it wins the press.
local function grip_view(n)
	return ui.col({
		id = "grip:" .. n.id,
		absolute = true, left = n.x + n.w - 14, top = n.y + n.h - 14, w = 14, h = 14,
		radius = 4, center = true, hover_fill = "#58a6ff33",
		ui.frame({ visual = grip_visual }),
		on_drag = function(e)
			local phase, dx, dy = e.phase, e.dx, e.dy
			if phase == "start" then
				commit_edit()
				S.resize = { id = n.id, from_w = n.w, from_h = n.h }
			end
			if not (S.resize and S.resize.id == n.id) then return end
			n.w, n.h = graph.clamp_size(n, S.resize.from_w + dx, S.resize.from_h + dy)
			if phase == "end" then S.resize = nil end
		end,
	})
end

-- A sibling rather than a node child: the node's rounded fill would clip a dot on its edge.
-- The drop target is found by geometry here, not `on_drop`, so the preview can snap mid-drag.
local function port_view(n)
	local px, py = graph.out_port(n)
	local active = S.connect ~= nil and S.connect.from == n.id
	return ui.col({
		id = "port:" .. n.id,
		absolute = true, left = px - 6, top = py - 6, w = 12, h = 12, radius = 6,
		fill = active and C.accent or C.handle,
		hover_fill = C.accent,
		on_drag = function(e)
			local phase, dx, dy = e.phase, e.dx, e.dy
			if phase == "start" then
				commit_edit()
				S.connect = { from = n.id, x = px, y = py }
			end
			local c = S.connect
			if not (c and c.from == n.id) then return end
			c.x, c.y = px + dx, py + dy
			local hit = graph.node_at(c.x, c.y)
			c.target = hit and graph.can_connect(n.id, hit.id) and hit.id or nil
			c.spawn = nil
			if not hit and (c.x - px) ^ 2 + (c.y - py) ^ 2 > SPAWN_MIN ^ 2 then
				-- The new node's in-port lands under the pointer.
				local x, y = graph.clamp_pos(c.x, c.y - graph.NODE_H / 2, graph.NODE_W, graph.NODE_H)
				c.spawn = { x = x, y = y, w = graph.NODE_W, h = graph.NODE_H }
			end
			if phase == "end" then
				if c.target then
					graph.connect(n.id, c.target)
				elseif c.spawn then
					graph.connect(n.id, graph.add_node(c.spawn.x, c.spawn.y).id)
				end
				S.connect = nil
			end
		end,
	})
end

-- Where a release will put the new node. No id and no handlers, so it never takes a hit.
local function ghost_view()
	local s = S.connect and S.connect.spawn
	if not s then return false end
	return ui.col({
		absolute = true, left = s.x, top = s.y,
		w = graph.NODE_W, h = graph.NODE_H, radius = 10, center = true,
		stroke_dash = { 1, C.accent, 5, 4 },
		ui.text({ "New node", color = C.muted, font_size = 13, no_wrap = true }),
	})
end

local function status()
	if S.editing then
		local label = graph.get(S.editing.id).label
		if S.editing.field == "body" then
			return "editing notes of " .. label .. " · click outside saves · esc cancels"
		end
		return "renaming " .. label .. " · enter saves · esc cancels"
	end
	if S.resize then
		local n = graph.get(S.resize.id)
		return string.format("resizing %s · %d × %d", n.label, n.w, n.h)
	end
	if S.connect then
		local c = S.connect
		local to = c.target and graph.get(c.target).label or c.spawn and "new node" or "drop on a node"
		return "connecting " .. graph.get(c.from).label .. " → " .. to
	end
	if S.drag then
		local n = graph.get(S.drag.id)
		return string.format("dragging %s @ %d, %d", n.label, n.x, n.y)
	end
	-- A deleted edge can stay hovered until the pointer moves.
	local hot = S.hovered ~= S.selected and graph.get(S.hovered)
	if hot then
		return graph.get(hot.from).label .. " → " .. graph.get(hot.to).label .. " · click to select"
	end
	if S.selected then
		local e = graph.get(S.selected)
		return "selected " .. graph.get(e.from).label .. " → " .. graph.get(e.to).label
	end
	return "drag nodes · click an edge to select · click a name or notes to edit · drag a port to connect or create · drag a corner to resize"
end

local function add_button()
	return ui.button({
		id = "add-node", h = 28, px = 12, radius = 6, center = true,
		fill = "#1c2a3d", hover_fill = "#24406a",
		ui.text({ "Add node", color = C.text, font_size = 12, no_wrap = true }),
		on_click = function()
			commit_edit()
			-- Lua can't see the camera, so free nodes cascade from the canvas corner.
			local k = #graph.nodes % 8
			graph.add_node(40 + k * 28, 40 + k * 28)
		end,
	})
end

local function delete_button()
	if not S.selected then return false end
	return ui.button({
		id = "delete-edge", h = 28, px = 12, radius = 6, center = true,
		fill = "#3a1d22", hover_fill = "#5a2630",
		ui.text({ "Delete edge", color = "#ffb4bd", font_size = 12, no_wrap = true }),
		on_click = function()
			commit_edit()
			graph.disconnect(S.selected)
			S.selected = nil
		end,
	})
end

return function()
	-- Paint order is hit order: edges, nodes, then ports and grips on top.
	local body = { ui.frame({ id = "graph-edges", absolute = true, left = 0, top = 0, visual = edges_visual() }) }
	for _, n in ipairs(graph.nodes) do table.insert(body, node_view(n)) end
	for _, n in ipairs(graph.nodes) do table.insert(body, port_view(n)) end
	for _, n in ipairs(graph.nodes) do table.insert(body, grip_view(n)) end
	table.insert(body, ghost_view())

	return ui.col({
		full = true, fill = C.bg,
		ui.row({
			h = 44, px = 16, gap = 16, align_center = true, fill = C.bar,
			ui.text({ "Node graph", color = C.text, font_size = 14, no_wrap = true }),
			ui.text({ status(), color = C.muted, font_size = 12, no_wrap = true }),
			ui.col({ grow = true }),
			add_button(),
			delete_button(),
		}),
		ui.col({
			id = "graph-camera", grow = true, zoomable = true,
			ui.col({
				id = "graph-canvas", w = graph.W, h = graph.H, no_shrink = true, fill = C.canvas,
				-- Canvas units with zoom undone, so picking an edge is geometry. Past the press slop
				-- the press pans instead.
				on_click = function(e)
					local x, y = e.x, e.y
					commit_edit()
					local e = pick_edge(x, y)
					S.selected = e and e.id or nil
				end,
				on_hover = function(e)
					local phase, x, y = e.phase, e.x, e.y
					local e = phase ~= "leave" and pick_edge(x, y)
					S.hovered = e and e.id or nil
				end,
				body,
			}),
		}),
	})
end
