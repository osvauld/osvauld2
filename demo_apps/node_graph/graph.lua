-- Seed graph and edge math. Positions are viewer-local: nothing here writes a document.
local graph = {}

graph.W, graph.H = 1400, 900
-- Nodes carry their own `w`/`h`; these are the size a new node starts at and the floor a
-- resize stops at.
graph.NODE_W, graph.NODE_H = 160, 64
graph.MIN_W, graph.MIN_H = 96, 48

graph.nodes = {
	{ id = "ingest", label = "Ingest", x = 60, y = 380, body = "Pulls events off the queue" },
	{ id = "parse", label = "Parse", x = 280, y = 220, body = "JSON → typed records" },
	{ id = "validate", label = "Validate", x = 280, y = 540 },
	{ id = "enrich", label = "Enrich", x = 520, y = 140 },
	{ id = "dedupe", label = "Dedupe", x = 520, y = 380 },
	{ id = "quarantine", label = "Quarantine", x = 520, y = 640, body = "Held for manual review" },
	{ id = "index", label = "Index", x = 760, y = 240 },
	{ id = "store", label = "Store", x = 760, y = 500 },
	{ id = "search", label = "Search", x = 1000, y = 300 },
	{ id = "report", label = "Report", x = 1000, y = 580 },
}

graph.edges = {
	{ id = "e1", from = "ingest", to = "parse" },
	{ id = "e2", from = "ingest", to = "validate" },
	{ id = "e3", from = "parse", to = "enrich" },
	{ id = "e4", from = "parse", to = "dedupe" },
	{ id = "e5", from = "validate", to = "dedupe" },
	{ id = "e6", from = "validate", to = "quarantine" },
	{ id = "e7", from = "enrich", to = "index" },
	{ id = "e8", from = "dedupe", to = "store" },
	{ id = "e9", from = "dedupe", to = "index" },
	{ id = "e10", from = "index", to = "search" },
	{ id = "e11", from = "store", to = "report" },
	{ id = "e12", from = "quarantine", to = "report" },
}

local by_id = {}
for _, n in ipairs(graph.nodes) do
	n.w, n.h, n.body = graph.NODE_W, graph.NODE_H, n.body or ""
	by_id[n.id] = n
end
for _, e in ipairs(graph.edges) do by_id[e.id] = e end

function graph.get(id)
	return by_id[id]
end

function graph.out_port(n)
	return n.x + n.w, n.y + n.h / 2
end

function graph.in_port(n)
	return n.x, n.y + n.h / 2
end

-- Topmost node under a canvas point; later nodes paint over earlier ones.
function graph.node_at(x, y)
	for i = #graph.nodes, 1, -1 do
		local n = graph.nodes[i]
		if x >= n.x and x <= n.x + n.w and y >= n.y and y <= n.y + n.h then
			return n
		end
	end
end

-- Keeps a box of size w×h inside the canvas.
function graph.clamp_pos(x, y, w, h)
	return math.max(0, math.min(graph.W - w, x)), math.max(0, math.min(graph.H - h, y))
end

-- A resize grows from the top-left corner, so the canvas edge caps it from the node's origin.
function graph.clamp_size(n, w, h)
	return math.max(graph.MIN_W, math.min(graph.W - n.x, w)),
		math.max(graph.MIN_H, math.min(graph.H - n.y, h))
end

local next_node = #graph.nodes + 1

function graph.add_node(x, y)
	local n = { id = "n" .. next_node, label = "Node " .. next_node, body = "" }
	next_node = next_node + 1
	n.w, n.h = graph.NODE_W, graph.NODE_H
	n.x, n.y = graph.clamp_pos(x, y, n.w, n.h)
	table.insert(graph.nodes, n)
	by_id[n.id] = n
	return n
end

function graph.can_connect(from, to)
	if from == to then return false end
	for _, e in ipairs(graph.edges) do
		if e.from == from and e.to == to then return false end
	end
	return true
end

local next_edge = #graph.edges + 1

function graph.connect(from, to)
	if not graph.can_connect(from, to) then return nil end
	local e = { id = "e" .. next_edge, from = from, to = to }
	next_edge = next_edge + 1
	table.insert(graph.edges, e)
	by_id[e.id] = e
	return e
end

function graph.disconnect(id)
	for i, e in ipairs(graph.edges) do
		if e.id == id then
			table.remove(graph.edges, i)
			by_id[id] = nil
			return
		end
	end
end

-- Handles pull horizontally so the curve leaves and enters level; the 60pt floor makes a
-- backward edge loop instead of kinking.
function graph.curve_between(x0, y0, x3, y3)
	local pull = math.max(60, math.abs(x3 - x0) * 0.5)
	return x0, y0, x0 + pull, y0, x3 - pull, y3, x3, y3
end

function graph.curve(a, b)
	local x0, y0 = graph.out_port(a)
	local x3, y3 = graph.in_port(b)
	return graph.curve_between(x0, y0, x3, y3)
end

function graph.point_at(t, x0, y0, x1, y1, x2, y2, x3, y3)
	local u = 1 - t
	local a, b, c, d = u * u * u, 3 * u * u * t, 3 * u * t * t, t * t * t
	return a * x0 + b * x1 + c * x2 + d * x3, a * y0 + b * y1 + c * y2 + d * y3
end

local function seg_dist2(px, py, ax, ay, bx, by)
	local dx, dy = bx - ax, by - ay
	local len2 = dx * dx + dy * dy
	local t = len2 > 0 and math.max(0, math.min(1, ((px - ax) * dx + (py - ay) * dy) / len2)) or 0
	local cx, cy = ax + t * dx - px, ay + t * dy - py
	return cx * cx + cy * cy
end

local STEPS = 24

-- Nearest edge within `tol` of a canvas point, measured against each curve cut into chords.
function graph.edge_at(x, y, tol)
	local best, best_d = nil, tol * tol
	for _, e in ipairs(graph.edges) do
		local x0, y0, x1, y1, x2, y2, x3, y3 = graph.curve(by_id[e.from], by_id[e.to])
		local ax, ay = x0, y0
		for i = 1, STEPS do
			local bx, by = graph.point_at(i / STEPS, x0, y0, x1, y1, x2, y2, x3, y3)
			local d = seg_dist2(x, y, ax, ay, bx, by)
			if d <= best_d then best, best_d = e, d end
			ax, ay = bx, by
		end
	end
	return best
end

-- A cubic's end tangent points from its last control point to its end point.
function graph.arrow(x2, y2, x3, y3, len, half)
	local tx, ty = x3 - x2, y3 - y2
	local d = math.sqrt(tx * tx + ty * ty)
	if d < 1e-6 then tx, ty, d = 1, 0, 1 end
	tx, ty = tx / d, ty / d
	local bx, by = x3 - tx * len, y3 - ty * len
	return {
		{ "move", x3, y3 },
		{ "line", bx - ty * half, by + tx * half },
		{ "line", bx + ty * half, by - tx * half },
		{ "close" },
	}
end

return graph
