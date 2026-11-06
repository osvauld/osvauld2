local C = require("theme")
local story = require("story")
local M = {}

local edges = {
	{ id = "save", from = { 138, 178 }, to = { 138, 250 } },
	{ id = "sync", from = { 252, 299 }, to = { 308, 129 }, bend = true },
	{ id = "merge", from = { 422, 178 }, to = { 422, 250 } },
	{ id = "push", from = { 536, 299 }, to = { 592, 129 }, bend = true },
	{ id = "apply", from = { 706, 178 }, to = { 706, 250 } },
	{ id = "rollback", from = { 308, 129 }, to = { 252, 299 }, bend = true },
}
local by_id = {}
for _, edge in ipairs(edges) do by_id[edge.id] = edge end

local function curve(edge)
	local a, b = edge.from, edge.to
	if edge.bend then
		local mid = (a[1] + b[1]) / 2
		return { a, { mid, a[2] }, { mid, b[2] }, b }
	end
	return { a, a, b, b }
end

function M.point(id, t)
	local p = curve(by_id[id])
	local u = 1 - t
	return u^3*p[1][1] + 3*u*u*t*p[2][1] + 3*u*t*t*p[3][1] + t^3*p[4][1],
		u^3*p[1][2] + 3*u*u*t*p[2][2] + 3*u*t*t*p[3][2] + t^3*p[4][2]
end

local paths, arrows = {}, {}
for _, edge in ipairs(edges) do
	local p = curve(edge)
	paths[edge.id] = gfx.path({
		{ "move", p[1][1], p[1][2] },
		{ "cubic", p[2][1], p[2][2], p[3][1], p[3][2], p[4][1], p[4][2] },
	})
	local x, y = edge.to[1], edge.to[2]
	local dx = edge.bend and (edge.to[1] > edge.from[1] and 1 or -1) or 0
	local dy = edge.bend and 0 or 1
	arrows[edge.id] = gfx.path({
		{ "move", x, y },
		{ "line", x - dx*9 - dy*5, y - dy*9 + dx*5 },
		{ "line", x - dx*9 + dy*5, y - dy*9 - dx*5 },
		{ "close" },
	})
end

local brushes = { base = gfx.solid(C.edge), mint = gfx.solid(C.mint), amber = gfx.solid(C.amber), red = gfx.solid(C.red) }
local frames = {}
for _, key in ipairs({ "base", "save", "sync", "merge", "push", "apply", "rollback" }) do
	frames[key] = {}
	for _, color in ipairs({ "mint", "amber", "red" }) do
		local items = { width = C.canvas_w, height = C.canvas_h }
		for _, edge in ipairs(edges) do
			if edge.id ~= "rollback" or key == "rollback" then
				local active = edge.id == key
				local brush = active and brushes[color] or brushes.base
				items[#items + 1] = gfx.stroke({
					path = paths[edge.id], brush = brush, width = active and 3 or 1.5,
					dashes = active and color ~= "mint" and { 5, 5 } or nil,
				})
				items[#items + 1] = gfx.fill({ path = arrows[edge.id], brush = brush })
			end
		end
		frames[key][color] = gfx.frame(items)
	end
end

local function label(text, size, color)
	return ui.text({ text, font_size = size, color = color, no_wrap = true })
end

function M.view(s, select_node)
	local scenario = story.scenarios[s.scenario]
	local stage = scenario.stages[s.step]
	local color = stage.refused and "red" or stage.waiting and "amber" or "mint"
	local statuses = story.statuses(s.scenario, s.step)
	local body = {
		ui.frame({ absolute = true, left = 0, top = 0, visual = frames[stage.edge or "base"][color] }),
	}
	for i, title in ipairs({ "YOUR DESKTOP", "HOME NODE · KUNKI", "OTHER DESKTOP" }) do
		body[#body + 1] = ui.col({
			absolute = true, left = 24 + (i - 1)*284, top = 22, w = C.node_w, gap = 6,
			label(title, 11, C.muted),
			ui.col({ h = 1, w_full = true, fill = C.line }),
		})
	end
	for i, node in ipairs(story.nodes) do
		local selected = s.selected == node.id
		local current = stage.node == node.id
		local accent = current and C[color] or C.blue
		body[#body + 1] = ui.col({
			id = "sync-node:" .. node.id, absolute = true, left = node.x, top = node.y,
			w = C.node_w, h = C.node_h, pad = 14, gap = 5, radius = 12,
			fill = current and C.active or C.node,
			stroke = { selected and 2 or 1, selected and accent or C.line },
			hover_stroke = { 2, C.blue }, cursor = "pointer",
			on_click = function() select_node(node.id) end,
			ui.row({
				gap = 8, align_center = true,
				label(string.format("%02d", i), 11, current and accent or C.muted),
				label(node.title, 15, C.text),
			}),
			label(node.caption, 11, C.muted),
			ui.text({ statuses[node.id], id = "sync-node-status:" .. node.id, font_size = 12, color = current and accent or C.muted, no_wrap = true }),
		})
	end
	if s.playing and stage.edge and not stage.waiting then
		local x, y = M.point(stage.edge, s.phase or 0)
		body[#body + 1] = ui.col({ absolute = true, left = x - 5, top = y - 5, w = 10, h = 10, radius = 5, fill = C[color] })
	end
	return ui.col({
		id = "sync-map", h = C.canvas_h, w_full = true, scroll_x = true,
		ui.col({ w = C.canvas_w, h = C.canvas_h, no_shrink = true, fill = C.canvas, body }),
	})
end

return M
